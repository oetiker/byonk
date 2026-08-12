//! Error diffusion kernel definitions.
//!
//! This module defines the diffusion kernels for various error diffusion
//! algorithms. Each kernel specifies how quantization error is distributed
//! to neighboring pixels.

/// An error diffusion kernel.
///
/// The kernel defines how quantization error is distributed to neighboring
/// pixels that haven't been processed yet. Each entry specifies an offset
/// (dx, dy) and a weight for that neighbor.
///
/// # Error Propagation
///
/// The total error propagated is `sum(weights) / divisor`. Most kernels
/// propagate 100% of error (sum equals divisor), but Atkinson intentionally
/// propagates only 75% to reduce color bleeding with small palettes.
///
/// # Buffer Sizing
///
/// The `max_dy` field indicates how many rows ahead the kernel reaches,
/// which determines the error buffer depth needed: `max_dy + 1` rows.
#[derive(Debug, Clone, Copy)]
pub struct Kernel {
    /// (dx, dy, weight) entries for error diffusion.
    ///
    /// - `dx`: horizontal offset (positive = right, flipped for serpentine)
    /// - `dy`: vertical offset (always positive = below current row)
    /// - `weight`: fraction of error to diffuse (as numerator, divisor is separate)
    pub entries: &'static [(i32, i32, u8)],

    /// Total divisor for normalizing weights.
    ///
    /// Each neighbor receives `error * weight / divisor`.
    pub divisor: u8,

    /// Maximum dy value in entries.
    ///
    /// Used to determine error buffer depth: need `max_dy + 1` rows.
    pub max_dy: usize,
}

/// Atkinson dithering kernel.
///
/// Distributes error to 6 neighbors with 75% total propagation (6/8).
/// The 25% "lost" error reduces color bleeding with small palettes,
/// making this ideal for e-ink displays.
///
/// ```text
///        X   1   1
///    1   1   1
///        1
/// ```
///
/// Originally developed by Bill Atkinson for the Apple Macintosh.
pub const ATKINSON: Kernel = Kernel {
    entries: &[
        (1, 0, 1),  // right
        (2, 0, 1),  // two right
        (-1, 1, 1), // bottom-left
        (0, 1, 1),  // bottom
        (1, 1, 1),  // bottom-right
        (0, 2, 1),  // two below
    ],
    divisor: 8,
    max_dy: 2,
};

/// Floyd-Steinberg dithering kernel.
///
/// Distributes error to 4 neighbors with 100% total propagation (16/16).
/// The most widely known error diffusion algorithm.
///
/// ```text
///        X   7
///    3   5   1
/// ```
pub const FLOYD_STEINBERG: Kernel = Kernel {
    entries: &[
        (1, 0, 7),  // right
        (-1, 1, 3), // bottom-left
        (0, 1, 5),  // bottom
        (1, 1, 1),  // bottom-right
    ],
    divisor: 16,
    max_dy: 1,
};

/// Jarvis-Judice-Ninke dithering kernel.
///
/// Distributes error to 12 neighbors over 3 rows with 100% propagation (48/48).
/// Produces smoother gradients than Floyd-Steinberg but is slower due to
/// the larger kernel size.
///
/// ```text
///            X   7   5
///    3   5   7   5   3
///    1   3   5   3   1
/// ```
pub const JARVIS_JUDICE_NINKE: Kernel = Kernel {
    entries: &[
        (1, 0, 7),
        (2, 0, 5),
        (-2, 1, 3),
        (-1, 1, 5),
        (0, 1, 7),
        (1, 1, 5),
        (2, 1, 3),
        (-2, 2, 1),
        (-1, 2, 3),
        (0, 2, 5),
        (1, 2, 3),
        (2, 2, 1),
    ],
    divisor: 48,
    max_dy: 2,
};

