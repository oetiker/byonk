//! **Prototype, not production.** Does anchoring the compression direction at
//! the gamut cusp beat compressing chroma at fixed lightness?
//!
//! Session 8 established that a chroma-only mapper cannot reach the panel's
//! yellow ink: at yellow's own lightness the reachable chroma is 0.073 against
//! the ink's 0.197, so saturated yellow washes out to cream. But 0.028 of
//! lightness lower there is a point at 97% of the ink's chroma. Fixed-lightness
//! compression is structurally unable to go there because it refuses to move
//! `L`.
//!
//! This file implements the general form — compress along a line converging on
//! an anchor on the neutral axis — and measures three anchor choices against
//! the current mapper on identical input. The knee curve, the adaptation factor
//! and the hull are all reused unchanged; only the *direction* differs.
//!
//! `Anchor::FixedL` reproduces production exactly (`compress_chroma` is
//! homogeneous, so compressing the ray parameter and compressing chroma are the
//! same operation when the ray is horizontal). It is included as a self-check:
//! if it ever disagrees with `GamutMapper`, this harness is wrong, not the
//! finding.
//!
//! Run with:
//!     cargo test -p eink-dither --test gamut_cusp_prototype -- --ignored --nocapture

use eink_dither::gamut::adapt::adaptation_factor;
use eink_dither::gamut::cmax::{CmaxTable, LIGHTNESS_BINS};
use eink_dither::gamut::hull::Hull;
use eink_dither::gamut::knee::compress_chroma;
use eink_dither::{
    EinkDitherer, GamutMapper, GamutOptions, LinearRgb, Oklab, Oklch, Palette, Srgb,
};
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

/// Where on the neutral axis the compression lines converge.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Anchor {
    /// The source colour's own lightness — horizontal lines. This is what
    /// production does today.
    FixedL,
    /// The lightness of the gamut cusp at this hue: the lightness at which the
    /// panel can be most colourful for that hue.
    CuspL,
    /// Mid grey, regardless of hue. The naive choice, included because it costs
    /// nothing to test and would be simpler if it won.
    MidGrey,
    /// Half way between the source lightness and mid grey. Mid-grey anchoring
    /// keeps the most colour but moves lightness hard (sRGB green drops 0.26);
    /// this asks what a bounded excursion buys back.
    HalfWay,
}

impl Anchor {
    fn name(self) -> &'static str {
        match self {
            Anchor::FixedL => "fixed-L (production)",
            Anchor::CuspL => "cusp-L",
            Anchor::MidGrey => "mid-grey",
            Anchor::HalfWay => "half-way to mid",
        }
    }
}

/// Lightness of the most chromatic reachable point at each hue.
///
/// Derived by `argmax` over the existing `CmaxTable` — no new geometry.
struct CuspTable {
    /// Cusp lightness per hue bin of the source table.
    l: Vec<f32>,
    /// Cusp chroma per hue bin, kept for reporting.
    c: Vec<f32>,
    bins: usize,
}

impl CuspTable {
    fn build(table: &CmaxTable, bins: usize) -> Self {
        let mut l = Vec::with_capacity(bins);
        let mut c = Vec::with_capacity(bins);
        for hi in 0..bins {
            let h = -std::f32::consts::PI + (hi as f32 / bins as f32) * std::f32::consts::TAU;
            let (mut best_l, mut best_c) = (0.5f32, 0.0f32);
            for li in 0..LIGHTNESS_BINS {
                let ll = li as f32 / (LIGHTNESS_BINS - 1) as f32;
                let cc = table.sample(h, ll);
                if cc > best_c {
                    best_c = cc;
                    best_l = ll;
                }
            }
            l.push(best_l);
            c.push(best_c);
        }
        Self { l, c, bins }
    }

    fn lightness(&self, h: f32) -> f32 {
        let tau = std::f32::consts::TAU;
        let hn = ((h + std::f32::consts::PI).rem_euclid(tau)) / tau;
        let i = ((hn * self.bins as f32) as usize).min(self.bins - 1);
        self.l[i]
    }

