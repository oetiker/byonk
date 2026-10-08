use crate::assets::AssetLoader;
// The MCP tool's argument schema embeds these two types, so they derive it.
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Reserved device key whose `screen` is shown by every not-yet-configured
/// device (unregistered, or registered without its own screen). Replaces the
/// former `default_screen` + `registration.screen` settings.
pub const RESERVED_DEFAULT_KEY: &str = "DEFAULT";

/// Gamut mapping knobs, at every level of the tuning priority chain.
///
/// Frame-level, not per adaptation group: groups change only which pixels are
/// measured together to derive the compression factor, not the curve's shape.
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, schemars::JsonSchema)]
pub struct GamutTuningValues {
    /// Where compression begins, as a fraction of the reachable chroma limit.
    pub knee: Option<f32>,
    /// 0 = no mapping, 1 = full. Only 1 guarantees in-gamut output.
    pub amount: Option<f32>,
    /// Cap on the compression factor.
    pub max_compression: Option<f32>,
}

impl GamutTuningValues {
    /// Merge: self takes priority, other fills gaps.
    pub fn or(&self, other: &GamutTuningValues) -> GamutTuningValues {
        GamutTuningValues {
            knee: self.knee.or(other.knee),
            amount: self.amount.or(other.amount),
            max_compression: self.max_compression.or(other.max_compression),
        }
    }

    /// Returns true if all fields are None.
    pub fn is_empty(&self) -> bool {
        self.knee.is_none() && self.amount.is_none() && self.max_compression.is_none()
    }

    /// Fill the gaps from the crate defaults.
    pub fn resolve(&self) -> eink_dither::GamutOptions {
        let d = eink_dither::GamutOptions::default();
        eink_dither::GamutOptions {
            knee: self.knee.unwrap_or(d.knee),
            amount: self.amount.unwrap_or(d.amount),
            max_compression: self.max_compression.unwrap_or(d.max_compression),
        }
    }
}

/// Dither tuning values for max_error, noise_scale, chroma_clamp, strength.
///
/// Used at every level of the tuning priority chain:
/// panel defaults, device config, script overrides, dev UI overrides.
#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct DitherTuningValues {
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    #[serde(default)]
    pub gamut: GamutTuningValues,

    /// Captured value of the removed `error_clamp` key — never applied.
    ///
    /// 0.18.0 changed what the knob bounds: it used to cap the resulting
    /// pixel value and now caps the accumulated error, moving the useful
    /// range from around `0.1` to around `1.0`. A stale value still parses
    /// and still renders, just flat, so honouring it silently would be worse
    /// than ignoring it. It is kept only so
    /// [`AppConfig::deprecation_warnings`] can name the line to edit.
    #[serde(default, rename = "error_clamp")]
    pub deprecated_error_clamp: Option<f32>,
}

impl DitherTuningValues {
    /// Merge: self takes priority, other fills gaps.
    pub fn or(&self, other: &DitherTuningValues) -> DitherTuningValues {
        DitherTuningValues {
            max_error: self.max_error.or(other.max_error),
            noise_scale: self.noise_scale.or(other.noise_scale),
            chroma_clamp: self.chroma_clamp.or(other.chroma_clamp),
            strength: self.strength.or(other.strength),
            gamut: self.gamut.or(&other.gamut),
            deprecated_error_clamp: self.deprecated_error_clamp.or(other.deprecated_error_clamp),
        }
    }

    /// Returns true if all fields that are actually applied are None.
    ///
    /// `deprecated_error_clamp` is deliberately excluded: a block holding
    /// only the ignored key configures nothing.
    pub fn is_empty(&self) -> bool {
        self.max_error.is_none()
            && self.noise_scale.is_none()
            && self.chroma_clamp.is_none()
            && self.strength.is_none()
            && self.gamut.is_empty()
    }
}

/// An `f32` as the `f64` with the same shortest decimal, so `0.8` is written
/// and reported as `0.8` and not `0.800000011920929`. The one place f32
/// representation noise is removed (plan ruling PF1).
pub fn tidy(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

/// `serialize_with` for an `Option<f32>` field: `None` stays `null`, `Some`
/// goes through [`tidy`].
pub fn serialize_tidy_opt<S: serde::Serializer>(v: &Option<f32>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => s.serialize_some(&tidy(*v)),
        None => s.serialize_none(),
    }
}

/// The settings that take part in the resolution cascade.
///
/// Used for the `final:` block and as the shape the resolver works in. One
/// struct and one resolver is what stops a newly added setting from quietly
/// acquiring a precedence order of its own, which is how `dither` came to be
/// inverted relative to its four neighbours.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CascadeValues {
    pub colors: Option<String>,
    pub colors_actual: Option<String>,
    pub dither: Option<String>,
    pub max_error: Option<f32>,
    pub noise_scale: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub strength: Option<f32>,
    pub refresh: Option<u32>,
    #[serde(default)]
    pub gamut: GamutTuningValues,
}

impl CascadeValues {
    /// Merge: `self` takes priority, `other` fills the gaps, field by field.
    pub fn or(&self, other: &CascadeValues) -> CascadeValues {
        CascadeValues {
            colors: self.colors.clone().or_else(|| other.colors.clone()),
            colors_actual: self
                .colors_actual
                .clone()
                .or_else(|| other.colors_actual.clone()),
            dither: self.dither.clone().or_else(|| other.dither.clone()),
            max_error: self.max_error.or(other.max_error),
            noise_scale: self.noise_scale.or(other.noise_scale),
            chroma_clamp: self.chroma_clamp.or(other.chroma_clamp),
            strength: self.strength.or(other.strength),
            refresh: self.refresh.or(other.refresh),
            gamut: self.gamut.or(&other.gamut),
        }
    }
}

impl DeviceConfig {
    /// The device's base rung as a `CascadeValues`.
    pub fn base(&self) -> CascadeValues {
        CascadeValues {
            colors: self.colors.clone(),
            colors_actual: self.colors_actual.clone(),
            dither: self.dither.clone(),
            max_error: self.max_error,
            noise_scale: self.noise_scale,
            chroma_clamp: self.chroma_clamp,
            strength: self.strength,
            refresh: self.refresh,
            gamut: self.gamut.clone(),
        }
    }

    /// The device's final rung, or an empty rung when there is none. Every
    /// render path reads it through this, so "no `final:` block" and "a
    /// `final:` block that sets nothing" resolve identically — which they
    /// must, or deleting the last field from the block would change pixels.
    pub fn device_final(&self) -> CascadeValues {
        self.final_rung.clone().unwrap_or_default()
    }
}