/// Sierra (full/Sierra-3) dithering kernel.
///
/// Distributes error to 10 neighbors over 3 rows with 100% propagation (32/32).
/// Similar to JJN but with smaller coefficients.
///
/// ```text
///            X   5   3
///    2   4   5   4   2
///        2   3   2
/// ```
pub const SIERRA: Kernel = Kernel {
    entries: &[
        (1, 0, 5),
        (2, 0, 3),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 5),
        (1, 1, 4),
        (2, 1, 2),
        (-1, 2, 2),
        (0, 2, 3),
        (1, 2, 2),
    ],
    divisor: 32,
    max_dy: 2,
};

/// Sierra Two-Row dithering kernel.
///
/// Distributes error to 7 neighbors over 2 rows with 100% propagation (16/16).
/// A faster approximation of the full Sierra kernel.
///
/// ```text
///            X   4   3
///    1   2   3   2   1
/// ```
pub const SIERRA_TWO_ROW: Kernel = Kernel {
    entries: &[
        (1, 0, 4),
        (2, 0, 3),
        (-2, 1, 1),
        (-1, 1, 2),
        (0, 1, 3),
        (1, 1, 2),
        (2, 1, 1),
    ],
    divisor: 16,
    max_dy: 1,
};

/// Sierra Lite dithering kernel.
///
/// Distributes error to 3 neighbors with 100% propagation (4/4).
/// The fastest Sierra variant, minimal 2x2 pattern.
///
/// ```text
///    X   2
///    1   1
/// ```
pub const SIERRA_LITE: Kernel = Kernel {
    entries: &[(1, 0, 2), (-1, 1, 1), (0, 1, 1)],
    divisor: 4,
    max_dy: 1,
};

/// Stucki dithering kernel.
///
/// Distributes error to 12 neighbors over 3 rows with 100% propagation (42/42).
/// Similar to JJN but with different weight distribution — higher center weights
/// and lower corner weights produce slightly sharper results.
///
/// ```text
///            X   8   4
///    2   4   8   4   2
///    1   2   4   2   1
/// ```
pub const STUCKI: Kernel = Kernel {
    entries: &[
        (1, 0, 8),
        (2, 0, 4),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 8),
        (1, 1, 4),
        (2, 1, 2),
        (-2, 2, 1),
        (-1, 2, 2),
        (0, 2, 4),
        (1, 2, 2),
        (2, 2, 1),
    ],
    divisor: 42,
    max_dy: 2,
};

