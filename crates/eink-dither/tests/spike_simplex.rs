//! SPIKE: can the accuracy of mixture-aware selection be had without the
//! contours that hard candidate restriction draws?
//!
//! Throwaway code answering one question. It reimplements a minimal diffusion
//! loop rather than extending the real one, because the real one has no
//! selection hook and adding one is the design decision this spike informs.
//!
//! Story so far, all measured in this file:
//!
//! - Restricting candidates to the optimal mixture's support does NOT remove
//!   the scalloped arcs. That hypothesis is dead.
//! - Restriction *alone* is worse than production: it removes green, but
//!   black stays unreachable and red absorbs the slack (dE 0.0496 -> 0.0616).
//!   Green was partly standing in for the black that never arrives.
//! - Restriction *plus full propagation* is excellent on flat patches --
//!   black 1% -> 42% against an optimal 47%, and blue held exactly at the
//!   physical bound where full propagation alone collapses it to 80% black.
//! - But it bands smooth gradients, and the banding is intrinsic: refining
//!   the support lookup from 64 to 255 levels (8k -> 120k cache entries)
//!   changes nothing. Support membership is binary, so an ink appears or
//!   vanishes across a locus, and in a vertical gradient those are horizontal
//!   lines. Straight lines in smooth content are worse than the arcs.
//!
//! The question now: replace the binary gate with a continuous bias. Each
//! ink's distance is reduced in proportion to its weight in the optimal
//! mixture, so an ink fades in as its weight rises off zero instead of
//! switching on at a threshold. The optimum of the mixture problem moves
//! continuously with the target, so the bias field should be continuous and
//! leave no locus to contour along -- while a zero-weight ink is still
//! penalised enough not to be chosen, which is what made full propagation
//! safe.
//!
//! `lambda = 0` is production selection; large lambda approaches the hard
//! gate. If accuracy survives at a lambda whose renders stay clean, the
//! approach is viable; if accuracy only arrives together with the contours,
//! it is not.
//!
//! Run: cargo test -p eink-dither --test spike_simplex -- --ignored --nocapture

use eink_dither::{LinearRgb, Oklab, Palette, Srgb};
use std::collections::HashMap;
use std::path::PathBuf;

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

const NAMES: [&str; 6] = ["blk", "wht", "red", "yel", "blu", "grn"];

/// Optimal convex mixture of the palette's actual colours, by coordinate
/// descent from a deterministic spread of starts.
fn best_mixture(palette: &Palette, target: Oklab) -> Vec<f32> {
    let n = palette.len();
    let cost = |w: &[f32]| -> f32 {
        let total: f32 = w.iter().sum();
        if total <= 0.0 {
            return f32::MAX;
        }
        let mut mix = [0.0f32; 3];
        for (i, &wi) in w.iter().enumerate() {
            let c = palette.actual_linear(i);
            mix[0] += wi * c.r;
            mix[1] += wi * c.g;
            mix[2] += wi * c.b;
        }
        let mix = LinearRgb::new(mix[0] / total, mix[1] / total, mix[2] / total);
        Oklab::from(mix).distance_squared(target).sqrt()
    };

    let mut starts: Vec<Vec<f32>> = (0..n)
        .map(|i| {
            let mut w = vec![0.0; n];
            w[i] = 1.0;
            w
        })
        .collect();
    starts.push(vec![1.0 / n as f32; n]);

    let mut best = starts[0].clone();
    let mut best_c = cost(&best);
    for start in starts {
        let mut w = start;
        let mut c = cost(&w);
        let mut step = 0.5f32;
        while step > 1e-3 {
            let mut improved = false;
            for i in 0..n {
                for dir in [step, -step] {
                    let old = w[i];
                    w[i] = (w[i] + dir).max(0.0);
                    let nc = cost(&w);
                    if nc < c {
                        c = nc;
                        improved = true;
                    } else {
                        w[i] = old;
                    }
                }
            }
            if !improved {
                step *= 0.5;
            }
        }
        if c < best_c {
            best_c = c;
            best = w;
        }
    }
    let total: f32 = best.iter().sum();
    best.iter().map(|w| w / total).collect()
}

