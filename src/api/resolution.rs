//! The render-setting cascade: rungs, their statuses, and one resolver.
//!
//! See `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
//! §2 and §4.4. Precedence is an argument, not a property of the types, so the
//! same machinery serves today's per-setting orders and the spec's single one.

use crate::api::display;
use crate::models::{
    normalize_algorithm_name, CascadeValues, DitherTuningValues, PanelDitherConfig,
};
use crate::services::content_pipeline::ScriptResult;
use serde::Serialize;
use std::collections::BTreeMap;

/// Response header carrying the [`Resolution`] of a dev render.
pub const RESOLUTION_HEADER: &str = "x-byonk-resolution";

/// A level of the cascade. `Fallback` is reported but never configured: it
/// is byonk's own default or the device's self-report (spec §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    Override,
    DeviceFinal,
    Screen,
    DeviceBase,
    Panel,
    Fallback,
}

impl Rung {
    /// How a person who writes `config.yaml` would name this level, for
    /// messages an operator reads. `Debug` names (`DeviceBase`) are Rust
    /// identifiers, not words anyone configures.
    pub fn label(self) -> &'static str {
        match self {
            Rung::Override => "the dev override",
            Rung::DeviceFinal => "`final:`",
            Rung::Screen => "the screen",
            Rung::DeviceBase => "the device",
            Rung::Panel => "the panel",
            Rung::Fallback => "the firmware header",
        }
    }
}

/// What happened to one rung's value during resolution (spec §4.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RungStatus {
    Winner,
    /// A higher rung carried an accepted value.
    Outranked,
    /// Supplied, but unusable — e.g. a `colors_actual` whose entry count
    /// does not match the palette.
    Rejected {
        reason: String,
    },
    Absent,
}

/// Every value a setting could take, one slot per rung.
#[derive(Debug, Clone)]
pub struct Ladder<T> {
    pub over: Option<T>,
    pub device_final: Option<T>,
    pub screen: Option<T>,
    pub device_base: Option<T>,
    pub panel: Option<T>,
    pub fallback: Option<T>,
}

impl<T> Default for Ladder<T> {
    fn default() -> Self {
        Self {
            over: None,
            device_final: None,
            screen: None,
            device_base: None,
            panel: None,
            fallback: None,
        }
    }
}

impl<T> Ladder<T> {
    pub fn get(&self, rung: Rung) -> Option<&T> {
        match rung {
            Rung::Override => self.over.as_ref(),
            Rung::DeviceFinal => self.device_final.as_ref(),
            Rung::Screen => self.screen.as_ref(),
            Rung::DeviceBase => self.device_base.as_ref(),
            Rung::Panel => self.panel.as_ref(),
            Rung::Fallback => self.fallback.as_ref(),
        }
    }
}

/// The outcome of resolving one setting.
#[derive(Debug, Clone)]
pub struct Resolved<T> {
    pub effective: Option<T>,
    /// The winning rung; `Fallback` when no configured rung won, even if the
    /// fallback slot is empty too.
    pub from: Rung,
    /// Highest first, as passed to [`resolve`].
    pub order: Vec<Rung>,
    pub rungs: Vec<(Rung, Option<T>, RungStatus)>,
}

impl<T> Resolved<T> {
    /// The value the renderer receives: the effective value unless it is the
    /// fallback, which the engine applies on its own (plan ruling 7).
    pub fn configured(&self) -> Option<&T> {
        if self.from == Rung::Fallback {
            None
        } else {
            self.effective.as_ref()
        }
    }

