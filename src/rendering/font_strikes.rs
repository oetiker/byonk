//! Bitmap strike sizes for a font face.
//!
//! This used to be `fontdb::FaceInfo::bitmap_strikes`, a field byonk's fork of
//! fontdb carried. Upstream fontdb has no such field and never will — it is a
//! separate project from resvg, so no resvg PR could add it — and carrying the
//! fork was the only reason byonk pinned fontdb to the resvg repository at all.
//!
//! It was never new capability, only a cached convenience: skrifa exposes the
//! data directly, over the same font bytes byonk already owns. Computing it here
//! is what lets the fontdb pin disappear.

use skrifa::bitmap::BitmapStrikes;
use skrifa::FontRef;

/// The ppem sizes of `data`'s embedded bitmap strikes, ascending and deduplicated.
///
/// Returns empty for an outline-only font and for data that does not parse —
/// an unreadable face is not an error here, it simply has no strikes to offer.
pub fn bitmap_strikes_for(data: &[u8], index: u32) -> Vec<u16> {
    let Ok(font) = FontRef::from_index(data, index) else {
        return Vec::new();
    };

    let mut sizes: Vec<u16> = BitmapStrikes::new(&font)
        .iter()
        .map(|s| s.ppem().round() as u16)
        .filter(|&ppem| ppem > 0)
        .collect();

    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// X11Helv is a bitmap pixel font byonk bundles; it is the fixture the
    /// existing `test_bitmap_strikes_exposed` relies on.
    fn x11helv_bytes() -> Vec<u8> {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        loader
            .get_fonts()
            .into_iter()
            .find(|(name, _)| name.contains("X11Helv"))
            .map(|(_, data)| data.into_owned())
            .expect("X11Helv must be bundled")
    }

    #[test]
    fn x11helv_strikes_are_non_empty_and_ascending() {
        let strikes = bitmap_strikes_for(&x11helv_bytes(), 0);
        assert!(
            !strikes.is_empty(),
            "X11Helv is a bitmap font and must report strikes"
        );
        for w in strikes.windows(2) {
            assert!(
                w[0] < w[1],
                "strikes must be ascending and deduplicated: {strikes:?}"
            );
        }
    }

    #[test]
    fn an_outline_font_reports_no_strikes() {
        // The control: without this, a `bitmap_strikes_for` that returned a
        // hardcoded non-empty list would pass the test above.
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let outfit = loader
            .get_fonts()
            .into_iter()
            .find(|(name, _)| name.contains("Outfit"))
            .map(|(_, data)| data.into_owned())
            .expect("Outfit must be bundled");
        assert!(
            bitmap_strikes_for(&outfit, 0).is_empty(),
            "Outfit is an outline font and must report no strikes"
        );
    }

    #[test]
    fn garbage_input_reports_no_strikes() {
        assert!(bitmap_strikes_for(b"not a font at all", 0).is_empty());
    }
}
