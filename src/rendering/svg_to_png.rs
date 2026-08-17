use crate::error::RenderError;
use crate::models::DisplaySpec;
use crate::rendering::font_config::{FontConfig, HintingSpec};
use eink_dither::{
    DitherAlgorithm, EinkDitherer, GamutMapper, GamutOptions, Palette as EinkPalette,
    Srgb as EinkSrgb,
};

/// Optional per-render tuning overrides (dev mode, script, device config).
///
/// `gamut` belongs here rather than in a separate parameter because gamut
/// mapping runs in the same stage as dithering, against the same palette, and
/// is configured from the same places in the priority chain.
#[derive(Debug, Default)]
pub struct DitherTuning {
    pub serpentine: Option<bool>,
    pub error_clamp: Option<f32>,
    pub chroma_clamp: Option<f32>,
    pub noise_scale: Option<f32>,
    pub strength: Option<f32>,
    /// Gamut mapping knobs. `None` uses [`GamutOptions::default`]; mapping
    /// still only happens where the document marks a continuous-tone region.
    pub gamut: Option<GamutOptions>,
}
use resvg::usvg::{self, Transform};
use std::io::Cursor;
use std::sync::Arc;
use tiny_skia::Pixmap;

/// Renders SVG to PNG with palette-aware dithering for e-ink displays.
///
/// All rendering goes through a single palette-based path. The PNG output
/// format is chosen automatically:
/// - Pure grey palette with ≤4 entries → grayscale color type 0, 2-bit
/// - Pure grey palette with 5-16 entries → grayscale color type 0, 4-bit
/// - Color palette → indexed color type 3 with PLTE chunk
pub struct SvgRenderer {
    /// Font database for text rendering
    fontdb: Arc<fontdb::Database>,
    /// Bitmap strike sizes per face, ascending.
    ///
    /// Computed once at load time rather than per query: `with_face_data`
    /// re-parses the font, and the Lua `fonts` global reads this for every face
    /// on every script run.
    strikes: std::collections::HashMap<fontdb::ID, Vec<u16>>,
}

impl SvgRenderer {
    /// Create a new SVG renderer with fonts loaded from the provided data
    pub fn with_fonts(fonts: Vec<(String, std::borrow::Cow<'static, [u8]>)>) -> Self {
        let mut fontdb = fontdb::Database::new();

        for (name, data) in fonts {
            fontdb.load_font_data(data.into_owned());
            tracing::debug!(font = %name, "Loaded font");
        }

        // Load system fonts as fallback
        fontdb.load_system_fonts();

        // Point the generic families at fonts we actually ship.
        //
        // fontdb defaults these to Arial / Times New Roman / Courier New, none
        // of which byonk bundles. That is invisible on a developer machine —
        // `load_system_fonts` above finds them — but the release image is
        // `FROM scratch`, so on the device nothing matches, usvg skips the text
        // and the screen renders blank. byonk's own `v1/base.svg`,
        // `v1/header.svg`, `v1/footer.svg` and the built-in error screens all
        // ask for `sans-serif`.
        //
        // These must be set AFTER `load_system_fonts()`: on Linux that call
        // parses fontconfig and overwrites the generics with whatever the host
        // aliases them to. Deterministic rendering across dev, CI and the
        // release image is the point.
        // The Source trio are the generics proper: three families designed
        // together, each drawn for its own role, so a screen that asks for
        // `serif` gets a serif rather than the house sans wearing the label.
        // `monospace` in particular used to resolve to Terminus, a bitmap face
        // that only renders as designed at the nine sizes it carries strikes
        // for; Source Code Pro is an outline face and holds at any size.
        fontdb.set_sans_serif_family("Source Sans 3");
        fontdb.set_serif_family("Source Serif 4");
        fontdb.set_monospace_family("Source Code Pro");
        // No Source member is decorative, and an unmapped generic drops out of
        // the `FROM scratch` image entirely, so these stay on the house sans.
        fontdb.set_cursive_family("Outfit");
        fontdb.set_fantasy_family("Outfit");

        tracing::info!(
            font_count = fontdb.len(),
            "Loaded fonts for SVG text rendering"
        );

        let families: std::collections::HashSet<_> = fontdb
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .collect();
        tracing::debug!(families = ?families, "Available font families");

        let ids: Vec<fontdb::ID> = fontdb.faces().map(|f| f.id).collect();
        let strikes = ids
            .into_iter()
            .map(|id| {
                let sizes = fontdb
                    .with_face_data(id, crate::rendering::font_introspection::bitmap_strikes_for)
                    .unwrap_or_default();
                (id, sizes)
            })
            .collect();

        Self {
            fontdb: Arc::new(fontdb),
            strikes,
        }
    }

    /// Access the font database faces
    pub fn font_faces(&self) -> impl Iterator<Item = &fontdb::FaceInfo> {
        self.fontdb.faces()
    }

    /// Bitmap strike sizes for a face, ascending. Empty for outline fonts.
    pub fn bitmap_strikes(&self, id: fontdb::ID) -> &[u16] {
        self.strikes.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Create a new SVG renderer with no custom fonts (system fonts only)
    pub fn new() -> Self {
        Self::with_fonts(Vec::new())
    }

    /// Render SVG to PNG using the given color palette.
    ///
    /// The output format is chosen automatically based on the palette content.
    /// The `dither` parameter selects the dithering algorithm:
    /// - `"photo"` / `"atkinson"` - Atkinson error diffusion (best color accuracy)
    /// - `"atkinson-hybrid"` - Atkinson hybrid (100% achromatic / 75% chromatic)
    /// - `"floyd-steinberg"` - Floyd-Steinberg with blue noise jitter (smooth gradients)
    /// - `"jarvis-judice-ninke"` - JJN error diffusion (wide kernel, least oscillation)
    /// - `"sierra"` - Sierra full error diffusion (wide kernel)
    /// - `"sierra-two-row"` - Sierra two-row error diffusion
    /// - `"sierra-lite"` - Sierra Lite error diffusion (minimal kernel)
    /// - `"graphics"` (default) - Blue noise ordered dithering
    /// - `"simplex"` - Barycentric ordered dithering (up to 4-color blending)
    ///
    /// When `actual` measured colors are provided, the ditherer uses them to model
    /// what the panel really displays. When `use_actual` is true, the PNG output
    /// uses measured colors (for dev mode preview); otherwise official colors are used.
    #[allow(clippy::too_many_arguments)]
    pub fn render_to_palette_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        palette: &[(u8, u8, u8)],
        actual: Option<&[(u8, u8, u8)]>,
        use_actual: bool,
        dither: Option<&str>,
        tuning: Option<&DitherTuning>,
        fonts: Option<&FontConfig>,
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec, fonts)?;

        // Build eink-dither palette with dedup (eink-dither rejects duplicates)
        let (eink_palette, output_palette) = build_eink_palette(palette, actual, use_actual)?;

        // Determine algorithm
        let algorithm = match dither {
            Some(s) if s.eq_ignore_ascii_case("atkinson-hybrid") => DitherAlgorithm::AtkinsonHybrid,
            Some(s) if s.eq_ignore_ascii_case("floyd-steinberg") => DitherAlgorithm::FloydSteinberg,
            Some(s) if s.eq_ignore_ascii_case("jarvis-judice-ninke") => {
                DitherAlgorithm::JarvisJudiceNinke
            }
            Some(s) if s.eq_ignore_ascii_case("sierra") => DitherAlgorithm::Sierra,
            Some(s) if s.eq_ignore_ascii_case("sierra-two-row") => DitherAlgorithm::SierraTwoRow,
            Some(s) if s.eq_ignore_ascii_case("sierra-lite") => DitherAlgorithm::SierraLite,
            Some(s) if s.eq_ignore_ascii_case("stucki") => DitherAlgorithm::Stucki,
            Some(s) if s.eq_ignore_ascii_case("burkes") => DitherAlgorithm::Burkes,
            _ => DitherAlgorithm::Atkinson,
        };

        // Convert RGBA pixmap to eink-dither Srgb pixels
        let mut pixels = rgba_to_eink_srgb(pixmap.data());

        // The tone mask has three consumers: gamut mapping acts INSIDE marked
        // regions; outside them the colour model is nominal and exact matches
        // are pinned; and no error crosses between the two. So it is rasterized
        // whenever the document carries markup, and only the mapping is skipped
        // when amount is zero. An unmarked document skips the second
        // rasterization entirely: every pixel is structure.
        let tone_mask: Option<Vec<bool>> = if crate::rendering::tone_mask::has_tone_markup(svg_data)
        {
            // The same `fonts` as the frame above, deliberately: the mask
            // selects which of the frame's pixels are continuous-tone, so a
            // mask whose text resolved to a different face — or the same face
            // hinted differently — would be offset from what it masks.
            let mask = self.rasterize_tone_mask(svg_data, spec, fonts)?;
            if mask.len() != pixels.len() {
                // Cannot happen: both rasterize to `spec`. Loud rather than
                // silently skipped.
                return Err(RenderError::Dither(format!(
                    "tone mask length {} does not match frame {}",
                    mask.len(),
                    pixels.len()
                )));
            }
            Some(mask)
        } else {
            None
        };

        if let Some(mask) = tone_mask.as_ref() {
            let gamut_opts = tuning.and_then(|t| t.gamut).unwrap_or_default();
            if gamut_opts.amount != 0.0 {
                let marked = mask.iter().filter(|m| **m).count();
                tracing::debug!(
                    marked_pixels = marked,
                    total_pixels = pixels.len(),
                    knee = gamut_opts.knee,
                    amount = gamut_opts.amount,
                    max_compression = gamut_opts.max_compression,
                    "applying gamut mapping to continuous-tone regions"
                );
                GamutMapper::new(&eink_palette).map_frame(&mut pixels, mask, gamut_opts);
            }
        }

        // Passed through unchanged: this IS the tone mask, not its inverse.
        // A document with no markup is all-structure.
        let continuous: Vec<bool> = match tone_mask {
            Some(mask) => mask,
            None => vec![false; pixels.len()],
        };

        // Dither using eink-dither
        let mut ditherer = EinkDitherer::new(eink_palette).algorithm(algorithm);
        if let Some(t) = tuning {
            if let Some(s) = t.serpentine {
                ditherer = ditherer.serpentine(s);
            }
            if let Some(ec) = t.error_clamp {
                ditherer = ditherer.error_clamp(ec);
            }
            if let Some(cc) = t.chroma_clamp {
                ditherer = ditherer.chroma_clamp(cc);
            }
            if let Some(ns) = t.noise_scale {
                ditherer = ditherer.noise_scale(ns);
            }
            if let Some(st) = t.strength {
                ditherer = ditherer.strength(st);
            }
        }
        let result = ditherer.dither_with_regions(
            &pixels,
            spec.width as usize,
            spec.height as usize,
            Some(&continuous),
        );

        // eink-dither indices are into the deduped palette, which matches output_palette
        let indices: Vec<u8> = result.indices().to_vec();

        // Use output_palette for PNG encoding (measured colors in dev mode, official otherwise)
        let out = &output_palette;

        // Choose PNG format and pack pixel data.
        // When use_actual=true, always use indexed PNG so measured colors appear in PLTE.
        let (color_type, bit_depth, plte, packed) = if is_grey_palette(out) && !use_actual {
            if out.len() <= 4 {
                let mapped = map_grey_indices(&indices, out, 3);
                (
                    png::ColorType::Grayscale,
                    png::BitDepth::Two,
                    None,
                    pack_nbits(&mapped, spec.width, 2),
                )
            } else {
                let mapped = map_grey_indices(&indices, out, 15);
                (
                    png::ColorType::Grayscale,
                    png::BitDepth::Four,
                    None,
                    pack_nbits(&mapped, spec.width, 4),
                )
            }
        } else {
            let (depth, bits) = match out.len() {
                0..=2 => (png::BitDepth::One, 1),
                3..=4 => (png::BitDepth::Two, 2),
                5..=16 => (png::BitDepth::Four, 4),
                _ => (png::BitDepth::Eight, 8),
            };
            let plte: Vec<u8> = out.iter().flat_map(|&(r, g, b)| [r, g, b]).collect();
            let packed = if bits == 8 {
                indices
            } else {
                pack_nbits(&indices, spec.width, bits)
            };
            (png::ColorType::Indexed, depth, Some(plte), packed)
        };

        // Encode PNG (fast settings — oxipng will re-compress optimally)
        let png_bytes = encode_png(spec, color_type, bit_depth, plte.as_deref(), &packed)?;

        // Re-compress with oxipng (zopfli + adaptive filter selection)
        let optimized = optimize_png(png_bytes);
        spec.validate_size(optimized.len())?;
        Ok(optimized)
    }