    pub fn report(&self, to_json: impl Fn(&T) -> serde_json::Value) -> SettingReport {
        SettingReport {
            effective: self
                .effective
                .as_ref()
                .map(&to_json)
                .unwrap_or(serde_json::Value::Null),
            from: self.from,
            order: self.order.clone(),
            // An absent rung is left out: `order` names every rung, so a rung
            // missing here is absent. Listing them cost ~1.5 KiB of a header
            // that must fit nginx's 4 KiB default buffer with 16-entry
            // palettes on several rungs.
            rungs: self
                .rungs
                .iter()
                .filter(|(_, _, status)| *status != RungStatus::Absent)
                .map(|(rung, value, status)| {
                    (
                        *rung,
                        RungReport {
                            value: value.as_ref().map(&to_json),
                            status: status.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
}

/// Walk `order` (highest first). The first present value that `accept`
/// allows wins; a present value it refuses before the winner is `Rejected`
/// and the walk continues; everything present below the winner is
/// `Outranked` without being checked.
pub fn resolve<T: Clone>(
    ladder: &Ladder<T>,
    order: &[Rung],
    accept: impl Fn(&T) -> Result<(), String>,
) -> Resolved<T> {
    let mut winner: Option<(Rung, T)> = None;
    let mut rungs = Vec::with_capacity(order.len());
    for &rung in order {
        let value = ladder.get(rung).cloned();
        let status = match (&value, &winner) {
            (None, _) => RungStatus::Absent,
            (Some(_), Some(_)) => RungStatus::Outranked,
            (Some(v), None) => match accept(v) {
                Ok(()) => {
                    winner = Some((rung, v.clone()));
                    RungStatus::Winner
                }
                Err(reason) => RungStatus::Rejected { reason },
            },
        };
        rungs.push((rung, value, status));
    }
    let (from, effective) = match winner {
        Some((rung, v)) => (rung, Some(v)),
        None => (Rung::Fallback, None),
    };
    Resolved {
        effective,
        from,
        order: order.to_vec(),
        rungs,
    }
}

/// The `accept` for settings with no validity rule.
pub fn accept_any<T>(_: &T) -> Result<(), String> {
    Ok(())
}

/// One setting's entry in the [`Resolution`] payload.
#[derive(Debug, Clone, Serialize)]
pub struct SettingReport {
    pub effective: serde_json::Value,
    pub from: Rung,
    pub order: Vec<Rung>,
    pub rungs: BTreeMap<Rung, RungReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RungReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(flatten)]
    pub status: RungStatus,
}

/// Setting name → report. Keys are the spec's field names; the three gamut
/// knobs appear as `gamut.knee`, `gamut.amount`, `gamut.max_compression`
/// because they resolve field by field.
pub type Resolution = BTreeMap<&'static str, SettingReport>;

/// The header form of a [`Resolution`]: one line of JSON, every non-ASCII
/// character escaped, because HTTP header values must be visible ASCII.
pub fn header_value(resolution: &Resolution) -> String {
    let json = serde_json::to_string(resolution).expect("a Resolution always serializes");
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

// One order, for every setting. Spec §2: `override → device final → screen →
// device base → panel → fallback`. The constants stay per-setting rather than
// collapsing into one so that a future setting that genuinely needs a
// different walk has to say so here, in the one file that owns precedence,
// instead of growing its own chain in a handler.

/// Spec §2's order.
pub const ORDER_COLORS: &[Rung] = &[
    Rung::Override,
    Rung::DeviceFinal,
    Rung::Screen,
    Rung::DeviceBase,
    Rung::Panel,
    Rung::Fallback,
];
/// Spec §2's order. Piece 2 put `Screen` on top to match the pre-cascade
/// resolver, which resolved the script's candidate before the caller's
/// override chain. The override rung exists so a dev-UI calibration push can
/// show what it does, so it goes back on top.
pub const ORDER_COLORS_ACTUAL: &[Rung] = ORDER_COLORS;
/// Spec §2's order. **This is the release's one behaviour change**: the
/// pre-cascade resolver put `DeviceBase` above `Screen`, so a bare
/// `devices.<k>.dither` vetoed a screen that set its own. The veto now lives
/// in `final:`, where it can be told apart from a default. Pinned across all
/// five render paths by `tests/render_cascade_convergence_test.rs`.
pub const ORDER_DITHER: &[Rung] = ORDER_COLORS;
/// Spec §2's order.
pub const ORDER_TUNING: &[Rung] = ORDER_COLORS;
/// Spec §2's order. A panel has no `refresh`, so its rung is always absent;
/// it is listed so every setting reports the same six rungs.
pub const ORDER_REFRESH: &[Rung] = ORDER_COLORS;

impl Rung {
    /// The `SRC_*` label this rung contributes to
    /// `RenderParams::measured_source` (plan ruling 2).
    ///
    /// The labels are the ones byonk has always reported, kept because
    /// `measured_source` reaches the MCP `render_screen` tool's diagnostics
    /// and this release promises exactly one behaviour change. `Override`
    /// takes the caller's label: the override rung genuinely has a different
    /// origin per path, and naming that origin is what the diagnostic is for.
    pub fn measured_source(
        self,
        override_source: &'static str,
        fallback_present: bool,
    ) -> &'static str {
        match self {
            Rung::Override => override_source,
            Rung::DeviceFinal | Rung::DeviceBase => display::SRC_DEVICE,
            Rung::Screen => display::SRC_SCRIPT,
            Rung::Panel => display::SRC_PANEL_ACTUAL,
            Rung::Fallback if fallback_present => display::SRC_MEASURED_HEADER,
            Rung::Fallback => display::SRC_NONE,
        }
    }
}

/// One rung's settings, parsed. Colours are RGB because each source parses
/// its own way (a script returns a list, config holds a comma string) and the
/// resolver must compare like with like.
#[derive(Debug, Clone, Default)]
pub struct RungSettings {
    pub colors: Option<Vec<(u8, u8, u8)>>,
    pub colors_actual: Option<Vec<(u8, u8, u8)>>,
    pub dither: Option<String>,
    pub tuning: DitherTuningValues,
    pub refresh: Option<u32>,
}

impl RungSettings {
    pub fn from_cascade(v: &CascadeValues) -> Self {
        Self {
            colors: v.colors.as_deref().map(display::parse_colors_header),
            colors_actual: v.colors_actual.as_deref().map(display::parse_colors_header),
            dither: v.dither.clone(),
            tuning: DitherTuningValues {
                deprecated_error_clamp: None,
                max_error: v.max_error,
                noise_scale: v.noise_scale,
                chroma_clamp: v.chroma_clamp,
                strength: v.strength,
                gamut: v.gamut.clone(),
            },
            refresh: v.refresh,
        }
    }

    /// Build the screen rung from a script's return value.
    ///
    /// The one place all four script-driven render paths share. `refresh` is
    /// left `None` on purpose: three paths resolve it in `content_pipeline`,
    /// where the script result lives; the dev preview opts in with
    /// [`RungSettings::with_refresh`].
    ///
    /// The two colour slots are parsed DIFFERENTLY, and that is deliberate.
    /// `colors` is joined and re-split (`parse_colors_header`), but
    /// `colors_actual` is parsed entry by entry (`parse_measured_color_list`).
    /// Never join-then-parse a measured list: an entry containing a comma
    /// would fracture into extra "valid" colours, inflate the parsed count and
    /// mask a genuine length mismatch (see `parse_measured_color_list`).
    pub fn from_script(script: &ScriptResult) -> Self {
        Self {
            colors: script
                .script_colors
                .as_deref()
                .map(|c| display::parse_colors_header(&c.join(","))),
            colors_actual: script
                .script_colors_actual
                .as_deref()
                .map(display::parse_measured_color_list),
            dither: script
                .script_dither
                .as_deref()
                .map(normalize_algorithm_name),
            tuning: DitherTuningValues {
                deprecated_error_clamp: None,
                max_error: script.script_max_error,
                noise_scale: script.script_noise_scale,
                chroma_clamp: script.script_chroma_clamp,
                strength: script.script_strength,
                gamut: script.script_gamut.clone().unwrap_or_default(),
            },
            refresh: None,
        }
    }

    /// Set the rung's `refresh` (see [`RungSettings::from_script`]).
    pub fn with_refresh(mut self, refresh: Option<u32>) -> Self {
        self.refresh = refresh;
        self
    }
}

/// The panel `byonk render` uses for a device.
///
/// The CLI has no live device report (separate process), so `--board` and
/// `--device` stand in for the Board and Model headers: the same ladder as
/// /api/display -- explicit `panel:`, then Board, then Model.
pub fn cli_panel_for<'a>(
    config: &'a crate::models::AppConfig,
    device_config: Option<&crate::models::DeviceConfig>,
    board: Option<&str>,
    model: &str,
) -> Option<&'a crate::models::config::PanelConfig> {
    use crate::api::display::{device_panel_key, resolve_panel};
    use crate::server::Sighting;

    let key = match device_config {
        Some(dc) => {
            let sighting = Sighting {
                board: board.map(str::to_string),
                model: model.to_string(),
            };
            device_panel_key(config, dc, Some(&sighting))
        }
        None => resolve_panel(config, None, board, model).1,
    };
    key.and_then(|k| config.get_panel(&k))
}

/// The `byonk render` CLI's binding-to-rung mapping (spec §2).
///
/// Lives here rather than in `main.rs` because the binary crate is
/// unreachable from an integration test: the five-paths convergence test
/// (`tests/render_cascade_convergence_test.rs`) has to drive the same
/// function `run_render_command` calls, or the CLI is the one path whose
/// wiring nothing outside its own unit test pins.
///
/// The CLI has no override flags of its own (its only flag that feeds a
/// setting is `--colors`, which becomes `fallback_palette`), so the override
/// rung is empty. The device rungs come from the device config the MAC — or
/// the registration code — selected, through `base()` and `device_final()`;
/// with no device config both are empty. The panel rung is derived from the
/// profile itself, so a caller cannot cross `colors` with `colors_actual`.
pub fn cli_render_inputs_for<'a>(
    device_config: Option<&crate::models::DeviceConfig>,
    panel: Option<&'a crate::models::config::PanelConfig>,
    script: &ScriptResult,
    fallback_palette: Vec<(u8, u8, u8)>,
) -> RenderInputs<'a> {
    use crate::api::display::{parse_colors_header, SRC_RENDER_OPTS};

    RenderInputs {
        over: RungSettings::default(),
        device_final: RungSettings::from_cascade(
            &device_config.map(|d| d.device_final()).unwrap_or_default(),
        ),
        // `refresh` stays `None`: resolved in content_pipeline.
        screen: RungSettings::from_script(script),
        device_base: RungSettings::from_cascade(
            &device_config.map(|d| d.base()).unwrap_or_default(),
        ),
        panel: RungSettings {
            colors: panel.map(|p| parse_colors_header(&p.colors)),
            colors_actual: panel
                .and_then(|p| p.colors_actual.as_deref())
                .map(parse_colors_header),
            ..Default::default()
        },
        panel_dither: panel.and_then(|p| p.dither.as_ref()),
        fallback_palette,
        // Only `/api/display` has a firmware `Measured-Colors` header.
        fallback_colors_actual: None,
        screen_default_refresh: 0,
        override_source: SRC_RENDER_OPTS,
    }
}

/// Everything the cascade needs from one render, whichever path asked for it.
///
/// Renamed from `RenderInputs` in piece 3: four production paths build it
/// now, and a type called "dev" that `/api/display` constructs would be a lie
/// in the name.
pub struct RenderInputs<'a> {
    pub over: RungSettings,
    pub device_final: RungSettings,
    pub screen: RungSettings,
    pub device_base: RungSettings,
    /// Colours only; the panel's algorithm (its `dither.algorithm`) and its
    /// per-algorithm tuning both come from `panel_dither` (spec §4.2).
    pub panel: RungSettings,
    pub panel_dither: Option<&'a PanelDitherConfig>,
    /// The model's default palette — this path's stand-in for the firmware
    /// `Colors` header.
    pub fallback_palette: Vec<(u8, u8, u8)>,
    /// The firmware `Measured-Colors` header, the fallback below the panel
    /// rung for `colors_actual` (spec §2: a hardware self-report is evidence,
    /// not policy, so it is a fallback and not a rung). Only `/api/display`
    /// ever has one; the other four paths pass `None`.
    pub fallback_colors_actual: Option<Vec<(u8, u8, u8)>>,
    /// The screen's `meta.yaml` `refresh`.
    pub screen_default_refresh: u32,
    /// What to call the override rung in `measured_source` — `SRC_DEV_OVERRIDE`
    /// on the device paths, `SRC_RENDER_OPTS` where a `RenderOpts` supplied it.
    pub override_source: &'static str,
}

pub struct RenderResolution {
    pub palette: Vec<(u8, u8, u8)>,
    pub measured_colors: Option<Vec<(u8, u8, u8)>>,
    /// Which rung supplied `measured_colors`, as a `SRC_*` label.
    pub measured_source: &'static str,
    /// Canonical name, or `None` for the engine default.
    pub dither: Option<String>,
    /// Configured winners only — never fallbacks (plan ruling 7).
    pub tuning: DitherTuningValues,
    pub refresh: u32,
    pub report: Resolution,
    /// What this path calls its override rung (`RenderInputs::override_source`),
    /// kept so diagnostics built later name the real origin.
    pub override_source: &'static str,
}