    fn chroma(&self, h: f32) -> f32 {
        let tau = std::f32::consts::TAU;
        let hn = ((h + std::f32::consts::PI).rem_euclid(tau)) / tau;
        let i = ((hn * self.bins as f32) as usize).min(self.bins - 1);
        self.c[i]
    }
}

/// The general mapper: compress along the line from the anchor to the colour.
struct RayMapper {
    hull: Hull,
    cusp: CuspTable,
    l_min: f32,
    l_max: f32,
    anchor: Anchor,
}

/// How far past the source the boundary search looks. The source sits at
/// `t = 1`, so this admits boundaries up to 6x further out than the colour.
const T_HI: f32 = 6.0;
const T_STEPS: usize = 24;

impl RayMapper {
    fn new(palette: &Palette, anchor: Anchor) -> Self {
        let hull = Hull::from_palette(palette);
        let table = CmaxTable::build(&hull);
        let (l_min, l_max) = table.lightness_range();
        let cusp = CuspTable::build(&table, 128);
        Self {
            hull,
            cusp,
            l_min,
            l_max,
            anchor,
        }
    }

    fn anchor_l(&self, src_l: f32, h: f32) -> f32 {
        match self.anchor {
            Anchor::FixedL => src_l,
            Anchor::CuspL => self.cusp.lightness(h),
            Anchor::MidGrey => 0.5,
            Anchor::HalfWay => 0.5 * (src_l + 0.5),
        }
        .clamp(self.l_min, self.l_max)
    }

    fn inside(&self, l: f32, c: f32, h: f32) -> bool {
        self.hull.contains(LinearRgb::from(Oklab::from(Oklch {
            l,
            c: c.max(0.0),
            h,
        })))
    }

    /// Largest `t` with `anchor + t * (source - anchor)` still in the hull.
    ///
    /// Bisection, so it finds the *first* exit. Where the locus leaves and
    /// re-enters — which is exactly what strands yellow — this deliberately
    /// returns the conservative answer rather than jumping the gap.
    fn t_max(&self, src: Oklch, a_l: f32) -> f32 {
        let (dl, dc) = (src.l - a_l, src.c);
        let at = |t: f32| self.inside(a_l + t * dl, t * dc, src.h);
        if !at(0.0) {
            return 0.0;
        }
        if at(T_HI) {
            return T_HI;
        }
        let (mut lo, mut hi) = (0.0f32, T_HI);
        for _ in 0..T_STEPS {
            let mid = 0.5 * (lo + hi);
            if at(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// `rho` along the ray: the source sits at `t = 1`, so this is `1 / t_max`.
    fn rho(&self, p: Srgb) -> f32 {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(p)));
        if lch.c <= 1e-6 {
            return 0.0;
        }
        let a_l = self.anchor_l(lch.l.clamp(self.l_min, self.l_max), lch.h);
        let tm = self.t_max(lch, a_l);
        if tm <= 0.0 {
            f32::INFINITY
        } else {
            1.0 / tm
        }
    }

    fn map_color(&self, p: Srgb, r: f32, opts: GamutOptions) -> Srgb {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(p)));
        let l = lch.l.clamp(self.l_min, self.l_max);
        if lch.c <= 1e-6 {
            return p;
        }
        let a_l = self.anchor_l(l, lch.h);
        let tm = self.t_max(Oklch { l, ..lch }, a_l);
        if tm <= 0.0 {
            return Srgb::from(clamp_linear(LinearRgb::from(Oklab::from(Oklch {
                l,
                c: 0.0,
                h: lch.h,
            }))));
        }
        // The source is at t = 1; compress that against the boundary at t_max.
        // `compress_chroma` is homogeneous, so it applies to a ray parameter
        // exactly as it does to a chroma.
        let t = compress_chroma(1.0, tm, opts.knee, r);
        let amount = opts.amount.clamp(0.0, 1.0);
        let t = 1.0 + amount * (t - 1.0);
        let out = Oklch {
            l: a_l + t * (l - a_l),
            c: (t * lch.c).max(0.0),
            h: lch.h,
        };
        Srgb::from(clamp_linear(LinearRgb::from(Oklab::from(out))))
    }

    fn map_frame(&self, pixels: &mut [Srgb], opts: GamutOptions) -> f32 {
        let mut rhos: Vec<f32> = pixels.iter().map(|p| self.rho(*p)).collect();
        let r = adaptation_factor(&mut rhos, opts.max_compression);
        for p in pixels.iter_mut() {
            *p = self.map_color(*p, r, opts);
        }
        r
    }
}

