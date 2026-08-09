//! Side-by-side original/dithered PNGs for judging algorithms by eye.
//!
//! Mean dE is not sufficient to choose a dithering algorithm here. It is
//! actively misleading for out-of-gamut targets, where a patch rendered as
//! 80% black scores about as well as one rendered as the correct solid ink --
//! so ranking on it promotes algorithms that visibly destroy saturated
//! colours. These images are the check the numbers cannot provide.
//!
//! Run with:
//!     cargo test -p eink-dither --test visual_compare -- --ignored --nocapture
//!
//! Output goes to `target/dither-compare/`.

use eink_dither::{DitherAlgorithm, EinkDitherer, LinearRgb, Palette, Srgb};
use std::path::PathBuf;

/// The measured E1002 six-colour panel: what the inks really look like, as
/// opposed to the idealised values an author writes.
fn panel() -> Palette {
    let official = [
        Srgb::from_u8(0, 0, 0),
        Srgb::from_u8(255, 255, 255),
        Srgb::from_u8(255, 0, 0),
        Srgb::from_u8(255, 255, 0),
        Srgb::from_u8(0, 0, 255),
        Srgb::from_u8(0, 255, 0),
    ];
    let actual = [
        Srgb::from_u8(0, 0, 0),
        Srgb::from_u8(255, 255, 255),
        Srgb::from_u8(0xB5, 0x03, 0x03),
        Srgb::from_u8(0xFF, 0xEE, 0x00),
        Srgb::from_u8(0x20, 0x54, 0x97),
        Srgb::from_u8(0x0D, 0x87, 0x6B),
    ];
    Palette::new(&official, Some(&actual)).unwrap()
}

const ALGOS: &[(&str, DitherAlgorithm)] = &[
    ("atkinson", DitherAlgorithm::Atkinson),
    ("atkinson-hybrid", DitherAlgorithm::AtkinsonHybrid),
    ("floyd-steinberg", DitherAlgorithm::FloydSteinberg),
    ("jarvis-judice-ninke", DitherAlgorithm::JarvisJudiceNinke),
    ("stucki", DitherAlgorithm::Stucki),
    ("burkes", DitherAlgorithm::Burkes),
];

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dither-compare");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compose `original | dithered` into one RGB8 buffer with a separator.
fn side_by_side(orig: &[Srgb], dithered_rgb: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    const GAP: usize = 8;
    let out_w = w * 2 + GAP;
    let mut buf = vec![0x80u8; out_w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let s = orig[y * w + x];
            let d = (y * out_w + x) * 3;
            buf[d] = (s.r.clamp(0.0, 1.0) * 255.0).round() as u8;
            buf[d + 1] = (s.g.clamp(0.0, 1.0) * 255.0).round() as u8;
            buf[d + 2] = (s.b.clamp(0.0, 1.0) * 255.0).round() as u8;

            let sd = (y * w + x) * 3;
            let dd = (y * out_w + (x + w + GAP)) * 3;
            buf[dd] = dithered_rgb[sd];
            buf[dd + 1] = dithered_rgb[sd + 1];
            buf[dd + 2] = dithered_rgb[sd + 2];
        }
    }
    (buf, out_w, h)
}

/// Compose `original | before | after` into one RGB8 buffer.
///
/// Three panels rather than two: a before/after pair shows that something
/// changed but not whether it moved toward the source, which is the only
/// question worth asking of a rendering change.
fn triptych(
    orig: &[Srgb],
    before: &[u8],
    after: &[u8],
    w: usize,
    h: usize,
) -> (Vec<u8>, usize, usize) {
    const GAP: usize = 8;
    let out_w = w * 3 + GAP * 2;
    let mut buf = vec![0x80u8; out_w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let s = orig[y * w + x];
            let d = (y * out_w + x) * 3;
            buf[d] = (s.r.clamp(0.0, 1.0) * 255.0).round() as u8;
            buf[d + 1] = (s.g.clamp(0.0, 1.0) * 255.0).round() as u8;
            buf[d + 2] = (s.b.clamp(0.0, 1.0) * 255.0).round() as u8;

            let sd = (y * w + x) * 3;
            let db = (y * out_w + (x + w + GAP)) * 3;
            buf[db..db + 3].copy_from_slice(&before[sd..sd + 3]);
            let da = (y * out_w + (x + 2 * (w + GAP))) * 3;
            buf[da..da + 3].copy_from_slice(&after[sd..sd + 3]);
        }
    }
    (buf, out_w, h)
}

