//! Gamut mapping for continuous-tone regions.

pub mod cmax;
pub mod hull;

/// Palettes shared by the gamut modules' unit tests.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::{Palette, Srgb};

    /// The six-ink panel palette, official colours.
    pub(crate) fn six_colour() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(255, 255, 0),
            ],
            None,
        )
        .unwrap()
    }

    /// A four-level greyscale panel — the degenerate-hull case.
    pub(crate) fn four_grey() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(85, 85, 85),
                Srgb::from_u8(170, 170, 170),
                Srgb::from_u8(255, 255, 255),
            ],
            None,
        )
        .unwrap()
    }
}