/// Ruling 5: `linear_to_srgb` has an epsilon-free `debug_assert!`.
fn clamp_linear(c: LinearRgb) -> LinearRgb {
    LinearRgb::new(
        c.r.clamp(0.0, 1.0),
        c.g.clamp(0.0, 1.0),
        c.b.clamp(0.0, 1.0),
    )
}

// ---------------------------------------------------------------- reporting

fn chroma_of(c: Srgb) -> f32 {
    Oklch::from(Oklab::from(LinearRgb::from(c))).c
}

fn mean_chroma(v: &[Srgb]) -> f32 {
    v.iter().map(|c| chroma_of(*c)).sum::<f32>() / v.len() as f32
}

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/dither-compare");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(name: &str, buf: &[u8], w: usize, h: usize) {
    let path = out_dir().join(name);
    image::save_buffer(&path, buf, w as u32, h as u32, image::ColorType::Rgb8).unwrap();
    eprintln!("  wrote {}", path.display());
}

fn to_rgb(v: &[Srgb]) -> Vec<u8> {
    v.iter()
        .flat_map(|c| {
            let b = c.to_bytes();
            [b[0], b[1], b[2]]
        })
        .collect()
}

/// Stack labelled RGB panels vertically with grey separators.
fn stack(panels: &[Vec<u8>], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    const GAP: usize = 6;
    let out_h = panels.len() * h + GAP * (panels.len() - 1);
    let mut buf = vec![0x60u8; w * out_h * 3];
    for (i, p) in panels.iter().enumerate() {
        let y0 = i * (h + GAP);
        buf[y0 * w * 3..(y0 + h) * w * 3].copy_from_slice(p);
    }
    (buf, w, out_h)
}

fn hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h * 6.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        (r + m).clamp(0.0, 1.0),
        (g + m).clamp(0.0, 1.0),
        (b + m).clamp(0.0, 1.0),
    )
}

/// A full-saturation hue/lightness field — the hard case.
fn hue_field(w: usize, h: usize) -> Vec<Srgb> {
    let mut v = Vec::with_capacity(w * h);
    for y in 0..h {
        let l = 0.12 + 0.76 * (y as f32 / (h - 1) as f32);
        for x in 0..w {
            let (r, g, b) = hsl(x as f32 / w as f32, 1.0, l);
            v.push(Srgb::new(r, g, b));
        }
    }
    v
}

/// The panel's own inks as solid blocks, plus the saturated sRGB primaries.
/// If a mapper cannot keep these, it cannot keep anything.
fn ink_swatches(w: usize, h: usize) -> Vec<Srgb> {
    let cols = [
        Srgb::from_u8(0xB5, 0x03, 0x03),
        Srgb::from_u8(0xFF, 0xEE, 0x00),
        Srgb::from_u8(0x20, 0x54, 0x97),
        Srgb::from_u8(0x0D, 0x87, 0x6B),
        Srgb::from_u8(255, 0, 0),
        Srgb::from_u8(255, 255, 0),
        Srgb::from_u8(0, 0, 255),
        Srgb::from_u8(0, 255, 0),
    ];
    let mut v = Vec::with_capacity(w * h);
    for _ in 0..h {
        for x in 0..w {
            v.push(cols[(x * cols.len() / w).min(cols.len() - 1)]);
        }
    }
    v
}

