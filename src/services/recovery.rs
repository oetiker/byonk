//! Panel-recovery sessions.
//!
//! A recovery session asks a device to run full-panel wipes instead of showing
//! content, to clear ghosting and burn-in. The device-side work is TRMNL's own
//! `display_wipe()`, which the firmware runs whenever the display response's
//! `filename` is `screen_wiper.png`. One wipe is 100 x `fullUpdate(CLEAR_SLOW)`
//! with panel power held for the whole burst, which the panel runs at about
//! one black/white cycle per second: ~200 cycles taking ~190 s, measured on a
//! TRMNL X on 2026-08-21. Holding power across the burst is the point --
//! otherwise each cycle would pay its own power-up.
//!
//! Sessions live in memory only. A byonk restart cancels every run, which is
//! the same way [`crate::models::Device`] already treats runtime state, and it
//! fails safe: a device whose server forgets about it simply goes back to
//! showing content on its next poll. Nothing is persisted on the device either,
//! so a reboot or a lost network also ends a run by itself.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::DeviceId;

/// Upper bound on wipes per session.
///
/// One wipe measured ~190 s end to end and a full poll cycle ~270 s, so this
/// ceiling is roughly 15 hours of wiping. Generous, but still short of a day.
pub const MAX_WIPES: u32 = 200;

/// Wipes requested when the caller does not say.
pub const DEFAULT_WIPES: u32 = 20;

/// Refresh rate served to a device while a recovery run is in progress.
///
/// A run has to set its own poll cadence. A screen's `refresh` says how often
/// its *content* goes stale, which tells us nothing about how fast a wipe
/// sequence should proceed -- and obeying it between wipes is what stretched a
/// 10-wipe run from 45 minutes to 10 hours on 2026-08-21, when the panel was
/// showing a calibration screen with `refresh: 3600`.
///
/// 5 s is not a number invented here: it is the firmware's own fast-poll
/// interval (`RefreshInterval::fastPollSeconds` in `lib/trmnl`), and
/// `applyServerRate()` stores whatever the server sends without clamping. So
/// this is a cadence the device already runs at during normal setup.
pub const RECOVERY_REFRESH_RATE_SECS: u32 = 5;

/// Every written form of `key` that could name the same device.
///
/// A [`DeviceId`] is an opaque string, so both ends of a run — the admin API
/// that files a session and the poll that looks for it — have to spell the
/// device the same way. A MAC has one spelling, because the device reports it.
/// A registration code has two, raw `ABCDEFGHJK` and hyphenated
/// `ABCDE-FGHJK`, and an operator may type either in any case. Before the
/// device has ever checked in nothing connects that code to a MAC, so the
/// spelling they chose is all there is to go on.
///
/// The canonical hyphenated form leads, so a new session filed from this list
/// gets one predictable name. A key that is not a registration code — a MAC,
/// most of all — comes back as itself and nothing else.
pub fn spellings_of(key: &str) -> Vec<DeviceId> {
    let mut out = Vec::new();
    let normalized = key.to_uppercase().replace('-', "");
    // Codes are ten characters drawn from an uppercase alphabet, so anything
    // else is some other kind of key and must not be re-spelled.
    if normalized.len() == 10 && normalized.bytes().all(|b| b.is_ascii_uppercase()) {
        out.push(DeviceId::new(format!(
            "{}-{}",
            &normalized[..5],
            &normalized[5..]
        )));
        out.push(DeviceId::new(normalized));
    }
    let given = DeviceId::new(key);
    if !out.contains(&given) {
        out.push(given);
    }
    out
}

/// A recovery run in progress for one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverySession {
    /// Wipes requested when the session started.
    pub total: u32,
    /// Wipes already handed to the device.
    pub done: u32,
    /// When the session started.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// True between a wipe being handed out and the firmware's own follow-up
    /// poll arriving. See [`RecoveryRegistry::on_poll`] — the firmware polls
    /// twice per wipe, and only the first poll may count.
    awaiting_post_wipe_poll: bool,
}

impl RecoverySession {
    /// Wipes still to serve.
    pub fn remaining(&self) -> u32 {
        self.total.saturating_sub(self.done)
    }
}

#[cfg(test)]
impl RecoverySession {
    /// Build a session directly, for tests in other modules that only care
    /// about the reported numbers.
    pub fn for_test(total: u32, done: u32) -> Self {
        Self {
            total,
            done,
            started_at: chrono::Utc::now(),
            awaiting_post_wipe_poll: false,
        }
    }
}

/// In-memory store of active recovery sessions, keyed by device.
#[derive(Default)]
pub struct RecoveryRegistry {
    sessions: Arc<RwLock<HashMap<DeviceId, RecoverySession>>>,
}