/// Panel dither configuration with flat defaults and per-algorithm overrides.
///
/// Deserialized from a YAML map where scalar keys (`max_error`, `noise_scale`,
/// `chroma_clamp`) become `defaults` and map-valued keys become per-algorithm
/// overrides in `algorithms`.
///
/// ```yaml
/// dither:
///   max_error: 0.1         # flat default for all algorithms
///   noise_scale: 5.0
///   floyd-steinberg:          # per-algorithm override
///     max_error: 0.08
///     noise_scale: 4.0
/// ```
#[derive(Debug, Clone, Default)]
pub struct PanelDitherConfig {
    pub defaults: DitherTuningValues,
    pub algorithms: HashMap<String, DitherTuningValues>,
}

impl PanelDitherConfig {
    /// Resolve tuning for a specific algorithm.
    /// Returns per-algorithm values (if present) merged with flat defaults.
    pub fn resolve_for_algorithm(&self, algorithm: Option<&str>) -> DitherTuningValues {
        if let Some(algo) = algorithm {
            let normalized = normalize_algorithm_name(algo);
            if let Some(algo_tuning) = self.algorithms.get(&normalized) {
                return algo_tuning.or(&self.defaults);
            }
        }
        self.defaults.clone()
    }
}

impl<'de> Deserialize<'de> for PanelDitherConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct PanelDitherVisitor;

        impl<'de> Visitor<'de> for PanelDitherVisitor {
            type Value = PanelDitherConfig;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a map with optional scalar tuning keys and per-algorithm sub-maps")
            }

            fn visit_map<M>(self, mut map: M) -> Result<PanelDitherConfig, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut defaults = DitherTuningValues::default();
                let mut algorithms = HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "max_error" => {
                            defaults.max_error = Some(map.next_value()?);
                        }
                        "error_clamp" => {
                            // Removed in 0.18.0; captured, warned about and
                            // ignored. Without this arm it would fall through
                            // to the algorithm-name branch below and fail to
                            // parse as a sub-map.
                            defaults.deprecated_error_clamp = Some(map.next_value()?);
                        }
                        "noise_scale" => {
                            defaults.noise_scale = Some(map.next_value()?);
                        }
                        "chroma_clamp" => {
                            defaults.chroma_clamp = Some(map.next_value()?);
                        }
                        "strength" => {
                            defaults.strength = Some(map.next_value()?);
                        }
                        "gamut" => {
                            defaults.gamut = map.next_value()?;
                        }
                        _ => {
                            // Treat as algorithm name with sub-map of tuning values
                            let tuning: DitherTuningValues = map.next_value()?;
                            let normalized = normalize_algorithm_name(&key);
                            algorithms.insert(normalized, tuning);
                        }
                    }
                }

                Ok(PanelDitherConfig {
                    defaults,
                    algorithms,
                })
            }
        }

        deserializer.deserialize_map(PanelDitherVisitor)
    }
}

/// Canonical dither algorithm names byonk understands.
///
/// The single source of truth for "is this a real algorithm". Every entry must
/// be a fixed point of [`normalize_algorithm_name`] — a test below asserts it,
/// so an alias can never be listed here as if it were canonical.
pub const DITHER_ALGORITHMS: &[&str] = &[
    "floyd-steinberg",
    "atkinson",
    "atkinson-hybrid",
    "jarvis-judice-ninke",
    "sierra",
    "sierra-two-row",
    "sierra-lite",
    "stucki",
    "burkes",
];

/// Normalize dither algorithm name to its canonical form.
///
/// Accepts common aliases and returns the canonical hyphenated name
/// used throughout the system.
pub fn normalize_algorithm_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "atkinson" => "atkinson".to_string(),
        "atkinson-hybrid" | "atkinson_hybrid" | "atkinsonhybrid" => "atkinson-hybrid".to_string(),
        "floyd-steinberg" | "floyd_steinberg" | "floydsteinberg" => "floyd-steinberg".to_string(),
        "jjn" | "jarvis-judice-ninke" | "jarvis_judice_ninke" => "jarvis-judice-ninke".to_string(),
        "sierra" => "sierra".to_string(),
        "sierra-two-row" | "sierra_two_row" | "sierratworow" => "sierra-two-row".to_string(),
        // "sierra-light" is a long-standing misspelling of "sierra-lite" that
        // reached shipped config and the admin API's algorithm list. Kept as
        // an alias so those configs select the algorithm they name instead of
        // falling through to the Atkinson default.
        "sierra-lite" | "sierra_lite" | "sierralite" | "sierra-light" | "sierra_light"
        | "sierralight" => "sierra-lite".to_string(),
        "stucki" => "stucki".to_string(),
        "burkes" => "burkes".to_string(),
        other => other.to_string(),
    }
}

/// Map an algorithm name to the `eink-dither` variant that implements it.
///
/// Aliases are accepted — the name is normalized first — and an unrecognised
/// name falls back to the default variant, the same way the renderer treats a
/// name it does not know.
///
/// This exists so the dev UI can publish each algorithm's *tuning defaults*
/// (`DitherAlgorithm::defaults()`) instead of carrying its own copy of the
/// numbers, which is what the old dev page did and what let the preview
/// disagree with the engine.
pub fn dither_algorithm_for_name(name: &str) -> eink_dither::DitherAlgorithm {
    use eink_dither::DitherAlgorithm as A;
    match normalize_algorithm_name(name).as_str() {
        "atkinson" => A::Atkinson,
        "atkinson-hybrid" => A::AtkinsonHybrid,
        "floyd-steinberg" => A::FloydSteinberg,
        "jarvis-judice-ninke" => A::JarvisJudiceNinke,
        "sierra" => A::Sierra,
        "sierra-two-row" => A::SierraTwoRow,
        "sierra-lite" => A::SierraLite,
        "stucki" => A::Stucki,
        "burkes" => A::Burkes,
        _ => A::default(),
    }
}

/// Reference to a named screen screen repo.
///
/// `repo == None` means the embedded byonk-builtin screen repo.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ScreenRepoRef {
    #[serde(default)]
    pub repo: Option<String>,
    /// Writable local directory backing this screen repo, as an alternative
    /// to a git `repo`. Mutually exclusive with `repo`.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub pin: Option<String>,
    /// Secret token; redacted in read APIs.
    #[serde(default)]
    pub token: Option<String>,
}

impl ScreenRepoRef {
    /// Reject nonsensical combinations. `handle` is only for the error message.
    pub fn validate(&self, handle: &str) -> Result<(), String> {
        if self.repo.is_some() && self.path.is_some() {
            return Err(format!(
                "screen repo '{handle}': set either 'repo' or 'path', not both"
            ));
        }
        Ok(())
    }
}