/// Why don't the panel's own inks dither to themselves?
///
/// Two separate questions, and only one of them is the mapper's fault.
///
/// 1. **Unmapped, they do.** A flat fill of a measured ink dithers to that
///    single ink with zero error. Asserted below, because it is the premise of
///    everything else here.
/// 2. **Mapped, they do not** — and that is structural. The knee bends at
///    `k*Cmax` and the shoulder above it is asymptotic to `Cmax`, so no input
///    can ever be mapped *onto* the boundary. A panel ink sits exactly on the
///    boundary (`rho = 1`), lands above the knee, and comes back at roughly
///    `0.82*Cmax` at the shipped `k = 0.8`. Then the ditherer, asked for a
///    colour that is not an ink, has to mix — hence the speckle.
///
/// The asymptote is not gratuitous: it is what stops distinct out-of-gamut
/// colours collapsing onto a shared value. So the knee is a genuine trade, and
/// this measures both sides of it rather than asserting one.
#[test]
#[ignore = "prototype; prints"]
fn what_the_knee_costs_the_panels_own_inks() {
    let p = panel();
    let inks = [
        ("red", Srgb::from_u8(0xB5, 0x03, 0x03)),
        ("yellow", Srgb::from_u8(0xFF, 0xEE, 0x00)),
        ("blue", Srgb::from_u8(0x20, 0x54, 0x97)),
        ("green", Srgb::from_u8(0x0D, 0x87, 0x6B)),
    ];

    // (1) The premise: unmapped, a flat ink fill dithers to itself exactly.
    println!("unmapped flat fills of the measured inks:");
    for (name, ink) in inks {
        const N: usize = 32;
        let px = vec![ink; N * N];
        let out = EinkDitherer::new(p.clone())
            .dither(&px, N, N)
            .to_rgb_actual();
        let first = [out[0], out[1], out[2]];
        let uniform = out.chunks(3).all(|c| c == first);
        let want = ink.to_bytes();
        println!(
            "  {name:6}: #{:02X}{:02X}{:02X} -> #{:02X}{:02X}{:02X}  {}",
            want[0],
            want[1],
            want[2],
            first[0],
            first[1],
            first[2],
            if uniform { "solid" } else { "MIXED" }
        );
        assert!(uniform, "{name} did not dither to a single ink");
        assert_eq!(
            first,
            [want[0], want[1], want[2]],
            "{name} dithered to the wrong ink"
        );
    }

    // (2) What the knee costs them, and what raising it costs the tail.
    let m = RayMapper::new(&p, Anchor::HalfWay);
    println!("\nhalf-way anchor, R pinned at 2.5 — chroma kept by each ink:");
    print!("  {:<10}", "knee");
    for (n, _) in inks {
        print!(" {n:>8}");
    }
    println!("   {:>17}   {:>7}", "tail span", "distinct");
    for knee in [0.8f32, 0.9, 0.95, 0.99] {
        let opts = GamutOptions {
            knee,
            ..Default::default()
        };
        print!("  {knee:<10.2}");
        for (_, ink) in inks {
            let kept = chroma_of(m.map_color(ink, opts.max_compression, opts)) / chroma_of(ink);
            print!(" {:>7.0}%", kept * 100.0);
        }
        // Does the out-of-gamut tail still hold detail, or does it band?
        //
        // Measured on *real sRGB colours* — a saturation ramp at one hue —
        // because that is the only input the mapper ever sees. An earlier
        // version of this probe synthesised colours at rho = 4, which lie far
        // outside sRGB and were pulled back by the clamp, so it compared two
        // colours that had both already collapsed.
        //
        // Swept over 24 hues x 5 lightnesses, not one leaf: a single ramp had
        // only 5 out-of-gamut steps, far too thin to generalise from. Reported
        // as the mean output-chroma span within a leaf's tail, and the widest
        // single leaf.
        // `distinct` is the banding metric and the one that matters: a wide
        // total span says nothing if the steps inside it have collapsed onto
        // shared values. Counted on the 8-bit output, which is what the
        // ditherer is handed.
        let mut spans: Vec<f32> = Vec::new();
        let (mut steps, mut distinct) = (0usize, 0usize);
        for hi in 0..24 {
            for li in 1..=5 {
                let l = li as f32 / 6.0;
                let mut outs: Vec<f32> = Vec::new();
                let mut bytes: Vec<[u8; 3]> = Vec::new();
                for i in 0..=64 {
                    let (r, g, b) = hsl(hi as f32 / 24.0, i as f32 / 64.0, l);
                    let src = Srgb::new(r, g, b);
                    if m.rho(src) <= 1.0 {
                        continue; // in gamut; the tail is what is under test
                    }
                    let out = m.map_color(src, opts.max_compression, opts);
                    outs.push(chroma_of(out));
                    let b = out.to_bytes();
                    bytes.push([b[0], b[1], b[2]]);
                }
                if outs.len() < 2 {
                    continue;
                }
                steps += outs.len();
                bytes.sort_unstable();
                bytes.dedup();
                distinct += bytes.len();
                spans.push(
                    outs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                        - outs.iter().cloned().fold(f32::INFINITY, f32::min),
                );
            }
        }
        let mean = spans.iter().sum::<f32>() / spans.len() as f32;
        println!(
            "   {:>17}   {:>6.1}%",
            format!("{mean:.4}"),
            distinct as f32 * 100.0 / steps as f32
        );
    }
    println!(
        "\n  One JND in Oklab chroma is roughly 0.02, so a separation well under\n  \
         that is a difference the panel cannot show anyway."
    );
}

