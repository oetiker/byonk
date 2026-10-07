//! The render-setting cascade: rungs, their statuses, and one resolver.
//!
//! See `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
//! §2 and §4.4. Precedence is an argument, not a property of the types, so the
//! same machinery serves today's per-setting orders and the spec's single one.

use crate::api::display;
use crate::models::{
    normalize_algorithm_name, CascadeValues, DitherTuningValues, PanelDitherConfig,
};
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
/// `Outranked` without being checked — the same rule
/// `display::resolve_measured_colors` applies.
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
/// Spec §2's order. Piece 2 put `Screen` on top to match the legacy
/// `resolve_render_params`, which resolved the script's candidate before the
/// caller's override chain. The override rung exists so a dev-UI calibration
/// push can show what it does, so it goes back on top.
pub const ORDER_COLORS_ACTUAL: &[Rung] = ORDER_COLORS;
/// Spec §2's order. **This is the release's one behaviour change**: the
/// legacy `display::effective_dither` put `DeviceBase` above `Screen`, so a
/// bare `devices.<k>.dither` vetoed a screen that set its own. The veto now
/// lives in `final:`, where it can be told apart from a default.
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
    /// Colours only; the panel's tuning is per algorithm and comes from
    /// `panel_dither` once the algorithm is known (spec §4.2).
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
    // `Floyd_Steinberg` and `floyd-steinberg` are one value.
    let dither = resolve(
        &ladder_of(
            i,
            |r| r.dither.as_deref().map(normalize_algorithm_name),
            None,
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
    // The panel block is looked up for the CONFIGURED algorithm: with no
    // rung setting `dither` that is `None`, which selects the panel's flat
    // defaults only — what every other render path does today. `algo` (the
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
            report.insert($key, r.report(|v| serde_json::json!(v)));
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn col6() -> Vec<(u8, u8, u8)> {
        display::parse_colors_header("#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00")
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

    type Legacy = (
        Vec<(u8, u8, u8)>,
        Option<Vec<(u8, u8, u8)>>,
        Option<String>,
        DitherTuningValues,
    );

    /// The legacy computation `handle_render` performed before this piece,
    /// driven through the NEW order: `effective_dither` for the algorithm, the
    /// panel block for that algorithm, `resolve_effective_tuning`, then
    /// `resolve_render_params`. It is not verbatim: the screen's dither is
    /// permuted into the device slot (and the device base's into the script
    /// slot), the override's dither blanks the script slot, and
    /// `script_actual` is always blanked so the override's measurement can
    /// outrank the screen's. Do NOT permute these back to what `dev.rs` did.
    fn legacy(i: &RenderInputs) -> Legacy {
        // The legacy resolver's two dither slots rank the DEVICE slot above the
        // SCRIPT slot. Spec §2 ranks the screen above the device base, so the
        // screen's dither is fed through the device slot and the device base's
        // through the script slot — that is what expresses the new order
        // with the old function.
        let (script_dither, device_dither) = match i.over.dither.as_deref() {
            Some(d) => (None, Some(d)),
            None => (i.device_base.dither.as_deref(), i.screen.dither.as_deref()),
        };
        let algo = display::effective_dither(device_dither, script_dither);
        let panel_tuning = i
            .panel_dither
            .map(|p| p.resolve_for_algorithm(algo.as_deref()))
            .unwrap_or_default();
        let tuning = display::resolve_effective_tuning(
            &i.over.tuning,
            &i.screen.tuning,
            &i.device_base.tuning,
            &panel_tuning,
        );
        let hex = |c: &Vec<(u8, u8, u8)>| display::colors_to_hex_strings(c);
        let script_colors = i.screen.colors.as_ref().map(hex);
        let dc_colors = i.device_base.colors.as_ref().map(|c| hex(c).join(","));
        let panel_colors = i.panel.colors.as_ref().map(|c| hex(c).join(","));
        // The legacy resolver always tries the script's measurement first;
        // spec §2 puts the override above the screen. So the screen's
        // measurement is passed as a plain candidate, after the override's.
        let candidates = vec![
            (display::SRC_DEV_OVERRIDE, i.over.colors_actual.clone()),
            (display::SRC_SCRIPT, i.screen.colors_actual.clone()),
            (display::SRC_PANEL_ACTUAL, i.panel.colors_actual.clone()),
        ];
        let p = display::resolve_render_params(
            script_colors.as_deref(),
            None,
            script_dither,
            dc_colors.as_deref(),
            device_dither,
            panel_colors.as_deref(),
            &i.fallback_palette,
            &candidates,
            &tuning,
            &mut Vec::new(),
        );
        let t = DitherTuningValues {
            max_error: p.max_error,
            noise_scale: p.noise_scale,
            chroma_clamp: p.chroma_clamp,
            strength: p.strength,
            gamut: p.gamut,
            ..Default::default()
        };
        (p.palette, p.measured_colors, p.dither, t)
    }

    // Piece 3 flipped ORDER_DITHER and ORDER_COLORS_ACTUAL (spec §2), so the
    // ladder deliberately DISAGREES with the legacy resolvers' own order on
    // these two settings. `legacy()` above feeds the legacy functions through
    // the new order (screen dither above device base; override measurement
    // above the screen's), so the matrix keeps its full size and encodes the
    // new, intended answer; every other row still pins the ladder to the
    // legacy behaviour. This is not a tautology: `legacy()` is a second,
    // independent encoding of the order (permuted slots versus the ladder's
    // order constants), so reverting either constant, or the permutation,
    // makes the two disagree. It is the only guard on the new dither order
    // outside `resolution.rs`'s unit tests: a failure here is a real
    // disagreement, not a test to loosen, and the permutation must not be
    // "fixed" back to the old order. This whole test is retired in Task 8,
    // when the legacy resolvers are deleted and the five-paths convergence
    // test takes over.
    ///
    /// The ladder must produce what the legacy resolvers produce when driven
    /// through the new order, for every combination below.
    #[test]
    fn resolve_render_matches_the_legacy_resolver_under_the_new_order() {
        let mut algorithms = HashMap::new();
        algorithms.insert(
            "floyd-steinberg".to_string(),
            DitherTuningValues {
                strength: Some(0.3),
                max_error: Some(0.6),
                ..Default::default()
            },
        );
        // A block for the FALLBACK algorithm: with no rung setting `dither`,
        // the legacy path looks the panel up for `None` (its defaults only),
        // never for the engine's own default algorithm.
        algorithms.insert(
            "atkinson".to_string(),
            DitherTuningValues {
                strength: Some(0.4),
                ..Default::default()
            },
        );
        let pdc = PanelDitherConfig {
            defaults: DitherTuningValues {
                strength: Some(0.9),
                ..Default::default()
            },
            algorithms,
        };
        let dithers = [
            None,
            Some("atkinson"),
            Some("floyd-steinberg"),
            Some("Floyd_Steinberg"),
        ];
        let palettes = [None, Some(grey4()), Some(col6())];
        let actuals = [None, Some(act4()), Some(act6())];
        let strengths = [None, Some(0.1f32)];
        let mut cases = 0;
        for pd in [None, Some(&pdc)] {
            for od in dithers {
                for sd in dithers {
                    for bd in dithers {
                        for os in strengths {
                            for ss in strengths {
                                for bs in strengths {
                                    let mut i = inputs();
                                    i.panel_dither = pd;
                                    i.over.dither = od.map(str::to_string);
                                    i.screen.dither = sd.map(str::to_string);
                                    i.device_base.dither = bd.map(str::to_string);
                                    i.over.tuning.strength = os;
                                    i.screen.tuning.strength = ss;
                                    i.device_base.tuning.strength = bs;
                                    i.screen.tuning.max_error = Some(0.4);
                                    let n = resolve_render(&i);
                                    assert_eq!(
                                        (n.palette, n.measured_colors, n.dither, n.tuning),
                                        legacy(&i),
                                        "{od:?} {sd:?} {bd:?} {os:?} {ss:?} {bs:?} panel={}",
                                        pd.is_some()
                                    );
                                    cases += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        for sc in palettes.clone() {
            for bc in palettes.clone() {
                for pc in palettes.clone() {
                    for sa in actuals.clone() {
                        for oa in actuals.clone() {
                            for pa in actuals.clone() {
                                let mut i = inputs();
                                i.screen.colors = sc.clone();
                                i.device_base.colors = bc.clone();
                                i.panel.colors = pc.clone();
                                i.screen.colors_actual = sa.clone();
                                i.over.colors_actual = oa.clone();
                                i.panel.colors_actual = pa.clone();
                                let n = resolve_render(&i);
                                assert_eq!(
                                    (n.palette, n.measured_colors, n.dither, n.tuning),
                                    legacy(&i),
                                    "{sc:?} {bc:?} {pc:?} {sa:?} {oa:?} {pa:?}"
                                );
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
        // 2 panels × 4³ dithers × 2³ strengths + 3⁶ colour combinations.
        assert_eq!(cases, 1753, "the matrix changed size");
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
    /// rung supplies colours only — a panel has a `dither:` *block*, not an
    /// algorithm name, and its numbers come from `panel_dither` once the
    /// algorithm is known (spec §4.2).
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
        // The panel and fallback rungs are per-setting (a panel has no `dither`,
        // no `refresh`), so the walk stops at DeviceBase and the lower rungs get
        // their own assertions below.
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
}