/// Optimal mixture weights, cached on a quantisation of the source colour.
struct MixtureCache {
    palette: Palette,
    map: HashMap<(u8, u8, u8), Vec<f32>>,
    levels: f32,
}

impl MixtureCache {
    fn new(palette: Palette) -> Self {
        Self::with_levels(palette, 63.0)
    }

    fn with_levels(palette: Palette, levels: f32) -> Self {
        Self {
            palette,
            map: HashMap::new(),
            levels,
        }
    }

    fn weights(&mut self, c: Srgb) -> &[f32] {
        let lv = self.levels;
        let key = (
            (c.r.clamp(0.0, 1.0) * lv).round() as u8,
            (c.g.clamp(0.0, 1.0) * lv).round() as u8,
            (c.b.clamp(0.0, 1.0) * lv).round() as u8,
        );
        let palette = &self.palette;
        self.map.entry(key).or_insert_with(|| {
            let q = Srgb::new(key.0 as f32 / lv, key.1 as f32 / lv, key.2 as f32 / lv);
            best_mixture(palette, Oklab::from(LinearRgb::from(q)))
        })
    }
}

#[derive(Clone, Copy)]
struct Arm {
    /// Hard gate: drop inks whose mixture weight is at or below `threshold`.
    restrict: bool,
    threshold: f32,
    /// Soft bias: subtract `lambda * weight` from each ink's distance.
    /// Continuous in the weight, so no threshold to contour along.
    lambda: f32,
    full_propagation: bool,
    jitter: f32,
}

impl Arm {
    fn production() -> Self {
        Self {
            restrict: false,
            threshold: 0.0,
            lambda: 0.0,
            full_propagation: false,
            jitter: 0.0,
        }
    }
    fn soft(lambda: f32) -> Self {
        Self {
            restrict: false,
            threshold: 0.0,
            lambda,
            full_propagation: true,
            jitter: 2.0,
        }
    }
    fn hard() -> Self {
        Self {
            restrict: true,
            threshold: 0.02,
            lambda: 0.0,
            full_propagation: true,
            jitter: 2.0,
        }
    }
}