/// Application configuration loaded from config.yaml
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Device to screen mappings
    #[serde(default)]
    pub devices: HashMap<String, DeviceConfig>,

    /// Panel profiles with measured display colors
    #[serde(default)]
    pub panels: HashMap<String, PanelConfig>,

    /// Device registration settings
    #[serde(default)]
    pub registration: RegistrationConfig,

    /// Authentication mode advertised to devices via /api/setup
    /// Values: "api_key" (default), "ed25519"
    /// Note: /api/display always accepts both methods regardless of this setting
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,

    /// Admin/management API settings
    #[serde(default)]
    pub admin: AdminConfig,

    /// Screen screen repo registry
    #[serde(default)]
    pub screen_repos: HashMap<String, ScreenRepoRef>,

    /// Interval (seconds) at which mutable (tag/branch) screen repo pins are
    /// re-fetched by a background task. `0` disables periodic refresh.
    #[serde(default)]
    pub screen_repo_refresh_interval: u64,
}

/// Panel profile with official and measured display colors
#[derive(Debug, Deserialize, Clone)]
pub struct PanelConfig {
    /// Human-readable display name (e.g. "TRMNL OG (4-grey)")
    pub name: String,
    /// Exact string match against the firmware `Board` header (or the `Model`
    /// header as a fallback, for devices that report panel identity there).
    #[serde(rename = "match")]
    pub match_pattern: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Official colors (comma-separated hex)
    pub colors: String,
    /// Measured/actual colors (comma-separated hex)
    pub colors_actual: Option<String>,
    /// Per-panel dither tuning defaults
    #[serde(default)]
    pub dither: Option<PanelDitherConfig>,
}

/// Configuration for a specific device
#[derive(Debug, Deserialize, Clone, Default)]
pub struct DeviceConfig {
    /// Which screen to display
    pub screen: String,

    /// Parameters passed to the Lua script
    #[serde(default)]
    pub params: HashMap<String, serde_yaml::Value>,

    /// Optional display color override (comma-separated hex, e.g. "#000000,#FFFFFF,#FF0000")
    pub colors: Option<String>,

    /// Optional measured-colour override for this one display,
    /// comma-separated hex. The **base rung** of `colors_actual`: it outranks
    /// the panel's calibration (which describes the hardware type) and is
    /// outranked by a screen that measures its own, and by `final:`.
    ///
    /// Same validity rule as every other rung: an entry count that does not
    /// match the resolved palette is reported `rejected` and the resolver
    /// walks on down (spec §4.4).
    pub colors_actual: Option<String>,

    /// Optional dither algorithm override (e.g. "atkinson", "floyd-steinberg")
    pub dither: Option<String>,

    /// Optional panel profile name (references panels section)
    pub panel: Option<String>,

    /// Optional cap on accumulated dithering error (e.g. 1.0)
    pub max_error: Option<f32>,

    /// Captured value of the removed `error_clamp` key — never applied.
    /// See [`DitherTuningValues::deprecated_error_clamp`].
    #[serde(default, rename = "error_clamp")]
    pub deprecated_error_clamp: Option<f32>,

    /// Optional blue noise jitter scale override (e.g. 0.6)
    pub noise_scale: Option<f32>,

    /// Optional chroma clamp override for dithering
    pub chroma_clamp: Option<f32>,

    /// Optional dither strength override (0.0 = no diffusion, 1.0 = standard)
    pub strength: Option<f32>,

    /// Optional gamut mapping overrides for continuous-tone regions
    #[serde(default)]
    pub gamut: GamutTuningValues,

    /// Optional per-device refresh override in seconds (0/absent = use Lua/screen default)
    #[serde(default)]
    pub refresh: Option<u32>,

    /// Optional friendly name (mirrored from Home Assistant; absent = identify by MAC)
    #[serde(default)]
    pub name: Option<String>,

    /// Optional e-ink refresh profile, sent to the device in the `/api/display`
    /// response. `default` | `a` | `b`. Absent means `default`.
    ///
    /// What this does depends on which panel is listening, and the two cases
    /// have almost nothing in common:
    ///
    /// - **TRMNL OG / Gen2 / DIY (`bb_epaper` builds)** — indexes a table of
    ///   waveform look-up tables (`dpList[]` in the firmware's `display.cpp`),
    ///   picking a different drive waveform for the 1-bit and 2-bit modes.
    /// - **TRMNL X (`BOARD_X_CLASS`, FastEPD)** — no waveform table exists.
    ///   The value is used once, as a boolean: any non-`default` profile
    ///   upgrades the pre-draw clearing from `CLEAR_FAST` (2 phases) to
    ///   `CLEAR_SLOW` (4 phases of 8 passes) on *every* update instead of only
    ///   every 8th. `a` and `b` are therefore indistinguishable on an X.
    ///
    /// TRMNL's documented remedy for ghosting is to try the profiles, so on an
    /// X expect heavier flashing and less residue, not a different waveform.
    /// Requires firmware >= 1.8.4.
    ///
    /// `c` is deliberately **not** accepted: firmware 1.8.14 still has its
    /// mapping commented out in `parse_response_api_display.cpp`, so a device
    /// silently reads `c` as `default` and turns the extra clearing back off.
    #[serde(default)]
    pub temperature_profile: Option<String>,

    /// Optional "maximum compatibility" flag, sent to the device in the
    /// `/api/display` response.
    ///
    /// Makes the firmware disable fast refresh and force a full-waveform
    /// refresh on every update. The cost is real: full refreshes flash visibly
    /// and take longer, and the firmware also drops 2-bit support. Absent means
    /// absent from the response, leaving the device on its own default.
    ///
    /// **No effect on a TRMNL X.** The flag is parsed and logged there, but
    /// every line that reads it sits inside `#ifdef BB_EPAPER`, which
    /// `BOARD_X_CLASS` does not define (`display.cpp:18`). The X has no partial
    /// refresh path at all — it always calls `fullUpdate()` — so there is
    /// nothing for the flag to switch off. Keep it for the other panels.
    #[serde(default)]
    pub maximum_compatibility: Option<bool>,

