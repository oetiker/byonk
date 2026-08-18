//! Facts read from a font face's own bytes.
//!
//! Everything here has the same shape: skrifa over the raw bytes byonk already
//! owns, answering one question fontdb does not, and treating an unparseable
//! face as "no" rather than as an error — a face byonk cannot read is a face
//! whose capabilities it cannot rely on.
//!
//! [`bitmap_strikes_for`] used to be `fontdb::FaceInfo::bitmap_strikes`, a field
//! byonk's fork of fontdb carried. Upstream fontdb has no such field and never
//! will — it is a separate project from resvg, so no resvg PR could add it — and
//! carrying the fork was the only reason byonk pinned fontdb to the resvg
//! repository at all. It was never new capability, only a cached convenience.

use skrifa::bitmap::BitmapStrikes;
use skrifa::{FontRef, Tag};

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

/// Whether the TrueType interpreter has any real hinting program to run.
///
/// `false` for a face whose whole contribution is the seven-byte `prep` stub
/// modern build tools emit — `SCANCTRL`/`SCANTYPE`, which only ask for dropout
/// control and shape no glyph. skrifa's `AutoFallback` engine still picks the
/// interpreter for such a face, because it asks only whether `fpgm` or `prep`
/// is non-empty, so the fallback never fires and the text renders unhinted.
///
/// The rule is structural, not a match on those seven bytes: glyph-level
/// hinting needs function definitions (`fpgm`) or control values (`cvt `) to be
/// worth running, while a `prep` is a pre-program that in every face examined
/// here does nothing but set scan-conversion flags. A byte-match would stop
/// working the moment a build tool changed one instruction, and would not carry
/// over to other toolchains.
///
/// This can only ever move a face towards the automatic hinter, never away from
/// it, so disagreeing with skrifa is safe: where skrifa would already choose
/// the autohinter, saying `true` here changes nothing.
pub fn has_interpreter_hinting(data: &[u8], index: u32) -> bool {
    let Ok(font) = FontRef::from_index(data, index) else {
        return false;
    };

    [Tag::new(b"fpgm"), Tag::new(b"cvt ")]
        .iter()
        .any(|&tag| font.table_data(tag).is_some_and(|d| !d.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled face whose file name contains `needle`.
    fn bundled(needle: &str) -> Vec<u8> {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        loader
            .get_fonts()
            .into_iter()
            .find(|(name, _)| name.contains(needle))
            .map(|(_, data)| data.into_owned())
            .unwrap_or_else(|| panic!("{needle} must be bundled"))
    }

    /// X11Helv is a bitmap pixel font byonk bundles; it is the fixture the
    /// existing `test_bitmap_strikes_exposed` relies on.
    fn x11helv_bytes() -> Vec<u8> {
        bundled("X11Helv")
    }

    #[test]
    fn a_stub_prep_alone_is_not_interpreter_hinting() {
        // Outfit's entire hinting contribution is the seven-byte
        // `b8 01 ff 85 b0 04 8d` — `SCANCTRL`/`SCANTYPE` — with no `fpgm` and
        // no `cvt `. This is the case `AutoFallback` gets wrong.
        assert!(
            !has_interpreter_hinting(&bundled("Outfit"), 0),
            "a face with only the dropout-control stub has nothing to interpret"
        );
    }

    #[test]
    fn control_values_count_as_interpreter_hinting() {
        // Terminus carries a `cvt ` and no `prep`/`fpgm`.
        assert!(
            has_interpreter_hinting(&bundled("TerminusTTF.ttf"), 0),
            "a face carrying control values has something to interpret"
        );
    }

    #[test]
    fn two_bundled_faces_disagree() {
        // The control. Without it, a function returning a constant — in either
        // direction — passes both tests above.
        assert_ne!(
            has_interpreter_hinting(&bundled("Outfit"), 0),
            has_interpreter_hinting(&bundled("TerminusTTF.ttf"), 0),
            "the two fixtures must land on opposite sides, or neither test \
             distinguishes a real answer from a hardcoded one"
        );
    }

    #[test]
    fn garbage_input_has_no_interpreter_hinting() {
        assert!(!has_interpreter_hinting(b"not a font at all", 0));
    }

    /// The `fpgm` half of the rule, which no bundled face exercises — every
    /// font byonk ships has an empty `fpgm`, so the tests above only ever prove
    /// the `cvt ` half.
    ///
    /// Ignored because the fixture is not in the tree and must not be: Roboto
    /// is not a font byonk bundles. Run with an unpacked Google Fonts checkout:
    ///
    /// ```text
    /// ROBOTO_TTF=<gfonts>/ofl/roboto/Roboto[wdth,wght].ttf \
    ///   cargo test --lib fpgm_counts_as_interpreter_hinting -- --ignored
    /// ```
    #[test]
    #[ignore = "needs an external Roboto fixture; see the doc comment"]
    fn fpgm_counts_as_interpreter_hinting() {
        let path = std::env::var("ROBOTO_TTF").expect("set ROBOTO_TTF to a Roboto .ttf");
        let bytes = std::fs::read(&path).expect("ROBOTO_TTF must be readable");
        assert!(
            has_interpreter_hinting(&bytes, 0),
            "Roboto carries a real `fpgm` and must count as interpreter-hinted"
        );
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