fn write(name: &str, buf: &[u8], w: usize, h: usize) {
    let path = out_dir().join(name);
    image::save_buffer(&path, buf, w as u32, h as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {}", path.display());
}

/// Mean luminance in *linear* light, which is where averaging is physical.
///
/// Averaging gamma-encoded sRGB would answer a different question and get
/// the sign of small differences wrong.
fn mean_luminance(rgb: &[LinearRgb]) -> f32 {
    let sum: f32 = rgb
        .iter()
        .map(|c| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b)
        .sum();
    sum / rgb.len() as f32
}

fn render_all(label: &str, pixels: &[Srgb], w: usize, h: usize) {
    let palette = panel();

    // "Darker" is only a defect if it moves away from the original. An
    // algorithm that selects black more often may be *correcting* an
    // under-selection, in which case darker is more faithful, not less.
    let orig_lin: Vec<LinearRgb> = pixels.iter().map(|&s| LinearRgb::from(s)).collect();
    let orig_y = mean_luminance(&orig_lin);
    eprintln!("  mean linear luminance -- original {orig_y:.4}");

    for &(name, algo) in ALGOS {
        let out = EinkDitherer::new(palette.clone())
            .algorithm(algo)
            .dither(pixels, w, h);

        let dith_lin: Vec<LinearRgb> = out
            .indices()
            .iter()
            .map(|&i| palette.actual_linear(i as usize))
            .collect();
        let y = mean_luminance(&dith_lin);
        eprintln!("    {name:<20} {y:.4}   delta {:+.4}", y - orig_y);

        let rgb = out.to_rgb_actual();
        let (buf, ow, oh) = side_by_side(pixels, &rgb, w, h);
        write(&format!("{label}-{name}.png"), &buf, ow, oh);
    }
}

/// HSL -> sRGB, matching the domain tests' sweep generator.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    // Clamp: the arithmetic can land a hair outside 0..=1 (-7e-9 was observed),
    // and srgb_to_linear's LUT rejects out-of-range input.
    let m = l - c / 2.0;
    (
        (r1 + m).clamp(0.0, 1.0),
        (g1 + m).clamp(0.0, 1.0),
        (b1 + m).clamp(0.0, 1.0),
    )
}

/// Hue across, lightness down, at full saturation.
///
/// This is the systematic view: the dark warm band near the top-left is the
/// open defect, and the blue/violet band on the right is where the kernels
/// that score best on mean dE collapse to near-black.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_hue_lightness_field() {
    const W: usize = 480;
    const H: usize = 320;
    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let hue = x as f32 / W as f32;
            let (r, g, b) = hsl_to_rgb(hue, 1.0, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }
    eprintln!("\n=== hue (across) x lightness (down), saturated ===");
    render_all("field-saturated", &pixels, W, H);
}

/// The same field at half saturation, which is closer to real content.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_hue_lightness_field_muted() {
    const W: usize = 480;
    const H: usize = 320;
    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let hue = x as f32 / W as f32;
            let (r, g, b) = hsl_to_rgb(hue, 0.5, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }
    eprintln!("\n=== hue x lightness, 50% saturation ===");
    render_all("field-muted", &pixels, W, H);
}

/// Sharp structure over the gradient: edges, thin lines, fine detail.
///
/// Continuous-tone scenes only exercise half of what a kernel does. E-ink
/// screens are mostly rules, labels, icons and chart strokes, and error
/// diffusion damages those in ways a gradient never reveals: thin lines
/// break up, hard edges ring, and fine detail dissolves into the dot
/// pattern. Atkinson's reputation rests on holding edges well, so the
/// comparison is not decidable without this scene.
///
/// The elements are chosen so each failure mode has somewhere to show:
///   - a radial star sweeps every angle at once, so directional bias in
///     the kernel appears as spokes surviving unevenly;
///   - line groups at 1/2/3 px find the width where a stroke stops being
///     continuous;
///   - flat swatches with hard borders show edge ringing and whether a
///     deliberate flat fill survives as one ink;
///   - a fine checkerboard is the resolution limit -- the point where the
///     dither pattern and the content are the same frequency.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_sharp_structures() {
    let (px, w, h) = sharp_scene();
    eprintln!("\n=== sharp structures over gradient ===");
    render_all("sharp", &px, w, h);
}