impl RenderResolution {
    /// The shape the renderer and `/api/display`'s response already speak.
    ///
    /// One conversion for all five paths: a path that built its own
    /// `RenderParams` from a `RenderResolution` could drop a field, and the
    /// convergence test would then be comparing two different mistakes.
    pub fn into_params(self) -> display::RenderParams {
        display::RenderParams {
            palette: self.palette,
            measured_colors: self.measured_colors,
            measured_source: self.measured_source,
            dither: self.dither,
            max_error: self.tuning.max_error,
            noise_scale: self.tuning.noise_scale,
            chroma_clamp: self.tuning.chroma_clamp,
            strength: self.tuning.strength,
            gamut: self.tuning.gamut,
            // The measurement's own validity complaints. A disagreement
            // between rungs is NOT one of them (spec §4.3).
            warnings: self
                .report
                .get("colors_actual")
                .map(|r| rejection_warnings(r, self.override_source))
                .unwrap_or_default(),
        }
    }
}

/// The `Rejected` reasons from one setting's report, as the warning lines the
/// device paths log and the authoring paths put in the script log.
///
/// The override rung is named by `override_source`, because its origin differs
/// per path (a dev-UI push vs a caller-supplied `RenderOpts`); the other rungs
/// use `Rung::label()`.
fn rejection_warnings(report: &SettingReport, override_source: &str) -> Vec<String> {
    report
        .rungs
        .iter()
        .filter_map(|(rung, entry)| match &entry.status {
            RungStatus::Rejected { reason } => {
                let layer = match rung {
                    Rung::Override => override_source,
                    other => other.label(),
                };
                Some(format!(
                    "measured colours from {layer} ignored: {reason} (entries that are not \
                     6-digit hex colours are dropped, which also shortens the list)"
                ))
            }
            _ => None,
        })
        .collect()
}

/// The panel rung of `dither`: the panel block's `algorithm` (task U22).
///
/// Read from `panel_dither`, the one panel input every path already passes
/// both resolvers, so no call site can build a panel rung that forgets it.
/// `RungSettings::dither` on the panel rung is never read.
fn panel_algorithm(panel_dither: Option<&PanelDitherConfig>) -> Option<String> {
    panel_dither.and_then(|p| p.algorithm.clone())
}

fn ladder_of<T: Clone>(
    i: &RenderInputs,
    pick: impl Fn(&RungSettings) -> Option<T>,
    panel: Option<T>,
    fallback: Option<T>,
) -> Ladder<T> {
    Ladder {
        over: pick(&i.over),
        device_final: pick(&i.device_final),
        screen: pick(&i.screen),
        device_base: pick(&i.device_base),
        panel,
        fallback,
    }
}

fn hex_json(c: &[(u8, u8, u8)]) -> serde_json::Value {
    serde_json::json!(display::colors_to_hex_strings(c).join(","))
}

pub fn resolve_render(i: &RenderInputs) -> RenderResolution {
    let mut report = Resolution::new();

    // Colours first: the measured colours' validity depends on the palette.
    let colors = resolve(
        &ladder_of(
            i,
            |r| r.colors.clone(),
            i.panel.colors.clone(),
            Some(i.fallback_palette.clone()),
        ),
        ORDER_COLORS,
        accept_any,
    );
    report.insert("colors", colors.report(|c| hex_json(c)));
    let palette = colors
        .effective
        .clone()
        .unwrap_or_else(|| i.fallback_palette.clone());

    let palette_len = palette.len();
    let colors_actual = resolve(
        &ladder_of(
            i,
            |r| r.colors_actual.clone(),
            i.panel.colors_actual.clone(),
            i.fallback_colors_actual.clone(),
        ),
        ORDER_COLORS_ACTUAL,
        |c: &Vec<(u8, u8, u8)>| {
            if c.len() == palette_len {
                Ok(())
            } else {
                Err(format!(
                    "has {} usable entries but the resolved palette has {palette_len}",
                    c.len()
                ))
            }
        },
    );
    report.insert("colors_actual", colors_actual.report(|c| hex_json(c)));

    // The algorithm before its numbers (spec §4.2). Compared canonically, so
    // `Floyd_Steinberg` and `floyd-steinberg` are one value. The panel rung
    // is the panel block's `algorithm` (task U22).
    let dither = resolve(
        &ladder_of(
            i,
            |r| r.dither.as_deref().map(normalize_algorithm_name),
            panel_algorithm(i.panel_dither),
            Some("atkinson".to_string()),
        ),
        ORDER_DITHER,
        accept_any,
    );
    report.insert("dither", dither.report(|d| serde_json::json!(d)));
    let algo = dither
        .effective
        .clone()
        .unwrap_or_else(|| "atkinson".to_string());
    // The panel block is looked up for the CONFIGURED algorithm (the panel's
    // own `algorithm` included): with no rung setting `dither` that is
    // `None`, which selects the panel's flat defaults only. `algo` (the
    // fallback included) only names the engine defaults reported below.
    let panel_tuning = i
        .panel_dither
        .map(|p| p.resolve_for_algorithm(dither.configured().map(String::as_str)))
        .unwrap_or_default();
    let (default_max_error, default_noise_scale) =
        crate::models::config::dither_algorithm_for_name(&algo).defaults();
    let engine = eink_dither::DitherOptions::default();
    let gamut_defaults = eink_dither::GamutOptions::default();

    let mut tuning = DitherTuningValues::default();
    macro_rules! number {
        ($key:literal, $($field:ident).+, $fallback:expr) => {{
            let r = resolve(
                &ladder_of(i, |s| s.tuning.$($field).+, panel_tuning.$($field).+, $fallback),
                ORDER_TUNING,
                accept_any,
            );
            report.insert(
                $key,
                r.report(|v| serde_json::json!(crate::models::config::tidy(*v))),
            );
            tuning.$($field).+ = r.configured().copied();
        }};
    }
    number!("max_error", max_error, Some(default_max_error));
    number!("noise_scale", noise_scale, Some(default_noise_scale));
    // The engine's default is "no damping" (infinity), which JSON cannot
    // carry; it is reported as no fallback value at all.
    number!("chroma_clamp", chroma_clamp, None);
    number!("strength", strength, Some(engine.strength));
    number!("gamut.knee", gamut.knee, Some(gamut_defaults.knee));
    number!("gamut.amount", gamut.amount, Some(gamut_defaults.amount));
    number!(
        "gamut.max_compression",
        gamut.max_compression,
        Some(gamut_defaults.max_compression)
    );

    let refresh = resolve(
        &// `0` means "unset" at every layer of today's refresh chain.
        ladder_of(
            i,
            |r| r.refresh.filter(|&v| v > 0),
            None,
            Some(i.screen_default_refresh),
        ),
        ORDER_REFRESH,
        accept_any,
    );
    report.insert("refresh", refresh.report(|v| serde_json::json!(v)));

    RenderResolution {
        palette,
        measured_colors: colors_actual.effective.clone(),
        measured_source: colors_actual.from.measured_source(
            i.override_source,
            // Only an ACCEPTED measurement may be named as a source: a
            // rejected fallback (wrong entry count) leaves `effective` None.
            colors_actual.effective.is_some(),
        ),
        dither: dither.configured().cloned(),
        tuning,
        refresh: refresh.effective.unwrap_or(i.screen_default_refresh),
        report,
        override_source: i.override_source,
    }
}

/// What the script is told about the device before it runs.
///
/// The same cascade as [`resolve_render`] with the screen rung left empty:
/// what the script is about to return IS that rung, so it cannot be known
/// yet. The order is therefore `override -> device final -> device base ->
/// panel -> fallback` — [`ORDER_COLORS`] walked over a [`Ladder`] whose
/// `screen` slot is `None`. Everything a script reads as `device.*` for the
/// device's own settings comes from here; three call sites used to hand-roll
/// this and all three forgot the `final:` rung at once.
pub struct PreScriptInputs<'a> {
    /// The live dev-UI override. Every field is read by the ladder, but the
    /// call sites currently fill only `dither` and `colors_actual`; the
    /// override's `colors` and tuning (e.g. `dev_tuning_override`) are not
    /// shown to scripts. Populating them WOULD change script-visible output.
    pub over: RungSettings,
    /// The device's `final:` rung — the owner's veto. Every field counts.
    pub device_final: RungSettings,
    /// The device's base rung; every field counts, its measured colours
    /// included (the renderer honours them, so the script is told them).
    pub device_base: RungSettings,
    /// Colours and measured colours; its algorithm and tuning come from
    /// `panel_dither`.
    pub panel: RungSettings,
    pub panel_dither: Option<&'a PanelDitherConfig>,
    pub fallback_palette: Vec<(u8, u8, u8)>,
    /// The firmware `Measured-Colors` header (`/api/display` only).
    pub fallback_colors_actual: Option<Vec<(u8, u8, u8)>>,
}

