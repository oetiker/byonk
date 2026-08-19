//! Cache for device screen previews (`GET /api/admin/devices/{key}/preview`).
//!
//! A preview is a full render — Lua script, SVG rasterization, dithering —
//! and the client that asks for it is a Home Assistant camera entity, which
//! pulls a frame every few seconds for as long as somebody is looking at the
//! device page. Rendering per frame would be absurd, so a rendered PNG is
//! held here and re-served until one of two things happens:
//!
//! 1. **The fingerprint moves.** The caller hashes everything the render
//!    depends on that it can see — screen ref, params, panel, dither,
//!    geometry. Change the screen in Home Assistant and the next frame is a
//!    fresh render, which is what makes the preview feel live while you
//!    configure a device.
//! 2. **The entry ages past the screen's own refresh rate.** A screen that
//!    redraws itself every 5 minutes on the panel should not sit frozen in
//!    the preview for longer than that. The script's `refresh_rate` is
//!    already byonk's answer to "how often is this stale", so it is reused
//!    verbatim rather than invented here — floored at [`MIN_TTL_SECS`],
//!    because a screen declaring a few seconds would otherwise turn an open
//!    device page into a render loop.
//!
//! Nothing here runs on a timer. Entries are only examined when a request
//! arrives, so a preview nobody is watching costs nothing at all.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

/// Floor on how long a rendered preview is served before it is re-rendered.
///
/// A screen may declare a very short `refresh_rate` (a clock ticking every
/// few seconds). Honouring that literally would mean a continuous render
/// loop for as long as a device page is open — the preview is a thumbnail on
/// a config screen, not the panel, and does not need to keep up.
pub const MIN_TTL_SECS: i64 = 30;

/// How many previews to hold. Keys are per device *and* per view variant
/// (dithered/undithered x measured/spec colours), so this is a few times the
/// device count. A backstop against unbounded growth, not a tuning knob.
const DEFAULT_CAPACITY: usize = 64;

struct PreviewEntry {
    fingerprint: String,
    png: Vec<u8>,
    rendered_at: DateTime<Utc>,
    ttl: Duration,
}

/// Per-device cache of rendered previews, keyed by device key.
pub struct PreviewCache {
    entries: Mutex<HashMap<String, PreviewEntry>>,
    capacity: usize,
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewCache {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity: capacity.max(1),
        }
    }

    /// The cached PNG for `key`, if one was rendered from the same
    /// `fingerprint` and has not aged past its TTL. `now` is passed in so
    /// the expiry rule is testable without sleeping.
    pub fn get(&self, key: &str, fingerprint: &str, now: DateTime<Utc>) -> Option<Vec<u8>> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.fingerprint != fingerprint {
            return None;
        }
        if now - entry.rendered_at >= entry.ttl {
            return None;
        }
        Some(entry.png.clone())
    }

    /// Store a freshly rendered preview. `refresh_rate` is the script's own,
    /// in seconds; it is floored at [`MIN_TTL_SECS`]. Pass `0` for a render
    /// that has no opinion (a failed render's error image) to get the floor.
    pub fn store(
        &self,
        key: &str,
        fingerprint: &str,
        png: Vec<u8>,
        refresh_rate: u32,
        now: DateTime<Utc>,
    ) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.insert(
            key.to_string(),
            PreviewEntry {
                fingerprint: fingerprint.to_string(),
                png,
                rendered_at: now,
                ttl: Duration::seconds(i64::from(refresh_rate).max(MIN_TTL_SECS)),
            },
        );
        // Evict oldest-first while over capacity. Insert-then-evict (rather
        // than evict-then-insert) so the entry just rendered is measured
        // alongside the rest instead of being exempt from the rule.
        while entries.len() > self.capacity {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, e)| e.rendered_at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
    }

    /// Drop every cached preview.
    ///
    /// Called on a config reload. A preview's fingerprint covers the *device's*
    /// own configuration, but a render also depends on config a device only
    /// points at — a panel profile's colours and dither tuning, say — and
    /// editing one of those leaves every fingerprint that references it
    /// unchanged. Clearing wholesale on reload is both cheaper and more
    /// reliable than trying to enumerate what a config change could have
    /// touched; the cost is one re-render per device page that is open.
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp")
    }

    #[test]
    fn returns_the_stored_png_for_a_matching_fingerprint() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1, 2, 3], 300, t0());
        assert_eq!(cache.get("AA", "fp1", t0()), Some(vec![1, 2, 3]));
    }

    #[test]
    fn a_changed_fingerprint_misses() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1, 2, 3], 300, t0());
        assert_eq!(cache.get("AA", "fp2", t0()), None);
    }

    #[test]
    fn an_unknown_key_misses() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1, 2, 3], 300, t0());
        assert_eq!(cache.get("BB", "fp1", t0()), None);
    }

    /// The screen's own refresh rate is the TTL: still served just inside it,
    /// gone the moment it elapses.
    #[test]
    fn the_refresh_rate_is_the_ttl() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1], 300, t0());
        assert!(cache
            .get("AA", "fp1", t0() + Duration::seconds(299))
            .is_some());
        assert!(cache
            .get("AA", "fp1", t0() + Duration::seconds(300))
            .is_none());
    }

    /// A screen declaring a refresh rate below the floor still gets the
    /// floor — otherwise an open device page renders continuously.
    #[test]
    fn a_short_refresh_rate_is_floored() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1], 5, t0());
        assert!(cache
            .get("AA", "fp1", t0() + Duration::seconds(MIN_TTL_SECS - 1))
            .is_some());
        assert!(cache
            .get("AA", "fp1", t0() + Duration::seconds(MIN_TTL_SECS))
            .is_none());
    }

    /// A render with no refresh rate of its own (a failed render's error
    /// image) gets the floor rather than expiring instantly.
    #[test]
    fn a_zero_refresh_rate_gets_the_floor() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1], 0, t0());
        assert!(cache
            .get("AA", "fp1", t0() + Duration::seconds(MIN_TTL_SECS - 1))
            .is_some());
    }

    #[test]
    fn clear_drops_every_entry() {
        let cache = PreviewCache::new();
        cache.store("A", "fp", vec![1], 300, t0());
        cache.store("B", "fp", vec![2], 300, t0());
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn storing_the_same_key_twice_replaces_rather_than_grows() {
        let cache = PreviewCache::new();
        cache.store("AA", "fp1", vec![1], 300, t0());
        cache.store("AA", "fp2", vec![2], 300, t0());
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("AA", "fp2", t0()), Some(vec![2]));
    }

    #[test]
    fn over_capacity_evicts_the_oldest() {
        let cache = PreviewCache::with_capacity(2);
        cache.store("A", "fp", vec![1], 300, t0());
        cache.store("B", "fp", vec![2], 300, t0() + Duration::seconds(1));
        cache.store("C", "fp", vec![3], 300, t0() + Duration::seconds(2));

        assert_eq!(cache.len(), 2);
        assert!(cache.get("A", "fp", t0() + Duration::seconds(2)).is_none());
        assert!(cache.get("B", "fp", t0() + Duration::seconds(2)).is_some());
        assert!(cache.get("C", "fp", t0() + Duration::seconds(2)).is_some());
    }
}