/// Jitter is measured as a strict improvement on flat patches, but flat
/// patches cannot see the risk: noise near a hard edge or a one-pixel stroke
/// could dissolve exactly the detail those defaults are meant to protect.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_sharp_noise_sweep() {
    let (px, w, h) = sharp_scene();
    let palette = panel();
    eprintln!("\n=== sharp structures vs noise_scale ===");
    for (name, algo) in [
        ("atkinson", DitherAlgorithm::Atkinson),
        ("jarvis-judice-ninke", DitherAlgorithm::JarvisJudiceNinke),
    ] {
        for scale in [0.0f32, 6.0, 16.0] {
            let out = EinkDitherer::new(palette.clone())
                .algorithm(algo)
                .noise_scale(scale)
                .dither(&px, w, h);
            let rgb = out.to_rgb_actual();
            let (buf, ow, oh) = side_by_side(&px, &rgb, w, h);
            write(&format!("sharpnoise-{name}-{scale:04.1}.png"), &buf, ow, oh);
        }
    }
}

fn sharp_scene() -> (Vec<Srgb>, usize, usize) {
    const W: usize = 480;
    const H: usize = 320;

    // Background: the same muted field, so structure is judged in context
    // rather than against a flat white page it will never sit on.
    let mut px = vec![Srgb::new(0.0, 0.0, 0.0); W * H];
    for y in 0..H {
        let l = 0.20 + 0.60 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 0.45, l);
            px[y * W + x] = Srgb::new(r, g, b);
        }
    }

    let mut set = |x: i32, y: i32, c: Srgb| {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            px[y as usize * W + x as usize] = c;
        }
    };
    let black = Srgb::from_u8(0, 0, 0);
    let white = Srgb::from_u8(255, 255, 255);

    // Radial star, top-left: 24 black spokes on a white disc.
    let (cx, cy, rad) = (72i32, 74i32, 56i32);
    for y in cy - rad..=cy + rad {
        for x in cx - rad..=cx + rad {
            let (dx, dy) = ((x - cx) as f32, (y - cy) as f32);
            let d = (dx * dx + dy * dy).sqrt();
            if d > rad as f32 {
                continue;
            }
            let ang = dy.atan2(dx);
            let spoke = (ang * 24.0 / std::f32::consts::PI).rem_euclid(2.0) < 1.0;
            set(x, y, if spoke { black } else { white });
        }
    }

    // Line groups: 1, 2 and 3 px, horizontal then vertical, black then white.
    let mut ly = 24i32;
    for width in [1i32, 2, 3] {
        for (i, c) in [black, white].into_iter().enumerate() {
            for w in 0..width {
                for x in 170..300 {
                    set(x, ly + w, c);
                }
            }
            ly += width + 6 + i as i32;
        }
    }
    let mut lx = 320i32;
    for width in [1i32, 2, 3] {
        for c in [black, white] {
            for w in 0..width {
                for y in 20..120 {
                    set(lx + w, y, c);
                }
            }
            lx += width + 7;
        }
    }

    // Diagonals, which no axis-aligned kernel treats the same as the above.
    for i in 0..90 {
        set(180 + i, 120 + i, black);
        set(184 + i, 120 + i, white);
    }

    // Fine checkerboards at 1, 2 and 4 px -- the resolution limit.
    for (k, cell) in [1usize, 2, 4].into_iter().enumerate() {
        let ox = 30 + k * 70;
        for y in 0..56 {
            for x in 0..60 {
                let on = ((x / cell) + (y / cell)) % 2 == 0;
                set(
                    (ox + x) as i32,
                    (170 + y) as i32,
                    if on { black } else { white },
                );
            }
        }
    }

    // Flat swatches with hard borders: deliberate fills, which should survive
    // as a single ink rather than dissolving into a mixture.
    let swatches = [
        Srgb::from_u8(0xB5, 0x03, 0x03),
        Srgb::from_u8(0xFF, 0xEE, 0x00),
        Srgb::from_u8(0x20, 0x54, 0x97),
        Srgb::from_u8(0x0D, 0x87, 0x6B),
        Srgb::from_u8(0x80, 0x80, 0x80),
        Srgb::from_u8(0xC0, 0x60, 0x20),
    ];
    for (i, c) in swatches.iter().enumerate() {
        let ox = 250 + (i % 3) * 74;
        let oy = 170 + (i / 3) * 62;
        for y in 0..54 {
            for x in 0..66 {
                set((ox + x) as i32, (oy + y) as i32, *c);
            }
        }
    }

    // Text-like bars: the smallest structure a screen routinely renders.
    for row in 0..6 {
        for ch in 0..22 {
            let bx = 20 + ch * 6;
            let by = 240 + row * 6;
            for y in 0..4 {
                for x in 0..3 {
                    if (ch + row + x) % 3 != 0 {
                        set(bx + x, by + y, black);
                    }
                }
            }
        }
    }

    (px, W, H)
}