    /// Render SVG straight to a full-color 8-bit RGB PNG, with no e-ink
    /// dithering or palette restriction applied — the pre-dither preview
    /// authoring diagnostics compare the palette-restricted device output
    /// against. Reuses the same rasterize/encode helpers
    /// `render_to_palette_png` uses; skips the `eink-dither` step entirely,
    /// so this never duplicates the dithering logic.
    pub fn render_to_raw_png(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        fonts: Option<&FontConfig>,
    ) -> Result<Vec<u8>, RenderError> {
        let pixmap = self.rasterize_svg(svg_data, spec, fonts)?;
        let rgb: Vec<u8> = rgba_to_eink_srgb(pixmap.data())
            .into_iter()
            .flat_map(|c| c.to_bytes())
            .collect();

        let png_bytes = encode_png(spec, png::ColorType::Rgb, png::BitDepth::Eight, None, &rgb)?;
        Ok(optimize_png(png_bytes))
    }

    /// A [`usvg::FontResolver`] implementing `fonts`' variants and their
    /// hinting and bitmap-strike policy.
    ///
    /// Variants exist because usvg's `select_hinting` and `select_bitmap`
    /// hooks are keyed on face ID, so two runs of text that resolve to the
    /// same face cannot be configured apart. fontdb does not deduplicate
    /// identical font data — loading the same bytes twice yields two distinct
    /// IDs both reporting the same family — so a variant is a second load of
    /// an existing font, reachable from the SVG through a plain
    /// `font-family`: standard markup, no custom attributes.
    ///
    /// The load has to happen here, lazily, rather than eagerly in
    /// [`SvgRenderer::with_fonts`]: variants are declared per script while the
    /// database is built once at startup, so there is nothing to load
    /// eagerly *from*. `select_font` receives `&mut Arc<Database>` precisely
    /// so a resolver may load fonts on demand, and `Source::Binary` is
    /// `Arc`-backed, so the `Arc::make_mut` below duplicates face *metadata*,
    /// not font bytes.
    fn font_resolver(fonts: Option<&FontConfig>) -> usvg::FontResolver<'static> {
        let mut resolver = usvg::FontResolver::default();

        // Face ID -> the variant alias we loaded it for. Populated lazily by
        // `select_font` and read by the other two hooks, which receive only an
        // ID. The hooks are `Fn + Send + Sync`, hence the mutex.
        let aliases: Arc<std::sync::Mutex<std::collections::HashMap<usvg::fontdb::ID, String>>> =
            Default::default();
        // Empty when no variants are declared, which is the ordinary case. The
        // hinting hook below is installed either way, because resolving
        // `AutoFallback` is not a variant feature — see `resolve_auto_fallback`.
        let variants = fonts.map(|c| c.variants.clone()).unwrap_or_default();

        if !variants.is_empty() {
            Self::install_variant_hooks(&mut resolver, &variants, &aliases);
        }

        let hinting_variants = variants;
        let seen = aliases;
        // Face ID -> whether its interpreter has a real program to run.
        // `select_hinting` is called once per glyph and answering re-parses the
        // font, so the answer is computed once per face.
        let interpreter_usable: Arc<
            std::sync::Mutex<std::collections::HashMap<usvg::fontdb::ID, bool>>,
        > = Default::default();
        resolver.select_hinting = Box::new(move |id, _size, global, db| {
            let alias = seen.lock().unwrap().get(&id).cloned();
            let requested = match alias
                .and_then(|a| hinting_variants.get(&a).and_then(|v| v.hinting.clone()))
            {
                // The variant overrides: `Some(spec)` hints it its own way,
                // `None` is hinting explicitly off for this variant.
                Some(over) => over.map(|s| s.to_usvg()),
                // Either not a variant face, or a variant that does not
                // override hinting: inherit the document default, which
                // `global` already carries.
                None => global,
            };
            resolve_auto_fallback(&interpreter_usable, id, db, requested)
        });

        resolver
    }

    /// The `select_font` and `select_bitmap` halves of the resolver, which only
    /// exist to serve declared variants. Split out so [`Self::font_resolver`]
    /// can install the hinting hook unconditionally without nesting the whole
    /// body in an `if`.
    fn install_variant_hooks(
        resolver: &mut usvg::FontResolver<'static>,
        declared: &std::collections::BTreeMap<String, crate::rendering::font_config::FontVariant>,
        aliases: &Arc<std::sync::Mutex<std::collections::HashMap<usvg::fontdb::ID, String>>>,
    ) {
        // (alias, base face) -> the face we loaded for it, so repeated text
        // runs asking for the same variant share one face instead of adding
        // another copy of the font to the database each time.
        let loaded_for: Arc<
            std::sync::Mutex<
                std::collections::HashMap<(String, usvg::fontdb::ID), usvg::fontdb::ID>,
            >,
        > = Default::default();

        let variants = declared.clone();
        let seen = aliases.clone();
        let base_selector = usvg::FontResolver::default_font_selector();
        resolver.select_font = Box::new(move |font, db| {
            for family in font.families() {
                let usvg::FontFamily::Named(name) = family else {
                    continue;
                };
                let Some(variant) = variants.get(name.as_str()) else {
                    continue;
                };

                // Resolve the variant's base face with the *requesting*
                // element's style, weight and stretch, so a variant of a
                // family with several faces still picks the right one.
                let query = usvg::fontdb::Query {
                    families: &[usvg::fontdb::Family::Name(&variant.font)],
                    weight: usvg::fontdb::Weight(font.weight()),
                    stretch: to_fontdb_stretch(font.stretch()),
                    style: to_fontdb_style(font.style()),
                };
                let Some(base) = db.query(&query) else {
                    continue;
                };

                let key = (name.clone(), base);
                if let Some(id) = loaded_for.lock().unwrap().get(&key) {
                    return Some(*id);
                }

                let Some((source, index)) = db.face_source(base) else {
                    continue;
                };
                // A second load of the same bytes yields a second face ID —
                // fontdb does not deduplicate identical font data — and that
                // is what gives the variant its own hinting and strike config.
                let loaded = usvg::fontdb::Database::load_font_source(Arc::make_mut(db), source);
                let Some(id) = loaded.get(index as usize).copied() else {
                    continue;
                };
                seen.lock().unwrap().insert(id, name.clone());
                loaded_for.lock().unwrap().insert(key, id);
                return Some(id);
            }
            base_selector(font, db)
        });

        let variants = declared.clone();
        let seen = aliases.clone();
        resolver.select_bitmap = Box::new(move |id, _size, _db| {
            let alias = seen.lock().unwrap().get(&id).cloned();
            alias
                .and_then(|a| variants.get(&a).and_then(|v| v.strikes))
                .unwrap_or(true) // resvg's default: strikes are used
        });
    }

    /// The parse options for one render.
    ///
    /// Shared by the frame and the tone mask on purpose: the mask selects which
    /// of the frame's pixels are continuous-tone, so text that resolved to a
    /// different face — or the same face hinted or rasterized differently —
    /// would leave the mask offset from what it masks. One function means the
    /// two cannot drift apart.
    fn parse_options(&self, fonts: Option<&FontConfig>) -> usvg::Options<'static> {
        usvg::Options {
            fontdb: self.fontdb.clone(),
            font_hinting: fonts
                .and_then(|f| f.default.as_ref())
                .map(HintingSpec::to_usvg),
            // The *default* text-rendering, which is what carries glyph
            // aliasing into resvg. usvg only consults it where the document
            // says nothing, so an SVG setting its own `text-rendering` wins.
            text_rendering: fonts
                .map(FontConfig::text_rendering_default)
                .unwrap_or_default(),
            font_resolver: Self::font_resolver(fonts),
            ..Default::default()
        }
    }

    /// Parse and rasterize SVG to an RGBA pixmap
    fn rasterize_svg(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        fonts: Option<&FontConfig>,
    ) -> Result<Pixmap, RenderError> {
        let options = self.parse_options(fonts);
        let tree = usvg::Tree::from_data(svg_data, &options)
            .map_err(|e| RenderError::SvgParse(e.to_string()))?;

        let svg_size = tree.size();
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::WHITE);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap)
    }

    /// The scale-and-centre transform that fits an SVG of `svg_w` x `svg_h`
    /// into `spec`.
    ///
    /// Shared by the frame and the tone mask: if these two ever disagreed the
    /// mask would be offset from the pixels it selects. Takes plain floats
    /// rather than a size type so it does not depend on which crate
    /// `usvg::Tree::size` currently returns.
    fn fit_transform(svg_w: f32, svg_h: f32, spec: DisplaySpec) -> Transform {
        let scale = (spec.width as f32 / svg_w).min(spec.height as f32 / svg_h);
        let offset_x = (spec.width as f32 - svg_w * scale) / 2.0;
        let offset_y = (spec.height as f32 - svg_h * scale) / 2.0;
        Transform::from_scale(scale, scale).post_translate(offset_x, offset_y)
    }

    /// Rasterize the tone mask for `svg_data`.
    ///
    /// The mask document is the original with paint forced to white inside
    /// `data-byonk-tone="continuous"` subtrees and black elsewhere, drawn over
    /// a black background so unpainted area reads as unmarked. Edge
    /// antialiasing produces greys; threshold at 0.5.
    ///
    /// Failure is a hard error, deliberately. The mask comes from a document
    /// that just rasterized successfully, by the same renderer, with only
    /// paint values changed — so the realistic failure paths are all our own
    /// bugs in the rewriter. Silently rendering something materially different
    /// while reporting success is the failure mode that costs hours.
    fn rasterize_tone_mask(
        &self,
        svg_data: &[u8],
        spec: DisplaySpec,
        fonts: Option<&FontConfig>,
    ) -> Result<Vec<bool>, RenderError> {
        let mask_svg = crate::rendering::tone_mask::build_mask_svg(svg_data)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let options = self.parse_options(fonts);
        let tree = usvg::Tree::from_data(&mask_svg, &options)
            .map_err(|e| RenderError::SvgParse(format!("tone mask: {e}")))?;

        let svg_size = tree.size();
        let transform = Self::fit_transform(svg_size.width(), svg_size.height(), spec);

        let mut pixmap =
            Pixmap::new(spec.width, spec.height).ok_or(RenderError::PixmapAllocation)?;
        pixmap.fill(tiny_skia::Color::BLACK);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Ok(pixmap
            .data()
            .chunks_exact(4)
            .map(|px| {
                // Premultiplied RGBA over an opaque black fill: the green
                // channel alone separates white from black cleanly.
                px[1] >= 128
            })
            .collect())
    }
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve usvg's `AutoFallback` hinting engine for one face.
///
/// `AutoFallback` is documented as picking the interpreter for fonts that carry
/// hints and the automatic hinter for those that don't. skrifa decides it by
/// asking only whether `fpgm` or `prep` is non-empty, so a face whose entire
/// program is the seven-byte dropout-control stub modern build tools emit gets
/// the interpreter — which then has nothing to run, and the text comes out
/// unhinted while the caller believes it asked for a fallback. Byonk answers
/// the question the documentation poses and substitutes `Auto` where the
/// interpreter would idle.
///
/// This can only move a face towards the automatic hinter, never away from it,
/// so it cannot override an author who asked for `Interpreter` outright.
///
/// The answer is memoised per face because usvg calls `select_hinting` once per
/// glyph while reading it re-parses the font. Upstream will not change the
/// selection rule — skrifa matches FreeType deliberately and Skia depends on
/// that parity — so this is byonk's to resolve.
fn resolve_auto_fallback(
    interpreter_usable: &std::sync::Mutex<std::collections::HashMap<usvg::fontdb::ID, bool>>,
    id: usvg::fontdb::ID,
    db: &usvg::fontdb::Database,
    requested: Option<usvg::FontHintingOptions>,
) -> Option<usvg::FontHintingOptions> {
    let mut opts = requested?;
    if !matches!(opts.engine, usvg::FontHintingEngine::AutoFallback) {
        return Some(opts);
    }

    let usable = *interpreter_usable
        .lock()
        .unwrap()
        .entry(id)
        .or_insert_with(|| {
            db.with_face_data(
                id,
                crate::rendering::font_introspection::has_interpreter_hinting,
            )
            // A face whose bytes cannot be read is one byonk cannot claim
            // carries hints, so it falls back rather than idling.
            .unwrap_or(false)
        });

    if !usable {
        opts.engine = usvg::FontHintingEngine::Auto;
    }
    Some(opts)
}