/// The values for `DeviceContext`'s `colors`, `colors_actual` and `dither_*`.
#[derive(Debug, Clone)]
pub struct PreScriptContext {
    pub palette: Vec<(u8, u8, u8)>,
    pub colors_actual: Option<Vec<(u8, u8, u8)>>,
    /// The rung `colors_actual` came from (`Fallback` when none or the header).
    pub colors_actual_from: Rung,
    /// As configured (not re-normalised), `"atkinson"` when nothing sets one.
    pub algorithm: String,
    /// Winners only; `None` where no rung sets a value.
    pub tuning: DitherTuningValues,
}

/// Resolve what a script is told as `device.*` before it runs.
///
/// This is the render resolution ([`resolve_render`]) with the screen rung
/// empty: the screen rung is what the script is about to return, so it cannot
/// be known beforehand. Order: `override -> device final -> device base ->
/// panel -> fallback`. The panel's per-algorithm tuning is looked up for the
/// algorithm that won.
pub fn resolve_pre_script(i: &PreScriptInputs) -> PreScriptContext {
    // One ladder per setting, screen slot empty by construction.
    fn ladder<T: Clone>(
        i: &PreScriptInputs,
        pick: impl Fn(&RungSettings) -> Option<T>,
        panel: Option<T>,
        fallback: Option<T>,
    ) -> Ladder<T> {
        Ladder {
            over: pick(&i.over),
            device_final: pick(&i.device_final),
            screen: None,
            device_base: pick(&i.device_base),
            panel,
            fallback,
        }
    }

    let palette = resolve(
        &ladder(
            i,
            |r| r.colors.clone(),
            i.panel.colors.clone(),
            Some(i.fallback_palette.clone()),
        ),
        ORDER_COLORS,
        accept_any,
    )
    .effective
    .unwrap_or_else(|| i.fallback_palette.clone());

    // No length rule: the palette the renderer will use is not final yet.
    let actual = resolve(
        &Ladder {
            over: i.over.colors_actual.clone(),
            device_final: i.device_final.colors_actual.clone(),
            screen: None,
            device_base: i.device_base.colors_actual.clone(),
            panel: i.panel.colors_actual.clone(),
            fallback: i.fallback_colors_actual.clone(),
        },
        ORDER_COLORS_ACTUAL,
        accept_any,
    );

    // The algorithm before its numbers: the panel's per-algorithm block is
    // looked up for whichever algorithm WON, so a `final:` dither brings its
    // own tuning with it. Unlike the renderer, the lookup is always for a
    // concrete name — `"atkinson"` when nothing is configured — which is
    // what the script has always been shown.
    let algorithm = resolve(
        &ladder(
            i,
            |r| r.dither.clone(),
            panel_algorithm(i.panel_dither),
            Some("atkinson".to_string()),
        ),
        ORDER_DITHER,
        accept_any,
    )
    .effective
    .unwrap_or_else(|| "atkinson".to_string());
    let panel_tuning = i
        .panel_dither
        .map(|p| p.resolve_for_algorithm(Some(&algorithm)))
        .unwrap_or_default();

    let mut tuning = DitherTuningValues::default();
    macro_rules! number {
        ($($field:ident).+) => {
            tuning.$($field).+ = resolve(
                &ladder(i, |s| s.tuning.$($field).+, panel_tuning.$($field).+, None),
                ORDER_TUNING,
                accept_any,
            )
            .effective;
        };
    }
    number!(max_error);
    number!(noise_scale);
    number!(chroma_clamp);
    number!(strength);
    number!(gamut.knee);
    number!(gamut.amount);
    number!(gamut.max_compression);

    PreScriptContext {
        palette,
        colors_actual_from: actual.from,
        colors_actual: actual.effective,
        algorithm,
        tuning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cli_walks_the_server_ladder_with_board_and_model() {
        let config: crate::models::AppConfig = serde_yaml::from_str(
            "\
panels:
  boardp: {name: B, match: trmnl_x, colors: '#000000,#FFFFFF'}
  modelp: {name: M, match: e1004, colors: '#000000,#FFFFFF'}
  pinned: {name: P, colors: '#000000,#FFFFFF'}
devices:
  office: {screen: local/a}
  fixed: {screen: local/a, panel: pinned}
",
        )
        .unwrap();
        let name = |p: Option<&crate::models::config::PanelConfig>| p.map(|p| p.name.clone());
        let office = config.get_device_config("office");
        // board only
        assert_eq!(
            name(cli_panel_for(&config, office, Some("trmnl_x"), "og")).as_deref(),
            Some("B")
        );
        // model only
        assert_eq!(
            name(cli_panel_for(&config, office, None, "e1004")).as_deref(),
            Some("M")
        );
        // both: the Board wins
        assert_eq!(
            name(cli_panel_for(&config, office, Some("trmnl_x"), "e1004")).as_deref(),
            Some("B")
        );
        // neither matches
        assert!(cli_panel_for(&config, office, Some("nope"), "og").is_none());
        // no device config: still the ladder
        assert_eq!(
            name(cli_panel_for(&config, None, Some("trmnl_x"), "og")).as_deref(),
            Some("B")
        );
        // explicit `panel:` beats a Board that names another panel
        let fixed = config.get_device_config("fixed");
        assert_eq!(
            name(cli_panel_for(&config, fixed, Some("trmnl_x"), "e1004")).as_deref(),
            Some("P")
        );
    }

    fn ladder(
        over: Option<i32>,
        screen: Option<i32>,
        base: Option<i32>,
        fallback: Option<i32>,
    ) -> Ladder<i32> {
        Ladder {
            over,
            screen,
            device_base: base,
            fallback,
            ..Default::default()
        }
    }

    const ORDER: &[Rung] = &[
        Rung::Override,
        Rung::DeviceFinal,
        Rung::Screen,
        Rung::DeviceBase,
        Rung::Panel,
        Rung::Fallback,
    ];

    #[test]
    fn highest_present_rung_wins_and_lower_ones_are_outranked() {
        let r = resolve(&ladder(None, Some(2), Some(3), Some(9)), ORDER, accept_any);
        assert_eq!(r.effective, Some(2));
        assert_eq!(r.from, Rung::Screen);
        let status = |rung| {
            r.rungs
                .iter()
                .find(|(x, _, _)| *x == rung)
                .unwrap()
                .2
                .clone()
        };
        assert_eq!(status(Rung::Override), RungStatus::Absent);
        assert_eq!(status(Rung::Screen), RungStatus::Winner);
        assert_eq!(status(Rung::DeviceBase), RungStatus::Outranked);
        assert_eq!(status(Rung::Fallback), RungStatus::Outranked);
        assert_eq!(r.configured(), Some(&2));
    }

    #[test]
    fn the_order_argument_decides_not_the_field_names() {
        // Today's `dither` puts the device base above the screen.
        let order = &[
            Rung::Override,
            Rung::DeviceFinal,
            Rung::DeviceBase,
            Rung::Screen,
            Rung::Panel,
            Rung::Fallback,
        ];
        let r = resolve(&ladder(None, Some(2), Some(3), None), order, accept_any);
        assert_eq!(r.from, Rung::DeviceBase);
        assert_eq!(r.effective, Some(3));
    }

    #[test]
    fn a_rejected_rung_is_reported_and_the_walk_continues() {
        let r = resolve(&ladder(Some(-1), Some(2), None, None), ORDER, |v: &i32| {
            if *v < 0 {
                Err("negative".to_string())
            } else {
                Ok(())
            }
        });
        assert_eq!(r.from, Rung::Screen);
        assert_eq!(
            r.rungs[0].2,
            RungStatus::Rejected {
                reason: "negative".to_string()
            }
        );
    }

    #[test]
    fn nothing_set_resolves_to_fallback_and_configured_is_none() {
        let r = resolve(&ladder(None, None, None, Some(9)), ORDER, accept_any);
        assert_eq!(r.from, Rung::Fallback);
        assert_eq!(r.effective, Some(9));
        assert_eq!(
            r.configured(),
            None,
            "the renderer must never see a fallback value"
        );

        let empty = resolve(&Ladder::<i32>::default(), ORDER, accept_any);
        assert_eq!(empty.from, Rung::Fallback);
        assert_eq!(empty.effective, None);
    }

    #[test]
    fn report_serializes_in_the_documented_shape() {
        let r = resolve(&ladder(None, Some(2), Some(3), None), ORDER, accept_any);
        let json = serde_json::to_value(r.report(|v| serde_json::json!(v))).unwrap();
        assert_eq!(json["effective"], 2);
        assert_eq!(json["from"], "screen");
        assert_eq!(json["order"][0], "override");
        assert_eq!(
            json["rungs"]["screen"],
            serde_json::json!({"value": 2, "status": "winner"})
        );
        assert_eq!(
            json["rungs"]["device_base"],
            serde_json::json!({"value": 3, "status": "outranked"})
        );
        assert!(
            json["rungs"].get("override").is_none(),
            "an absent rung is left out of the report"
        );
    }

    #[test]
    fn header_value_is_ascii_even_for_non_ascii_input() {
        let r = resolve(
            &Ladder {
                screen: Some("grün".to_string()),
                ..Default::default()
            },
            ORDER,
            accept_any,
        );
        let mut res = Resolution::new();
        res.insert("dither", r.report(|v| serde_json::json!(v)));
        let h = header_value(&res);
        assert!(h.is_ascii(), "{h}");
        assert!(h.contains("\\u00fc"), "{h}");
        let back: serde_json::Value = serde_json::from_str(&h).unwrap();
        assert_eq!(back["dither"]["effective"], "grün");
    }

    use crate::api::display;
    use crate::models::{DitherTuningValues, PanelDitherConfig};
    use std::collections::HashMap;

    fn grey4() -> Vec<(u8, u8, u8)> {
        display::parse_colors_header("#000000,#555555,#AAAAAA,#FFFFFF")
    }
    fn act4() -> Vec<(u8, u8, u8)> {
        display::parse_colors_header("#101010,#505050,#A0A0A0,#E0E0E0")
    }
    fn act6() -> Vec<(u8, u8, u8)> {
        display::parse_colors_header("#101010,#E0E0E0,#B03030,#D0C040,#3040A0,#00994D")
    }

    fn inputs<'a>() -> RenderInputs<'a> {
        RenderInputs {
            over: RungSettings::default(),
            device_final: RungSettings::default(),
            screen: RungSettings::default(),
            device_base: RungSettings::default(),
            panel: RungSettings::default(),
            panel_dither: None,
            fallback_palette: grey4(),
            fallback_colors_actual: None,
            screen_default_refresh: 900,
            override_source: display::SRC_DEV_OVERRIDE,
        }
    }

    #[test]
    fn a_wrong_length_colors_actual_is_rejected_and_a_lower_rung_wins() {
        let mut i = inputs();
        i.over.colors_actual = Some(act6()); // palette is 4 entries
        i.panel.colors_actual = Some(act4());
        let r = resolve_render(&i);
        assert_eq!(r.measured_colors, Some(act4()));
        let rep = &r.report["colors_actual"];
        assert_eq!(rep.from, Rung::Panel);
        assert!(matches!(
            rep.rungs[&Rung::Override].status,
            RungStatus::Rejected { .. }
        ));
    }

    #[test]
    fn panel_tuning_is_looked_up_for_the_winning_algorithm() {
        let mut algorithms = HashMap::new();
        algorithms.insert(
            "floyd-steinberg".to_string(),
            DitherTuningValues {
                strength: Some(0.3),
                ..Default::default()
            },
        );
        let pdc = PanelDitherConfig {
            algorithm: None,
            defaults: DitherTuningValues {
                strength: Some(0.9),
                ..Default::default()
            },
            algorithms,
        };
        let mut i = inputs();
        i.panel_dither = Some(&pdc);
        i.screen.dither = Some("floyd-steinberg".to_string()); // wins: screen beats device base
        i.device_base.dither = Some("atkinson".to_string());
        let r = resolve_render(&i);
        assert_eq!(r.dither.as_deref(), Some("floyd-steinberg"));
        assert_eq!(r.tuning.strength, Some(0.3));
        assert_eq!(r.report["strength"].from, Rung::Panel);
    }

    /// The panel's per-algorithm block applies only to an algorithm a rung
    /// NAMED. With no rung setting `dither`, `resolve_for_algorithm(None)`
    /// returns the flat defaults, even though the fallback algorithm
    /// (atkinson) has a block -- what every render path did before the cascade.
    #[test]
    fn a_panel_algorithm_block_does_not_apply_when_no_rung_sets_dither() {
        let mut algorithms = HashMap::new();
        algorithms.insert(
            "atkinson".to_string(),
            DitherTuningValues {
                strength: Some(0.3),
                ..Default::default()
            },
        );
        let pdc = PanelDitherConfig {
            algorithm: None,
            defaults: DitherTuningValues {
                strength: Some(0.9),
                ..Default::default()
            },
            algorithms,
        };
        let mut i = inputs();
        i.panel_dither = Some(&pdc);
        let r = resolve_render(&i);
        assert_eq!(r.dither, None, "no rung sets dither");
        assert_eq!(
            r.tuning.strength,
            Some(0.9),
            "flat panel default, not the atkinson block"
        );
        assert_eq!(r.report["strength"].from, Rung::Panel);
    }

    /// Task U22: a panel with `dither.algorithm` sets the panel rung of
    /// `dither`, and its per-algorithm block for THAT algorithm applies --
    /// the sibling of the test above, for a panel that names one.
    fn panel_naming(algorithm: &str) -> PanelDitherConfig {
        let mut algorithms = HashMap::new();
        for (name, strength) in [("floyd-steinberg", 0.3), ("stucki", 0.5)] {
            algorithms.insert(
                name.to_string(),
                DitherTuningValues {
                    strength: Some(strength),
                    ..Default::default()
                },
            );
        }
        PanelDitherConfig {
            algorithm: Some(algorithm.to_string()),
            defaults: DitherTuningValues {
                strength: Some(0.9),
                ..Default::default()
            },
            algorithms,
        }
    }

    #[test]
    fn a_panel_algorithm_wins_over_the_fallback_and_brings_its_block() {
        let pdc = panel_naming("floyd-steinberg");
        let mut i = inputs();
        i.panel_dither = Some(&pdc);
        let r = resolve_render(&i);
        assert_eq!(r.dither.as_deref(), Some("floyd-steinberg"));
        let rep = &r.report["dither"];
        assert_eq!(rep.from, Rung::Panel);
        assert_eq!(rep.rungs[&Rung::Panel].status, RungStatus::Winner);
        assert_eq!(rep.rungs[&Rung::Fallback].status, RungStatus::Outranked);
        assert_eq!(
            r.tuning.strength,
            Some(0.3),
            "the panel's own floyd-steinberg block"
        );
    }

    #[test]
    fn a_panel_algorithm_loses_to_every_rung_above_it() {
        let pdc = panel_naming("floyd-steinberg");
        type Set = fn(&mut RenderInputs);
        let cases: [(Rung, Set); 4] = [
            (Rung::DeviceBase, |i| {
                i.device_base.dither = Some("stucki".into())
            }),
            (Rung::Screen, |i| i.screen.dither = Some("stucki".into())),
            (Rung::DeviceFinal, |i| {
                i.device_final.dither = Some("stucki".into())
            }),
            (Rung::Override, |i| i.over.dither = Some("stucki".into())),
        ];
        for (rung, set) in cases {
            let mut i = inputs();
            i.panel_dither = Some(&pdc);
            set(&mut i);
            let r = resolve_render(&i);
            assert_eq!(r.dither.as_deref(), Some("stucki"), "{rung:?}");
            let rep = &r.report["dither"];
            assert_eq!(rep.from, rung);
            assert_eq!(rep.rungs[&Rung::Panel].status, RungStatus::Outranked);
            assert_eq!(
                r.tuning.strength,
                Some(0.5),
                "the winner's block, not the panel algorithm's ({rung:?})"
            );
        }
    }

    #[test]
    fn the_script_is_told_the_panel_algorithm() {
        let pdc = panel_naming("floyd-steinberg");
        let pre = resolve_pre_script(&PreScriptInputs {
            over: RungSettings::default(),
            device_final: RungSettings::default(),
            device_base: RungSettings::default(),
            panel: RungSettings::default(),
            panel_dither: Some(&pdc),
            fallback_palette: grey4(),
            fallback_colors_actual: None,
        });
        assert_eq!(pre.algorithm, "floyd-steinberg");
        assert_eq!(pre.tuning.strength, Some(0.3));
    }

    #[test]
    fn a_float_setting_is_reported_without_f32_noise() {
        let mut i = inputs();
        i.device_base.tuning.strength = Some(0.4);
        i.device_base.tuning.gamut.knee = Some(0.7);
        let r = resolve_render(&i);
        let json = serde_json::to_value(&r.report["strength"]).unwrap();
        assert_eq!(json["effective"], 0.4, "the effective value");
        assert_eq!(json["rungs"]["device_base"]["value"], 0.4, "the rung value");
        let knee = serde_json::to_value(&r.report["gamut.knee"]).unwrap();
        assert_eq!(knee["effective"], 0.7);
    }

    #[test]
    fn fallbacks_are_reported_but_not_handed_to_the_renderer() {
        let r = resolve_render(&inputs());
        assert_eq!(r.dither, None);
        assert_eq!(r.report["dither"].from, Rung::Fallback);
        assert_eq!(r.report["dither"].effective, serde_json::json!("atkinson"));
        assert_eq!(r.tuning, DitherTuningValues::default());
        assert_eq!(r.report["strength"].effective, serde_json::json!(1.0));
        assert_eq!(
            r.palette,
            grey4(),
            "the palette fallback IS used — it is the model default"
        );
        assert_eq!(r.report["refresh"].effective, serde_json::json!(900));
        assert_eq!(r.refresh, 900);
    }

    #[test]
    fn device_final_is_absent_from_the_report_when_the_device_sets_none() {
        let r = resolve_render(&inputs());
        for (name, rep) in &r.report {
            assert!(
                !rep.rungs.contains_key(&Rung::DeviceFinal),
                "{name}: device_final must be absent (left out of the report)"
            );
            assert!(rep.order.contains(&Rung::DeviceFinal), "{name}");
        }
    }

    #[test]
    fn a_zero_refresh_means_unset_on_every_rung() {
        // `content_pipeline::resolve_refresh_rate`: `0` is "unset" at every layer.
        let mut i = inputs();
        i.screen.refresh = Some(0);
        i.device_base.refresh = Some(600);
        let r = resolve_render(&i);
        assert_eq!(r.refresh, 600);
        assert_eq!(r.report["refresh"].from, Rung::DeviceBase);
    }

    use crate::models::config::{CascadeValues, GamutTuningValues};

    /// A distinct value for every cascade setting, keyed by rung number so a
    /// winner names its rung unambiguously. `colors` and `colors_actual` get the
    /// same entry count, or the validity rule would reject the measurement and
    /// mask the ordering this test is about.
    fn rung_values(n: u8) -> CascadeValues {
        let hex = format!("#{0:02X}{0:02X}{0:02X},#FFFFFF", n * 16);
        CascadeValues {
            colors: Some(hex.clone()),
            colors_actual: Some(hex),
            dither: Some(DITHER_BY_RUNG[n as usize].to_string()),
            max_error: Some(n as f32),
            noise_scale: Some(n as f32),
            chroma_clamp: Some(n as f32),
            strength: Some(n as f32),
            refresh: Some(100 * n as u32),
            gamut: GamutTuningValues {
                knee: Some(n as f32),
                amount: Some(n as f32),
                max_compression: Some(n as f32),
            },
        }
    }

    /// Real canonical algorithm names — the resolver normalises and the renderer
    /// matches canonical names only, so a made-up name would silently become
    /// atkinson and the test would pass for the wrong reason.
    const DITHER_BY_RUNG: [&str; 6] = [
        "burkes",              // 0 — unused, keeps the indices readable
        "stucki",              // 1 — panel (never supplied; see below)
        "sierra",              // 2 — device base
        "jarvis-judice-ninke", // 3 — screen
        "atkinson",            // 4 — device final
        "floyd-steinberg",     // 5 — override
    ];
    const OVERRIDE_DITHER: &str = DITHER_BY_RUNG[5];

    fn rung_inputs(rungs: &[Rung]) -> (RungSettings, RungSettings, RungSettings, RungSettings) {
        let pick = |r: Rung, n: u8| {
            if rungs.contains(&r) {
                RungSettings::from_cascade(&rung_values(n))
            } else {
                RungSettings::default()
            }
        };
        let over = pick(Rung::Override, 5);
        (
            over,
            pick(Rung::DeviceFinal, 4),
            pick(Rung::Screen, 3),
            pick(Rung::DeviceBase, 2),
        )
    }

    /// Build a `RenderInputs` in which exactly `rungs` carry values. The panel
    /// rung supplies colours only here; a panel's algorithm and numbers come
    /// from `panel_dither`, which the `panel_algorithm_*` tests cover.
    fn table_inputs(rungs: &[Rung]) -> RenderInputs<'static> {
        let (over, device_final, screen, device_base) = rung_inputs(rungs);
        let panel = if rungs.contains(&Rung::Panel) {
            RungSettings {
                colors: Some(display::parse_colors_header("#111111,#FFFFFF")),
                colors_actual: Some(display::parse_colors_header("#121212,#FEFEFE")),
                ..Default::default()
            }
        } else {
            RungSettings::default()
        };
        RenderInputs {
            over,
            device_final,
            screen,
            device_base,
            panel,
            panel_dither: None,
            fallback_palette: display::parse_colors_header("#000000,#FFFFFF"),
            fallback_colors_actual: None,
            screen_default_refresh: 900,
            override_source: display::SRC_DEV_OVERRIDE,
        }
    }

    fn winner(res: &Resolution, setting: &str) -> Rung {
        res.get(setting)
            .unwrap_or_else(|| panic!("no report for `{setting}`"))
            .from
    }

    /// Every setting resolves through the SAME order. This is the test a newly
    /// added setting with a precedence order of its own fails — which is exactly
    /// how `dither` came to be inverted relative to its four neighbours.
    #[test]
    fn resolution_table_every_setting_walks_the_same_six_rungs() {
        const SETTINGS: [&str; 10] = [
            "colors",
            "colors_actual",
            "dither",
            "max_error",
            "noise_scale",
            "chroma_clamp",
            "strength",
            "gamut.knee",
            "gamut.amount",
            "refresh",
        ];
        // Highest first. Peel one rung off the top at a time; the next must win.
        // The panel and fallback rungs are per-setting (a panel's `dither`
        // lives in `panel_dither`, and a panel has no `refresh`), so the walk
        // stops at DeviceBase; the lower rungs get their own assertions below
        // and in the `a_panel_algorithm_*` tests.
        let ladder = [
            Rung::Override,
            Rung::DeviceFinal,
            Rung::Screen,
            Rung::DeviceBase,
        ];
        for peeled in 0..ladder.len() {
            let present = &ladder[peeled..];
            let res = resolve_render(&table_inputs(present)).report;
            let expected = present[0];
            for setting in SETTINGS {
                assert_eq!(
                    winner(&res, setting),
                    expected,
                    "`{setting}` with rungs {present:?} must be won by {expected:?}"
                );
            }
            // Everything still present below the winner is Outranked, never Absent
            // — a rung the builder forgot reports Absent and would pass a
            // winner-only assertion.
            for setting in SETTINGS {
                let report = &res[setting];
                for lower in &present[1..] {
                    let found = report.rungs.iter().find(|(r, _)| *r == lower);
                    let (_, entry) = found.unwrap_or_else(|| {
                        panic!(
                            "`{setting}`: rung {lower:?} is missing from the report, so the \
                         ladder builder never offered it a slot"
                        )
                    });
                    assert_eq!(
                        entry.status,
                        RungStatus::Outranked,
                        "`{setting}`: rung {lower:?} below the winner must be Outranked"
                    );
                }
            }
        }
    }

    /// Spec §7, named explicitly because this is the one behaviour change in the
    /// release: today the device's bare `dither` beats a screen that sets its own.
    #[test]
    fn dither_screen_beats_device_base() {
        let res = resolve_render(&table_inputs(&[Rung::Screen, Rung::DeviceBase]));
        assert_eq!(res.dither.as_deref(), Some(DITHER_BY_RUNG[3]));
        assert_eq!(winner(&res.report, "dither"), Rung::Screen);
    }

    #[test]
    fn dither_device_final_beats_screen() {
        let res = resolve_render(&table_inputs(&[Rung::DeviceFinal, Rung::Screen]));
        assert_eq!(res.dither.as_deref(), Some(DITHER_BY_RUNG[4]));
        assert_eq!(winner(&res.report, "dither"), Rung::DeviceFinal);
    }

    #[test]
    fn dither_override_beats_everything() {
        let res = resolve_render(&table_inputs(&[
            Rung::Override,
            Rung::DeviceFinal,
            Rung::Screen,
            Rung::DeviceBase,
        ]));
        assert_eq!(res.dither.as_deref(), Some(OVERRIDE_DITHER));
        assert_eq!(winner(&res.report, "dither"), Rung::Override);
    }

    /// `colors_actual` loses its exception: piece 2 put the screen above the
    /// override to match the legacy resolver. Spec §2 gives it the same order as
    /// everything else, so a dev-UI calibration push now wins — which is the
    /// whole point of the override rung.
    #[test]
    fn colors_actual_override_beats_the_screen() {
        let res = resolve_render(&table_inputs(&[Rung::Override, Rung::Screen]));
        assert_eq!(winner(&res.report, "colors_actual"), Rung::Override);
    }

    /// Spec §7: the test that would have caught the first draft's inverted
    /// migration instruction. `refresh` is a ZERO behaviour change — a script's
    /// `refresh_rate` still beats a bare `devices.<k>.refresh`.
    #[test]
    fn refresh_script_still_beats_the_device_base_rung() {
        let res = resolve_render(&table_inputs(&[Rung::Screen, Rung::DeviceBase]));
        assert_eq!(
            res.refresh, 300,
            "the script's 100*3, not the device's 100*2"
        );
        assert_eq!(winner(&res.report, "refresh"), Rung::Screen);
    }

    /// …and the one new capability: an owner's veto on how often a
    /// battery-powered display wakes up.
    #[test]
    fn refresh_final_beats_the_script() {
        let res = resolve_render(&table_inputs(&[Rung::DeviceFinal, Rung::Screen]));
        assert_eq!(res.refresh, 400);
        assert_eq!(winner(&res.report, "refresh"), Rung::DeviceFinal);
    }

    /// Field by field, not block by block (regression test for defect 5.3, at
    /// the resolver level): the final rung setting one number must leave the
    /// others to the rungs below it.
    #[test]
    fn a_rung_setting_one_field_leaves_the_others_to_lower_rungs() {
        let mut i = table_inputs(&[Rung::DeviceBase]);
        i.device_final = RungSettings {
            tuning: crate::models::DitherTuningValues {
                strength: Some(0.25),
                ..Default::default()
            },
            ..Default::default()
        };
        let res = resolve_render(&i);
        assert_eq!(
            res.tuning.strength,
            Some(0.25),
            "the final rung's own field"
        );
        assert_eq!(
            res.tuning.max_error,
            Some(2.0),
            "max_error must still come from the device base rung"
        );
        assert_eq!(winner(&res.report, "strength"), Rung::DeviceFinal);
        assert_eq!(winner(&res.report, "max_error"), Rung::DeviceBase);
    }

    /// Ruling 2's mapping, which Tasks 3–6 depend on for their diagnostics.
    #[test]
    fn measured_source_names_the_rung_that_won() {
        let res = resolve_render(&table_inputs(&[Rung::Screen]));
        assert_eq!(res.measured_source, display::SRC_SCRIPT);
        let res = resolve_render(&table_inputs(&[Rung::DeviceFinal]));
        assert_eq!(res.measured_source, display::SRC_DEVICE);
        let res = resolve_render(&table_inputs(&[Rung::Panel]));
        assert_eq!(res.measured_source, display::SRC_PANEL_ACTUAL);
        let res = resolve_render(&table_inputs(&[Rung::Override]));
        assert_eq!(res.measured_source, display::SRC_DEV_OVERRIDE);
        let res = resolve_render(&table_inputs(&[]));
        assert_eq!(res.measured_source, display::SRC_NONE);

        // A fallback (the `Measured-Colors` header) that matches the resolved
        // palette's length wins and is named as such.
        let mut inputs = table_inputs(&[]);
        inputs.fallback_colors_actual = Some(display::parse_colors_header("#010101,#FEFEFE"));
        let res = resolve_render(&inputs);
        assert!(res.measured_colors.is_some());
        assert_eq!(res.measured_source, display::SRC_MEASURED_HEADER);

        // A wrong-length header is rejected: no measurement is used, so none
        // may be named as the source.
        let mut inputs = table_inputs(&[]);
        inputs.fallback_colors_actual = Some(display::parse_colors_header("#010101"));
        let res = resolve_render(&inputs);
        assert_eq!(res.measured_colors, None);
        assert_eq!(res.measured_source, display::SRC_NONE);
    }

    // ---- RungSettings::from_script -------------------------------------

    fn distinct_script() -> ScriptResult {
        let mut s = crate::services::screen_store::tests::script_result_for_wiring();
        s.script_colors = Some(vec!["#000001".into(), "#FFFFFE".into()]);
        s.script_colors_actual = Some(vec!["#010203".into(), "#FEFDFC".into()]);
        s.script_dither = Some("Sierra_Lite".into());
        s.script_max_error = Some(0.21);
        s.script_noise_scale = Some(0.22);
        s.script_chroma_clamp = Some(0.23);
        s.script_strength = Some(0.24);
        s.script_gamut = Some(crate::models::GamutTuningValues {
            knee: Some(0.31),
            amount: Some(0.32),
            max_compression: Some(0.33),
        });
        s.script_refresh_rate = Some(777);
        s
    }

    #[test]
    fn from_script_puts_every_field_in_its_own_slot() {
        let r = RungSettings::from_script(&distinct_script());
        assert_eq!(r.colors, Some(vec![(0, 0, 1), (255, 255, 254)]));
        assert_eq!(r.colors_actual, Some(vec![(1, 2, 3), (254, 253, 252)]));
        assert_eq!(r.dither, Some(normalize_algorithm_name("Sierra_Lite")));
        assert_eq!(r.tuning.max_error, Some(0.21));
        assert_eq!(r.tuning.noise_scale, Some(0.22));
        assert_eq!(r.tuning.chroma_clamp, Some(0.23));
        assert_eq!(r.tuning.strength, Some(0.24));
        assert_eq!(r.tuning.gamut.knee, Some(0.31));
        assert_eq!(r.tuning.gamut.amount, Some(0.32));
        assert_eq!(r.tuning.gamut.max_compression, Some(0.33));
        assert_eq!(r.tuning.deprecated_error_clamp, None);
    }

    #[test]
    fn from_script_leaves_refresh_to_the_caller() {
        let s = distinct_script();
        assert_eq!(RungSettings::from_script(&s).refresh, None);
        assert_eq!(
            RungSettings::from_script(&s)
                .with_refresh(s.script_refresh_rate)
                .refresh,
            Some(777)
        );
    }

    #[test]
    fn from_script_does_not_inflate_colors_actual_with_a_comma_entry() {
        let mut s = distinct_script();
        // One list element that itself holds two colours.
        s.script_colors_actual = Some(vec!["#111111,#222222".into(), "#333333".into()]);
        let r = RungSettings::from_script(&s);
        // Entry-by-entry: the comma entry is dropped, so the count stays
        // honest (1) instead of fracturing into 3 "valid" colours.
        assert_eq!(r.colors_actual, Some(vec![(0x33, 0x33, 0x33)]));
        // `colors` is the deliberate join-and-split: the same shape yields 3.
        s.script_colors = Some(vec!["#111111,#222222".into(), "#333333".into()]);
        assert_eq!(RungSettings::from_script(&s).colors.unwrap().len(), 3);
    }

    // ---- resolve_pre_script ----

    fn rgb(n: u8) -> Vec<(u8, u8, u8)> {
        vec![(n, n, n), (n, n, 255)]
    }

    /// A panel with a flat default and two per-algorithm blocks whose numbers
    /// differ, so looking the numbers up for the wrong algorithm is visible.
    fn two_algorithm_panel() -> PanelDitherConfig {
        let block = |max_error: f32| DitherTuningValues {
            max_error: Some(max_error),
            ..Default::default()
        };
        let mut algorithms = std::collections::HashMap::new();
        algorithms.insert("sierra".to_string(), block(0.5));
        algorithms.insert("atkinson".to_string(), block(0.9));
        PanelDitherConfig {
            algorithm: None,
            defaults: DitherTuningValues {
                max_error: Some(0.1),
                noise_scale: Some(9.0),
                chroma_clamp: Some(0.6),
                gamut: crate::models::GamutTuningValues {
                    knee: Some(0.05),
                    amount: Some(0.5),
                    max_compression: None,
                },
                ..Default::default()
            },
            algorithms,
        }
    }

    fn pre_inputs(panel_dither: Option<&PanelDitherConfig>) -> PreScriptInputs<'_> {
        PreScriptInputs {
            over: RungSettings::default(),
            device_final: RungSettings::default(),
            device_base: RungSettings::default(),
            panel: RungSettings::default(),
            panel_dither,
            fallback_palette: rgb(4),
            fallback_colors_actual: None,
        }
    }

    #[test]
    fn pre_script_every_rung_lands_where_it_should() {
        let panel_dither = PanelDitherConfig {
            defaults: DitherTuningValues {
                strength: Some(0.3),
                gamut: crate::models::GamutTuningValues {
                    knee: Some(0.03),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let mut i = pre_inputs(Some(&panel_dither));
        i.over = RungSettings {
            colors_actual: Some(rgb(1)),
            ..Default::default()
        };
        i.device_final = RungSettings {
            colors: Some(rgb(2)),
            colors_actual: Some(rgb(3)),
            tuning: DitherTuningValues {
                max_error: Some(0.2),
                ..Default::default()
            },
            ..Default::default()
        };
        i.device_base = RungSettings {
            colors: Some(rgb(5)),
            colors_actual: Some(rgb(99)),
            tuning: DitherTuningValues {
                max_error: Some(0.4),
                noise_scale: Some(4.0),
                ..Default::default()
            },
            ..Default::default()
        };
        i.panel = RungSettings {
            colors: Some(rgb(6)),
            colors_actual: Some(rgb(7)),
            ..Default::default()
        };
        i.fallback_colors_actual = Some(rgb(8));

        let r = resolve_pre_script(&i);
        // final beats base beats panel beats fallback, field by field.
        assert_eq!(r.palette, rgb(2), "final colours beat base (5), panel (6)");
        assert_eq!(r.colors_actual, Some(rgb(1)), "override beats final (3)");
        assert_eq!(r.colors_actual_from, Rung::Override);
        assert_eq!(r.tuning.max_error, Some(0.2), "final beats base (0.4)");
        assert_eq!(
            r.tuning.noise_scale,
            Some(4.0),
            "base fills what final omits"
        );
        assert_eq!(r.tuning.strength, Some(0.3), "panel fills what both omit");
        assert_eq!(r.tuning.gamut.knee, Some(0.03));
        assert_eq!(r.tuning.chroma_clamp, None, "nobody sets it");

        // Peel the top rung off, one at a time; each next rung must surface.
        i.over.colors_actual = None;
        let r = resolve_pre_script(&i);
        assert_eq!(r.colors_actual, Some(rgb(3)), "final measured");
        assert_eq!(r.colors_actual_from, Rung::DeviceFinal);
        i.device_final.colors_actual = None;
        i.device_final.colors = None;
        let r = resolve_pre_script(&i);
        assert_eq!(
            r.colors_actual,
            Some(rgb(99)),
            "the base measurement beats the panel's (7)"
        );
        assert_eq!(r.colors_actual_from, Rung::DeviceBase);
        assert_eq!(r.palette, rgb(5), "base colours once final has none");
        i.device_base.colors_actual = None;
        let r = resolve_pre_script(&i);
        assert_eq!(r.colors_actual, Some(rgb(7)), "panel measurement next");
        assert_eq!(r.colors_actual_from, Rung::Panel);
        i.device_base.colors = None;
        assert_eq!(resolve_pre_script(&i).palette, rgb(6), "panel colours");
        i.panel.colors = None;
        assert_eq!(resolve_pre_script(&i).palette, rgb(4), "fallback palette");
        i.panel.colors_actual = None;
        let r = resolve_pre_script(&i);
        assert_eq!(r.colors_actual, Some(rgb(8)), "firmware header");
        assert_eq!(r.colors_actual_from, Rung::Fallback);
    }

    #[test]
    fn pre_script_algorithm_ladder_and_default() {
        let mut i = pre_inputs(None);
        assert_eq!(resolve_pre_script(&i).algorithm, "atkinson");
        i.device_base.dither = Some("burkes".into());
        assert_eq!(resolve_pre_script(&i).algorithm, "burkes");
        i.device_final.dither = Some("stucki".into());
        assert_eq!(
            resolve_pre_script(&i).algorithm,
            "stucki",
            "final beats base"
        );
        i.over.dither = Some("sierra".into());
        assert_eq!(
            resolve_pre_script(&i).algorithm,
            "sierra",
            "override beats final"
        );
    }

    /// The failure the algorithm-then-numbers rule exists to prevent: the
    /// veto wins the name but the panel numbers are looked up for the loser.
    #[test]
    fn pre_script_panel_numbers_follow_the_final_algorithm() {
        let panel = two_algorithm_panel();
        let mut i = pre_inputs(Some(&panel));
        i.device_base.dither = Some("sierra".into());
        assert_eq!(resolve_pre_script(&i).tuning.max_error, Some(0.5));
        i.device_final.dither = Some("atkinson".into());
        let r = resolve_pre_script(&i);
        assert_eq!(r.algorithm, "atkinson");
        assert_eq!(
            r.tuning.max_error,
            Some(0.9),
            "atkinson's block, not the base algorithm's 0.5"
        );
        assert_eq!(r.tuning.noise_scale, Some(9.0), "flat default still merges");
    }

    /// With no `final:` the function must agree with the plain precedence
    /// chain `override > device base > panel > fallback`, written out here
    /// independently of `Ladder`. Compared over every present/absent
    /// combination of: the override's dither and measurement; the base rung's
    /// colours, measurement, dither and four numbers (`max_error`,
    /// `noise_scale`, `chroma_clamp`, `gamut.knee`), each with a value that
    /// differs from the panel's; and the panel's colours, measurement and
    /// whole per-algorithm dither block. Every number the function reports is
    /// therefore decided by both a base and a panel contender.
    #[test]
    fn pre_script_without_final_matches_the_legacy_chain() {
        let panel = two_algorithm_panel();
        for mask in 0u32..(1 << 13) {
            let on = |bit: u32| mask & (1 << bit) != 0;
            let base = RungSettings {
                colors: on(0).then(|| rgb(10)),
                colors_actual: on(1).then(|| rgb(11)),
                dither: on(2).then(|| "sierra".to_string()),
                tuning: DitherTuningValues {
                    max_error: on(3).then_some(0.25),
                    strength: on(4).then_some(0.75),
                    noise_scale: on(9).then_some(4.0),
                    chroma_clamp: on(10).then_some(0.2),
                    gamut: crate::models::GamutTuningValues {
                        knee: on(11).then_some(0.3),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                refresh: None,
            };
            let over = RungSettings {
                dither: on(5).then(|| "floyd-steinberg".to_string()),
                colors_actual: on(6).then(|| rgb(12)),
                ..Default::default()
            };
            let panel_rung = RungSettings {
                colors: on(7).then(|| rgb(13)),
                colors_actual: on(8).then(|| rgb(14)),
                ..Default::default()
            };
            let panel_dither = on(12).then_some(&panel);
            let mut i = pre_inputs(panel_dither);
            i.over = over.clone();
            i.device_base = base.clone();
            i.panel = panel_rung.clone();
            i.fallback_colors_actual = Some(rgb(15));
            let got = resolve_pre_script(&i);

            // Legacy chain.
            let algo = over
                .dither
                .as_deref()
                .or(base.dither.as_deref())
                .unwrap_or("atkinson");
            let legacy_tuning = base.tuning.or(&panel_dither
                .map(|p| p.resolve_for_algorithm(Some(algo)))
                .unwrap_or_default());
            let legacy_palette = base
                .colors
                .clone()
                .or(panel_rung.colors.clone())
                .unwrap_or_else(|| rgb(4));
            let legacy_actual = over
                .colors_actual
                .clone()
                .or(base.colors_actual.clone())
                .or(panel_rung.colors_actual.clone())
                .or(Some(rgb(15)));

            assert_eq!(got.algorithm, algo, "mask {mask:#b}");
            assert_eq!(got.palette, legacy_palette, "mask {mask:#b}");
            assert_eq!(got.colors_actual, legacy_actual, "mask {mask:#b}");
            assert_eq!(
                got.tuning.max_error, legacy_tuning.max_error,
                "mask {mask:#b}"
            );
            assert_eq!(
                got.tuning.noise_scale, legacy_tuning.noise_scale,
                "mask {mask:#b}"
            );
            assert_eq!(
                got.tuning.strength, legacy_tuning.strength,
                "mask {mask:#b}"
            );
            assert_eq!(
                got.tuning.chroma_clamp, legacy_tuning.chroma_clamp,
                "mask {mask:#b}"
            );
            assert_eq!(got.tuning.gamut, legacy_tuning.gamut, "mask {mask:#b}");
        }
    }
}