/// A real photograph -- the case the numbers are least able to judge.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_photo() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../screens/builtin/calibration/color/photo.png");
    let Ok(img) = image::open(&src) else {
        eprintln!("skipping: {} not readable", src.display());
        return;
    };
    let img = img.resize(480, 480, image::imageops::FilterType::Lanczos3);
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let pixels: Vec<Srgb> = rgb
        .pixels()
        .map(|p| Srgb::from_u8(p[0], p[1], p[2]))
        .collect();
    eprintln!("\n=== photo {w}x{h} ===");
    render_all("photo", &pixels, w, h);
}

/// Magnified crops of the upper field, where the reported streak sits.
///
/// Luminance statistics kept pointing at the lower half while the visible
/// line is near the top, which is itself evidence: a seam the eye finds but
/// luminance does not is a change in *which inks* are used at roughly
/// constant brightness. Nearest-neighbour upscaling keeps the dot pattern
/// legible instead of blurring the thing being examined.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_crop_upper_field() {
    const W: usize = 480;
    const H: usize = 320;
    const Y0: usize = 55;
    const Y1: usize = 145;
    const ZOOM: usize = 3;

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let hue = x as f32 / W as f32;
            let (r, g, b) = hsl_to_rgb(hue, 0.5, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let palette = panel();
    // Survey the whole frame in bands, not just the first streak found. There
    // turned out to be more than one boundary, and counting them matters: a
    // single artifact invites a local patch, several at every ink transition
    // point at the selection rule.
    let bands = [(0usize, 90usize), (Y0, Y1), (145, 235), (230, 320)];
    eprintln!("\n=== field crops in bands, {ZOOM}x ===");
    for &(name, algo) in ALGOS {
        let out = EinkDitherer::new(palette.clone())
            .algorithm(algo)
            .dither(&pixels, W, H);
        let rgb = out.to_rgb_actual();

        for (bi, &(y0, y1)) in bands.iter().enumerate() {
            let ch = y1 - y0;
            let (ow, oh) = (W * ZOOM, ch * ZOOM);
            let mut buf = vec![0u8; ow * oh * 3];
            for y in 0..oh {
                for x in 0..ow {
                    let s = ((y0 + y / ZOOM) * W + x / ZOOM) * 3;
                    let d = (y * ow + x) * 3;
                    buf[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
                }
            }
            let label = if bi == 1 {
                format!("crop-{name}.png")
            } else {
                format!("band{bi}-{name}.png")
            };
            write(&label, &buf, ow, oh);
        }
    }
}

/// Does the blue-noise jitter break up the structured artifacts?
///
/// Error diffusion on a smooth gradient can lock into a limit cycle instead
/// of staying stochastic, producing repeating scallops at ink-set boundaries
/// and herringbone across flat areas. The jitter exists to break that up, and
/// Atkinson is the one kernel that opts out (`noise_scale = 0.0`) -- so if
/// the mechanism is what it looks like, raising the scale should visibly
/// reduce the structure, and Atkinson should be the worst offender at 0.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_noise_scale_sweep() {
    const W: usize = 480;
    const H: usize = 320;
    // Two bands, because they answer different questions: the upper one
    // holds the scalloped arcs at the ink-set boundary, the lower one the
    // thin green streak through the yellow field. Noise fixes one, not both.
    const BANDS: [(usize, usize); 2] = [(55, 145), (145, 235)];
    const ZOOM: usize = 3;

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let hue = x as f32 / W as f32;
            let (r, g, b) = hsl_to_rgb(hue, 0.5, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let palette = panel();
    eprintln!("\n=== noise_scale sweep ===");
    for (name, algo) in [
        ("atkinson", DitherAlgorithm::Atkinson),
        ("jarvis-judice-ninke", DitherAlgorithm::JarvisJudiceNinke),
    ] {
        for scale in [0.0f32, 6.0, 16.0] {
            let out = EinkDitherer::new(palette.clone())
                .algorithm(algo)
                .noise_scale(scale)
                .dither(&pixels, W, H);

            // Jitter cannot be free: it perturbs the kernel weights, so it
            // buys smoothness with accuracy. Report the cost next to the
            // image so the trade is visible rather than assumed.
            let dith: Vec<LinearRgb> = out
                .indices()
                .iter()
                .map(|&i| palette.actual_linear(i as usize))
                .collect();
            let orig: Vec<LinearRgb> = pixels.iter().map(|&s| LinearRgb::from(s)).collect();
            eprintln!(
                "  {name:<20} noise {scale:>4.1}  luminance delta {:+.4}",
                mean_luminance(&dith) - mean_luminance(&orig)
            );

            let rgb = out.to_rgb_actual();

            for (bi, &(y0, y1)) in BANDS.iter().enumerate() {
                let (ow, oh) = (W * ZOOM, (y1 - y0) * ZOOM);
                let mut buf = vec![0u8; ow * oh * 3];
                for y in 0..oh {
                    for x in 0..ow {
                        let s = ((y0 + y / ZOOM) * W + x / ZOOM) * 3;
                        let d = (y * ow + x) * 3;
                        buf[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
                    }
                }
                write(&format!("noise{bi}-{name}-{scale:04.1}.png"), &buf, ow, oh);
            }
        }
    }
}

/// Locate horizontal seams in a smooth gradient.
///
/// Streaks across a gradient are far more objectionable than the dE they
/// cost, because the eye finds straight lines in smooth content instantly.
/// The input varies smoothly down the frame, so any sharp row-to-row jump in
/// the output is an artifact of the algorithm, not of the content.
///
/// Where it lands identifies the cause, so this reports the row rather than
/// just the fact:
///   - a multiple of 64 implicates the blue-noise jitter table, which is
///     indexed `[y % 64][x % 64]` and seams if it is not toroidal;
///   - the same row under every kernel implicates the noise table or the
///     palette decision boundary;
///   - a kernel-dependent row implicates the error buffer or serpentine.
#[test]
#[ignore = "diagnostic; run with --ignored --nocapture"]
fn visual_find_horizontal_seams() {
    const W: usize = 480;
    const H: usize = 320;
    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let hue = x as f32 / W as f32;
            let (r, g, b) = hsl_to_rgb(hue, 0.5, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let palette = panel();
    eprintln!("\n=== horizontal seams in the 50%-saturation field ===");
    eprintln!("(rows whose luminance error jumps most from the row above)\n");

    for &(name, algo) in ALGOS {
        let out = EinkDitherer::new(palette.clone())
            .algorithm(algo)
            .dither(&pixels, W, H);
        let idx = out.indices();

        // Per-row luminance error against the row's own input.
        let row_err: Vec<f32> = (0..H)
            .map(|y| {
                let mut got = 0.0;
                let mut want = 0.0;
                for x in 0..W {
                    let c = palette.actual_linear(idx[y * W + x] as usize);
                    got += 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
                    let s = LinearRgb::from(pixels[y * W + x]);
                    want += 0.2126 * s.r + 0.7152 * s.g + 0.0722 * s.b;
                }
                (got - want) / W as f32
            })
            .collect();

        let mut jumps: Vec<(f32, usize)> = (1..H)
            .map(|y| ((row_err[y] - row_err[y - 1]).abs(), y))
            .collect();
        jumps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        let mean_jump: f32 = jumps.iter().map(|j| j.0).sum::<f32>() / (H - 1) as f32;
        let top: Vec<String> = jumps
            .iter()
            .take(5)
            .map(|&(mag, y)| {
                let mult64 = if y % 64 == 0 { " [64x]" } else { "" };
                format!("y={y}{mult64} ({:.1}x)", mag / mean_jump)
            })
            .collect();
        eprintln!("  {name:<20} {}", top.join("  "));
    }

    // A seam confined to one hue range is invisible to a full-width row
    // average, which is most of them: the ink set changes with hue, so a
    // decision boundary sweeps across only part of the frame. Splitting into
    // hue bands is what makes a local streak measurable.
    const BANDS: usize = 16;
    let bw = W / BANDS;
    eprintln!("\nWorst *local* seam per kernel (hue band x row):");
    for &(name, algo) in ALGOS {
        let out = EinkDitherer::new(palette.clone())
            .algorithm(algo)
            .dither(&pixels, W, H);
        let idx = out.indices();

        let mut band_err = vec![vec![0.0f32; H]; BANDS];
        for (b, col) in band_err.iter_mut().enumerate() {
            for (y, cell) in col.iter_mut().enumerate() {
                let mut got = 0.0;
                let mut want = 0.0;
                for x in b * bw..(b + 1) * bw {
                    let c = palette.actual_linear(idx[y * W + x] as usize);
                    got += 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
                    let s = LinearRgb::from(pixels[y * W + x]);
                    want += 0.2126 * s.r + 0.7152 * s.g + 0.0722 * s.b;
                }
                *cell = (got - want) / bw as f32;
            }
        }

        let mut worst = (0.0f32, 0usize, 0usize);
        let mut total = 0.0f32;
        let mut n = 0usize;
        for (b, col) in band_err.iter().enumerate() {
            for y in 1..H {
                let d = (col[y] - col[y - 1]).abs();
                total += d;
                n += 1;
                if d > worst.0 {
                    worst = (d, b, y);
                }
            }
        }
        let mean = total / n as f32;
        let hue_deg = (worst.1 * bw + bw / 2) as f32 / W as f32 * 360.0;
        eprintln!(
            "  {name:<20} y={:<4} hue~{hue_deg:>5.0}deg  {:.1}x mean",
            worst.2,
            worst.0 / mean
        );
    }
    eprintln!();
}

/// Before/after for the one change the evidence currently supports.
///
/// Nothing in the ditherer has changed yet, so "after" here means the
/// proposed `noise_scale` defaults rather than committed work. The sweep
/// found raising them improves colour accuracy monotonically on both
/// reachable and gamut-limited targets while leaving sharp structure
/// untouched, which is a strong enough claim that it should be checked by
/// eye on real scenes before it lands.
///
/// Panels are original | before | after, at the same scale, so the question
/// "did it move toward the source" is answerable directly.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_before_after_noise_defaults() {
    // (label, algorithm, shipped default, proposed)
    let cases = [
        ("atkinson", DitherAlgorithm::Atkinson, 0.0f32, 8.0f32),
        ("atkinson-hybrid", DitherAlgorithm::AtkinsonHybrid, 0.0, 8.0),
        (
            "jarvis-judice-ninke",
            DitherAlgorithm::JarvisJudiceNinke,
            6.0,
            16.0,
        ),
    ];

    let mut scenes: Vec<(&str, Vec<Srgb>, usize, usize)> = Vec::new();

    const W: usize = 480;
    const H: usize = 320;
    let mut muted = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 0.5, l);
            muted.push(Srgb::new(r, g, b));
        }
    }
    scenes.push(("field", muted, W, H));

    let (sharp, sw, sh) = sharp_scene();
    scenes.push(("sharp", sharp, sw, sh));

    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../screens/builtin/calibration/color/photo.png");
    if let Ok(img) = image::open(&src) {
        let img = img.resize(480, 480, image::imageops::FilterType::Lanczos3);
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        scenes.push((
            "photo",
            rgb.pixels()
                .map(|p| Srgb::from_u8(p[0], p[1], p[2]))
                .collect(),
            w,
            h,
        ));
    }

    let palette = panel();
    eprintln!("\n=== before/after: proposed noise_scale defaults ===");
    for (scene, px, w, h) in &scenes {
        let orig: Vec<LinearRgb> = px.iter().map(|&s| LinearRgb::from(s)).collect();
        let oy = mean_luminance(&orig);
        for &(name, algo, before_scale, after_scale) in &cases {
            let render = |scale: f32| {
                let out = EinkDitherer::new(palette.clone())
                    .algorithm(algo)
                    .noise_scale(scale)
                    .dither(px, *w, *h);
                let lin: Vec<LinearRgb> = out
                    .indices()
                    .iter()
                    .map(|&i| palette.actual_linear(i as usize))
                    .collect();
                (out.to_rgb_actual(), mean_luminance(&lin))
            };
            let (before, by) = render(before_scale);
            let (after, ay) = render(after_scale);
            eprintln!(
                "  {scene:<6} {name:<20} luminance delta {:+.4} -> {:+.4}",
                by - oy,
                ay - oy
            );
            let (buf, ow, oh) = triptych(px, &before, &after, *w, *h);
            write(&format!("BA-{scene}-{name}.png"), &buf, ow, oh);

            // Full-frame is too small to judge a dot pattern. The artifacts
            // this change targets live at pixel scale, so crop and magnify
            // the bands that hold them: the limit-cycle streak through the
            // yellow field, and the flat area that herringbones.
            if *scene != "field" {
                continue;
            }
            // Stacked, not side by side: these bands are wide and short, so
            // laying them left/right gives an unreadable aspect ratio. Before
            // on top, after below, same columns aligned.
            const ZOOM: usize = 6;
            let regions = [
                ("wire", 55usize, 195usize, 180usize, 215usize),
                ("weave", 330, 470, 60, 105),
            ];
            for (tag, x0, x1, y0, y1) in regions {
                let (cw, ch) = (x1 - x0, y1 - y0);
                let (ow2, oh2) = (cw * ZOOM, ch * ZOOM * 2 + 8);
                let mut buf = vec![0x80u8; ow2 * oh2 * 3];
                for y in 0..ch * ZOOM {
                    for x in 0..ow2 {
                        let sd = ((y0 + y / ZOOM) * *w + x0 + x / ZOOM) * 3;
                        let db = (y * ow2 + x) * 3;
                        buf[db..db + 3].copy_from_slice(&before[sd..sd + 3]);
                        let da = ((y + ch * ZOOM + 8) * ow2 + x) * 3;
                        buf[da..da + 3].copy_from_slice(&after[sd..sd + 3]);
                    }
                }
                write(&format!("BAzoom-{tag}-{name}.png"), &buf, ow2, oh2);
            }
        }
    }
}

