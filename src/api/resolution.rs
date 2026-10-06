//! The render-setting cascade: rungs, their statuses, and one resolver.
//!
//! See `docs/superpowers/specs/2026-10-04-render-resolution-cascade-design.md`
//! §2 and §4.4. Precedence is an argument, not a property of the types, so the
//! same machinery serves today's per-setting orders and the spec's single one.

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
            rungs: self
                .rungs
                .iter()
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
        assert_eq!(
            json["rungs"]["override"],
            serde_json::json!({"status": "absent"})
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
}
