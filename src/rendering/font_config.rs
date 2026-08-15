//! Resolved font behaviour for one render.
//!
//! `FontConfig` is pure data describing hinting for a render: a server-side
//! default plus optional per-font-variant overrides. It has no knowledge of
//! Lua directives or the renderer — those are wired in by later tasks. Kept
//! pure so the adaptive default can be tested exhaustively without a render
//! in the loop.

use std::collections::BTreeMap;

/// The hinting engine to use, mirroring `usvg::FontHintingEngine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HintingEngine {
    /// The TrueType or PostScript interpreter, i.e. the hints embedded in the
    /// font itself.
    Interpreter,
    /// The automatic hinter, which adjusts outlines without relying on hints
    /// embedded in the font.
    Auto,
    /// Picks the interpreter for fonts that carry hints and the automatic
    /// hinter for those that don't.
    AutoFallback,
}

/// The basic mode for [`HintingTarget::Smooth`], mirroring
/// `usvg::FontHintingSmoothMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HintingMode {
    /// The standard smooth hinting mode.
    Normal,
    /// Hinting with a lighter touch, meaning less aggressive adjustment in
    /// the horizontal direction.
    Light,
    /// Hinting optimized for subpixel rendering with horizontal LCD layouts.
    Lcd,
    /// Hinting optimized for subpixel rendering with vertical LCD layouts.
    VerticalLcd,
}

/// The rasterization the hinted outline is being prepared for, mirroring
/// `usvg::FontHintingTarget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HintingTarget {
    /// A strong hinting style intended for aliased, monochrome
    /// rasterization.
    Mono,
    /// A hinting style suitable for anti-aliased rasterization.
    Smooth {
        /// The basic mode for smooth hinting.
        mode: HintingMode,
        /// If true, TrueType bytecode may assume that the outline will be
        /// rasterized with vertical supersampling.
        symmetric_rendering: bool,
        /// If true, the hinting engine may not adjust the glyph advance.
        preserve_linear_metrics: bool,
    },
}

/// Font hinting configuration for one render, mirroring
/// `usvg::FontHintingOptions`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HintingSpec {
    /// The hinting engine to use.
    pub engine: HintingEngine,
    /// The rasterization the outline is being prepared for.
    pub target: HintingTarget,
}

impl Default for HintingSpec {
    fn default() -> Self {
        Self {
            engine: HintingEngine::Auto,
            target: HintingTarget::Smooth {
                mode: HintingMode::Normal,
                symmetric_rendering: true,
                preserve_linear_metrics: false,
            },
        }
    }
}

impl HintingSpec {
    /// Maps this spec onto usvg's own hinting options type.
    pub fn to_usvg(&self) -> usvg::FontHintingOptions {
        usvg::FontHintingOptions {
            engine: match self.engine {
                HintingEngine::Interpreter => usvg::FontHintingEngine::Interpreter,
                HintingEngine::Auto => usvg::FontHintingEngine::Auto,
                HintingEngine::AutoFallback => usvg::FontHintingEngine::AutoFallback,
            },
            target: match self.target {
                HintingTarget::Mono => usvg::FontHintingTarget::Mono,
                HintingTarget::Smooth {
                    mode,
                    symmetric_rendering,
                    preserve_linear_metrics,
                } => usvg::FontHintingTarget::Smooth {
                    mode: match mode {
                        HintingMode::Normal => usvg::FontHintingSmoothMode::Normal,
                        HintingMode::Light => usvg::FontHintingSmoothMode::Light,
                        HintingMode::Lcd => usvg::FontHintingSmoothMode::Lcd,
                        HintingMode::VerticalLcd => usvg::FontHintingSmoothMode::VerticalLcd,
                    },
                    symmetric_rendering,
                    preserve_linear_metrics,
                },
            },
        }
    }
}