/// Visual golden: the hue x lightness field with and without gamut mapping.
///
/// Mean dE is *expected to worsen*; what to look for is banding turning into
/// gradation, and hue bands that used to collapse onto one ink separating.
/// Render the field and look — flat-patch dE cannot tell you this.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_gamut_mapping_before_after() {
    use eink_dither::{GamutMapper, GamutOptions};

    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 1.0, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let before = EinkDitherer::new(palette.clone())
        .dither(&pixels, W, H)
        .to_rgb_actual();

    let mut mapped = pixels.clone();
    let mask = vec![true; mapped.len()];
    GamutMapper::new(&palette).map_frame(&mut mapped, &mask, GamutOptions::default());
    let after = EinkDitherer::new(palette.clone())
        .dither(&mapped, W, H)
        .to_rgb_actual();

    let (buf, ow, oh) = triptych(&pixels, &before, &after, W, H);
    write("gamut-mapping-field.png", &buf, ow, oh);
    eprintln!("original | unmapped | mapped — inspect by eye");
}

/// Mixed content: the case the adaptation factor is supposed to handle, and
/// the case the superseded `c / R` form got wrong.
///
/// Saturation rises left to right, so the left half is comfortably inside the
/// gamut and the right half is well outside it, and a vivid block is planted
/// in the middle to pin `R` at its cap. Under the old curve the whole frame
/// was divided by `R` and the in-gamut left half went flat along with
/// everything else. Under the current curve it must come through untouched.
///
/// Rendered **before dithering** so the mapping is visible on its own — dither
/// noise otherwise masks exactly the difference being judged.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_gamut_mixed_content() {
    use eink_dither::gamut::adapt::adaptation_factor;
    use eink_dither::gamut::cmax::CmaxTable;
    use eink_dither::gamut::hull::Hull;
    use eink_dither::gamut::knee::compress_chroma;
    use eink_dither::{GamutMapper, GamutOptions, Oklab, Oklch};

    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.20 + 0.60 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let s = x as f32 / (W - 1) as f32;
            // Hue sweeps slowly so the frame stays readable as colour, not as
            // a rainbow; saturation is the axis under test.
            let (r, g, b) = hsl_to_rgb(0.08 + 0.5 * (y as f32 / (H - 1) as f32), s, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }
    // The vivid intruder: one saturated block, enough of the frame to survive
    // the 99th-percentile guard and drive R to its cap.
    for y in 40..120 {
        for x in 40..200 {
            let (r, g, b) = hsl_to_rgb(0.95, 1.0, 0.5);
            pixels[y * W + x] = Srgb::new(r, g, b);
        }
    }

    let mapper = GamutMapper::new(&palette);
    let opts = GamutOptions::default();

    let mut mapped = pixels.clone();
    let mask = vec![true; mapped.len()];
    mapper.map_frame(&mut mapped, &mask, opts);

    // The superseded curve, reconstructed here so the two can be seen side by
    // side: chroma divided by R *before* the knee, which is what made the
    // in-gamut left half go flat.
    let table = CmaxTable::build(&Hull::from_palette(&palette));
    let r = adaptation_factor(
        &mut pixels.iter().map(|c| mapper.rho(*c)).collect::<Vec<_>>(),
        opts.max_compression,
    );
    let old: Vec<Srgb> = pixels
        .iter()
        .map(|p| {
            let lch = Oklch::from(Oklab::from(LinearRgb::from(*p)));
            let c_max = table.sample(lch.h, lch.l);
            let c = compress_chroma(lch.c / r.max(1.0), c_max, opts.knee, 1.0);
            let lin = LinearRgb::from(Oklab::from(Oklch {
                l: lch.l,
                c: c.max(0.0),
                h: lch.h,
            }));
            Srgb::from(LinearRgb::new(
                lin.r.clamp(0.0, 1.0),
                lin.g.clamp(0.0, 1.0),
                lin.b.clamp(0.0, 1.0),
            ))
        })
        .collect();

    let to_rgb = |v: &[Srgb]| -> Vec<u8> {
        v.iter()
            .flat_map(|c| {
                let b = c.to_bytes();
                [b[0], b[1], b[2]]
            })
            .collect()
    };

    let (buf, ow, oh) = triptych(&pixels, &to_rgb(&old), &to_rgb(&mapped), W, H);
    write("gamut-mixed-content.png", &buf, ow, oh);
    eprintln!("source | OLD curve (c/R before knee) | NEW curve — R = {r:.3}");

    // Quantify it, because a 1.7% difference is not a judgement the eye makes
    // reliably — the lesson session 7 paid for.
    let chroma = |v: &[Srgb]| -> f32 {
        v.iter()
            .map(|c| Oklch::from(Oklab::from(LinearRgb::from(*c))).c)
            .sum::<f32>()
            / v.len() as f32
    };
    // The in-gamut left third, where the promise lives.
    let left = |v: &[Srgb]| -> Vec<Srgb> {
        (0..H)
            .flat_map(|y| (0..W / 3).map(move |x| (y, x)))
            .map(|(y, x)| v[y * W + x])
            .collect()
    };
    eprintln!(
        "  mean Oklab chroma  whole frame: source {:.4}  old {:.4}  new {:.4}",
        chroma(&pixels),
        chroma(&old),
        chroma(&mapped)
    );
    eprintln!(
        "  mean Oklab chroma  low-saturation left third: source {:.4}  old {:.4}  new {:.4}",
        chroma(&left(&pixels)),
        chroma(&left(&old)),
        chroma(&left(&mapped))
    );
}

/// The same comparison at three knee values, to pick one by eye.
#[test]
#[ignore = "writes PNGs; run with --ignored"]
fn visual_gamut_knee_sweep() {
    use eink_dither::{GamutMapper, GamutOptions};

    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();
    let mapper = GamutMapper::new(&palette);

    let mut pixels = Vec::with_capacity(W * H);
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl_to_rgb(x as f32 / W as f32, 1.0, l);
            pixels.push(Srgb::new(r, g, b));
        }
    }

    let baseline = EinkDitherer::new(palette.clone())
        .dither(&pixels, W, H)
        .to_rgb_actual();

    for knee in [0.4f32, 0.6, 0.8] {
        let mut mapped = pixels.clone();
        let mask = vec![true; mapped.len()];
        mapper.map_frame(
            &mut mapped,
            &mask,
            GamutOptions {
                knee,
                ..GamutOptions::default()
            },
        );
        let out = EinkDitherer::new(palette.clone())
            .dither(&mapped, W, H)
            .to_rgb_actual();
        let (buf, ow, oh) = triptych(&pixels, &baseline, &out, W, H);
        write(&format!("gamut-knee-{knee:.1}.png"), &buf, ow, oh);
    }
}
