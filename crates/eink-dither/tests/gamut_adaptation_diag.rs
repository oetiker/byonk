//! Diagnostic: what the content-adaptation factor `R` actually does to colour.
//!
//! Written in session 7 to answer "why does everything look subdued?". The
//! answer was a real defect: `mapped_chroma` computed
//! `compress_chroma(c / R, c_max, knee)`, dividing **before** the knee was
//! consulted, so adaptation applied to every pixel unconditionally — including
//! the palette's own inks, which sit on the hull and need no compression at
//! all. They came out with 40% of their chroma.
//!
//! Session 8 redesigned it: `R` now scales only the input span of the tail
//! (`t = (C - k*Cmax) / ((R-k)*Cmax)`), so the sub-knee region is exact
//! identity at every `R`. This file is kept as the **standing guard** on that
//! property, and as the place the per-ink numbers are printed.
//!
//! **Yellow used to be excluded from this guard and no longer is.** At its own
//! lightness (L = 0.933) the constant-`L`, constant-hue chroma ray leaves the
//! hull at C ~ 0.073 and touches it again only at the vertex itself — a
//! measure-zero graze, confirmed by scanning `Hull::contains` along the ray at
//! 1e-4 resolution. So `Cmax ~ 0.073` was geometrically correct and
//! `rho(yellow) ~ 2.1` was a true statement about a **chroma-only** mapper:
//! compressing chroma at fixed lightness genuinely could not reach the ink,
//! which came back at 42% where red, blue and green managed 82%.
//!
//! Ruling 16 replaced that geometry with a ray converging on mid-grey, which
//! moves lightness as well. Yellow now measures `rho = 1.000` — on the boundary,
//! as an ink should be — and keeps 82% like the rest, so it is guarded here on
//! the same terms as the others.
//!
//! Run with:
//!     cargo test -p eink-dither --test gamut_adaptation_diag -- --ignored --nocapture
use eink_dither::gamut::adapt::adaptation_factor;
use eink_dither::gamut::cmax::CmaxTable;
use eink_dither::gamut::hull::Hull;
use eink_dither::{GamutMapper, GamutOptions, LinearRgb, Oklab, Oklch, Palette, Srgb};

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

