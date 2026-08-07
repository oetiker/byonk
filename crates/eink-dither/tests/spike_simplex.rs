//! SPIKE: does restricting the candidate set to the optimal mixture's support
//! remove the scalloped arcs at ink-set boundaries?
//!
//! This is throwaway code answering one question. It reimplements a minimal
//! Atkinson loop rather than extending the real one, because the real one has
//! no candidate-restriction hook and adding one is the design decision this
//! spike exists to inform.
//!
//! The claim under test, from approach B: the arcs form because greedy
//! nearest-ink selection changes its support *discontinuously* as the target
//! moves, so a new ink appears abruptly along a locus and the diffusion rings
//! against it. If instead the candidate set is the support of the optimal
//! mixture, an ink should enter only as its weight rises off zero -- i.e.
//! continuously -- and there should be no front to scallop.
//!
//! The alternative outcome, which would sink approach B, is that the support
//! changes just as abruptly and only moves the arcs somewhere else.
//!
//! Run: cargo test -p eink-dither --test spike_simplex -- --ignored --nocapture

use eink_dither::{DitherAlgorithm, EinkDitherer, LinearRgb, Oklab, Palette, Srgb};
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

/// Optimal convex mixture of the palette's actual colours, by coordinate
/// descent from a deterministic spread of starts. Same shape as
/// `best_reachable` in the domain tests.
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

    let mut starts: Vec<Vec<f32>> = Vec::new();
    for i in 0..n {
        let mut w = vec![0.0; n];
        w[i] = 1.0;
        starts.push(w);
    }
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

/// Candidate set = inks the optimal mixture actually uses.
///
/// Cached on a fine quantisation of the source colour. The quantisation is
/// deliberately fine (64 levels/channel): a coarse grid would introduce its
/// own discontinuities and could manufacture exactly the seam this spike is
/// trying to detect the absence of.
struct SupportCache {
    palette: Palette,
    map: HashMap<(u8, u8, u8), Vec<usize>>,
    threshold: f32,
    levels: f32,
}

impl SupportCache {
    fn new(palette: Palette, threshold: f32) -> Self {
        Self::with_levels(palette, threshold, 63.0)
    }

    fn with_levels(palette: Palette, threshold: f32, levels: f32) -> Self {
        Self {
            palette,
            map: HashMap::new(),
            threshold,
            levels,
        }
    }

    fn support(&mut self, c: Srgb) -> &[usize] {
        let lv = self.levels;
        let key = (
            (c.r.clamp(0.0, 1.0) * lv).round() as u8,
            (c.g.clamp(0.0, 1.0) * lv).round() as u8,
            (c.b.clamp(0.0, 1.0) * lv).round() as u8,
        );
        let palette = &self.palette;
        let threshold = self.threshold;
        self.map.entry(key).or_insert_with(|| {
            let q = Srgb::new(key.0 as f32 / lv, key.1 as f32 / lv, key.2 as f32 / lv);
            let target = Oklab::from(LinearRgb::from(q));
            let w = best_mixture(palette, target);
            let mut s: Vec<usize> = (0..palette.len()).filter(|&i| w[i] > threshold).collect();
            if s.is_empty() {
                s.push(0);
            }
            s
        })
    }
}