/// Burkes dithering kernel.
///
/// Distributes error to 7 neighbors over 2 rows with 100% propagation (32/32).
/// A simplified variant of Stucki using only 2 rows. Faster than Stucki/JJN
/// while maintaining wide error spread.
///
/// ```text
///            X   8   4
///    2   4   8   4   2
/// ```
pub const BURKES: Kernel = Kernel {
    entries: &[
        (1, 0, 8),
        (2, 0, 4),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 8),
        (1, 1, 4),
        (2, 1, 2),
    ],
    divisor: 32,
    max_dy: 1,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// No kernel ever deposits error on an earlier row.
    ///
    /// Ruling 23's containment argument leans on this directly. A horizontal
    /// region boundary needs no work in the upward direction *because* no tap
    /// can reach there: rows above such a boundary come out bit-identical to
    /// the no-boundary control, which is the strongest form the guarantee
    /// takes and the reason the horizontal case is cleaner than the vertical
    /// one. That was argued from the kernel tables by inspection and never
    /// asserted, so a new kernel with an upward tap would silently weaken
    /// containment while every containment test kept passing.
    ///
    /// Serpentine does not weaken this: it negates `dx` only, never `dy`
    /// (see `dither/mod.rs`'s `effective_dx`).
    #[test]
    fn no_kernel_propagates_error_upwards() {
        for algo in ALL_ALGORITHMS {
            let k = algo.kernel();
            // Non-degeneracy: an empty kernel would satisfy the sweep vacuously.
            assert!(!k.entries.is_empty(), "{algo:?} dispatches an empty kernel");
            for &(dx, dy, w) in k.entries {
                assert!(
                    dy >= 0,
                    "{algo:?} has an upward tap ({dx}, {dy}, {w}) — ruling 23's \
                     horizontal containment argument assumes every tap lands on \
                     the current row or below"
                );
            }
        }
    }

    /// Reached through `kernel()` rather than the constants, so this covers
    /// exactly the kernels the crate can actually dispatch.
    const ALL_ALGORITHMS: [crate::DitherAlgorithm; 9] = {
        use crate::DitherAlgorithm::*;
        [
            Atkinson,
            AtkinsonHybrid,
            FloydSteinberg,
            JarvisJudiceNinke,
            Sierra,
            SierraTwoRow,
            SierraLite,
            Stucki,
            Burkes,
        ]
    };

    /// Adding a `DitherAlgorithm` variant must break the build here.
    ///
    /// `ALL_ALGORITHMS` is hand-maintained and would otherwise go stale in
    /// silence — a new kernel with an upward tap would then slip past the
    /// sweep above while every containment test kept passing. This match has
    /// no wildcard arm, so the compiler is what keeps the list honest.
    #[allow(dead_code)]
    fn exhaustiveness_guard(a: crate::DitherAlgorithm) {
        use crate::DitherAlgorithm::*;
        match a {
            Atkinson | AtkinsonHybrid | FloydSteinberg | JarvisJudiceNinke | Sierra
            | SierraTwoRow | SierraLite | Stucki | Burkes => {}
        }
    }

    #[test]
    fn test_atkinson_propagation_75_percent() {
        let sum: u8 = ATKINSON.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 6, "Atkinson should have 6 weight units");
        assert_eq!(ATKINSON.divisor, 8, "Atkinson divisor should be 8");
        // 6/8 = 75% propagation
        assert!(
            (sum as f32 / ATKINSON.divisor as f32 - 0.75).abs() < f32::EPSILON,
            "Atkinson should propagate 75% of error"
        );
    }

    #[test]
    fn test_floyd_steinberg_propagation_100_percent() {
        let sum: u8 = FLOYD_STEINBERG.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 16, "Floyd-Steinberg weights should sum to 16");
        assert_eq!(
            FLOYD_STEINBERG.divisor, 16,
            "Floyd-Steinberg divisor should be 16"
        );
    }

    #[test]
    fn test_jarvis_judice_ninke_propagation_100_percent() {
        let sum: u8 = JARVIS_JUDICE_NINKE.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 48, "JJN weights should sum to 48");
        assert_eq!(JARVIS_JUDICE_NINKE.divisor, 48, "JJN divisor should be 48");
    }

    #[test]
    fn test_sierra_propagation_100_percent() {
        let sum: u8 = SIERRA.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 32, "Sierra weights should sum to 32");
        assert_eq!(SIERRA.divisor, 32, "Sierra divisor should be 32");
    }

    #[test]
    fn test_sierra_two_row_propagation_100_percent() {
        let sum: u8 = SIERRA_TWO_ROW.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 16, "Sierra Two-Row weights should sum to 16");
        assert_eq!(
            SIERRA_TWO_ROW.divisor, 16,
            "Sierra Two-Row divisor should be 16"
        );
    }

    #[test]
    fn test_sierra_lite_propagation_100_percent() {
        let sum: u8 = SIERRA_LITE.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 4, "Sierra Lite weights should sum to 4");
        assert_eq!(SIERRA_LITE.divisor, 4, "Sierra Lite divisor should be 4");
    }

    #[test]
    fn test_atkinson_max_dy() {
        let actual_max_dy = ATKINSON
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(actual_max_dy, ATKINSON.max_dy, "Atkinson max_dy mismatch");
        assert_eq!(ATKINSON.max_dy, 2, "Atkinson reaches 2 rows ahead");
    }

    #[test]
    fn test_floyd_steinberg_max_dy() {
        let actual_max_dy = FLOYD_STEINBERG
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(
            actual_max_dy, FLOYD_STEINBERG.max_dy,
            "Floyd-Steinberg max_dy mismatch"
        );
        assert_eq!(
            FLOYD_STEINBERG.max_dy, 1,
            "Floyd-Steinberg reaches 1 row ahead"
        );
    }

    #[test]
    fn test_jjn_max_dy() {
        let actual_max_dy = JARVIS_JUDICE_NINKE
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(
            actual_max_dy, JARVIS_JUDICE_NINKE.max_dy,
            "JJN max_dy mismatch"
        );
        assert_eq!(JARVIS_JUDICE_NINKE.max_dy, 2, "JJN reaches 2 rows ahead");
    }

    #[test]
    fn test_sierra_max_dy() {
        let actual_max_dy = SIERRA
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(actual_max_dy, SIERRA.max_dy, "Sierra max_dy mismatch");
        assert_eq!(SIERRA.max_dy, 2, "Sierra reaches 2 rows ahead");
    }

    #[test]
    fn test_sierra_two_row_max_dy() {
        let actual_max_dy = SIERRA_TWO_ROW
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(
            actual_max_dy, SIERRA_TWO_ROW.max_dy,
            "Sierra Two-Row max_dy mismatch"
        );
        assert_eq!(
            SIERRA_TWO_ROW.max_dy, 1,
            "Sierra Two-Row reaches 1 row ahead"
        );
    }

    #[test]
    fn test_sierra_lite_max_dy() {
        let actual_max_dy = SIERRA_LITE
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(
            actual_max_dy, SIERRA_LITE.max_dy,
            "Sierra Lite max_dy mismatch"
        );
        assert_eq!(SIERRA_LITE.max_dy, 1, "Sierra Lite reaches 1 row ahead");
    }

    #[test]
    fn test_stucki_propagation_100_percent() {
        let sum: u8 = STUCKI.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 42, "Stucki weights should sum to 42");
        assert_eq!(STUCKI.divisor, 42, "Stucki divisor should be 42");
    }

    #[test]
    fn test_stucki_max_dy() {
        let actual_max_dy = STUCKI
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(actual_max_dy, STUCKI.max_dy, "Stucki max_dy mismatch");
        assert_eq!(STUCKI.max_dy, 2, "Stucki reaches 2 rows ahead");
    }

    #[test]
    fn test_burkes_propagation_100_percent() {
        let sum: u8 = BURKES.entries.iter().map(|(_, _, w)| w).sum();
        assert_eq!(sum, 32, "Burkes weights should sum to 32");
        assert_eq!(BURKES.divisor, 32, "Burkes divisor should be 32");
    }

    #[test]
    fn test_burkes_max_dy() {
        let actual_max_dy = BURKES
            .entries
            .iter()
            .map(|(_, dy, _)| *dy as usize)
            .max()
            .unwrap();
        assert_eq!(actual_max_dy, BURKES.max_dy, "Burkes max_dy mismatch");
        assert_eq!(BURKES.max_dy, 1, "Burkes reaches 1 row ahead");
    }

    #[test]
    fn test_kernel_entry_count() {
        assert_eq!(ATKINSON.entries.len(), 6, "Atkinson should have 6 entries");
        assert_eq!(
            FLOYD_STEINBERG.entries.len(),
            4,
            "Floyd-Steinberg should have 4 entries"
        );
        assert_eq!(
            JARVIS_JUDICE_NINKE.entries.len(),
            12,
            "JJN should have 12 entries"
        );
        assert_eq!(SIERRA.entries.len(), 10, "Sierra should have 10 entries");
        assert_eq!(
            SIERRA_TWO_ROW.entries.len(),
            7,
            "Sierra Two-Row should have 7 entries"
        );
        assert_eq!(
            SIERRA_LITE.entries.len(),
            3,
            "Sierra Lite should have 3 entries"
        );
        assert_eq!(STUCKI.entries.len(), 12, "Stucki should have 12 entries");
        assert_eq!(BURKES.entries.len(), 7, "Burkes should have 7 entries");
    }
}