#[test]
#[ignore = "diagnostic; prints the per-ink numbers, guards the sub-knee promise"]
fn adaptation_factor_does_not_reach_in_gamut_colours() {
    const W: usize = 480;
    const H: usize = 320;
    let p = panel();
    let mapper = GamutMapper::new(&p);
    let table = CmaxTable::build(&Hull::from_palette(&p));

    let mut px = Vec::new();
    for y in 0..H {
        let l = 0.12 + 0.76 * (y as f32 / (H - 1) as f32);
        for x in 0..W {
            let (r, g, b) = hsl(x as f32 / W as f32, 1.0, l);
            px.push(Srgb::new(r, g, b));
        }
    }

    let mut rhos: Vec<f32> = px.iter().map(|c| mapper.rho(*c)).collect();
    let mut sorted = rhos.clone();
    sorted.sort_by(f32::total_cmp);
    let q = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
    println!(
        "rho: p50={:.3} p90={:.3} p99={:.3} max={:.3}",
        q(0.5),
        q(0.9),
        q(0.99),
        q(1.0)
    );

    let r = adaptation_factor(&mut rhos, 2.5);
    println!("\nADAPTATION FACTOR R = {r:.4}  (max_compression cap = 2.5)");
    println!(
        "  -> R widens the knee's tail input span to (R-k)*Cmax; sub-knee chroma is untouched"
    );

    // How many pixels actually reach the knee's shoulder?
    for knee in [0.4f32, 0.6, 0.8] {
        let mut above = 0usize;
        for c in &px {
            let lch = Oklch::from(Oklab::from(LinearRgb::from(*c)));
            let cmax = table.sample(lch.h, lch.l);
            if cmax > 0.0 && lch.c > knee * cmax {
                above += 1;
            }
        }
        println!(
            "  knee {knee}: {:5.1}% of pixels reach the shoulder",
            above as f32 * 100.0 / px.len() as f32
        );
    }

    // Mean chroma at each stage.
    let mean_c = |v: &[Srgb]| -> f32 {
        v.iter()
            .map(|c| Oklch::from(Oklab::from(LinearRgb::from(*c))).c)
            .sum::<f32>()
            / v.len() as f32
    };
    println!("\nmean Oklab chroma  source = {:.4}", mean_c(&px));
    for mc in [2.5f32, 2.0, 1.5, 1.2] {
        let mut m = px.clone();
        mapper.map_frame(
            &mut m,
            &vec![true; px.len()],
            GamutOptions {
                max_compression: mc,
                ..Default::default()
            },
        );
        println!(
            "  max_compression {mc:>4} -> R={:.3}  mean chroma = {:.4}",
            adaptation_factor(
                &mut px.iter().map(|c| mapper.rho(*c)).collect::<Vec<_>>(),
                mc
            ),
            mean_c(&m)
        );
    }

    // Where do the panel's own inks land?
    println!("\nthe panel's own measured inks, after mapping (R={r:.3}):");
    for (name, ink) in [
        ("red", Srgb::from_u8(0xB5, 0x03, 0x03)),
        ("yellow", Srgb::from_u8(0xFF, 0xEE, 0x00)),
        ("blue", Srgb::from_u8(0x20, 0x54, 0x97)),
        ("green", Srgb::from_u8(0x0D, 0x87, 0x6B)),
    ] {
        let before = Oklch::from(Oklab::from(LinearRgb::from(ink)));
        let after_s = mapper.map_color(ink, r, GamutOptions::default());
        let after = Oklch::from(Oklab::from(LinearRgb::from(after_s)));
        let b = after_s.to_bytes();
        println!("  {name:6}: L={:.3} rho={:.3}  chroma {:.4} -> {:.4}  ({:.0}% kept)   #{:02X}{:02X}{:02X} -> #{:02X}{:02X}{:02X}",
                 before.l, mapper.rho(ink),
                 before.c, after.c, after.c / before.c * 100.0,
                 ink.to_bytes()[0], ink.to_bytes()[1], ink.to_bytes()[2], b[0], b[1], b[2]);
    }

    // The standing guard. These three inks are reachable at their own lightness
    // (rho ~ 1.02-1.04), so a correct mapper must leave them very nearly alone
    // however hard the *rest* of the region is being compressed — here R is
    // pinned at its 2.5 cap by a deliberately saturated field.
    //
    // Before the session-8 redesign every one of them kept 40%.
    //
    // Yellow is **included** since ruling 16. Under the old fixed-lightness
    // mapper it was excluded as an admitted design limitation, keeping 42%
    // where the others managed 82%; the mid-grey ray reaches it and it now
    // measures `rho = 1.000` and 82% like every other ink.
    for (name, ink) in [
        ("red", Srgb::from_u8(0xB5, 0x03, 0x03)),
        ("yellow", Srgb::from_u8(0xFF, 0xEE, 0x00)),
        ("blue", Srgb::from_u8(0x20, 0x54, 0x97)),
        ("green", Srgb::from_u8(0x0D, 0x87, 0x6B)),
    ] {
        let before = Oklch::from(Oklab::from(LinearRgb::from(ink))).c;
        let after = Oklch::from(Oklab::from(LinearRgb::from(mapper.map_color(
            ink,
            r,
            GamutOptions::default(),
        ))))
        .c;
        assert!(
            after > before * 0.7,
            "the {name} ink is on the hull and needs no compression, yet kept \
             only {:.0}% of its chroma. The adaptation factor must not reach \
             colours that are already renderable.",
            after / before * 100.0
        );
    }
}