fn dither(
    palette: &Palette,
    cache: &mut MixtureCache,
    image: &[Srgb],
    w: usize,
    h: usize,
    arm: Arm,
) -> Vec<u8> {
    // Atkinson: 6 neighbours of weight 1, divisor 8 (25% discarded).
    const KA: [(i32, i32, f32); 6] = [
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ];
    const DIVA: f32 = 8.0;
    // Floyd-Steinberg: full propagation.
    const KF: [(i32, i32, f32); 4] = [(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)];
    const DIVF: f32 = 16.0;
    const CLAMP: f32 = 1.0;

    let (kern, div): (&[(i32, i32, f32)], f32) = if arm.full_propagation {
        (&KF, DIVF)
    } else {
        (&KA, DIVA)
    };

    let mut err = vec![[0.0f32; 3]; w * (h + 2)];
    let mut out = vec![0u8; w * h];

    for y in 0..h {
        let reverse = y % 2 == 1;
        let xs: Vec<usize> = if reverse {
            (0..w).rev().collect()
        } else {
            (0..w).collect()
        };
        for x in xs {
            let idx = y * w + x;
            let src = image[idx];
            let lin = LinearRgb::from(src);
            let e = err[idx];
            let px = LinearRgb::new(
                lin.r + e[0].clamp(-CLAMP, CLAMP),
                lin.g + e[1].clamp(-CLAMP, CLAMP),
                lin.b + e[2].clamp(-CLAMP, CLAMP),
            );
            let ok = Oklab::from(px);

            // Weights come from the *content*, not the accumulated error --
            // otherwise the guidance would drift with the error it bounds.
            let need_w = arm.restrict || arm.lambda > 0.0;
            let weights: Vec<f32> = if need_w {
                cache.weights(src).to_vec()
            } else {
                vec![0.0; palette.len()]
            };

            let mut best = usize::MAX;
            let mut best_d = f32::MAX;
            for (i, &wi) in weights.iter().enumerate() {
                if arm.restrict && wi <= arm.threshold {
                    continue;
                }
                let mut d = ok.distance_squared(palette.actual_oklab(i)).sqrt();
                if arm.lambda > 0.0 {
                    d -= arm.lambda * wi;
                }
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            if best == usize::MAX {
                best = 0;
            }
            out[idx] = best as u8;

            let c = palette.actual_linear(best);
            let diff = [px.r - c.r, px.g - c.g, px.b - c.b];

            let hshift = if arm.jitter > 0.0 {
                let mut hsh =
                    (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
                hsh ^= hsh >> 15;
                hsh = hsh.wrapping_mul(0x2545_F491);
                hsh ^= hsh >> 13;
                ((hsh & 0xFFFF) as f32 / 65535.0 - 0.5) * arm.jitter
            } else {
                0.0
            };

            for &(dx, dy, wt) in kern {
                let wt = if (dx, dy) == (1, 0) {
                    (wt - hshift).max(0.0)
                } else if (dx, dy) == (0, 1) {
                    (wt + hshift).max(0.0)
                } else {
                    wt
                };
                let ex = if reverse { -dx } else { dx };
                let nx = x as i32 + ex;
                let ny = y + dy as usize;
                if nx < 0 || nx as usize >= w || ny >= h {
                    continue;
                }
                let ni = ny * w + nx as usize;
                for ch in 0..3 {
                    err[ni][ch] += diff[ch] * wt / div;
                }
            }
        }
    }
    out
}

fn muted_field(w: usize, h: usize) -> Vec<Srgb> {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let l = 0.12 + 0.76 * (y as f32 / (h - 1) as f32);
        for x in 0..w {
            let (r, g, b) = hsl(x as f32 / w as f32, 0.5, l);
            px.push(Srgb::new(r, g, b));
        }
    }
    px
}

fn hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
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
    let m = l - c / 2.0;
    (
        (r1 + m).clamp(0.0, 1.0),
        (g1 + m).clamp(0.0, 1.0),
        (b1 + m).clamp(0.0, 1.0),
    )
}