/// usvg's font stretch as fontdb spells it, so a variant's base face is
/// queried with the same stretch the requesting element asked for.
fn to_fontdb_stretch(stretch: usvg::FontStretch) -> usvg::fontdb::Stretch {
    match stretch {
        usvg::FontStretch::UltraCondensed => usvg::fontdb::Stretch::UltraCondensed,
        usvg::FontStretch::ExtraCondensed => usvg::fontdb::Stretch::ExtraCondensed,
        usvg::FontStretch::Condensed => usvg::fontdb::Stretch::Condensed,
        usvg::FontStretch::SemiCondensed => usvg::fontdb::Stretch::SemiCondensed,
        usvg::FontStretch::Normal => usvg::fontdb::Stretch::Normal,
        usvg::FontStretch::SemiExpanded => usvg::fontdb::Stretch::SemiExpanded,
        usvg::FontStretch::Expanded => usvg::fontdb::Stretch::Expanded,
        usvg::FontStretch::ExtraExpanded => usvg::fontdb::Stretch::ExtraExpanded,
        usvg::FontStretch::UltraExpanded => usvg::fontdb::Stretch::UltraExpanded,
    }
}

/// usvg's font style as fontdb spells it. See [`to_fontdb_stretch`].
fn to_fontdb_style(style: usvg::FontStyle) -> usvg::fontdb::Style {
    match style {
        usvg::FontStyle::Normal => usvg::fontdb::Style::Normal,
        usvg::FontStyle::Italic => usvg::fontdb::Style::Italic,
        usvg::FontStyle::Oblique => usvg::fontdb::Style::Oblique,
    }
}

/// Convert RGBA pixel data to eink-dither Srgb, alpha-compositing against white.
fn rgba_to_eink_srgb(rgba_data: &[u8]) -> Vec<EinkSrgb> {
    rgba_data
        .chunks_exact(4)
        .map(|pixel| {
            let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
            if a == 255 {
                EinkSrgb::from_u8(r, g, b)
            } else if a == 0 {
                EinkSrgb::from_u8(255, 255, 255)
            } else {
                // Alpha composite against white
                let af = a as u16;
                let cr = ((r as u16 * af + 255 * (255 - af)) / 255) as u8;
                let cg = ((g as u16 * af + 255 * (255 - af)) / 255) as u8;
                let cb = ((b as u16 * af + 255 * (255 - af)) / 255) as u8;
                EinkSrgb::from_u8(cr, cg, cb)
            }
        })
        .collect()
}

/// Build an eink-dither palette from byonk's (u8,u8,u8) palette, deduplicating
/// colors since eink-dither rejects palettes with duplicate entries.
///
/// When `actual` measured colors are provided, they're passed to eink-dither so
/// dithering targets what the panel really displays. B&W forcing: if an official
/// color is exactly black or white, the measured value is forced to match.
///
/// Returns (eink_palette, output_palette) where output_palette uses
/// measured colors when `use_actual` is true, otherwise official colors.
type RgbTuple = (u8, u8, u8);

fn build_eink_palette(
    palette: &[RgbTuple],
    actual: Option<&[RgbTuple]>,
    use_actual: bool,
) -> Result<(EinkPalette, Vec<RgbTuple>), RenderError> {
    // Build actual colors with B&W forcing
    let actual_with_bw: Option<Vec<(u8, u8, u8)>> = actual.map(|a| {
        a.iter()
            .enumerate()
            .map(|(i, &(ar, ag, ab))| {
                if i < palette.len() {
                    let (or, og, ob) = palette[i];
                    // Force measured to match if official is pure black or white
                    if or == 0 && og == 0 && ob == 0 {
                        (0, 0, 0)
                    } else if or == 255 && og == 255 && ob == 255 {
                        (255, 255, 255)
                    } else {
                        (ar, ag, ab)
                    }
                } else {
                    (ar, ag, ab)
                }
            })
            .collect()
    });

    let mut unique_official: Vec<EinkSrgb> = Vec::new();
    let mut unique_actual: Vec<EinkSrgb> = Vec::new();
    // Track which original indices survived dedup, for building output_palette
    let mut kept_indices: Vec<usize> = Vec::new();

    for (orig_idx, &(r, g, b)) in palette.iter().enumerate() {
        let color = EinkSrgb::from_u8(r, g, b);
        let bytes = color.to_bytes();
        if !unique_official.iter().any(|c| c.to_bytes() == bytes) {
            kept_indices.push(orig_idx);
            unique_official.push(color);
            // Track corresponding actual color
            if let Some(ref abw) = actual_with_bw {
                if orig_idx < abw.len() {
                    let (ar, ag, ab) = abw[orig_idx];
                    unique_actual.push(EinkSrgb::from_u8(ar, ag, ab));
                }
            }
        }
    }

    let eink_actual = if !unique_actual.is_empty() && unique_actual.len() == unique_official.len() {
        Some(unique_actual.as_slice())
    } else {
        // Never fail a device render over calibration — but do not lose it
        // silently either. After the length check in
        // `api::display::resolve_measured_colors` this should be unreachable
        // from the script path; if it fires, something upstream disagrees
        // about palette length (e.g. dedup removed a duplicate official
        // colour without a matching measured entry).
        if actual.is_some() {
            tracing::warn!(
                official = unique_official.len(),
                measured = unique_actual.len(),
                "measured colours dropped: length disagrees with the deduplicated \
                 official palette; dithering will target the official colours"
            );
        }
        None
    };

    let eink_palette = EinkPalette::new(&unique_official, eink_actual)
        .map_err(|e| RenderError::Dither(format!("palette error: {e}")))?;

    // Build output palette: raw measured colors for dev preview, official for production.
    // Note: we use `actual` (without B&W forcing) for output so users see real panel colors.
    let output_palette: Vec<(u8, u8, u8)> = if use_actual {
        if let Some(a) = actual {
            kept_indices
                .iter()
                .map(|&i| if i < a.len() { a[i] } else { palette[i] })
                .collect()
        } else {
            kept_indices.iter().map(|&i| palette[i]).collect()
        }
    } else {
        kept_indices.iter().map(|&i| palette[i]).collect()
    };

    Ok((eink_palette, output_palette))
}

/// Check if a palette consists entirely of grey values (R == G == B).
fn is_grey_palette(palette: &[(u8, u8, u8)]) -> bool {
    palette.iter().all(|&(r, g, b)| r == g && g == b)
}

