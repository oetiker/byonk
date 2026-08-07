//! The `eink` base layer and device-aware endpoint derivation.

use crate::{Params, Preset, Sharpen};

/// Tone-domain luminance of the darkest and lightest palette entries.
///
/// `palette_aware` uses this to stop the tone mapping spending range the
/// panel physically cannot show: there is no point mapping a shadow to 0.0
/// when the panel's blackest ink measures 0.05.
pub fn palette_endpoints(palette: &[(u8, u8, u8)]) -> Option<(f32, f32)> {
    if palette.is_empty() {
        return None;
    }
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for &(r, g, b) in palette {
        let l = crate::color::luminance(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        lo = lo.min(l);
        hi = hi.max(l);
    }
    Some((lo, hi))
}

/// Fill unset fields from the preset. Explicit values always win — that is
/// the rule that keeps `preset` predictable as its values are retuned.
pub fn apply_base_layer(params: &Params) -> Params {
    let mut out = params.clone();
    if params.preset == Preset::None {
        return out;
    }

    // Starting values, chosen for a low-dynamic-range six-colour panel.
    // These are an implementation detail and are expected to move as they
    // are measured against the calibration screens — never depend on the
    // exact numbers, only on their signs.
    out.auto_levels = out.auto_levels.or(Some(true));
    out.shadows = out.shadows.or(Some(20.0));
    out.highlights = out.highlights.or(Some(-20.0));
    out.clarity = out.clarity.or(Some(25.0));
    out.vibrance = out.vibrance.or(Some(30.0));
    out.sharpen = out.sharpen.or(Some(Sharpen {
        amount: 30.0,
        radius: 1.0,
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Params, Preset};

    #[test]
    fn preset_none_changes_nothing() {
        let p = Params {
            contrast: Some(10.0),
            ..Default::default()
        };
        let out = apply_base_layer(&p);
        assert_eq!(out, p);
    }

    #[test]
    fn eink_preset_fills_unset_fields() {
        let out = apply_base_layer(&Params {
            preset: Preset::Eink,
            ..Default::default()
        });
        assert_eq!(out.auto_levels, Some(true));
        assert!(out.shadows.unwrap() > 0.0);
        assert!(out.highlights.unwrap() < 0.0);
        assert!(out.clarity.unwrap() > 0.0);
        assert!(out.vibrance.unwrap() > 0.0);
        assert!(out.sharpen.is_some());
    }

    #[test]
    fn an_explicit_value_overrides_the_preset() {
        // The rule that keeps the preset predictable as its values are
        // retuned: `{ preset = "eink", clarity = 0 }` means no clarity.
        let out = apply_base_layer(&Params {
            preset: Preset::Eink,
            clarity: Some(0.0),
            ..Default::default()
        });
        assert_eq!(out.clarity, Some(0.0));
        // ...while its neighbours still come from the preset.
        assert!(out.vibrance.unwrap() > 0.0);
    }

    #[test]
    fn palette_endpoints_finds_the_darkest_and_lightest() {
        let palette = [(10, 10, 10), (232, 230, 224), (168, 58, 48), (63, 122, 69)];
        let (lo, hi) = palette_endpoints(&palette).expect("non-empty palette");
        assert!(lo < 0.1, "darkest entry should be near black: {lo}");
        assert!(hi > 0.85, "lightest entry should be near white: {hi}");
        assert!(lo < hi);
    }

    #[test]
    fn palette_endpoints_of_an_empty_palette_is_none() {
        assert!(palette_endpoints(&[]).is_none());
    }
}