fn rgb_of(palette: &Palette, idx: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(idx.len() * 3);
    for &i in idx {
        let c = palette.actual(i as usize);
        v.push((c.r.clamp(0.0, 1.0) * 255.0).round() as u8);
        v.push((c.g.clamp(0.0, 1.0) * 255.0).round() as u8);
        v.push((c.b.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    v
}

fn out_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dither-compare");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn arms() -> Vec<(String, Arm)> {
    let mut v: Vec<(String, Arm)> = Vec::new();
    v.push(("production".to_string(), Arm::production()));
    v.push(("hard gate".to_string(), Arm::hard()));
    for l in [0.02f32, 0.05, 0.1, 0.2, 0.4] {
        v.push((format!("soft l={l:.2}"), Arm::soft(l)));
    }
    v
}

/// Accuracy across lambda, on the patches whose failures are known.
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_soft_bias_accuracy() {
    let palette = panel();
    let mut cache = MixtureCache::new(palette.clone());
    const P: usize = 32;
    let arms = arms();

    eprintln!("\n=== soft mixture bias: accuracy ===");
    for &(hue, l) in &[(30i32, 0.20f32), (45, 0.32), (240, 0.44), (255, 0.44)] {
        let (r, g, b) = hsl(hue as f32 / 360.0, 1.0, l);
        let src = Srgb::new(r, g, b);
        let px = vec![src; P * P];
        let target = Oklab::from(LinearRgb::from(src));
        let w = best_mixture(&palette, target);
        eprintln!(
            "\n  hue {hue}deg L {l:.2}   optimal {}",
            (0..6)
                .map(|i| format!("{}:{:>3.0}%", NAMES[i], w[i] * 100.0))
                .collect::<Vec<_>>()
                .join(" ")
        );
        for (label, arm) in &arms {
            let out = dither(&palette, &mut cache, &px, P, P, *arm);
            let mut hist = [0usize; 6];
            let mut acc = [0.0f32; 3];
            for &i in &out {
                hist[i as usize] += 1;
                let c = palette.actual_linear(i as usize);
                acc[0] += c.r;
                acc[1] += c.g;
                acc[2] += c.b;
            }
            let n = (P * P) as f32;
            let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
            eprintln!(
                "    {label:<12} {}  dE {:.4}",
                (0..6)
                    .map(|i| format!("{}:{:>3.0}%", NAMES[i], hist[i] as f32 / n * 100.0))
                    .collect::<Vec<_>>()
                    .join(" "),
                avg.distance_squared(target).sqrt()
            );
        }
    }
}

/// The other half: does a lambda that buys accuracy stay clean on a gradient,
/// or does the accuracy only arrive together with the contours?
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_soft_bias_render() {
    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();
    let px = muted_field(W, H);
    let mut cache = MixtureCache::new(palette.clone());

    let arms: Vec<(String, Arm)> = vec![
        ("production".to_string(), Arm::production()),
        ("hard".to_string(), Arm::hard()),
        // Below 0.2 the blue still collapses, so only the arms that pass the
        // accuracy bar are worth rendering.
        ("soft-0.20".to_string(), Arm::soft(0.20)),
        ("soft-0.40".to_string(), Arm::soft(0.40)),
    ];

    let mut imgs = Vec::new();
    for (label, arm) in &arms {
        let idx = dither(&palette, &mut cache, &px, W, H, *arm);
        imgs.push((label.clone(), rgb_of(&palette, &idx)));
    }

    const GAP: usize = 8;
    let ow = W * imgs.len() + GAP * (imgs.len() - 1);
    let mut buf = vec![0x80u8; ow * H * 3];
    for y in 0..H {
        for x in 0..W {
            let s = (y * W + x) * 3;
            for (i, (_, img)) in imgs.iter().enumerate() {
                let d = (y * ow + x + i * (W + GAP)) * 3;
                buf[d..d + 3].copy_from_slice(&img[s..s + 3]);
            }
        }
    }
    let p = out_dir().join("SPIKE-soft-full.png");
    image::save_buffer(&p, &buf, ow as u32, H as u32, image::ColorType::Rgb8).unwrap();
    let names: Vec<&str> = arms.iter().map(|(l, _)| l.as_str()).collect();
    eprintln!("  wrote {} ({})", p.display(), names.join(" | "));

    // Magnified crop of the teal band, where the hard gate drew its
    // horizontal red segments. Stacked, one arm per row, same order.
    const ZOOM: usize = 4;
    let (x0, x1, y0, y1) = (200usize, 400usize, 85usize, 150usize);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let ow2 = cw * ZOOM;
    let oh2 = ch * ZOOM * imgs.len() + GAP * (imgs.len() - 1);
    let mut buf2 = vec![0x80u8; ow2 * oh2 * 3];
    for (i, (_, img)) in imgs.iter().enumerate() {
        let y_off = i * (ch * ZOOM + GAP);
        for y in 0..ch * ZOOM {
            for x in 0..ow2 {
                let s = ((y0 + y / ZOOM) * W + x0 + x / ZOOM) * 3;
                let d = ((y_off + y) * ow2 + x) * 3;
                buf2[d..d + 3].copy_from_slice(&img[s..s + 3]);
            }
        }
    }
    let p2 = out_dir().join("SPIKE-soft-crop.png");
    image::save_buffer(&p2, &buf2, ow2 as u32, oh2 as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {}", p2.display());
}

/// Are the mixture weights actually a continuous function of the target?
///
/// The soft bias assumed they are: that is the whole premise for expecting no
/// contours. If the weights jump as lightness varies smoothly, then the bias
/// field jumps too, and every member of this family -- gate or bias -- will
/// draw a line wherever they do. That distinguishes "my solver is noisy" from
/// "the approach cannot work".
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_are_mixture_weights_continuous() {
    let palette = panel();
    eprintln!("\n=== mixture weights down a constant-hue column (hue 150deg) ===");
    eprintln!("  L      {}", NAMES.join("    "));
    let mut prev: Option<Vec<f32>> = None;
    let mut worst = (0.0f32, 0.0f32);
    for step in 0..48 {
        let l = 0.12 + 0.76 * (step as f32 / 47.0);
        let (r, g, b) = hsl(150.0 / 360.0, 0.5, l);
        let w = best_mixture(&palette, Oklab::from(LinearRgb::from(Srgb::new(r, g, b))));
        let jump = prev
            .as_ref()
            .map(|p| {
                w.iter()
                    .zip(p.iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max)
            })
            .unwrap_or(0.0);
        if jump > worst.0 {
            worst = (jump, l);
        }
        let flag = if jump > 0.15 { "  <-- JUMP" } else { "" };
        eprintln!(
            "  {l:.3}  {}{flag}",
            w.iter()
                .map(|x| format!("{:>5.2}", x))
                .collect::<Vec<_>>()
                .join(" ")
        );
        prev = Some(w);
    }
    eprintln!(
        "\n  largest single-step weight change: {:.3} at L={:.3}",
        worst.0, worst.1
    );
    eprintln!("  (steps are 0.016 apart in lightness; a smooth field would stay well under 0.1)");
}

/// Is the soft arm's banding the lookup quantisation rather than the method?
///
/// The weights themselves are continuous (see the column dump), so a smooth
/// bias field is available in principle. But the lookup is quantised to 64
/// levels per channel, which makes the bias piecewise constant, and in a
/// gradient those steps are horizontal lines. That confound was excluded for
/// the hard gate and never tested for the soft bias.
///
/// If refining to 255 levels cleans it up, the method is viable and the LUT
/// resolution is a design parameter. If not, the family is finished.
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_soft_bias_quantisation() {
    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();
    let px = muted_field(W, H);

    let mut imgs = Vec::new();
    for (label, levels) in [("q64", 63.0f32), ("q255", 255.0)] {
        let mut cache = MixtureCache::with_levels(palette.clone(), levels);
        let idx = dither(&palette, &mut cache, &px, W, H, Arm::soft(0.40));
        eprintln!("  soft-0.40 {label}: {} cache entries", cache.map.len());
        imgs.push(rgb_of(&palette, &idx));
    }

    const GAP: usize = 8;
    const ZOOM: usize = 4;
    let (x0, x1, y0, y1) = (200usize, 400usize, 85usize, 150usize);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let ow = cw * ZOOM;
    let oh = ch * ZOOM * imgs.len() + GAP * (imgs.len() - 1);
    let mut buf = vec![0x80u8; ow * oh * 3];
    for (i, img) in imgs.iter().enumerate() {
        let y_off = i * (ch * ZOOM + GAP);
        for y in 0..ch * ZOOM {
            for x in 0..ow {
                let s = ((y0 + y / ZOOM) * W + x0 + x / ZOOM) * 3;
                let d = ((y_off + y) * ow + x) * 3;
                buf[d..d + 3].copy_from_slice(&img[s..s + 3]);
            }
        }
    }
    let p = out_dir().join("SPIKE-soft-quant.png");
    image::save_buffer(&p, &buf, ow as u32, oh as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {} (q64 top, q255 bottom)", p.display());
}