impl RecoveryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start (or restart) a session. `wipes` is clamped to `1..=MAX_WIPES`.
    ///
    /// Restarting replaces any run already in progress rather than adding to
    /// it, so a caller who asks twice gets what they asked for the second time.
    pub async fn start(&self, device_id: &DeviceId, wipes: u32) -> RecoverySession {
        let total = wipes.clamp(1, MAX_WIPES);
        let session = RecoverySession {
            total,
            done: 0,
            started_at: chrono::Utc::now(),
            awaiting_post_wipe_poll: false,
        };
        self.sessions
            .write()
            .await
            .insert(device_id.clone(), session.clone());
        session
    }

    /// Cancel a session. Returns true if one was running.
    pub async fn cancel(&self, device_id: &DeviceId) -> bool {
        self.sessions.write().await.remove(device_id).is_some()
    }

    /// Current session for a device, if any.
    pub async fn get(&self, device_id: &DeviceId) -> Option<RecoverySession> {
        self.sessions.read().await.get(device_id).cloned()
    }

    /// The identifier this device's session is filed under, if it has one.
    ///
    /// A device polls with its MAC and nothing else, but a session can have
    /// been started under its *config* key: `/api/admin/devices/{key}/recover`
    /// takes the same key as `PATCH /devices/{key}`, and a device may be
    /// configured by registration code rather than by MAC. Checking every
    /// identifier the device answers to keeps the admin API and the poll path
    /// on one session, instead of the run silently never starting.
    ///
    /// Candidates are tried in order, so callers put the identifier they
    /// consider canonical first.
    pub async fn resolve_key(&self, candidates: &[DeviceId]) -> Option<DeviceId> {
        let sessions = self.sessions.read().await;
        candidates
            .iter()
            .find(|id| sessions.contains_key(*id))
            .map(|id| (*id).clone())
    }

    /// Every session currently running.
    pub async fn list(&self) -> Vec<(DeviceId, RecoverySession)> {
        self.sessions
            .read()
            .await
            .iter()
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect()
    }

    /// Decide what a device's poll should be answered with.
    ///
    /// Returns `Some(session)` when this poll should be told to wipe, and
    /// `None` when it should get normal content.
    ///
    /// The firmware polls **twice** per wipe: once to receive the instruction,
    /// then — inside `display_wipe()`'s caller in `bl.cpp` — again via
    /// `downloadAndShow()` as soon as the wipe finishes. Counting both would
    /// spend two wipes per wipe performed. Worse, answering the second poll
    /// with the wiper again trips the firmware's once-per-wake guard, which
    /// returns early and leaves the panel blank.
    ///
    /// So the follow-up poll is answered with normal content, and the next
    /// wipe waits for the next wake. If a device reboots after wiping without
    /// ever sending the follow-up poll, the next wake shows content once and
    /// then resumes — the run self-corrects rather than stalling.
    ///
    /// The session is dropped as the last wipe is handed out, so the firmware's
    /// own follow-up poll is what puts content back on the panel.
    pub async fn on_poll(&self, device_id: &DeviceId) -> Option<RecoverySession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(device_id)?;

        if session.awaiting_post_wipe_poll {
            session.awaiting_post_wipe_poll = false;
            return None;
        }

        session.done += 1;
        session.awaiting_post_wipe_poll = true;
        let claimed = session.clone();
        if claimed.remaining() == 0 {
            sessions.remove(device_id);
        }
        Some(claimed)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn spellings_of_a_code_cover_both_written_forms_canonical_first() {
        let ids = spellings_of("abcde-fghjk");
        let as_str: Vec<String> = ids.iter().map(|d| d.to_string()).collect();

        assert_eq!(
            as_str,
            vec!["ABCDE-FGHJK", "ABCDEFGHJK", "abcde-fghjk"],
            "hyphenated uppercase leads, so a new session gets one predictable name"
        );
    }

    #[test]
    fn spellings_of_a_mac_are_just_the_mac() {
        // The guard that keeps this true is what stops a MAC — or any other
        // key — being re-spelled into candidates that name nothing.
        // "1234567890" is the one that matters: ten characters, so only the
        // uppercase-alphabet half of the guard rejects it. Without that half it
        // would be re-spelled as "12345-67890", a key naming nothing.
        for key in [
            "AA:BB:CC:DD:EE:FF",
            "aabbccddeeff",
            "1234567890",
            "DEFAULT",
            "SHORT",
        ] {
            assert_eq!(
                spellings_of(key)
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>(),
                vec![key.to_string()],
                "{key} is not a registration code and must not be re-spelled"
            );
        }
    }
    use super::*;

    fn dev() -> DeviceId {
        DeviceId::new("AA:BB:CC:DD:EE:FF")
    }

    /// The firmware's two-polls-per-wipe rhythm: instruction, then the
    /// follow-up `downloadAndShow()` poll. Returns wipes actually performed.
    async fn run_wakes(reg: &RecoveryRegistry, id: &DeviceId, wakes: usize) -> usize {
        let mut wiped = 0;
        for _ in 0..wakes {
            if reg.on_poll(id).await.is_some() {
                wiped += 1;
                reg.on_poll(id).await; // the post-wipe re-poll
            }
        }
        wiped
    }

    #[tokio::test]
    async fn no_session_means_no_wipe() {
        let reg = RecoveryRegistry::new();
        assert!(reg.get(&dev()).await.is_none());
        assert!(reg.on_poll(&dev()).await.is_none());
    }

    #[tokio::test]
    async fn start_records_the_requested_total() {
        let reg = RecoveryRegistry::new();
        let s = reg.start(&dev(), 5).await;
        assert_eq!(s.total, 5);
        assert_eq!(s.done, 0);
        assert_eq!(s.remaining(), 5);
        assert_eq!(reg.get(&dev()).await, Some(s));
    }

    #[tokio::test]
    async fn wipes_are_clamped_to_the_allowed_range() {
        let reg = RecoveryRegistry::new();
        assert_eq!(reg.start(&dev(), 0).await.total, 1);
        assert_eq!(reg.start(&dev(), 999_999).await.total, MAX_WIPES);
    }

    #[tokio::test]
    async fn each_wake_performs_exactly_one_wipe() {
        let reg = RecoveryRegistry::new();
        reg.start(&dev(), 3).await;
        assert_eq!(run_wakes(&reg, &dev(), 3).await, 3);
        assert!(reg.get(&dev()).await.is_none());
    }

    /// The regression that motivates `awaiting_post_wipe_poll`: without it the
    /// firmware's own follow-up poll spends a second wipe, halving the run.
    #[tokio::test]
    async fn the_post_wipe_repoll_does_not_spend_a_wipe() {
        let reg = RecoveryRegistry::new();
        reg.start(&dev(), 4).await;

        assert_eq!(reg.on_poll(&dev()).await.unwrap().done, 1);
        assert!(
            reg.on_poll(&dev()).await.is_none(),
            "the follow-up poll must get content, not another wipe"
        );
        assert_eq!(reg.get(&dev()).await.unwrap().done, 1);

        assert_eq!(reg.on_poll(&dev()).await.unwrap().done, 2);
    }

    #[tokio::test]
    async fn a_reboot_that_skips_the_repoll_costs_one_cycle_not_the_run() {
        let reg = RecoveryRegistry::new();
        reg.start(&dev(), 2).await;
        reg.on_poll(&dev()).await; // wipe 1, no follow-up poll ever arrives
        assert!(reg.on_poll(&dev()).await.is_none()); // absorbed as the follow-up
        assert_eq!(reg.on_poll(&dev()).await.unwrap().done, 2); // resumes
        assert!(reg.get(&dev()).await.is_none());
    }

    #[tokio::test]
    async fn session_ends_as_the_last_wipe_is_handed_out() {
        let reg = RecoveryRegistry::new();
        reg.start(&dev(), 1).await;
        assert_eq!(reg.on_poll(&dev()).await.unwrap().done, 1);
        // Dropped, so the firmware's follow-up poll puts content back.
        assert!(reg.get(&dev()).await.is_none());
        assert!(reg.on_poll(&dev()).await.is_none());
    }

    #[tokio::test]
    async fn cancel_reports_whether_a_run_was_active() {
        let reg = RecoveryRegistry::new();
        assert!(!reg.cancel(&dev()).await);
        reg.start(&dev(), 10).await;
        assert!(reg.cancel(&dev()).await);
        assert!(reg.get(&dev()).await.is_none());
    }

    #[tokio::test]
    async fn restarting_replaces_rather_than_adds() {
        let reg = RecoveryRegistry::new();
        reg.start(&dev(), 10).await;
        reg.on_poll(&dev()).await;
        let s = reg.start(&dev(), 4).await;
        assert_eq!(s.total, 4);
        assert_eq!(s.done, 0);
    }

    #[tokio::test]
    async fn sessions_are_per_device() {
        let reg = RecoveryRegistry::new();
        let a = DeviceId::new("AA:AA:AA:AA:AA:AA");
        let b = DeviceId::new("BB:BB:BB:BB:BB:BB");
        reg.start(&a, 2).await;
        assert!(reg.on_poll(&b).await.is_none());
        assert_eq!(reg.get(&a).await.unwrap().remaining(), 2);
        assert_eq!(reg.list().await.len(), 1);
    }
}