    /// Optional minimum size, in bytes, of the PNG served to this device.
    /// Absent means no padding.
    ///
    /// A last lever for TRMNL X ghosting, and an unusual one: the X selects its
    /// grayscale rendering matrix from the **byte size** of the image it
    /// downloaded, not from its content. Above 102 400 bytes it uses a 38-pass
    /// gray table; below, a 9-pass one (`display.cpp:1902`). Byonk's PNGs run
    /// 3–60 KB, so an X has never reached the good table.
    ///
    /// Setting this to `102401` pads the file with an ancillary `tEXt` chunk
    /// that decoders skip, so the image is pixel-identical and only the byte
    /// count changes. The X accepts up to 750 000 bytes.
    ///
    /// Pointless on any other panel: their firmware caps downloads at 90 000
    /// bytes, below the threshold, so the 38-pass table is unreachable there by
    /// construction.
    #[serde(default)]
    pub min_png_bytes: Option<u32>,

    /// The **final rung** — the owner's veto: "whatever the screen says, on
    /// *this* display, no." Nothing outranks it but a live dev-UI override.
    ///
    /// `final` is a reserved word in Rust, hence the rename. Read it through
    /// [`DeviceConfig::device_final`], which saves every call site an
    /// `unwrap_or_default`.
    ///
    /// Strict, unlike the base rung: `CascadeValues` carries
    /// `deny_unknown_fields`, so `final: {screen: …}` fails at startup with
    /// the field named rather than being silently dropped (spec §3).
    #[serde(rename = "final", default)]
    pub final_rung: Option<CascadeValues>,
}

/// Admin/management API settings.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AdminConfig {
    /// Bearer token gating `/api/admin/*`. If unset (and `BYONK_ADMIN_TOKEN` is
    /// unset), the admin API is disabled (returns 404).
    #[serde(default)]
    pub token: Option<String>,
    /// Dev UI form login. Both required; set by env or add-on options.
    #[serde(default)]
    pub dev_username: Option<String>,
    #[serde(default)]
    pub dev_password: Option<String>,
}

/// Device registration settings
#[derive(Debug, Deserialize, Clone)]
pub struct RegistrationConfig {
    /// Whether device registration is required
    ///
    /// When enabled, devices not found in config.devices (by MAC or code)
    /// will display the registration screen with their registration code shown.
    /// Default: true
    #[serde(default = "default_registration_enabled")]
    pub enabled: bool,
}

fn default_registration_enabled() -> bool {
    true
}