/// Compose panels into a grid, row-major.
fn grid(panels: &[Vec<u8>], cols: usize, w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    const GAP: usize = 6;
    let rows = panels.len().div_ceil(cols);
    let out_w = cols * w + GAP * (cols - 1);
    let out_h = rows * h + GAP * (rows - 1);
    let mut buf = vec![0x60u8; out_w * out_h * 3];
    for (i, p) in panels.iter().enumerate() {
        let (cx, cy) = (i % cols, i / cols);
        let (x0, y0) = (cx * (w + GAP), cy * (h + GAP));
        for y in 0..h {
            let src = y * w * 3;
            let dst = ((y0 + y) * out_w + x0) * 3;
            buf[dst..dst + w * 3].copy_from_slice(&p[src..src + w * 3]);
        }
    }
    (buf, out_w, out_h)
}

/// The real test: photographs, where the lightness excursion is judged.
///
/// Swatches and hue fields cannot answer this. Mid-grey anchoring moves
/// lightness by up to 0.26 on saturated brights, and whether that reads as
/// "more colourful" or "muddy and flat" is a question only continuous-tone
/// content with recognisable subject matter can settle. Both images here are
/// byonk's own shipping assets, so they are exactly what the panel renders.
#[test]
#[ignore = "prototype; reads shipping assets and writes PNGs"]
fn photographs_under_each_anchor() {
    const SIDE: u32 = 400;
    let p = panel();
    let opts = GamutOptions::default();
    let anchors = [
        Anchor::FixedL,
        Anchor::CuspL,
        Anchor::MidGrey,
        Anchor::HalfWay,
    ];
    let mappers: Vec<RayMapper> = anchors.iter().map(|a| RayMapper::new(&p, *a)).collect();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (name, rel) in [
        ("portrait", "screens/builtin/calibration/color/photo.png"),
        ("background", "screens/builtin/default/background.jpg"),
    ] {
        let path = root.join(rel);
        let img = match image::open(&path) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("  skipping {name}: {e}");
                continue;
            }
        };
        let img = img
            .resize_to_fill(SIDE, SIDE, image::imageops::FilterType::Lanczos3)
            .to_rgb8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let source: Vec<Srgb> = img
            .pixels()
            .map(|px| Srgb::from_u8(px[0], px[1], px[2]))
            .collect();

        println!(
            "\n{name} ({w}x{h}): mean Oklab chroma source = {:.4}",
            mean_chroma(&source)
        );
        let mut mapped_panels = vec![to_rgb(&source)];
        let mut dithered_panels = vec![EinkDitherer::new(p.clone())
            .dither(&source, w, h)
            .to_rgb_actual()];

        for (m, a) in mappers.iter().zip(anchors.iter()) {
            let mut mapped = source.clone();
            let r = m.map_frame(&mut mapped, opts);
            // Mean absolute lightness shift: the cost that only photos reveal.
            let dl = source
                .iter()
                .zip(mapped.iter())
                .map(|(s, o)| {
                    (Oklch::from(Oklab::from(LinearRgb::from(*o))).l
                        - Oklch::from(Oklab::from(LinearRgb::from(*s))).l)
                        .abs()
                })
                .sum::<f32>()
                / source.len() as f32;
            // Whole-image mean chroma is a poor metric here: only a minority of
            // pixels are out of gamut, so the majority that pass through
            // untouched swamp the difference. The pixels the mapper actually
            // acts on are the ones worth measuring.
            let oog: Vec<usize> = (0..source.len())
                .filter(|&i| mappers[0].rho(source[i]) > 1.0)
                .collect();
            let oog_src = oog.iter().map(|&i| chroma_of(source[i])).sum::<f32>() / oog.len() as f32;
            let oog_out = oog.iter().map(|&i| chroma_of(mapped[i])).sum::<f32>() / oog.len() as f32;
            println!(
                "  {:<22} R={r:.3}  mean chroma {:.4}  mean |dL| {dl:.4}   \
                 out-of-gamut pixels ({:.0}%): {oog_src:.4} -> {oog_out:.4} ({:.0}% kept)",
                a.name(),
                mean_chroma(&mapped),
                oog.len() as f32 * 100.0 / source.len() as f32,
                oog_out / oog_src * 100.0
            );
            mapped_panels.push(to_rgb(&mapped));
            dithered_panels.push(
                EinkDitherer::new(p.clone())
                    .dither(&mapped, w, h)
                    .to_rgb_actual(),
            );
        }

        let (buf, ow, oh) = grid(&mapped_panels, 3, w, h);
        write(&format!("photo-{name}-mapped.png"), &buf, ow, oh);
        let (buf, ow, oh) = grid(&dithered_panels, 3, w, h);
        write(&format!("photo-{name}-dithered.png"), &buf, ow, oh);
        eprintln!("    grid order: source, fixed-L (production), cusp-L / mid-grey, half-way");
    }
}

