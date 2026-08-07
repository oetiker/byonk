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
    eprintln!("\n=== upper-field crop, rows {Y0}..{Y1}, {ZOOM}x ===");
    for &(name, algo) in ALGOS {
        let out = EinkDitherer::new(palette.clone())
            .algorithm(algo)
            .dither(&pixels, W, H);
        let rgb = out.to_rgb_actual();

        let ch = Y1 - Y0;
        let (ow, oh) = (W * ZOOM, ch * ZOOM);
        let mut buf = vec![0u8; ow * oh * 3];
        for y in 0..oh {
            for x in 0..ow {
                let sx = x / ZOOM;
                let sy = Y0 + y / ZOOM;
                let s = (sy * W + sx) * 3;
                let d = (y * ow + x) * 3;
                buf[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
            }
        }
        write(&format!("crop-{name}.png"), &buf, ow, oh);
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
    eprintln!("\n=== noise_scale sweep ===");
    for (name, algo) in [
        ("atkinson", DitherAlgorithm::Atkinson),
        ("jarvis-judice-ninke", DitherAlgorithm::JarvisJudiceNinke),
    ] {
        for scale in [0.0f32, 4.0, 8.0, 16.0] {
            let out = EinkDitherer::new(palette.clone())
                .algorithm(algo)
                .noise_scale(scale)
                .dither(&pixels, W, H);
            let rgb = out.to_rgb_actual();

            let ch = Y1 - Y0;
            let (ow, oh) = (W * ZOOM, ch * ZOOM);
            let mut buf = vec![0u8; ow * oh * 3];
            for y in 0..oh {
                for x in 0..ow {
                    let s = ((Y0 + y / ZOOM) * W + x / ZOOM) * 3;
                    let d = (y * ow + x) * 3;
                    buf[d..d + 3].copy_from_slice(&rgb[s..s + 3]);
                }
            }
            write(&format!("noise-{name}-{scale:04.1}.png"), &buf, ow, oh);
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