fn default_auth_mode() -> String {
    "api_key".to_string()
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl AppConfig {
    /// Load configuration from AssetLoader (embedded or external).
    ///
    /// Returns an error if the config file cannot be read or contains
    /// invalid YAML. The error message includes the line and column
    /// of the first problem found.
    pub fn load_from_assets(loader: &AssetLoader) -> anyhow::Result<Self> {
        let content = loader
            .read_config_string()
            .map_err(|e| anyhow::anyhow!("Failed to read config: {e}"))?;

        let config: Self = serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Invalid config.yaml: {e}"))?;

        config.validate().map_err(|e| anyhow::anyhow!(e))?;

        for warning in config.deprecation_warnings() {
            tracing::warn!("{warning}");
        }

        tracing::info!(devices = config.devices.len(), "Loaded configuration");

        Ok(config)
    }

    /// Every removed config key still present, with the path to edit.
    ///
    /// Returns one message per site. `load_from_assets` logs these at WARN;
    /// they are not errors, because the config is still valid — the key is
    /// simply ignored.
    pub fn deprecation_warnings(&self) -> Vec<String> {
        const RENAME_NOTE: &str = "`error_clamp` was removed in 0.18.0 and is IGNORED. \
             It bounded the resulting pixel value; the replacement `max_error` bounds the \
             accumulated error, so the useful range moved from around 0.1 to around 1.0. \
             Delete the key to take the default, or set `max_error` if you have retuned it.";

        let mut warnings = Vec::new();
        let mut report = |path: String, value: f32| {
            warnings.push(format!("{path}: {value} — {RENAME_NOTE}"));
        };

        let mut panels: Vec<_> = self.panels.iter().collect();
        panels.sort_by_key(|(name, _)| *name);
        for (panel_name, panel) in panels {
            let Some(dither) = &panel.dither else {
                continue;
            };
            if let Some(v) = dither.defaults.deprecated_error_clamp {
                report(format!("panels.{panel_name}.dither.error_clamp"), v);
            }
            let mut algorithms: Vec<_> = dither.algorithms.iter().collect();
            algorithms.sort_by_key(|(name, _)| *name);
            for (algorithm, tuning) in algorithms {
                if let Some(v) = tuning.deprecated_error_clamp {
                    report(
                        format!("panels.{panel_name}.dither.{algorithm}.error_clamp"),
                        v,
                    );
                }
            }
        }

        let mut devices: Vec<_> = self.devices.iter().collect();
        devices.sort_by_key(|(key, _)| *key);
        for (device_key, device) in devices {
            if let Some(v) = device.deprecated_error_clamp {
                report(format!("devices.{device_key}.error_clamp"), v);
            }
        }

        warnings
    }

    /// Validate cross-field invariants not expressible via serde alone.
    ///
    /// Currently checks that no `screen_repos` entry sets both `repo` and
    /// `path` (they are mutually exclusive: one is a git remote, the other a
    /// writable local directory).
    pub fn validate(&self) -> Result<(), String> {
        for (handle, entry) in &self.screen_repos {
            entry.validate(handle)?;
        }
        Ok(())
    }

    /// Check if a device is registered (by MAC or by registration code)
    ///
    /// This only checks whether a device config entry exists. Screen resolution
    /// happens separately via the screen repo loader (`handle/path` refs), so a
    /// registered device does not require any embedded screen definition.
    pub fn is_device_registered(&self, mac: &str, code: Option<&str>) -> bool {
        // Check by MAC first
        if self.get_device_config(mac).is_some() {
            return true;
        }
        // Check by registration code
        if let Some(code) = code {
            if self.get_device_config_for_code(code).is_some() {
                return true;
            }
        }
        false
    }

    /// Get device config by MAC address (without requiring screen to exist in config)
    pub fn get_device_config(&self, device_mac: &str) -> Option<&DeviceConfig> {
        self.devices.get(device_mac).or_else(|| {
            let normalized = device_mac.to_uppercase();
            if normalized != device_mac {
                self.devices.get(&normalized)
            } else {
                None
            }
        })
    }

    /// Get device config by registration code (without requiring screen to exist in config)
    ///
    /// Accepts both formats: `ABCDEFGHJK` or `ABCDE-FGHJK` (hyphenated)
    pub fn get_device_config_for_code(&self, code: &str) -> Option<&DeviceConfig> {
        let normalized = code.to_uppercase().replace('-', "");
        if normalized.len() == 10 {
            let hyphenated = format!("{}-{}", &normalized[..5], &normalized[5..]);
            if let Some(dc) = self.devices.get(&hyphenated) {
                return Some(dc);
            }
        }
        self.devices.get(&normalized)
    }

    /// Resolve the `config.devices` key an existing entry is stored under,
    /// given identifiers a device might be looked up by: its MAC
    /// (case-insensitive) and/or its registration code. Mirrors the lookup
    /// order `get_device_config` / `get_device_config_for_code` use, but
    /// returns the key the entry actually lives under instead of the entry
    /// itself — callers that need to mutate the entry in place (rather than
    /// risk creating a shadowing duplicate under a different key) need the
    /// key, not just confirmation that *some* entry resolves.
    pub fn resolve_device_key(&self, mac: &str, code: Option<&str>) -> Option<String> {
        if self.devices.contains_key(mac) {
            return Some(mac.to_string());
        }
        let upper = mac.to_uppercase();
        if upper != mac && self.devices.contains_key(&upper) {
            return Some(upper);
        }
        if let Some(code) = code {
            let normalized = code.to_uppercase().replace('-', "");
            if normalized.len() == 10 {
                let hyphenated = format!("{}-{}", &normalized[..5], &normalized[5..]);
                if self.devices.contains_key(&hyphenated) {
                    return Some(hyphenated);
                }
            }
            if self.devices.contains_key(&normalized) {
                return Some(normalized);
            }
        }
        None
    }

    /// Get a panel config by name
    pub fn get_panel(&self, name: &str) -> Option<&PanelConfig> {
        self.panels.get(name)
    }

    /// Find a panel by matching its match_pattern against a board string
    pub fn find_panel_for_board(&self, board: &str) -> Option<(&str, &PanelConfig)> {
        self.panels
            .iter()
            .find(|(_, panel)| {
                panel
                    .match_pattern
                    .as_deref()
                    .is_some_and(|pat| pat == board)
            })
            .map(|(name, panel)| (name.as_str(), panel))
    }

    /// Screen ref of the reserved DEFAULT device, if one is configured.
    ///
    /// This is the fallback for any device without its own screen mapping.
    pub fn default_device_screen(&self) -> Option<&str> {
        self.devices
            .get(RESERVED_DEFAULT_KEY)
            .map(|d| d.screen.as_str())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            devices: HashMap::new(),
            panels: HashMap::new(),
            registration: RegistrationConfig::default(),
            auth_mode: default_auth_mode(),
            admin: AdminConfig::default(),
            screen_repos: HashMap::new(),
            screen_repo_refresh_interval: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_token_parses_and_defaults() {
        let yaml = "admin:\n  token: secret123\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.admin.token.as_deref(), Some("secret123"));

        let cfg2: AppConfig = serde_yaml::from_str("registration:\n  enabled: true\n").unwrap();
        assert_eq!(cfg2.admin.token, None);
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();

        assert!(config.devices.is_empty());
        assert!(config.screen_repos.is_empty());
    }

    #[test]
    fn test_package_refresh_interval_defaults_zero() {
        let c: AppConfig = serde_yaml::from_str("auth_mode: api_key\n").unwrap();
        assert_eq!(c.screen_repo_refresh_interval, 0);
    }

    #[test]
    fn test_deserialize_config() {
        let yaml = r#"
devices:
  "AA:BB:CC:DD:EE:FF":
    screen: examples/hello
    params:
      name: "Test User"
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();

        assert!(config.devices.contains_key("AA:BB:CC:DD:EE:FF"));

        let device = config.devices.get("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(device.screen, "examples/hello");
    }

    #[test]
    fn test_registration_config_default() {
        let reg = RegistrationConfig::default();
        assert!(reg.enabled); // Registration is enabled by default
    }

    #[test]
    fn test_deserialize_config_with_registration() {
        let yaml = r#"
registration:
  enabled: true
"#;

        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.registration.enabled);
    }

    #[test]
    fn test_is_device_registered() {
        let mut config = AppConfig::default();

        // Register by MAC
        config.devices.insert(
            "AA:BB:CC:DD:EE:FF".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                ..Default::default()
            },
        );

        // Register by code
        config.devices.insert(
            "XYZABCDEFG".to_string(),
            DeviceConfig {
                screen: "test".to_string(),
                ..Default::default()
            },
        );

        // Should find by MAC
        assert!(config.is_device_registered("AA:BB:CC:DD:EE:FF", None));
        // Should find by code
        assert!(config.is_device_registered("00:00:00:00:00:00", Some("XYZABCDEFG")));
        // Should not find unknown
        assert!(!config.is_device_registered("00:00:00:00:00:00", Some("UNKNOWNCODE")));
    }

    /// `DITHER_ALGORITHMS` is the list the admin API advertises and the device
    /// write path validates against. Every entry must be canonical — i.e. a
    /// fixed point of `normalize_algorithm_name`. Listing an alias here would
    /// let a caller store a name that the renderer then fails to match, and
    /// `svg_to_png`'s `_ =>` arm would silently render Atkinson instead.
    #[test]
    fn test_every_advertised_algorithm_is_canonical() {
        for name in DITHER_ALGORITHMS {
            assert_eq!(
                &normalize_algorithm_name(name),
                name,
                "`{name}` is advertised but is not its own canonical form"
            );
        }
    }

    /// Every advertised algorithm must map to a variant of its own. A missing
    /// arm in `dither_algorithm_for_name` falls through to the Atkinson
    /// default, which would publish Atkinson's tuning defaults under another
    /// algorithm's name — exactly the preview-lies-about-the-engine failure
    /// that serving the defaults from the engine is meant to end.
    #[test]
    fn test_every_advertised_algorithm_maps_to_a_distinct_variant() {
        let mut seen: Vec<(&str, eink_dither::DitherAlgorithm)> = Vec::new();
        for name in DITHER_ALGORITHMS {
            let algo = dither_algorithm_for_name(name);
            if let Some((other, _)) = seen.iter().find(|(_, a)| *a == algo) {
                panic!("`{name}` and `{other}` both map to {algo:?}");
            }
            seen.push((name, algo));
        }
        assert_eq!(seen.len(), DITHER_ALGORITHMS.len());
    }

    /// `sierra-light` is a misspelling that reached shipped config and the
    /// admin API's advertised algorithm list. The renderer only matches
    /// canonical names and otherwise falls back to Atkinson silently, so
    /// before this alias existed a device configured for `sierra-light` was
    /// quietly rendered with Atkinson AND lost its per-algorithm panel
    /// tuning, with nothing in the output to say so.
    #[test]
    fn test_sierra_light_misspelling_resolves_to_sierra_lite() {
        for name in [
            "sierra-light",
            "sierra_light",
            "SierraLight",
            "SIERRA-LIGHT",
        ] {
            assert_eq!(
                normalize_algorithm_name(name),
                "sierra-lite",
                "`{name}` must canonicalize to sierra-lite, not fall through"
            );
        }
        // The correct spellings keep working.
        assert_eq!(normalize_algorithm_name("sierra-lite"), "sierra-lite");
        assert_eq!(normalize_algorithm_name("sierralite"), "sierra-lite");
        // And it must not swallow the other Sierra variants.
        assert_eq!(normalize_algorithm_name("sierra"), "sierra");
        assert_eq!(normalize_algorithm_name("sierra-two-row"), "sierra-two-row");
    }

    /// Per-algorithm panel tuning is keyed by canonical name, so a config
    /// written with the misspelling must still find its tuning block.
    #[test]
    fn test_panel_tuning_found_via_sierra_light_alias() {
        let yaml = r#"
defaults:
  max_error: 0.5
sierra-lite:
  max_error: 0.11
  noise_scale: 5
"#;
        let cfg: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        let tuning = cfg.resolve_for_algorithm(Some("sierra-light"));
        assert_eq!(
            tuning.max_error,
            Some(0.11),
            "misspelled algorithm must still resolve its per-algorithm tuning"
        );
        assert_eq!(tuning.noise_scale, Some(5.0));
    }

    // ------------------------------------------------------------------
    // `error_clamp` -> `max_error` deprecation (0.18.0 changed the meaning)
    // ------------------------------------------------------------------

    /// A pre-0.18.0 `error_clamp` must be captured and IGNORED, never applied.
    ///
    /// If this breaks, it means: a stale value is reaching the ditherer. The
    /// knob used to bound the resulting pixel value and now bounds the error,
    /// so the useful range moved from ~0.1 to ~1.0. A value like `0.11` still
    /// parses and still renders — flat. Silently honouring it under the new
    /// semantics is the failure this guards against.
    #[test]
    fn test_deprecated_error_clamp_is_ignored_in_panel_defaults() {
        let yaml = r#"
error_clamp: 0.11
noise_scale: 5.0
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.defaults.max_error, None,
            "the deprecated key must not set max_error"
        );
        assert_eq!(
            config.defaults.deprecated_error_clamp,
            Some(0.11),
            "the deprecated key must still be captured so it can be warned about"
        );
        assert_eq!(config.defaults.noise_scale, Some(5.0));
        assert!(
            config.algorithms.is_empty(),
            "error_clamp must not fall through to the algorithm-name branch"
        );
    }

    /// The same, inside a per-algorithm sub-map — the path the E1004's stale
    /// `sierra-light` block actually takes.
    #[test]
    fn test_deprecated_error_clamp_is_ignored_in_per_algorithm_block() {
        let yaml = r#"
sierra-lite:
  error_clamp: 0.11
  noise_scale: 5
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        let tuning = config.resolve_for_algorithm(Some("sierra-lite"));
        assert_eq!(tuning.max_error, None);
        assert_eq!(tuning.deprecated_error_clamp, Some(0.11));
        assert_eq!(tuning.noise_scale, Some(5.0));
    }

    /// A deprecated key must not make a block count as tuning.
    #[test]
    fn test_deprecated_error_clamp_alone_is_empty_tuning() {
        assert!(
            DitherTuningValues {
                deprecated_error_clamp: Some(0.11),
                ..Default::default()
            }
            .is_empty(),
            "a block holding only the ignored key sets nothing, so it is empty"
        );
    }

    /// Every deprecated key in the loaded config must be reported with the
    /// path the user has to edit.
    #[test]
    fn test_deprecation_warnings_name_the_config_path() {
        let yaml = r##"
panels:
  reterminal_e1004:
    name: reTerminal E1004
    colors: "#000000,#FFFFFF"
    dither:
      error_clamp: 0.2
      sierra-lite:
        error_clamp: 0.11
devices:
  "44:1B:F6:83:93:38":
    screen: local/clock
    error_clamp: 0.5
"##;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.deprecation_warnings();
        assert_eq!(
            warnings.len(),
            3,
            "one warning per site, got: {warnings:#?}"
        );
        for path in [
            "panels.reterminal_e1004.dither.error_clamp",
            "panels.reterminal_e1004.dither.sierra-lite.error_clamp",
            "devices.44:1B:F6:83:93:38.error_clamp",
        ] {
            assert!(
                warnings.iter().any(|w| w.contains(path)),
                "no warning names {path}, got: {warnings:#?}"
            );
        }
        assert!(
            warnings.iter().all(|w| w.contains("max_error")),
            "every warning must name the replacement, got: {warnings:#?}"
        );
    }

    /// A clean config produces no noise.
    #[test]
    fn test_no_deprecation_warnings_for_current_keys() {
        let yaml = r##"
panels:
  p:
    name: P
    colors: "#000000,#FFFFFF"
    dither:
      max_error: 1.0
      sierra-lite:
        max_error: 1.0
devices:
  "AA:BB:CC:DD:EE:FF":
    screen: local/clock
    max_error: 1.0
"##;
        let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.deprecation_warnings().is_empty());
    }

    #[test]
    fn test_dither_tuning_values_or() {
        let a = DitherTuningValues {
            deprecated_error_clamp: None,
            max_error: Some(0.1),
            noise_scale: None,
            chroma_clamp: Some(2.0),
            strength: None,
            gamut: Default::default(),
        };
        let b = DitherTuningValues {
            deprecated_error_clamp: None,
            max_error: Some(0.2),
            noise_scale: Some(5.0),
            chroma_clamp: None,
            strength: Some(0.8),
            gamut: Default::default(),
        };
        let merged = a.or(&b);
        assert_eq!(merged.max_error, Some(0.1)); // a wins
        assert_eq!(merged.noise_scale, Some(5.0)); // b fills gap
        assert_eq!(merged.chroma_clamp, Some(2.0)); // a wins
        assert_eq!(merged.strength, Some(0.8)); // b fills gap
    }

    #[test]
    fn test_dither_tuning_values_is_empty() {
        assert!(DitherTuningValues::default().is_empty());
        assert!(!DitherTuningValues {
            deprecated_error_clamp: None,
            max_error: Some(0.1),
            ..Default::default()
        }
        .is_empty());
    }

    #[test]
    fn test_panel_dither_config_flat_only() {
        let yaml = r#"
max_error: 0.1
noise_scale: 5.0
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.defaults.max_error, Some(0.1));
        assert_eq!(config.defaults.noise_scale, Some(5.0));
        assert_eq!(config.defaults.chroma_clamp, None);
        assert!(config.algorithms.is_empty());
    }

    #[test]
    fn test_panel_dither_config_per_algorithm_only() {
        let yaml = r#"
floyd-steinberg:
  max_error: 0.08
  noise_scale: 4.0
atkinson:
  max_error: 0.12
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.defaults.is_empty());
        assert_eq!(config.algorithms.len(), 2);
        let fs = config.algorithms.get("floyd-steinberg").unwrap();
        assert_eq!(fs.max_error, Some(0.08));
        assert_eq!(fs.noise_scale, Some(4.0));
        let atk = config.algorithms.get("atkinson").unwrap();
        assert_eq!(atk.max_error, Some(0.12));
    }

    #[test]
    fn test_panel_dither_config_mixed() {
        let yaml = r#"
max_error: 0.1
noise_scale: 5.0
floyd-steinberg:
  max_error: 0.08
  noise_scale: 4.0
atkinson:
  max_error: 0.12
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.defaults.max_error, Some(0.1));
        assert_eq!(config.defaults.noise_scale, Some(5.0));
        assert_eq!(config.algorithms.len(), 2);
    }

    #[test]
    fn test_panel_dither_config_resolve_for_algorithm() {
        let yaml = r#"
max_error: 0.1
noise_scale: 5.0
floyd-steinberg:
  max_error: 0.08
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();

        // Algorithm match: per-algo max_error, default noise_scale
        let resolved = config.resolve_for_algorithm(Some("floyd-steinberg"));
        assert_eq!(resolved.max_error, Some(0.08));
        assert_eq!(resolved.noise_scale, Some(5.0)); // from defaults

        // Algorithm miss: falls back to defaults
        let resolved = config.resolve_for_algorithm(Some("sierra"));
        assert_eq!(resolved.max_error, Some(0.1));
        assert_eq!(resolved.noise_scale, Some(5.0));

        // None algorithm: falls back to defaults
        let resolved = config.resolve_for_algorithm(None);
        assert_eq!(resolved.max_error, Some(0.1));
    }

    #[test]
    fn test_normalize_algorithm_name() {
        // Canonical names
        assert_eq!(normalize_algorithm_name("atkinson"), "atkinson");
        assert_eq!(
            normalize_algorithm_name("atkinson-hybrid"),
            "atkinson-hybrid"
        );
        assert_eq!(
            normalize_algorithm_name("floyd-steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(
            normalize_algorithm_name("jarvis-judice-ninke"),
            "jarvis-judice-ninke"
        );
        assert_eq!(normalize_algorithm_name("sierra"), "sierra");
        assert_eq!(normalize_algorithm_name("sierra-two-row"), "sierra-two-row");
        assert_eq!(normalize_algorithm_name("sierra-lite"), "sierra-lite");
        assert_eq!(normalize_algorithm_name("stucki"), "stucki");
        assert_eq!(normalize_algorithm_name("burkes"), "burkes");

        // Aliases
        assert_eq!(normalize_algorithm_name("jjn"), "jarvis-judice-ninke");

        // Case insensitive
        assert_eq!(
            normalize_algorithm_name("Floyd-Steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(normalize_algorithm_name("ATKINSON"), "atkinson");

        // Underscore variants
        assert_eq!(
            normalize_algorithm_name("floyd_steinberg"),
            "floyd-steinberg"
        );
        assert_eq!(
            normalize_algorithm_name("atkinson_hybrid"),
            "atkinson-hybrid"
        );
        assert_eq!(normalize_algorithm_name("sierra_two_row"), "sierra-two-row");
    }

    #[test]
    fn test_panel_dither_config_normalizes_aliases() {
        let yaml = r#"
atkinson:
  max_error: 0.12
jjn:
  max_error: 0.05
"#;
        let config: PanelDitherConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.algorithms.contains_key("atkinson"));
        assert!(config.algorithms.contains_key("jarvis-judice-ninke"));

        let resolved = config.resolve_for_algorithm(Some("atkinson"));
        assert_eq!(resolved.max_error, Some(0.12));

        let resolved = config.resolve_for_algorithm(Some("jarvis-judice-ninke"));
        assert_eq!(resolved.max_error, Some(0.05));
    }

    #[test]
    fn test_panel_config_with_dither() {
        let yaml = r##"
name: "Test Panel"
colors: "#000000,#FFFFFF"
dither:
  max_error: 0.1
  floyd-steinberg:
    max_error: 0.08
"##;
        let config: PanelConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.dither.is_some());
        let dither = config.dither.unwrap();
        assert_eq!(dither.defaults.max_error, Some(0.1));
        assert!(dither.algorithms.contains_key("floyd-steinberg"));
    }

    #[test]
    fn test_panel_config_without_dither() {
        let yaml = r##"
name: "Test Panel"
colors: "#000000,#FFFFFF"
"##;
        let config: PanelConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.dither.is_none());
    }

    #[test]
    fn test_packages_registry_parses() {
        let yaml = "screen_repos:\n  byonk-builtin: {}\n  weather:\n    repo: github.com/acme/screens\n    pin: v1.4.0\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(cfg.screen_repos.contains_key("byonk-builtin"));
        assert_eq!(cfg.screen_repos["weather"].pin.as_deref(), Some("v1.4.0"));
        assert!(cfg.screen_repos["byonk-builtin"].repo.is_none());
    }

    #[test]
    fn screen_repo_path_variant_parses() {
        let yaml = "screen_repos:\n  local:\n    path: /config/screens\n";
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            cfg.screen_repos["local"].path.as_deref(),
            Some("/config/screens")
        );
    }

    #[test]
    fn screen_repo_rejects_repo_and_path_together() {
        let r = ScreenRepoRef {
            repo: Some("github.com/a/b".into()),
            path: Some("/x".into()),
            pin: None,
            token: None,
        };
        assert!(r.validate("weather").is_err());
    }

    #[test]
    fn test_default_device_screen_accessor() {
        let mut config = AppConfig::default();
        assert_eq!(config.default_device_screen(), None);

        config.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig {
                screen: "byonk-builtin/default".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            config.default_device_screen(),
            Some("byonk-builtin/default")
        );
    }

    #[test]
    fn gamut_values_resolve_against_the_crate_defaults() {
        let defaults = eink_dither::GamutOptions::default();
        assert_eq!(GamutTuningValues::default().resolve(), defaults);

        let partial = GamutTuningValues {
            knee: Some(0.4),
            ..Default::default()
        };
        let resolved = partial.resolve();
        assert_eq!(resolved.knee, 0.4);
        assert_eq!(resolved.amount, defaults.amount);
        assert_eq!(resolved.max_compression, defaults.max_compression);
    }

    #[test]
    fn gamut_values_merge_with_self_winning() {
        let hi = GamutTuningValues {
            knee: Some(0.4),
            ..Default::default()
        };
        let lo = GamutTuningValues {
            knee: Some(0.9),
            amount: Some(0.5),
            max_compression: None,
        };
        let merged = hi.or(&lo);
        assert_eq!(merged.knee, Some(0.4), "self must win");
        assert_eq!(merged.amount, Some(0.5), "other must fill the gap");
    }

    #[test]
    fn dither_tuning_carries_gamut_through_the_chain() {
        let script = DitherTuningValues {
            deprecated_error_clamp: None,
            gamut: GamutTuningValues {
                amount: Some(0.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let panel = DitherTuningValues {
            deprecated_error_clamp: None,
            gamut: GamutTuningValues {
                knee: Some(0.55),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = script.or(&panel);
        assert_eq!(merged.gamut.amount, Some(0.0));
        assert_eq!(merged.gamut.knee, Some(0.55));
        assert!(!merged.is_empty());
    }

    #[test]
    fn cascade_values_refuses_unknown_fields() {
        let err = serde_yaml::from_str::<CascadeValues>("screen: local/x\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn device_base_copies_every_cascade_field() {
        let dc: DeviceConfig = serde_yaml::from_str(
            "screen: local/x\ncolors: '#000000,#FFFFFF'\ndither: atkinson\nmax_error: 0.5\nnoise_scale: 2.0\nchroma_clamp: 0.1\nstrength: 0.8\nrefresh: 600\ngamut: { knee: 0.7 }\n",
        )
        .unwrap();
        let b = dc.base();
        assert_eq!(b.colors.as_deref(), Some("#000000,#FFFFFF"));
        assert_eq!(b.dither.as_deref(), Some("atkinson"));
        assert_eq!(
            (b.max_error, b.noise_scale, b.chroma_clamp, b.strength),
            (Some(0.5), Some(2.0), Some(0.1), Some(0.8))
        );
        assert_eq!(b.refresh, Some(600));
        assert_eq!(b.gamut.knee, Some(0.7));
        assert_eq!(b.colors_actual, None);
    }

    #[test]
    fn device_final_rung_parses_from_the_final_key() {
        let yaml = r#"
screen: local/hello
dither: floyd-steinberg
final:
  dither: atkinson
  refresh: 1800
"#;
        let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(dc.dither.as_deref(), Some("floyd-steinberg"), "base rung");
        let f = dc.final_rung.expect("final rung present");
        assert_eq!(f.dither.as_deref(), Some("atkinson"));
        assert_eq!(f.refresh, Some(1800));
        // Untouched fields of the final rung stay None, so the resolver walks
        // past them to a lower rung instead of vetoing with nothing.
        assert_eq!(f.strength, None);
    }

    #[test]
    fn absent_final_key_is_none_not_an_empty_rung() {
        let dc: DeviceConfig = serde_yaml::from_str("screen: local/hello").expect("parses");
        assert!(dc.final_rung.is_none());
        // …and `device_final()` still answers, so callers need no branch.
        assert_eq!(dc.device_final(), CascadeValues::default());
    }

    /// The asymmetry spec §3 asks for: the `final:` block is strict so that a
    /// non-cascade key there is a startup error naming the field, because in
    /// add-on mode that message is the only diagnostic the operator sees.
    #[test]
    fn a_non_cascade_key_in_final_is_an_error_naming_the_field() {
        let yaml = r#"
screen: local/hello
final:
  screen: local/other
"#;
        let err = serde_yaml::from_str::<DeviceConfig>(yaml)
            .expect_err("`final.screen` must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("screen"),
            "the error must name the offending field, got: {msg}"
        );
    }

    /// The base rung keeps today's tolerant behaviour. Making it strict is a
    /// separate, larger decision about every existing config.yaml (spec §3).
    #[test]
    fn an_unknown_key_on_the_base_rung_is_still_tolerated() {
        let yaml = r#"
screen: local/hello
some_future_key: 42
"#;
        serde_yaml::from_str::<DeviceConfig>(yaml).expect("base rung stays tolerant");
    }

    #[test]
    fn device_colors_actual_is_a_base_rung_string() {
        let yaml = r#"
screen: local/hello
colors_actual: '#1C5240,#FFFFFF'
"#;
        let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(dc.colors_actual.as_deref(), Some("#1C5240,#FFFFFF"));
    }

    /// `base()` is how every render path reads the base rung. A field it forgets
    /// is a setting that silently stops working at that rung only.
    #[test]
    fn base_carries_every_cascade_field_including_the_new_ones() {
        let yaml = r#"
screen: local/hello
colors: '#000000,#FFFFFF'
colors_actual: '#1C5240,#FEFEFE'
dither: stucki
max_error: 1.5
noise_scale: 0.6
chroma_clamp: 2.0
strength: 0.9
refresh: 900
gamut:
  knee: 0.8
"#;
        let dc: DeviceConfig = serde_yaml::from_str(yaml).expect("parses");
        let b = dc.base();
        assert_eq!(b.colors.as_deref(), Some("#000000,#FFFFFF"));
        assert_eq!(b.colors_actual.as_deref(), Some("#1C5240,#FEFEFE"));
        assert_eq!(b.dither.as_deref(), Some("stucki"));
        assert_eq!(b.max_error, Some(1.5));
        assert_eq!(b.noise_scale, Some(0.6));
        assert_eq!(b.chroma_clamp, Some(2.0));
        assert_eq!(b.strength, Some(0.9));
        assert_eq!(b.refresh, Some(900));
        assert_eq!(b.gamut.knee, Some(0.8));
    }

    /// The dev render's `draft.panel.dither` arrives as JSON, so the
    /// hand-written deserializer must accept the YAML shape from JSON too.
    #[test]
    fn panel_dither_config_deserializes_from_json() {
        let pdc: PanelDitherConfig =
            serde_json::from_str(r#"{"max_error": 1.0, "floyd-steinberg": {"noise_scale": 0.6}}"#)
                .unwrap();
        assert_eq!(pdc.defaults.max_error, Some(1.0));
        assert_eq!(pdc.algorithms["floyd-steinberg"].noise_scale, Some(0.6));
    }
}
