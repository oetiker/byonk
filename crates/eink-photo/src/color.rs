//! sRGB transfer curve and luminance.

/// sRGB-encoded value (0..=1) to linear light. The exact IEC 61966-2-1 curve,
/// matching what `eink-dither` uses — a cheaper 2.2 power approximation would
/// put the two crates fractionally out of step at the shadow end.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to an sRGB-encoded value (0..=1). Values above 1.0 are
/// clamped, which is where an over-exposed highlight becomes paper white.
pub fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Rec. 709 luminance. Applied to whatever domain the caller passes — in the
/// tone domain this is a perceptual lightness proxy, which is what the tonal
/// operations want.
pub fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn transfer_round_trips() {
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            assert!(
                close(linear_to_srgb(srgb_to_linear(v)), v),
                "round trip failed at {v}"
            );
        }
    }

    #[test]
    fn transfer_endpoints_are_exact() {
        assert!(close(srgb_to_linear(0.0), 0.0));
        assert!(close(srgb_to_linear(1.0), 1.0));
        assert!(close(linear_to_srgb(0.0), 0.0));
        assert!(close(linear_to_srgb(1.0), 1.0));
    }

    #[test]
    fn mid_grey_is_about_eighteen_percent_linear() {
        // sRGB 0.5 is ~0.214 in linear light — the number that makes the
        // linear-vs-tone-domain distinction matter.
        assert!(
            (srgb_to_linear(0.5) - 0.2140).abs() < 1e-3,
            "got {}",
            srgb_to_linear(0.5)
        );
    }

    #[test]
    fn luminance_of_white_is_one() {
        assert!(close(luminance(1.0, 1.0, 1.0), 1.0));
    }
}