/// Map palette indices to native grayscale values (0..max_val).
fn map_grey_indices(indices: &[u8], palette: &[(u8, u8, u8)], max_val: u32) -> Vec<u8> {
    let max_level = (palette.len() - 1) as u32;
    let lut: Vec<u8> = (0..palette.len())
        .map(|i| ((i as u32 * max_val + max_level / 2) / max_level).min(max_val) as u8)
        .collect();
    indices.iter().map(|&idx| lut[idx as usize]).collect()
}

/// Encode packed pixel data as a PNG.
fn encode_png(
    spec: DisplaySpec,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    plte: Option<&[u8]>,
    packed: &[u8],
) -> Result<Vec<u8>, RenderError> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut buf, spec.width, spec.height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::FilterType::NoFilter);
        if let Some(plte) = plte {
            encoder.set_palette(plte);
        }
        let mut writer = encoder
            .write_header()
            .map_err(|e| RenderError::PngEncode(e.to_string()))?;
        writer
            .write_image_data(packed)
            .map_err(|e| RenderError::PngEncode(e.to_string()))?;
    }
    Ok(buf.into_inner())
}

/// Re-compress a PNG with oxipng (zopfli + adaptive filter selection).
/// Falls back to the input bytes unchanged if optimization fails. Shared by
/// `render_to_palette_png` (dithered device output) and `render_to_raw_png`
/// (pre-dither preview) — both encode fast/uncompressed via `encode_png`
/// and rely on this pass for the real compression.
fn optimize_png(png_bytes: Vec<u8>) -> Vec<u8> {
    oxipng::optimize_from_memory(
        &png_bytes,
        &oxipng::Options {
            strip: oxipng::StripChunks::Safe,
            optimize_alpha: false,
            ..Default::default()
        },
    )
    .unwrap_or(png_bytes)
}