/// A per-font-variant override.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontVariant {
    /// The font family or file this variant applies to.
    pub font: String,
    /// Override for bitmap font-strike rendering. `None` inherits the
    /// server-side behaviour.
    pub strikes: Option<bool>,
    /// Override for hinting. The outer `None` means "inherit the config's
    /// `default`"; the inner `None` means "hinting explicitly off for this
    /// variant".
    pub hinting: Option<Option<HintingSpec>>,
}

/// The resolved font behaviour for one render: a server-side default plus
/// per-variant overrides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontConfig {
    /// The default hinting applied when a variant doesn't override it.
    /// `None` means hinting is off by default.
    pub default: Option<HintingSpec>,
    /// Per-font-variant overrides, keyed by variant name.
    pub variants: BTreeMap<String, FontVariant>,
}

impl FontConfig {
    /// The server-side default, reproducing what `byonk-base/v1/hinting.svg`
    /// emitted before hinting moved behind a resolver: mono on a
    /// black-and-white panel, smooth once there are greys to anti-alias
    /// with.
    ///
    /// Screens need no Lua to get this. The `font_hinting` directive is a
    /// pure override, so migrating a screen is deleting its
    /// `{% include %}` line and its output is preserved by construction.
    pub fn adaptive_default(grey_count: usize) -> Self {
        let target = if grey_count <= 2 {
            HintingTarget::Mono
        } else {
            HintingTarget::Smooth {
                mode: HintingMode::Normal,
                symmetric_rendering: false,
                preserve_linear_metrics: true,
            }
        };
        Self {
            default: Some(HintingSpec {
                engine: HintingEngine::Auto,
                target,
            }),
            variants: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bw_panels_get_mono_hinting_matching_the_old_partial() {
        let cfg = FontConfig::adaptive_default(2);
        let h = cfg.default.expect("a BW panel must be hinted");
        assert!(matches!(h.target, HintingTarget::Mono));
        assert!(matches!(h.engine, HintingEngine::Auto));
    }

    #[test]
    fn greyscale_panels_get_smooth_hinting() {
        let cfg = FontConfig::adaptive_default(4);
        let h = cfg.default.expect("a greyscale panel must be hinted");
        assert!(
            matches!(h.target, HintingTarget::Smooth { .. }),
            "greyscale must not use mono hinting; the old v1/hinting.svg gated \
             mono on grey_count <= 2"
        );
    }

    #[test]
    fn the_two_defaults_actually_differ() {
        // Without this, both branches returning the same value would pass the
        // two tests above and the adaptivity would be a no-op.
        assert_ne!(
            format!("{:?}", FontConfig::adaptive_default(2).default),
            format!("{:?}", FontConfig::adaptive_default(4).default),
        );
    }

    #[test]
    fn hinting_spec_maps_onto_usvg_faithfully() {
        let spec = HintingSpec {
            engine: HintingEngine::Interpreter,
            target: HintingTarget::Smooth {
                mode: HintingMode::Light,
                symmetric_rendering: true,
                preserve_linear_metrics: true,
            },
        };
        let out = spec.to_usvg();
        assert!(matches!(out.engine, usvg::FontHintingEngine::Interpreter));
        // Assert the target's fields survive, not merely its discriminant —
        // a mapping that dropped `mode` would pass a discriminant-only check.
        match out.target {
            usvg::FontHintingTarget::Smooth {
                mode,
                symmetric_rendering,
                preserve_linear_metrics,
            } => {
                assert!(matches!(mode, usvg::FontHintingSmoothMode::Light));
                assert!(symmetric_rendering);
                assert!(preserve_linear_metrics);
            }
            other => panic!("expected Smooth, got {other:?}"),
        }
    }

    #[test]
    fn hinting_spec_maps_mono_and_auto_fallback() {
        let spec = HintingSpec {
            engine: HintingEngine::AutoFallback,
            target: HintingTarget::Mono,
        };
        let out = spec.to_usvg();
        assert!(matches!(out.engine, usvg::FontHintingEngine::AutoFallback));
        assert!(matches!(out.target, usvg::FontHintingTarget::Mono));
    }
}