#[test]
#[ignore = "prototype; prints and writes PNGs"]
fn cusp_anchored_vs_fixed_lightness() {
    const W: usize = 512;
    const H: usize = 160;
    let p = panel();
    let opts = GamutOptions::default();

    let anchors = [
        Anchor::FixedL,
        Anchor::CuspL,
        Anchor::MidGrey,
        Anchor::HalfWay,
    ];
    let mappers: Vec<RayMapper> = anchors.iter().map(|a| RayMapper::new(&p, *a)).collect();

    // ---- self-check: FixedL must reproduce production -------------------
    let production = GamutMapper::new(&p);
    let probes = [
        Srgb::from_u8(0xB5, 0x03, 0x03),
        Srgb::from_u8(0x20, 0x54, 0x97),
        Srgb::from_u8(200, 40, 160),
        Srgb::from_u8(40, 180, 90),
    ];
    println!("self-check — Anchor::FixedL against GamutMapper (R = 2.5):");
    let mut worst = 0.0f32;
    for c in probes {
        let a = production.map_color(c, 2.5, opts).to_bytes();
        let b = mappers[0].map_color(c, 2.5, opts).to_bytes();
        let d = (0..3)
            .map(|i| (a[i] as i32 - b[i] as i32).abs())
            .max()
            .unwrap() as f32;
        worst = worst.max(d);
        println!(
            "  #{:02X}{:02X}{:02X} -> production #{:02X}{:02X}{:02X}  ray #{:02X}{:02X}{:02X}  (max channel diff {d})",
            c.to_bytes()[0], c.to_bytes()[1], c.to_bytes()[2],
            a[0], a[1], a[2], b[0], b[1], b[2]
        );
    }
    // Not bit-exact, and shouldn't be: production reads `Cmax` from the
    // bilinear 128x64 table while this harness bisects the hull directly, so
    // they differ by the table's interpolation error (measured at ~5% of Cmax
    // in the ordinary case, more where the hull pinches). A few 1/255 steps is
    // that error and nothing else; a large divergence would mean the ray
    // geometry is wrong and every number below with it.
    assert!(
        worst <= 6.0,
        "the ray harness does not reproduce production at fixed L (max channel diff {worst}) — \
         the harness is wrong, so none of its other numbers can be trusted"
    );

    // ---- where are the cusps? -------------------------------------------
    println!("\ncusp per ink hue (the most colourful the panel gets at that hue):");
    for (name, ink) in [
        ("red", Srgb::from_u8(0xB5, 0x03, 0x03)),
        ("yellow", Srgb::from_u8(0xFF, 0xEE, 0x00)),
        ("blue", Srgb::from_u8(0x20, 0x54, 0x97)),
        ("green", Srgb::from_u8(0x0D, 0x87, 0x6B)),
    ] {
        let lch = Oklch::from(Oklab::from(LinearRgb::from(ink)));
        println!(
            "  {name:6}: ink at L={:.3} C={:.4}   cusp at L={:.3} C={:.4}",
            lch.l,
            lch.c,
            mappers[1].cusp.lightness(lch.h),
            mappers[1].cusp.chroma(lch.h)
        );
    }

    // ---- the inks, under each anchor ------------------------------------
    // R is pinned at the cap so the adaptation is working as hard as it can.
    println!("\nchroma kept, R pinned at {:.1}:", opts.max_compression);
    print!("  {:<8}", "colour");
    for a in anchors {
        print!(" {:>22}", a.name());
    }
    println!();
    for (name, ink) in [
        ("red", Srgb::from_u8(0xB5, 0x03, 0x03)),
        ("yellow", Srgb::from_u8(0xFF, 0xEE, 0x00)),
        ("blue", Srgb::from_u8(0x20, 0x54, 0x97)),
        ("green", Srgb::from_u8(0x0D, 0x87, 0x6B)),
        ("sRGB red", Srgb::from_u8(255, 0, 0)),
        ("sRGB yel", Srgb::from_u8(255, 255, 0)),
        ("sRGB blu", Srgb::from_u8(0, 0, 255)),
        ("sRGB grn", Srgb::from_u8(0, 255, 0)),
    ] {
        let before = chroma_of(ink);
        let cells: Vec<String> = mappers
            .iter()
            .map(|m| {
                let out = m.map_color(ink, opts.max_compression, opts);
                let b = out.to_bytes();
                format!(
                    "{:>3.0}% #{:02X}{:02X}{:02X} dL{:+.3}",
                    chroma_of(out) / before * 100.0,
                    b[0],
                    b[1],
                    b[2],
                    Oklch::from(Oklab::from(LinearRgb::from(out))).l
                        - Oklch::from(Oklab::from(LinearRgb::from(ink))).l
                )
            })
            .collect();
        print!("  {name:<8}");
        for c in &cells {
            print!(" {c:>22}");
        }
        println!();
    }

    // ---- whole-field measurement + visuals -------------------------------
    for (label, source) in [("swatches", ink_swatches(W, H)), ("field", hue_field(W, H))] {
        println!(
            "\n{label}: mean Oklab chroma, source = {:.4}",
            mean_chroma(&source)
        );
        let mut panels = vec![to_rgb(&source)];
        let mut dithered = vec![EinkDitherer::new(p.clone())
            .dither(&source, W, H)
            .to_rgb_actual()];
        for (m, a) in mappers.iter().zip(anchors.iter()) {
            let mut mapped = source.clone();
            let r = m.map_frame(&mut mapped, opts);
            println!(
                "  {:<22} R={r:.3}  mean chroma = {:.4}",
                a.name(),
                mean_chroma(&mapped)
            );
            panels.push(to_rgb(&mapped));
            dithered.push(
                EinkDitherer::new(p.clone())
                    .dither(&mapped, W, H)
                    .to_rgb_actual(),
            );
        }
        let (buf, ow, oh) = stack(&panels, W, H);
        write(&format!("cusp-{label}-mapped.png"), &buf, ow, oh);
        let (buf, ow, oh) = stack(&dithered, W, H);
        write(&format!("cusp-{label}-dithered.png"), &buf, ow, oh);
        eprintln!("    rows: source | fixed-L (production) | cusp-L | mid-grey | half-way");
    }
}