/// Pack pixel values into N-bit PNG row data (1, 2, or 4 bits per pixel).
fn pack_nbits(indices: &[u8], width: u32, bits: u8) -> Vec<u8> {
    let pixels_per_byte = 8 / bits as usize;
    let bytes_per_row = (width as usize).div_ceil(pixels_per_byte);
    let height = indices.len() / width as usize;
    let mask = (1u8 << bits) - 1;
    let mut packed = Vec::with_capacity(bytes_per_row * height);

    for row in indices.chunks(width as usize) {
        let mut byte = 0u8;
        for (i, &idx) in row.iter().enumerate() {
            let shift = (8 - bits) - (i % pixels_per_byte) as u8 * bits;
            byte |= (idx & mask) << shift;

            if (i % pixels_per_byte) == pixels_per_byte - 1 || i == row.len() - 1 {
                packed.push(byte);
                byte = 0;
            }
        }
    }

    packed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::font_config::{
        FontConfig, FontVariant, HintingEngine, HintingMode, HintingSpec, HintingTarget,
    };

    #[test]
    fn build_eink_palette_drops_mismatched_actual_but_still_builds() {
        // Three official colours, two measured: the measured list is dropped
        // (never fail a device render) — but this must not be silent.
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224)];
        let (_palette, output) =
            build_eink_palette(&official, Some(&actual), false).expect("must still build");
        assert_eq!(output, official);
    }

    #[test]
    fn build_eink_palette_keeps_matched_actual() {
        let official = vec![(0, 0, 0), (255, 255, 255), (255, 0, 0)];
        let actual = vec![(10, 10, 10), (232, 230, 224), (168, 58, 48)];
        let (palette, output) =
            build_eink_palette(&official, Some(&actual), true).expect("must build");
        // use_actual = true draws the output in the measured colours, except
        // that pure black/white are forced to match — the existing B&W rule
        // applies to the dither palette, while `output` uses raw measured.
        assert_eq!(output, actual);

        // The regression this test exists to catch: matched measured
        // colours must actually reach the `EinkPalette` that dithering
        // matches against, not just the display-facing `output` tuple
        // (which is computed independently of `eink_actual`). Assert on
        // `palette.actual(idx)` directly — the B&W-forced entries (0, 1)
        // collapse to pure black/white, while entry 2 carries the raw
        // measured red through unchanged.
        assert_eq!(palette.actual(0), EinkSrgb::from_u8(0, 0, 0));
        assert_eq!(palette.actual(1), EinkSrgb::from_u8(255, 255, 255));
        assert_eq!(palette.actual(2), EinkSrgb::from_u8(168, 58, 48));
    }

    #[test]
    fn generic_families_resolve_to_bundled_fonts() {
        use fontdb::{Family, Query};

        // Build the renderer the way production does: fonts byonk actually
        // ships, loaded via `with_fonts` (which also falls back to
        // `load_system_fonts`, matching the real code path).
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts.clone());
        let db = &renderer.fontdb;

        // The bundled families, computed independently of `db` above: load
        // *only* byonk's shipped font bytes into a scratch database, with no
        // `load_system_fonts()` fallback. If we instead derived "bundled"
        // from `db.faces()`, the check would be tautological — `db` already
        // contains every system font too (via `with_fonts`'s fallback), so
        // any face `query()` can possibly return is trivially a member of
        // its own face list, and the assertion could never fail regardless
        // of which family the generics actually resolve to.
        let mut bundled_only = fontdb::Database::new();
        for (_, data) in &fonts {
            bundled_only.load_font_data(data.clone().into_owned());
        }
        let bundled: std::collections::HashSet<String> = bundled_only
            .faces()
            .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
            .collect();

        // Named, not merely "something bundled": with only the membership
        // check below, the mapping could point every generic at one house font
        // and still pass. `cursive` and `fantasy` have no Source member and
        // stay on Outfit deliberately — a decorative generic has no better
        // answer here, and leaving them unmapped would drop them out of the
        // release image entirely.
        for (generic, expected) in [
            (Family::SansSerif, "Source Sans 3"),
            (Family::Serif, "Source Serif 4"),
            (Family::Monospace, "Source Code Pro"),
            (Family::Cursive, "Outfit"),
            (Family::Fantasy, "Outfit"),
        ] {
            let id = db
                .query(&Query {
                    families: &[generic],
                    ..Default::default()
                })
                .unwrap_or_else(|| panic!("{generic:?} did not resolve at all"));

            let family = db
                .face(id)
                .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
                .expect("resolved face must have a family name");

            assert!(
                bundled.contains(&family),
                "{generic:?} resolved to {family:?}, which is not a bundled font; \
                 it would not resolve in the FROM scratch release image"
            );

            assert_eq!(
                family, expected,
                "{generic:?} resolved to {family:?}, not the {expected:?} byonk \
                 maps it to"
            );
        }
    }

    #[test]
    fn test_bitmap_font_families() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        let mut x11_families: Vec<_> = renderer
            .fontdb
            .faces()
            .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
            .filter(|name| name.starts_with("X11"))
            .collect();
        x11_families.sort();
        x11_families.dedup();
        for fam in &x11_families {
            println!("fontdb family: {}", fam);
        }
        assert!(!x11_families.is_empty(), "No X11 font families found");
    }

    #[test]
    fn test_bitmap_strikes_exposed() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        // X11Helv should have bitmap strikes
        let x11_face = renderer
            .font_faces()
            .find(|f| f.families.first().map(|(n, _)| n.as_str()) == Some("X11Helv"))
            .expect("X11Helv face not found");

        let strikes = renderer.bitmap_strikes(x11_face.id);

        assert!(!strikes.is_empty(), "X11Helv should have bitmap strikes");
        // Strikes should be sorted
        for w in strikes.windows(2) {
            assert!(w[0] <= w[1], "bitmap_strikes should be sorted");
        }
        println!("X11Helv bitmap strikes: {:?}", strikes);
    }

    #[test]
    fn test_bitmap_font_render() {
        let loader = crate::assets::AssetLoader::new(None, None, None);
        let fonts = loader.get_fonts();
        let renderer = SvgRenderer::with_fonts(fonts);

        // Check what fontdb knows about X11Helv
        for face in renderer.fontdb.faces() {
            if let Some((name, _)) = face.families.first() {
                if name == "X11Helv" {
                    println!(
                        "Face: {} | style={:?} weight={:?} | source={:?}",
                        name, face.style, face.weight, face.source
                    );
                }
            }
        }

        // Render with bitmap fonts — font-size selects the bitmap strike
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 200" width="800" height="200">
          <rect width="800" height="200" fill="white"/>
          <text x="20" y="30" font-family="X11Helv" font-size="8" fill="black">X11Helv 8px: Hello World</text>
          <text x="20" y="60" font-family="NONEXISTENT_FONT" font-size="14" fill="black">NONEXISTENT: Hello World</text>
          <text x="20" y="90" font-family="X11Helv" font-size="14" fill="black">X11Helv 14px: Hello World</text>
        </svg>"#;

        let spec = DisplaySpec::from_dimensions(800, 200).unwrap();
        let palette = vec![(0, 0, 0), (255, 255, 255)];
        let png = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                None,
                None,
            )
            .unwrap();
        std::fs::write("/tmp/byonk-bitmap-font-test2.png", &png).unwrap();
        println!(
            "Wrote /tmp/byonk-bitmap-font-test2.png ({} bytes)",
            png.len()
        );
    }

    #[test]
    fn tone_mask_marks_only_the_marked_region() {
        let renderer = SvgRenderer::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="50" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec, None)
            .expect("mask must rasterize");

        assert_eq!(mask.len(), 100 * 100);
        assert!(mask[50 * 100 + 25], "left half must be marked");
        assert!(!mask[50 * 100 + 75], "right half must not be marked");
    }

    #[test]
    fn tone_mask_respects_occlusion_by_unmarked_shapes() {
        let renderer = SvgRenderer::new();
        // A marked photo area with an unmarked label drawn over its middle.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#336699"/>
            </g>
            <rect x="40" y="40" width="20" height="20" fill="#000000"/>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec, None)
            .unwrap();
        assert!(mask[10 * 100 + 10], "photo area must be marked");
        assert!(
            !mask[50 * 100 + 50],
            "the occluding unmarked rect must punch through the mask"
        );
    }

    /// A camelCase attribute must survive the rewrite. XML attribute names are
    /// case-sensitive, so emitting `viewbox` silently drops the coordinate
    /// system and the mask rasterizes empty — while every test whose viewBox
    /// matches its width/height still passes.
    #[test]
    fn tone_mask_preserves_camelcase_attributes() {
        let renderer = SvgRenderer::new();
        // viewBox deliberately differs from width/height.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="400" height="400">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="50" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        let mask_doc = crate::rendering::tone_mask::build_mask_svg(svg.as_bytes()).unwrap();
        let text = String::from_utf8_lossy(&mask_doc);
        assert!(
            text.contains("viewBox"),
            "viewBox must keep its case: {text}"
        );

        let spec = DisplaySpec::from_dimensions(400, 400).unwrap();
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec, None)
            .unwrap();
        let row = 200usize;
        let last = (0..400).rev().find(|&x| mask[row * 400 + x]);
        assert_eq!(
            last,
            Some(199),
            "the marked half must scale with the viewBox"
        );
    }

    /// The mask must not invent a stroke. This is the only kind of test that
    /// can catch it: the rewriter's own tests assert on the mask *document*,
    /// and an added `stroke` is only visible once the document is rasterized.
    ///
    /// Measured before the fix: case A marked 50..=150 and case B 40..=159.
    #[test]
    fn tone_mask_edge_does_not_spill_past_an_unstroked_shape() {
        let renderer = SvgRenderer::new();
        let spec = DisplaySpec::from_dimensions(200, 200).unwrap();
        let span = |svg: &str| {
            let mask = renderer
                .rasterize_tone_mask(svg.as_bytes(), spec, None)
                .unwrap();
            let row = 100usize;
            let first = (0..200).find(|&x| mask[row * 200 + x]).unwrap();
            let last = (0..200).rev().find(|&x| mask[row * 200 + x]).unwrap();
            (first, last)
        };

        // A: a plain unstroked shape. SVG's initial stroke is `none`, so the
        // mask edge must land exactly on the geometry.
        let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(span(plain), (50, 149), "an unstroked shape must not widen");

        // B: `stroke` is a paint property, so the stylesheet's `stroke: none`
        // is stripped — while `stroke-width` survives as geometry. Inventing a
        // stroke here would widen the mask by half of it, unboundedly.
        let css_width = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <style>.p { stroke: none; stroke-width: 20; }</style>
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect class="p" x="50" y="50" width="100" height="100" fill="#336699"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(css_width),
            (50, 149),
            "a stripped stroke must not resurrect via stroke-width"
        );

        // C: the control. A shape that genuinely IS stroked must still mark its
        // stroke, or the fix would just be deleting strokes.
        let stroked = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="200" height="200">
            <rect width="200" height="200" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="50" y="50" width="100" height="100" fill="#336699" stroke="#336699" stroke-width="20"/>
            </g>
          </svg>"##;
        assert_eq!(
            span(stroked),
            (40, 159),
            "a real stroke must still be marked"
        );
    }

    /// The opt-in guarantee: an unmarked document must render byte-identically
    /// whether or not the gamut knobs are present.
    #[test]
    fn unmarked_document_renders_byte_identically() {
        let renderer = SvgRenderer::new();
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <rect x="10" y="10" width="60" height="60" fill="#c06020"/>
          </svg>"##;
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let plain = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                None,
                None,
            )
            .unwrap();

        let tuning = DitherTuning {
            gamut: Some(eink_dither::GamutOptions::default()),
            ..Default::default()
        };
        let with_knobs = renderer
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                Some(&tuning),
                None,
            )
            .unwrap();

        assert_eq!(plain, with_knobs, "unmarked document must be unaffected");
    }

    /// A marked vivid region must actually change.
    #[test]
    fn marked_region_is_altered_by_mapping() {
        let renderer = SvgRenderer::new();
        let marked = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <rect width="100" height="100" fill="#ffffff"/>
            <g data-byonk-tone="continuous">
              <rect x="0" y="0" width="100" height="100" fill="#ff00aa"/>
            </g>
          </svg>"##;
        let unmarked = marked.replace(r#" data-byonk-tone="continuous""#, "");
        let spec = DisplaySpec::from_dimensions(100, 100).unwrap();
        let palette = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
        ];

        let a = renderer
            .render_to_palette_png(
                marked.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                None,
                None,
            )
            .unwrap();
        let b = renderer
            .render_to_palette_png(
                unmarked.as_bytes(),
                spec,
                &palette,
                None,
                false,
                None,
                None,
                None,
            )
            .unwrap();

        assert_ne!(a, b, "marking a vivid region must change the output");
    }

    // ---- Tone-mask fixtures shared by the pinning/containment tests ----

    /// The panel's six nominal inks, in the order byonk ships them.
    fn six_ink_official() -> Vec<(u8, u8, u8)> {
        vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (255, 255, 0),
            (0, 0, 255),
            (0, 255, 0),
        ]
    }

    /// A 32x32 document: a saturated field with one 1 px vertical line of
    /// `line` at x = 15, optionally marked continuous.
    ///
    /// `#FF00AA` is far from every ink in `six_ink_official`, so it diffuses
    /// hard into the line — measured below: an *unpinnable* line here keeps
    /// only 12.5% of its colour.
    fn line_in_hostile_field(line: &str, marked: bool) -> String {
        let attr = if marked {
            r#" data-byonk-tone="continuous""#
        } else {
            ""
        };
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
<rect x="0" y="0" width="32" height="32" fill="#FF00AA"/>
<rect{attr} x="15" y="0" width="1" height="32" fill="{line}"/>
</svg>"##
        )
    }

    /// Fraction of the 1 px line in `line_in_hostile_field` that came out
    /// pure black.
    fn line_black_share(svg: &str) -> f64 {
        let spec = DisplaySpec {
            width: 32,
            height: 32,
            max_size_bytes: 200_000,
        };
        let png = SvgRenderer::new()
            .render_to_palette_png(
                svg.as_bytes(),
                spec,
                &six_ink_official(),
                None,
                false,
                None,
                None,
                None,
            )
            .expect("render failed");
        let img = image::load_from_memory(&png)
            .expect("decode failed")
            .to_rgb8();
        let black = (0..32u32)
            .filter(|&y| img.get_pixel(15, y).0 == [0, 0, 0])
            .count();
        black as f64 / 32.0
    }

    /// Structural content keeps its pure inks (ruling 18's pinning), and a
    /// marked region is shielded from the field's error (ruling 23's
    /// containment).
    ///
    /// Three renders of the same geometry — a 1 px line through a field that
    /// diffuses hard into it — differing only in the line's fill (`#000000`
    /// vs `#010101`, one 8-bit step apart and visually identical) and in
    /// whether the line is marked:
    ///
    /// - **pure, unmarked**: pinnable, so it must survive.
    /// - **near-black, unmarked**: the control. Same geometry, same field,
    ///   but `(1,1,1)` is not a palette entry so it cannot be pinned. It must
    ///   still be eroded — that is what proves the pure line *needed*
    ///   rescuing and that pinning, not the geometry, is what rescued it.
    /// - **near-black, marked**: differs from the control only by the mark.
    ///   Still unpinnable, so its survival can only come from error
    ///   containment at the region boundary.
    ///
    /// Note the marked arm is NOT a "must be eroded" control: containment
    /// protects a marked region from outside error too. See the task report.
    #[test]
    fn structural_pure_ink_is_pinned_and_marked_regions_are_shielded() {
        let pure_unmarked = line_black_share(&line_in_hostile_field("#000000", false));
        let near_unmarked = line_black_share(&line_in_hostile_field("#010101", false));
        let near_marked = line_black_share(&line_in_hostile_field("#010101", true));

        assert!(
            near_unmarked < 0.5,
            "the unpinnable control line kept {:.1}% of its black, so this field does not \
             erode anything and the test cannot attribute the pure line's survival to \
             pinning — report it",
            near_unmarked * 100.0
        );
        assert!(
            pure_unmarked > 0.99,
            "only {:.1}% of the unmarked pure-black line stayed black (the unpinnable \
             control in the same geometry kept {:.1}%) — it is not being pinned",
            pure_unmarked * 100.0,
            near_unmarked * 100.0
        );
        assert!(
            near_marked > 0.99,
            "the marked line kept only {:.1}% of its black against the unmarked control's \
             {:.1}% — error is still crossing into the marked region",
            near_marked * 100.0,
            near_unmarked * 100.0
        );
    }

    /// Unmarked content is matched against the NOMINAL inks; marked content
    /// against the MEASURED ones (ruling 22).
    ///
    /// The fill is `#00FE00` — one 8-bit step off the nominal green — so it is
    /// **not** a palette entry and can never be pinned. Whatever difference the
    /// two arms show is therefore the colour model and nothing else.
    ///
    /// The marked arm sets `gamut.amount = 0.0` so that gamut mapping, the
    /// mask's other consumer, cannot contribute to the difference either.
    #[test]
    fn unmarked_content_is_matched_against_nominal_inks() {
        let official = six_ink_official();
        // byonk's shipped measured inks for this panel.
        let measured: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (0xB5, 0x03, 0x03),
            (0xFF, 0xEE, 0x00),
            (0x20, 0x54, 0x97),
            (0x0D, 0x87, 0x6B),
        ];
        assert_ne!(
            official[5], measured[5],
            "the two colour models are identical for green, so this test cannot see the switch"
        );

        let green_share = |marked: bool| -> f64 {
            let attr = if marked {
                r#" data-byonk-tone="continuous""#
            } else {
                ""
            };
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
<rect{attr} x="0" y="0" width="32" height="32" fill="#00FE00"/>
</svg>"##
            );
            let tuning = DitherTuning {
                gamut: Some(GamutOptions {
                    amount: 0.0,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let spec = DisplaySpec {
                width: 32,
                height: 32,
                max_size_bytes: 200_000,
            };
            let png = SvgRenderer::new()
                .render_to_palette_png(
                    svg.as_bytes(),
                    spec,
                    &official,
                    Some(&measured),
                    false,
                    None,
                    Some(&tuning),
                    None,
                )
                .expect("render failed");
            let img = image::load_from_memory(&png)
                .expect("decode failed")
                .to_rgb8();
            let green = img.pixels().filter(|p| p.0 == [0, 255, 0]).count();
            green as f64 / (32.0 * 32.0)
        };

        let unmarked = green_share(false);
        let marked = green_share(true);

        assert!(
            marked < 0.10,
            "the marked arm came out {:.1}% green, so measured matching is not sending this \
             fill to yellow and the comparison proves nothing — report it",
            marked * 100.0
        );
        assert!(
            unmarked > 0.99,
            "the unmarked arm came out only {:.1}% green against the marked arm's {:.1}%: \
             unmarked content is not being matched against the nominal inks",
            unmarked * 100.0,
            marked * 100.0
        );
    }

    /// Diagnostic (ruling 20: non-asserting; the printed output is the
    /// deliverable): the content-adaptation factor `R` for a **real screen**,
    /// derived from the pixels its tone mask actually marks.
    ///
    /// `crates/eink-dither/tests/gamut_adaptation_diag.rs` prints `R` for a
    /// synthetic `rho` field only. It cannot do this one: `eink-dither` has no
    /// dependencies at all and so cannot parse or rasterize an SVG. This lives
    /// in `byonk`, where `resvg`/`usvg` and `rasterize_tone_mask` are.
    ///
    /// Run with:
    ///     cargo test -p byonk --lib tone_screen_adaptation_factor -- --ignored --nocapture
    #[test]
    #[ignore = "diagnostic; prints R for the tone calibration screen"]
    fn tone_screen_adaptation_factor() {
        use crate::assets::AssetLoader;
        use crate::services::screen_repo_cache::ScreenRepoCache;
        use crate::services::screen_repo_manager::ScreenRepoManager;
        use crate::services::{ContentPipeline, DeviceContext, RenderService};
        use eink_dither::gamut::adapt::{adaptation_factor, MIN_DISCARD, PERCENTILE};

        // The screen as a device sees it: reterminal_e1002, 800x480.
        //
        // Transcribed from `config.yaml`'s `panels: reterminal_e1002:` rather
        // than read from it, so that a recalibration of that panel — new
        // `colors_actual`, or a new size — leaves this diagnostic silently
        // measuring the OLD panel. If these numbers ever matter for a
        // decision, diff them against the config first.
        let spec = DisplaySpec::from_dimensions(800, 480).unwrap();
        let official: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (255, 0, 0),
            (255, 255, 0),
            (0, 0, 255),
            (0, 255, 0),
        ];
        let actual: Vec<(u8, u8, u8)> = vec![
            (0, 0, 0),
            (255, 255, 255),
            (0xB5, 0x03, 0x03),
            (0xFF, 0xEE, 0x00),
            (0x20, 0x54, 0x97),
            (0x0D, 0x87, 0x6B),
        ];

        let loader = Arc::new(AssetLoader::new(None, None, None));
        let shared: crate::server::SharedConfig = Arc::new(arc_swap::ArcSwap::from(Arc::new(
            crate::models::AppConfig::default(),
        )));
        let render_service = Arc::new(RenderService::new(&loader).unwrap());
        let cache_root = std::env::temp_dir().join(format!(
            "byonk_tone_diag_cache_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let pm = ScreenRepoManager::new(
            loader.clone(),
            shared.clone(),
            ScreenRepoCache::new(cache_root.clone()),
            Default::default(),
            None,
            None,
        );
        pm.rebuild_loader();
        let pipeline = ContentPipeline::new(shared, loader, render_service, pm).unwrap();

        let ctx = DeviceContext {
            mac: "AA:BB:CC:DD:EE:01".to_string(),
            model: Some("og".to_string()),
            width: Some(spec.width),
            height: Some(spec.height),
            colors: Some(
                official
                    .iter()
                    .map(|&(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"))
                    .collect(),
            ),
            colors_actual: Some(
                actual
                    .iter()
                    .map(|&(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"))
                    .collect(),
            ),
            ..Default::default()
        };

        let script = pipeline
            .run_screen_by_name(
                "byonk-builtin/calibration/tone",
                Default::default(),
                Some(ctx.clone()),
            )
            .expect("tone screen runs");
        let svg = pipeline
            .render_svg_from_script(&script, Some(&ctx))
            .expect("tone screen templates");

        let renderer = SvgRenderer::new();
        let pixmap = renderer.rasterize_svg(svg.as_bytes(), spec, None).unwrap();
        let pixels = rgba_to_eink_srgb(pixmap.data());
        let mask = renderer
            .rasterize_tone_mask(svg.as_bytes(), spec, None)
            .unwrap();
        assert_eq!(mask.len(), pixels.len());

        let (eink_palette, _) = build_eink_palette(&official, Some(&actual), false).unwrap();
        let mapper = GamutMapper::new(&eink_palette);
        let opts = GamutOptions::default();

        let mut rhos: Vec<f32> = pixels
            .iter()
            .zip(mask.iter())
            .filter(|(_, m)| **m)
            .map(|(p, _)| mapper.rho(*p))
            .collect();

        let marked = rhos.len();
        let mut sorted = rhos.clone();
        sorted.sort_by(f32::total_cmp);
        let q = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
        println!(
            "tone screen {}x{}: {marked} marked pixels of {} ({:.1}%)",
            spec.width,
            spec.height,
            pixels.len(),
            marked as f32 * 100.0 / pixels.len() as f32
        );
        println!(
            "  rho over the marked set: p50={:.4} p90={:.4} p99={:.4} max={:.4}",
            q(0.5),
            q(0.9),
            q(0.99),
            q(1.0)
        );
        let discard = MIN_DISCARD
            .max((marked as f32 * (1.0 - PERCENTILE)).ceil() as usize)
            .min(marked.saturating_sub(1));
        println!("  PERCENTILE={PERCENTILE} MIN_DISCARD={MIN_DISCARD} -> discarding {discard}");
        let r = adaptation_factor(&mut rhos, opts.max_compression);
        println!(
            "ADAPTATION FACTOR R = {r:.4}   (max_compression cap = {})",
            opts.max_compression
        );

        let _ = std::fs::remove_dir_all(&cache_root);
    }

    /// A renderer carrying byonk's bundled fonts, the way production builds
    /// one. `SvgRenderer::default()` loads system fonts only, so it has no
    /// `Outfit` and every hinting fixture below would silently fall back.
    fn bundled_renderer() -> SvgRenderer {
        SvgRenderer::with_fonts(crate::assets::AssetLoader::new(None, None, None).get_fonts())
    }

    #[test]
    fn hinting_changes_the_rendered_pixels() {
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#fff"/>
          <text x="20" y="40" font-family="Outfit" font-size="11"
                style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
        </svg>"##;

        let r = bundled_renderer();
        let bw = &[(0u8, 0u8, 0u8), (255, 255, 255)];

        let unhinted = r
            .render_to_palette_png(
                SVG.as_bytes(),
                DisplaySpec::OG,
                bw,
                None,
                false,
                None,
                None,
                None,
            )
            .expect("unhinted render");

        // Deliberately NOT `adaptive_default(2)`: that also aliases the glyph
        // rasterisation, so the two arms would differ even if hinting stopped
        // reaching the renderer entirely and this test would keep passing while
        // measuring nothing it is named for. Pinning `aliased: false` leaves
        // hinting as the only difference.
        let cfg = FontConfig {
            default: Some(HintingSpec {
                engine: HintingEngine::Auto,
                target: HintingTarget::Mono { aliased: false },
            }),
            variants: Default::default(),
        };
        let hinted = r
            .render_to_palette_png(
                SVG.as_bytes(),
                DisplaySpec::OG,
                bw,
                None,
                false,
                None,
                None,
                Some(&cfg),
            )
            .expect("hinted render");

        assert_ne!(
            unhinted, hinted,
            "mono hinting at 11px must change the rasterisation; identical output \
             means the resolver is not reaching the renderer"
        );
    }

    #[test]
    fn a_variant_hints_differently_from_its_base_font_in_one_document() {
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#fff"/>
          <text x="20" y="40" font-family="Outfit" font-size="11"
                style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
          <text x="20" y="80" font-family="Crisp Body, Outfit" font-size="11"
                style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
        </svg>"##;

        let r = bundled_renderer();
        let bw = &[(0u8, 0u8, 0u8), (255, 255, 255)];
        let render = |cfg: &FontConfig| {
            r.render_to_palette_png(
                SVG.as_bytes(),
                DisplaySpec::OG,
                bw,
                None,
                false,
                None,
                None,
                Some(cfg),
            )
            .expect("render")
        };

        // A variant alias is a name the author makes up and byonk intercepts,
        // so it must not be a real family — otherwise ordinary font lookup
        // could satisfy the second line and this test could not tell the two
        // apart. `Crisp Body` says what the variant is *for*. It deliberately
        // does not read like a font: an alias built as `<RealFamily>
        // <TechnicalTerm>` — the earlier `Outfit Mono`, which meant Outfit
        // with mono *hinting*, not a monospaced Outfit — is read by everyone
        // as a font that does not exist.
        //
        // Baseline: no variants at all, so the second line falls back to the
        // same face as the first. The document names that fallback itself —
        // `Crisp Body, Outfit` — rather than leaving an unresolvable family to
        // land wherever the generic families happen to point. It used to land
        // on Outfit only because `sans-serif` did; repointing the generics at
        // the Source trio silently moved it to Source Sans 3 and broke the
        // second control below, which needs `plain` and `inheriting` to
        // resolve to the same face. Naming it keeps this test about hinting.
        let mut plain = FontConfig::adaptive_default(4);
        plain.variants.clear();

        let mut with_variant = plain.clone();
        with_variant.variants.insert(
            "Crisp Body".to_string(),
            FontVariant {
                font: "Outfit".to_string(),
                strikes: None,
                hinting: Some(Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Mono { aliased: false },
                })),
            },
        );

        assert_ne!(
            render(&plain),
            render(&with_variant),
            "declaring a Crisp Body variant must change the second line's \
             rasterisation; identical output means select_font never resolved it"
        );

        // Control: a variant nothing in the document references must change
        // nothing. Without this, a resolver applying its hinting to every face
        // would pass the assertion above.
        let mut unused = plain.clone();
        unused.variants.insert(
            "Never Referenced".to_string(),
            FontVariant {
                font: "Outfit".to_string(),
                strikes: None,
                hinting: Some(Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Mono { aliased: false },
                })),
            },
        );
        assert_eq!(
            render(&plain),
            render(&unused),
            "a variant no element uses must not affect the render"
        );

        // Second control: the *hinting* has to be what makes the difference,
        // not the mere existence of a second face. A variant that inherits
        // hinting resolves the second line to a duplicate of the same face
        // configured identically, so it must rasterize byte for byte the same.
        let mut inheriting = plain.clone();
        inheriting.variants.insert(
            "Crisp Body".to_string(),
            FontVariant {
                font: "Outfit".to_string(),
                strikes: None,
                hinting: None,
            },
        );
        assert_eq!(
            render(&plain),
            render(&inheriting),
            "a variant that overrides nothing must render identically; a \
             difference here means the assertion above measured the extra \
             face rather than its hinting"
        );
    }
    /// A variant can be drawn 1-bit, even though the Lua `aliased` flag is
    /// document-level and a variant cannot carry it.
    ///
    /// Aliasing reaches usvg through `Options::text_rendering`, which is an
    /// ordinary inheritable SVG property — so the *element* using the variant
    /// can ask for it with `text-rendering="optimizeSpeed"`. Pinned because
    /// without it a "mono" variant only gets grid-fitting, still anti-aliased,
    /// which looks so nearly identical to smooth that a broken variant and a
    /// working one are indistinguishable by eye. That is exactly how the
    /// bundled hinting demo shipped its mono column looking like its smooth
    /// one.
    ///
    /// Rendered on a 4-grey panel, where the document default is *not*
    /// aliased, so the element attribute is the only thing that can produce a
    /// 1-bit glyph.
    #[test]
    fn a_mono_variant_plus_optimize_speed_equals_the_document_level_aliased_mono() {
        const ALIASED_VARIANT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#fff"/>
          <text x="20" y="40" font-family="Crisp Body, Outfit" font-size="11"
                text-rendering="optimizeSpeed"
                style="font-variation-settings: 'wght' 400">illiIL1 xXHv Hamburgefonstiv</text>
        </svg>"##;
        const PLAIN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#fff"/>
          <text x="20" y="40" font-family="Outfit" font-size="11"
                style="font-variation-settings: 'wght' 400">illiIL1 xXHv Hamburgefonstiv</text>
        </svg>"##;

        let r = bundled_renderer();
        let grey4 = &[
            (0u8, 0u8, 0u8),
            (85, 85, 85),
            (170, 170, 170),
            (255, 255, 255),
        ];
        let render = |svg: &str, cfg: &FontConfig| {
            r.render_to_palette_png(
                svg.as_bytes(),
                DisplaySpec::OG,
                grey4,
                None,
                false,
                None,
                None,
                Some(cfg),
            )
            .expect("render")
        };

        let mono_variant = |aliased_default: bool| {
            let mut cfg = FontConfig::adaptive_default(4);
            cfg.variants.clear();
            if aliased_default {
                cfg.default = Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Mono { aliased: true },
                });
            }
            cfg
        };

        // The reference: the document itself asks for mono *with* aliasing,
        // which is the treatment Task 7 proved crisp on a black-and-white
        // panel. Nothing here is a variant.
        let reference = render(PLAIN, &mono_variant(true));

        // The claim: a variant asking for mono, with the element opting into
        // aliasing, reaches the same place.
        let mut via_variant = FontConfig::adaptive_default(4);
        via_variant.variants.clear();
        via_variant.variants.insert(
            "Crisp Body".to_string(),
            FontVariant {
                font: "Outfit".to_string(),
                strikes: None,
                hinting: Some(Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Mono { aliased: false },
                })),
            },
        );
        assert_eq!(
            reference,
            render(ALIASED_VARIANT, &via_variant),
            "a mono variant drawn with text-rendering=optimizeSpeed must match \
             the document-level aliased mono; a difference means part of a \
             screen can no longer be made crisp on its own"
        );

        // Control: the aliasing attribute alone is not what produced the
        // match. The same element, with a variant hinted smooth instead of
        // mono, is the known-bad aliased-without-mono state and must differ.
        let mut smooth_variant = FontConfig::adaptive_default(4);
        smooth_variant.variants.clear();
        smooth_variant.variants.insert(
            "Crisp Body".to_string(),
            FontVariant {
                font: "Outfit".to_string(),
                strikes: None,
                hinting: Some(Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Smooth {
                        mode: HintingMode::Normal,
                        symmetric_rendering: false,
                        preserve_linear_metrics: true,
                    },
                })),
            },
        );
        assert_ne!(
            reference,
            render(ALIASED_VARIANT, &smooth_variant),
            "aliasing without mono hinting must not match mono; if it does, \
             the variant's target is being ignored and the test above proved \
             nothing"
        );
    }

    /// The assumption the whole variant design rests on: a face loaded inside
    /// `select_font` via `Arc::make_mut` gets a NEW id and survives into the
    /// rest of the parse, so `select_hinting` is later called with that id.
    ///
    /// Kept as a direct test of usvg/fontdb behaviour rather than folded into
    /// the pixel-diff tests above: if a resvg bump ever breaks the mechanism,
    /// this says so in one line instead of leaving two renders mysteriously
    /// identical.
    #[test]
    fn make_mut_load_survives_into_select_hinting() {
        use std::sync::Mutex;
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <text x="20" y="40" font-family="Outfit" font-size="11">Hamburgefonstiv</text>
        </svg>"##;

        let renderer = bundled_renderer();
        let selected: Arc<Mutex<Vec<usvg::fontdb::ID>>> = Default::default();
        let hinted: Arc<Mutex<Vec<usvg::fontdb::ID>>> = Default::default();
        let base_ids: std::collections::HashSet<usvg::fontdb::ID> =
            renderer.fontdb.faces().map(|f| f.id).collect();

        let mut resolver = usvg::FontResolver::default();
        let sel = selected.clone();
        resolver.select_font = Box::new(move |font, db| {
            let query = usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name("Outfit")],
                weight: usvg::fontdb::Weight(font.weight()),
                stretch: usvg::fontdb::Stretch::Normal,
                style: usvg::fontdb::Style::Normal,
            };
            let base = db.query(&query).expect("Outfit must resolve");
            let (source, index) = db.face_source(base).unwrap();
            let loaded = usvg::fontdb::Database::load_font_source(Arc::make_mut(db), source);
            let id = loaded.get(index as usize).copied().unwrap();
            sel.lock().unwrap().push(id);
            Some(id)
        });
        let hin = hinted.clone();
        resolver.select_hinting = Box::new(move |id, _s, global, db| {
            hin.lock().unwrap().push(id);
            // The database select_hinting is handed must still contain the
            // face we loaded, or the "same parse" claim is false.
            assert!(db.face(id).is_some(), "loaded face vanished from the db");
            global
        });

        let options = usvg::Options {
            fontdb: renderer.fontdb.clone(),
            font_hinting: Some(FontConfig::adaptive_default(2).default.unwrap().to_usvg()),
            font_resolver: resolver,
            ..Default::default()
        };
        let tree = usvg::Tree::from_data(SVG.as_bytes(), &options).expect("parse");
        let mut pixmap = Pixmap::new(800, 480).unwrap();
        resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());

        let selected = selected.lock().unwrap().clone();
        let hinted = hinted.lock().unwrap().clone();
        println!("selected={selected:?} hinted={hinted:?}");
        assert_eq!(selected.len(), 1, "select_font must have run once");
        let id = selected[0];
        assert!(
            !base_ids.contains(&id),
            "the loaded face must be a NEW id, not the base font's"
        );
        assert!(
            hinted.contains(&id),
            "select_hinting never saw the id select_font returned: the \
             Arc::make_mut mutation did not survive the parse"
        );
    }

    // ---- Glyph rasterisation (aliased vs anti-aliased) --------------------

    /// A fixture carrying BOTH text and a curved shape, so a test can tell
    /// "glyphs were aliased" apart from "the whole document was aliased".
    ///
    /// The text lives in rows 0..80, the circle in rows 200..360; the two bands
    /// never overlap, so each can be measured on its own.
    const ALIASING_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#ffffff"/>
          <text x="20" y="40" font-family="Outfit" font-size="11"
                style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
          <circle cx="400" cy="280" r="60" fill="#000000"/>
        </svg>"##;

    /// Grey statistics for one horizontal band of a raw (pre-dither) render.
    ///
    /// `grey` counts pixels that are neither pure black nor pure white — the
    /// anti-aliasing byonk's ditherer turns into speckle. `ink` is total
    /// coverage in whole-pixel units, so it can be compared between an aliased
    /// and an anti-aliased render of the same glyphs.
    struct BandStats {
        grey: usize,
        black: usize,
        ink: f64,
    }

    fn band_stats(pixmap: &Pixmap, rows: std::ops::Range<u32>, width: u32) -> BandStats {
        let data = pixmap.data();
        let mut s = BandStats {
            grey: 0,
            black: 0,
            ink: 0.0,
        };
        for y in rows {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                // Opaque render over an opaque white fill, black paint only:
                // any channel is the coverage complement. Use green.
                let v = data[i + 1];
                if v == 0 {
                    s.black += 1;
                } else if v != 255 {
                    s.grey += 1;
                }
                s.ink += (255 - v) as f64 / 255.0;
            }
        }
        s
    }

    /// The bug this task fixes: a black-and-white panel asks for mono hinting,
    /// but resvg still anti-aliases the hinted outlines and the Atkinson
    /// ditherer turns those grey edges into speckle. On a 1-bit panel the
    /// glyphs must be rasterised 1-bit.
    ///
    /// Measured on the RAW pixmap, before dithering — that is the only place
    /// the anti-aliasing is visible; after dithering everything is 1-bit by
    /// definition and the test could not fail.
    #[test]
    fn bw_default_rasterises_glyphs_without_anti_aliasing() {
        let r = bundled_renderer();
        let spec = DisplaySpec::OG;

        // The baseline is mono-hinted but anti-aliased — the exact state this
        // task changes, not an unhinted render. That isolates the aliasing
        // axis: the two arms lay down identical hinted outlines and differ
        // only in how they are rasterised, so the ink comparison below can be
        // tight instead of having to absorb the hinting shift as well.
        let base_cfg = FontConfig {
            default: Some(HintingSpec {
                engine: HintingEngine::Auto,
                target: HintingTarget::Mono { aliased: false },
            }),
            variants: Default::default(),
        };
        let aa = r
            .rasterize_svg(ALIASING_SVG.as_bytes(), spec, Some(&base_cfg))
            .expect("anti-aliased baseline render");
        let cfg = FontConfig::adaptive_default(2);
        let al = r
            .rasterize_svg(ALIASING_SVG.as_bytes(), spec, Some(&cfg))
            .expect("bw render");

        let text_aa = band_stats(&aa, 0..80, spec.width);
        let text_al = band_stats(&al, 0..80, spec.width);

        // Sanity: the baseline must actually produce anti-aliased text, or
        // "no greys in the other arm" would prove nothing.
        assert!(
            text_aa.grey > 100,
            "the mono-hinted baseline produced only {} grey text pixels, so this \
             test cannot see aliasing at all",
            text_aa.grey
        );

        assert_eq!(
            text_al.grey, 0,
            "a black-and-white panel still anti-aliases its glyphs: {} grey pixels \
             in the text band. resvg is never told to rasterise 1-bit, so the \
             ditherer turns these into speckle",
            text_al.grey
        );

        // Hole closed #1: "text vanished" and "the band went solid black" both
        // produce zero greys and would pass the assertion above. Require the
        // aliased glyphs to carry roughly the same ink as the anti-aliased
        // ones — mono hinting redistributes coverage onto whole pixels, it does
        // not delete or invent strokes.
        assert!(
            text_al.black > 0,
            "the aliased text band has no black pixels at all: the text is gone"
        );
        // The band comes from the measurement, not from taste. With the
        // baseline pinned to the same mono-hinted outlines (above), the two
        // arms measured ink_aa = 345.02 px and ink_al = 336.00 px, a ratio of
        // 0.9738 — 2.62% below parity, which is the rounding of partial edge
        // coverage onto whole pixels and nothing more. The bound is parity ±3x
        // that measured deviation (±7.9%), leaving room for font and renderer
        // jitter while staying far from the failures it guards against: a
        // dropped stem or a flooded band moves this by tens of percent, not by
        // eight.
        let ratio = text_al.ink / text_aa.ink;
        assert!(
            (0.92..1.08).contains(&ratio),
            "aliased text carries {:.4}x the ink of the anti-aliased baseline \
             ({:.2} vs {:.2} px, measured at 0.9738): that is stems dropping out \
             or the band flooding, not 1-bit rasterisation of the same glyphs",
            ratio,
            text_al.ink,
            text_aa.ink
        );

        // Hole closed #2: setting `shape_rendering` (or otherwise aliasing the
        // WHOLE document) would also zero the text greys. The circle must keep
        // its anti-aliased edge — this is a text-rendering default, not a
        // document-wide one.
        let shape_al = band_stats(&al, 200..360, spec.width);
        assert!(
            shape_al.grey > 100,
            "the circle lost its anti-aliasing ({} grey pixels): the aliasing was \
             applied to the whole document, not to glyphs",
            shape_al.grey
        );
    }

    /// `AutoFallback` on a face carrying no real hinting program must actually
    /// fall back to the automatic hinter.
    ///
    /// Outfit's whole contribution is the seven-byte dropout-control stub in
    /// `prep`. skrifa asks only whether `fpgm` or `prep` is non-empty, so it
    /// picks the interpreter, which then has nothing to run — `AutoFallback`
    /// silently means "unhinted" for such a face. byonk resolves this itself.
    ///
    /// Note the `variants: Default::default()`: this is the ordinary path an
    /// unremarkable screen takes, so the substitution has to hold there and not
    /// only where a variant resolver is installed.
    #[test]
    fn auto_fallback_reaches_the_autohinter_on_a_stub_only_face() {
        let r = bundled_renderer();
        let spec = DisplaySpec::OG;

        let render = |engine| {
            let cfg = FontConfig {
                default: Some(HintingSpec {
                    engine,
                    // Mono, unaliased: the strongest hinting signal, with
                    // rasterisation held constant so only hinting can move.
                    target: HintingTarget::Mono { aliased: false },
                }),
                variants: Default::default(),
            };
            r.rasterize_svg(ALIASING_SVG.as_bytes(), spec, Some(&cfg))
                .expect("render")
                .data()
                .to_vec()
        };

        let auto = render(HintingEngine::Auto);
        let interpreter = render(HintingEngine::Interpreter);
        let fallback = render(HintingEngine::AutoFallback);

        // Compared as a count of differing pixels rather than with `assert_eq!`
        // on the buffers: these are 800x480 RGBA, and a failed `assert_eq!`
        // would print megabytes of them.
        let differing = |a: &[u8], b: &[u8]| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(x, y)| x != y)
                .count()
        };

        // The control, and it must come first: if the two engines happened to
        // agree on this face, both assertions below would hold no matter what
        // `AutoFallback` did, and the test would measure nothing.
        assert!(
            differing(&auto, &interpreter) > 0,
            "the automatic hinter and the interpreter render this face \
             identically, so this test cannot tell which one AutoFallback chose"
        );

        assert_eq!(
            differing(&fallback, &auto),
            0,
            "AutoFallback did not reach the automatic hinter on a face whose \
             only hinting program is the dropout-control stub"
        );
        assert!(
            differing(&fallback, &interpreter) > 0,
            "AutoFallback still selected the interpreter, which has nothing to \
             run on this face and leaves the outline unhinted"
        );
    }

    /// A greyscale panel must be untouched by the change above: `grey_count > 2`
    /// keeps smooth hinting AND anti-aliased glyphs.
    #[test]
    fn greyscale_default_keeps_anti_aliased_glyphs() {
        let r = bundled_renderer();
        let spec = DisplaySpec::OG;
        let cfg = FontConfig::adaptive_default(4);
        let px = r
            .rasterize_svg(ALIASING_SVG.as_bytes(), spec, Some(&cfg))
            .expect("greyscale render");
        let text = band_stats(&px, 0..80, spec.width);
        assert!(
            text.grey > 100,
            "a greyscale panel must keep anti-aliased text; only {} grey pixels",
            text.grey
        );
    }

    /// The in-document escape hatch from the black-and-white aliased default.
    ///
    /// Aliasing is a per-element, inheritable property (`text-rendering` on the
    /// text node) while hinting is per-face, so an element that wants smooth or
    /// no hinting on a BW panel would otherwise inherit `optimizeSpeed` and land
    /// in the known-bad "aliased without mono hinting" state, where thin stems
    /// drop out — tiny-skia has no dropout control.
    ///
    /// `text-rendering: optimizeLegibility` is the way out, and it is NOT the
    /// same as `geometricPrecision`: both restore anti-aliasing, but
    /// `flatten.rs`'s `hintable` excludes only `GeometricPrecision`, so
    /// `optimizeLegibility` keeps the glyphs hinted. This test exists because
    /// that distinction is about to be recommended to screen authors, so it must
    /// be proven rather than read off the source.
    #[test]
    fn optimize_legibility_restores_anti_aliasing_and_keeps_hinting() {
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#ffffff"/>
          <text x="20" y="40" font-family="Outfit" font-size="11"
                text-rendering="optimizeLegibility"
                style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
        </svg>"##;

        let r = bundled_renderer();
        let spec = DisplaySpec::OG;

        // A black-and-white panel: the config asks for mono hinting AND aliased
        // glyphs document-wide.
        let bw = FontConfig::adaptive_default(2);
        let escaped = r
            .rasterize_svg(SVG.as_bytes(), spec, Some(&bw))
            .expect("escape-hatch render");

        // Half one: the anti-aliasing really is back.
        let stats = band_stats(&escaped, 0..80, spec.width);
        assert!(
            stats.grey > 100,
            "text-rendering: optimizeLegibility did not restore anti-aliasing on a \
             BW panel: only {} grey pixels. The escape hatch does not work and must \
             not be recommended",
            stats.grey
        );

        // Half two: PROVEN, not asserted. Hinting has no direct observable, so
        // compare against the same document rendered with hinting switched off
        // and nothing else changed. Both arms rasterise anti-aliased (the
        // element states `optimizeLegibility` itself, and an unhinted config
        // implies usvg's own `OptimizeLegibility` default anyway), so the only
        // thing that can move a pixel is the hinting.
        let unhinted = FontConfig {
            default: None,
            variants: Default::default(),
        };
        let plain = r
            .rasterize_svg(SVG.as_bytes(), spec, Some(&unhinted))
            .expect("unhinted render");

        assert_ne!(
            escaped.data(),
            plain.data(),
            "an element using optimizeLegibility on a BW panel rasterised \
             identically hinted and unhinted: optimizeLegibility is dropping the \
             hinting too, so it is NOT a safe escape hatch — it behaves like \
             geometricPrecision"
        );

        // The control that gives the assertion above its meaning: under
        // `geometricPrecision` the same comparison must come out EQUAL, because
        // usvg refuses to hint that mode at all. Without this, a renderer that
        // ignored `text-rendering` entirely — leaving both arms mono-hinted and
        // aliased, differing by hinting — would satisfy the assert_ne above and
        // the test would "prove" the escape hatch while measuring nothing about
        // optimizeLegibility specifically.
        let gp_svg = SVG.replace("optimizeLegibility", "geometricPrecision");
        let gp_hinted = r
            .rasterize_svg(gp_svg.as_bytes(), spec, Some(&bw))
            .expect("gp hinted render");
        let gp_plain = r
            .rasterize_svg(gp_svg.as_bytes(), spec, Some(&unhinted))
            .expect("gp unhinted render");
        assert_eq!(
            gp_hinted.data(),
            gp_plain.data(),
            "geometricPrecision was still hinted, so the hinted/unhinted \
             comparison above does not isolate hinting"
        );
    }

    /// The tone mask must be rasterized exactly like the frame it selects
    /// from. If the frame aliased its glyphs and the mask anti-aliased them
    /// (or vice versa), the mask would be offset from the text it masks.
    ///
    /// Only the aliasing differs between the two arms — same config otherwise —
    /// so a difference in the mask can only come from the aliasing reaching the
    /// mask's rasterizer.
    #[test]
    fn glyph_aliasing_reaches_the_tone_mask_too() {
        const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
          <rect width="800" height="480" fill="#ffffff"/>
          <g data-byonk-tone="continuous">
            <text x="20" y="40" font-family="Outfit" font-size="11"
                  style="font-variation-settings: 'wght' 400">Hamburgefonstiv 0123456789</text>
          </g>
        </svg>"##;

        let r = bundled_renderer();
        let spec = DisplaySpec::OG;
        let mask = |aliased: bool| {
            let cfg = FontConfig {
                default: Some(HintingSpec {
                    engine: HintingEngine::Auto,
                    target: HintingTarget::Mono { aliased },
                }),
                variants: Default::default(),
            };
            r.rasterize_tone_mask(SVG.as_bytes(), spec, Some(&cfg))
                .expect("mask")
        };

        assert_ne!(
            mask(true),
            mask(false),
            "the tone mask ignored glyph aliasing: it would be computed from a \
             differently rasterized document than the frame it masks"
        );
    }

    /// The default must be a default. A document that states its own
    /// `text-rendering` keeps winning, whether as a presentation attribute or
    /// through the `style` attribute.
    #[test]
    fn an_explicit_text_rendering_beats_the_bw_default() {
        let r = bundled_renderer();
        let spec = DisplaySpec::OG;
        let cfg = FontConfig::adaptive_default(2);

        for decl in [
            r#"text-rendering="geometricPrecision""#,
            r#"style="text-rendering: geometricPrecision""#,
        ] {
            let svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="480">
                  <rect width="800" height="480" fill="#ffffff"/>
                  <text x="20" y="40" font-family="Outfit" font-size="11" {decl}>Hamburgefonstiv 0123456789</text>
                </svg>"##
            );
            let px = r
                .rasterize_svg(svg.as_bytes(), spec, Some(&cfg))
                .expect("render");
            let text = band_stats(&px, 0..80, spec.width);
            assert!(
                text.grey > 100,
                "{decl} was overridden: only {} grey pixels, so byonk is forcing \
                 aliasing rather than defaulting it",
                text.grey
            );
        }
    }
}