/// Minimal Atkinson error diffusion with a restricted candidate set.
///
/// `restrict = false` reproduces the production selection rule (all inks), so
/// the two arms differ in exactly one thing.
fn dither_restricted(
    palette: &Palette,
    cache: &mut SupportCache,
    image: &[Srgb],
    w: usize,
    h: usize,
    restrict: bool,
    full_propagation: bool,
    jitter: f32,
) -> Vec<u8> {
    // Atkinson: 6 neighbours of weight 1, divisor 8 (25% discarded).
    const K: [(i32, i32, f32); 6] = [
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ];
    const DIV: f32 = 8.0;
    // Floyd-Steinberg: full propagation, for the arm that pairs restriction
    // with an accumulator that can actually reach a distant ink.
    const KF: [(i32, i32, f32); 4] = [(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)];
    const DIVF: f32 = 16.0;
    const CLAMP: f32 = 1.0;
    let (kern, div): (&[(i32, i32, f32)], f32) = if full_propagation {
        (&KF, DIVF)
    } else {
        (&K, DIV)
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

            // The support is a property of the *content*, not of the
            // accumulated error -- otherwise the restriction would drift with
            // the error it is meant to bound.
            let candidates: Vec<usize> = if restrict {
                cache.support(src).to_vec()
            } else {
                (0..palette.len()).collect()
            };

            let mut best = candidates[0];
            let mut best_d = f32::MAX;
            for &i in &candidates {
                let d = ok.distance_squared(palette.actual_oklab(i));
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            out[idx] = best as u8;

            let c = palette.actual_linear(best);
            let diff = [px.r - c.r, px.g - c.g, px.b - c.b];
            // Production applies blue-noise jitter (now 8.0); the spike ran
            // with none, which on its own is known to make error diffusion
            // worm. A hash is white noise, not blue, so this understates the
            // real cure -- but it is enough to see how much of the mess is
            // simply the missing jitter.
            let hshift = if jitter > 0.0 {
                let mut hsh =
                    (x as u32).wrapping_mul(0x9E3779B1) ^ (y as u32).wrapping_mul(0x85EBCA77);
                hsh ^= hsh >> 15;
                hsh = hsh.wrapping_mul(0x2545F491);
                hsh ^= hsh >> 13;
                ((hsh & 0xFFFF) as f32 / 65535.0 - 0.5) * jitter
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
            let hue = x as f32 / w as f32;
            let (r, g, b) = hsl(hue, 0.5, l);
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

const SUPPORT_THRESHOLD: f32 = 0.02;

#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_arcs_under_support_restriction() {
    const W: usize = 480;
    const H: usize = 320;
    // Sweeping both suspected confounds: the missing jitter, and the support
    // threshold, whose hard on/off is what could band a smooth gradient.
    const ARM_JITTER: f32 = 2.0;
    let palette = panel();
    let px = muted_field(W, H);
    let mut cache = SupportCache::new(palette.clone(), SUPPORT_THRESHOLD);

    let base = dither_restricted(&palette, &mut cache, &px, W, H, false, false, 0.0);
    let restr = dither_restricted(&palette, &mut cache, &px, W, H, true, true, ARM_JITTER);
    eprintln!("  support cache entries: {}", cache.map.len());

    // Stacked crops of the band that holds the scalloped front, unrestricted
    // above and restricted below.
    const ZOOM: usize = 4;
    let (x0, x1, y0, y1) = (40usize, 300usize, 55usize, 145usize);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let (ow, oh) = (cw * ZOOM, ch * ZOOM * 2 + 8);
    let mut buf = vec![0x80u8; ow * oh * 3];
    let a = rgb_of(&palette, &base);
    let b = rgb_of(&palette, &restr);
    for y in 0..ch * ZOOM {
        for x in 0..ow {
            let s = ((y0 + y / ZOOM) * W + x0 + x / ZOOM) * 3;
            let d1 = (y * ow + x) * 3;
            buf[d1..d1 + 3].copy_from_slice(&a[s..s + 3]);
            let d2 = ((y + ch * ZOOM + 8) * ow + x) * 3;
            buf[d2..d2 + 3].copy_from_slice(&b[s..s + 3]);
        }
    }
    let p = out_dir().join("SPIKE-arcs.png");
    image::save_buffer(&p, &buf, ow as u32, oh as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {}", p.display());

    // Whole-frame pair too, since a fix that merely relocates the front would
    // look like a win in one crop.
    let mut full = vec![0x80u8; (W * 2 + 8) * H * 3];
    for y in 0..H {
        for x in 0..W {
            let s = (y * W + x) * 3;
            let d1 = (y * (W * 2 + 8) + x) * 3;
            full[d1..d1 + 3].copy_from_slice(&a[s..s + 3]);
            let d2 = (y * (W * 2 + 8) + x + W + 8) * 3;
            full[d2..d2 + 3].copy_from_slice(&b[s..s + 3]);
        }
    }
    let p2 = out_dir().join("SPIKE-arcs-full.png");
    image::save_buffer(
        &p2,
        &full,
        (W * 2 + 8) as u32,
        H as u32,
        image::ColorType::Rgb8,
    )
    .unwrap();
    eprintln!("  wrote {}", p2.display());
}

/// The quantitative half: do the known-bad patches improve too?
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_patches_under_support_restriction() {
    let palette = panel();
    let mut cache = SupportCache::new(palette.clone(), 0.02);
    let names = ["blk", "wht", "red", "yel", "blu", "grn"];
    const P: usize = 32;

    eprintln!("\n=== support-restricted vs production, on the known-bad patches ===");
    for &(hue, l) in &[(30i32, 0.20f32), (45, 0.32), (240, 0.44), (255, 0.44)] {
        let (r, g, b) = hsl(hue as f32 / 360.0, 1.0, l);
        let src = Srgb::new(r, g, b);
        let px = vec![src; P * P];
        let target = Oklab::from(LinearRgb::from(src));
        let w = best_mixture(&palette, target);
        let want: Vec<String> = (0..palette.len())
            .map(|i| format!("{}:{:>3.0}%", names[i], w[i] * 100.0))
            .collect();
        eprintln!("\n  hue {hue}deg L {l:.2}");
        eprintln!("    optimal    {}", want.join("  "));

        for (label, restrict, fullprop) in [
            ("production      ", false, false),
            ("restricted      ", true, false),
            ("floyd           ", false, true),
            ("restricted+floyd", true, true),
        ] {
            let out = dither_restricted(&palette, &mut cache, &px, P, P, restrict, fullprop, 0.0);
            let mut hist = vec![0usize; palette.len()];
            for &i in &out {
                hist[i as usize] += 1;
            }
            let n = (P * P) as f32;
            let got: Vec<String> = hist
                .iter()
                .map(|&c| format!("{:>3.0}%", c as f32 / n * 100.0))
                .collect();
            let mut acc = [0.0f32; 3];
            for &i in &out {
                let c = palette.actual_linear(i as usize);
                acc[0] += c.r;
                acc[1] += c.g;
                acc[2] += c.b;
            }
            let avg = Oklab::from(LinearRgb::new(acc[0] / n, acc[1] / n, acc[2] / n));
            eprintln!(
                "    {label} {}   dE {:.4}",
                got.iter()
                    .zip(names.iter())
                    .map(|(g, nm)| format!("{nm}:{g}"))
                    .collect::<Vec<_>>()
                    .join("  "),
                avg.distance_squared(target).sqrt()
            );
        }
    }

    // Guard against the obvious way this could be cheating: if restriction
    // helps only because it happens to disable dithering, the sharp scene and
    // the gradient would both go flat. Report the ink count actually used.
    let px = muted_field(240, 160);
    for (label, restrict) in [("production", false), ("restricted", true)] {
        let out = dither_restricted(&palette, &mut cache, &px, 240, 160, restrict, false, 0.0);
        let mut seen = vec![0usize; palette.len()];
        for &i in &out {
            seen[i as usize] += 1;
        }
        let used = seen.iter().filter(|&&c| c > 0).count();
        eprintln!("\n  field, {label}: {used} of 6 inks used, counts {seen:?}");
    }

    let _ = EinkDitherer::new(palette).algorithm(DitherAlgorithm::Atkinson);
}

/// Is the horizontal banding intrinsic to hard candidate restriction, or an
/// artifact of quantising the support lookup?
///
/// Lightness is constant along a row, so a band is a locus where the support
/// set changes. If it is the 64-level grid, refining to 255 levels moves the
/// bands and multiplies them into invisibility. If it is intrinsic -- the
/// support genuinely switching on and off at a real boundary -- refining
/// changes nothing, and hard restriction cannot be used on smooth content.
#[test]
#[ignore = "spike; run with --ignored --nocapture"]
fn spike_is_the_banding_intrinsic() {
    const W: usize = 480;
    const H: usize = 320;
    let palette = panel();
    let px = muted_field(W, H);

    let mut out = Vec::new();
    for (label, levels, threshold) in [
        ("q64  t0.02", 63.0f32, 0.02f32),
        ("q255 t0.02", 255.0, 0.02),
        ("q255 t0.001", 255.0, 0.001),
    ] {
        let mut cache = SupportCache::with_levels(palette.clone(), threshold, levels);
        let idx = dither_restricted(&palette, &mut cache, &px, W, H, true, true, 2.0);
        eprintln!("  {label}: {} cache entries", cache.map.len());
        out.push(rgb_of(&palette, &idx));
    }

    // Magnified crop of the teal band, where the horizontal red segments sit.
    // Stacked, one arm per row.
    const GAP: usize = 8;
    const ZOOM: usize = 4;
    let (x0, x1, y0, y1) = (200usize, 400usize, 85usize, 150usize);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let ow = cw * ZOOM;
    let oh = ch * ZOOM * out.len() + GAP * (out.len() - 1);
    let mut buf = vec![0x80u8; ow * oh * 3];
    for (i, img) in out.iter().enumerate() {
        let y_off = i * (ch * ZOOM + GAP);
        for y in 0..ch * ZOOM {
            for x in 0..ow {
                let s = ((y0 + y / ZOOM) * W + x0 + x / ZOOM) * 3;
                let d = ((y_off + y) * ow + x) * 3;
                buf[d..d + 3].copy_from_slice(&img[s..s + 3]);
            }
        }
    }
    let p = out_dir().join("SPIKE-banding.png");
    image::save_buffer(&p, &buf, ow as u32, oh as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {}", p.display());
}
