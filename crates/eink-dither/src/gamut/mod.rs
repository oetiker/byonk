//! Gamut mapping for continuous-tone regions.

pub mod adapt;
pub mod cmax;
pub mod hull;
pub mod knee;
pub mod mapper;

pub use mapper::{GamutMapper, GamutOptions};

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

    /// The six-ink panel as it actually measures, official colours declared
    /// against measured inks.
    ///
    /// This is the palette that exposes the hull's pinch: at the measured
    /// yellow's own lightness the constant-`L` slice reaches barely a third of
    /// the ink's chroma, so a fixed-lightness mapper cannot render the panel's
    /// own ink. `six_colour`'s idealised primaries do not reproduce that, which
    /// is why the geometry tests need this one.
    pub(crate) fn panel_measured() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(0, 255, 0),
            ],
            Some(&[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(0xB5, 0x03, 0x03),
                Srgb::from_u8(0xFF, 0xEE, 0x00),
                Srgb::from_u8(0x20, 0x54, 0x97),
                Srgb::from_u8(0x0D, 0x87, 0x6B),
            ]),
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
