# Mixture-aware quantiser — design

**Date:** 2026-08-22 · **Branch:** `feat/panel-clean-recovery` · **Crate:** `eink-dither`

## 1. The defect

byonk renders muted colours as a field of one chromatic ink. A neutral grey
comes out green; a brown face comes out greenish.

Measured on `gamut::test_support::panel_measured()` (a measured E1002; index
order `0 black, 1 white, 2 red, 3 yellow, 4 blue, 5 green`), dithering a
255×255 uniform patch:

| colour | blk | wht | red | yel | blu | grn | dominant | dE |
|---|---|---|---|---|---|---|---|---|
| skin warm `#896666` | **0.0%** | **0.0%** | 46.8% | 0.0% | 0.4% | 52.8% | grn 52.8% | 0.0365 |
| skin brown B `#966E55` | **0.0%** | **0.0%** | 45.7% | 0.0% | 0.0% | 54.3% | grn 54.3% | 0.0475 |
| skin cheek `#9390A3` | 0.0% | 15.4% | 12.7% | 0.0% | 17.6% | 54.2% | grn 54.2% | 0.0427 |
| muted scarf `#8C6C68` | **0.0%** | **0.0%** | 43.3% | 0.0% | 0.0% | 56.7% | grn 56.7% | 0.0436 |
| grey 128 | 0.0% | 2.0% | 18.8% | 0.0% | 1.6% | 77.6% | grn 77.6% | 0.0631 |

The same colours on the **idealised** BWRGBY palette get 30–60% black. The
difference is the panel, not the content.

**The root cause is that black and white are unreachable.** Eight of twelve
muted colours use 0% black. The measured inks are dull — green measures chroma
0.068 — so they crowd the neutral axis and beat black and white on plain
nearest-neighbour distance at every muted colour. Green is not "preferred"; it
is standing in for the black that never arrives, which is exactly what
`tests/spike_simplex.rs` recorded independently.

This is the textbook degenerate case in Chai Wah Wu, *"Error Diffusion: Recent
Developments in Theory and Applications"* (IS&T NIP20, 2004): a palette whose
points subtend an angle near 180° around the target, where classical error
diffusion's error bound degenerates while a barycentric rule stays bounded.

**dE is not the symptom.** Every row above has an acceptable dE. The average is
right and the *field colour* is wrong, because the eye does not average — it
reads the majority ink as the colour of the area.

### Already ruled out — do not redo

- **The dither kernel.** Atkinson 52.0%, Atkinson-hybrid 54.2%,
  Floyd-Steinberg 50.3% chromatic on a neutral ramp.
- **The calibration.** Perturbing every ink by the measured photographic
  uncertainty (2.02% of white) moves the result by at most dE 0.013.
- **A blanket chroma penalty.** `HyAB kchroma=10` cuts chromatic choice on
  neutrals from 50.4% to 9.4% but is biased for error diffusion on muted
  photographic colour. It fixes greys by breaking photographs.

## 2. Scope

**In scope:** a mixture-aware selection rule inside `eink-dither`, controlled
by one continuous internal parameter with a single measured default.

**Out of scope, recorded as follow-up:** probabilistic selection in the style of
HANS/PARAWACS (Morovič et al., IEEE TIP 21(2) 2012) — draw an ink using the
mixture weights as probabilities and the blue-noise value as the draw. It
reuses the same wedge fan this spec builds, so nothing here is wasted. Owner
ruling 2026-08-22: build the fan first, measure, then decide whether a second
halftoning method earns its keep. If it is built, it should surface to the user
as another entry in the dither list.

**Explicitly not config.** Owner ruling 2026-08-22: the lever is internal with
one good default. The right value is a property of how dull a panel's inks are,
not a taste setting. Nothing in this spec reaches `config.yaml`, byonk's
`DitherTuningValues`, or the dev UI's `DITHER_DEFAULTS` table.

## 3. The design

### 3.1 The wedge fan

Partition the palette's gamut into tetrahedra that **all share the black–white
edge** (Ostromoukhov, *"Chromaticity Gamut Enhancement by Heptatone Multi-Color
Printing"*, SPIE 1909, 1993).

Construction, from the palette's **actual** colours:

1. `white` = the entry with the highest OKLab L; `black` = the lowest.
2. Chromatic inks = the remaining entries with OKLab chroma above
   `CHROMA_DETECTION_THRESHOLD` (0.03, already defined in `palette.rs`).
   Sort them by OKLab hue angle.
3. Each hue-adjacent pair, plus black and white, is one tetrahedron. Six inks
   give four: `{W,K,R,Y}`, `{W,K,Y,G}`, `{W,K,G,B}`, `{W,K,B,R}`.
4. Precompute each tetrahedron's inverse matrix.

**Linear RGB, not OKLab.** Dot mixing is physical light addition and OKLab is
not additive. `gamut/hull.rs:3-7` already states this and computes its hull in
linear RGB for the same reason.

**Delaunay would not do.** Nothing in the empty-circumsphere criterion
guarantees the black–white segment is even an edge. The shared axis is the
whole point.

**Degenerate palettes build no fan.** `from_palette` returns `None`, the
feature switches off, and behaviour is today's bit for bit, when either:

- there are fewer than three chromatic inks (greyscale, black-and-white, a
  single spot colour), or
- `Hull::from_palette(palette).is_mappable()` is false. That existing predicate
  is `shape == Volume && neutral_found` — precisely "a full 3-D hull in which a
  reachable neutral was actually found", which is the condition under which a
  fan around the black-white axis means anything. Reuse it rather than
  reimplement the test. The hull costs under a millisecond and is built only
  when `mixture_bias > 0.0`.

### 3.2 Mixture weights

For a target colour, solve its barycentric coordinates in every wedge and keep
the wedge whose smallest coordinate is largest. Then **clamp negative
coordinates to zero and renormalise the remainder to sum to 1**, so the bias
has the same total magnitude at every pixel and `lambda` means one thing
everywhere.

- Inside the fan the "largest smallest coordinate" rule selects the containing
  wedge, and no coordinate is negative, so the clamp is a no-op.
- Outside it (an out-of-gamut colour) it degrades to the nearest wedge rather
  than needing a special case, and the clamp-and-renormalise projects the point
  onto that wedge.

**The field is continuous.** Two neighbouring wedges share a face on which the
absent ink's coordinate is exactly zero from both sides, so the weight vector
is identical there, and it varies continuously inside each wedge.

This is the decisive difference from `tests/spike_simplex.rs`, which restricted
candidates to the support of a *freely optimised* mixture and measured its own
support field jumping by 0.090 in one step. That optimum's support changes
discontinuously as the target moves; a fixed fan's does not, because white and
black are in every cell and never drop out.

### 3.3 The selection rule

```
score_i  =  dE(pixel_with_diffused_error, ink_i)  -  lambda * bary_i
```

Lowest score wins. `dE` is the palette's configured metric, square-rooted where
it is Euclidean so `lambda` reads in OKLab dE units.

- `lambda = 0` is today's nearest-neighbour selection, exactly.
- Large `lambda` approaches pure argmax-barycentric — the E Ink patent rule
  (US 10,554,854 / 10,771,652, Crounse, priority 2016-05-24) and maximum grey
  component replacement.
- Between them is the dial.

**Why it fixes the defect.** For a pure neutral the coordinates are
`(w, 0, 0, k)` in *every* wedge, so no chromatic ink can receive a discount at
all: grey balance is exact by construction. For `skin warm` the wedge is
`{W,K,R,Y}`, black's coordinate is large, and black gains enough to overcome
the ~0.35 dE by which green currently beats it.

**Why it is not the `kchroma` mistake.** The crate's history records HyAB's
chroma coupling as biased for error diffusion, uncorrectable by diffusion.
The difference:

- `kchroma` penalised **a property of the answer** (chroma). No mixture escapes
  it, so the achieved average itself shifts.
- `lambda * bary_i` **chooses between mixtures that all hit the same target.**
  Six inks and three equations leave a two-parameter family of exact solutions;
  the bias picks inside that family. The error term is untouched, so the
  average still converges.

This is a claim, and §5 tests it directly rather than assuming it.

**Why not the hard argmax (E Ink's own rule).** E Ink backed it out in
US 11,527,216 (Buckley, Crounse, Telfer, Sainis, 2017): *"image quality is
compromised by using barycentric quantization inside the color gamut hull."*
That is a published argument against this change, from the vendor, in our
domain. A continuous `lambda` lets us measure where quality actually peaks, and
gives an always-available retreat to today's behaviour.

### 3.4 Two rulings

- **Weights come from the source pixel, not the error-loaded pixel.** The bias
  is the recipe for the *content*; taking it from the error-loaded value would
  make the guidance drift with the error it bounds, and would roughen a field
  that is otherwise as smooth as the content.
- **`ColourModel::Nominal` gets no bias.** Those pixels are flat SVG fills
  meant to *be* an ink (owner ruling 22) and are usually pinned; a fan built
  from measured colours is not valid for them. Revisit only if a real case
  appears.

## 4. Implementation

### 4.1 New file `crates/eink-dither/src/gamut/wedges.rs`

Next to `hull.rs`, which already works in linear RGB.

```rust
/// The wedge fan around the palette's black-white axis.
pub struct WedgeFan { /* per wedge: 4 ink indices + a 3x3 inverse */ }

impl WedgeFan {
    /// `None` when the palette cannot carry a fan (fewer than three
    /// chromatic inks, or a degenerate hull).
    pub fn from_palette(palette: &Palette) -> Option<Self>;

    /// Clamped, normalised mixture weights for `target`, one per palette
    /// entry. `out.len()` must equal `palette.len()`.
    pub fn weights(&self, target: LinearRgb, out: &mut [f32]);
}
```

Registered as `pub mod wedges;` in `gamut/mod.rs`, matching `hull`.

### 4.2 New method on `Palette`

```rust
/// Like `find_nearest`, but entry `i`'s distance is reduced by
/// `lambda * bias[i]`. With `lambda == 0.0` this delegates to
/// `find_nearest` and is bit-for-bit identical.
pub fn find_nearest_biased(
    &self, color: Oklab, model: ColourModel, lambda: f32, bias: &[f32],
) -> (usize, f32);
```

The `lambda == 0.0` short-circuit is deliberate: it makes "cannot regress at
the default" a property of the code rather than a claim in a commit message.
Where the metric is Euclidean the method square-roots before biasing;
`is_euclidean()` exists and its doc already instructs callers to do this.

### 4.3 One new field on `DitherOptions`

```rust
/// Discount, in OKLab dE, applied to a palette entry in proportion to its
/// share of the pixel's exact mixture. 0.0 = plain nearest-neighbour.
pub mixture_bias: f32,
```

Not per-algorithm: `DitherAlgorithm::defaults()` keeps returning
`(max_error, noise_scale)`. Every `DitherOptions` in the workspace is built
through `Default` or the builder, so no construction site changes.

### 4.4 The dither loop

**No new parameter.** `Palette` is already an argument, so the fan is built at
the top of `dither_with_kernel_noise` — four 3×3 inversions, far cheaper than
`Hull::from_palette`, which the crate already runs per palette resolve. All 44
existing call sites compile and run unmodified.

```rust
let fan = (options.mixture_bias > 0.0)
    .then(|| WedgeFan::from_palette(palette))
    .flatten();
let mut bias = vec![0.0f32; palette.len()];
```

and, replacing the selection at `dither/mod.rs:409`:

```rust
let (nearest_idx, _dist) = match (&fan, model) {
    (Some(f), ColourModel::Measured) => {
        f.weights(image[idx], &mut bias);   // source pixel, not error-loaded
        palette.find_nearest_biased(oklab, model, options.mixture_bias, &bias)
    }
    _ => palette.find_nearest(oklab, model),
};
```

### 4.5 Cost

About 60 extra floating-point operations plus six square roots per pixel, and
only when `mixture_bias > 0.0`. Under 0.1 s on a 1200×1600 frame.

### 4.6 Staging

The default ships at `0.0` in the first commit, so every test stays green
except the one already red. §5's sweep chooses the value and a second commit
sets it. A regression can then only be attributed to one number.

## 5. Verification

### 5.1 Gates that must go green

1. **`test_neutral_grey_has_no_dominant_chromatic_ink`** — red today by design.
2. **New, written red first: `test_in_gamut_census`** in `domain_tests.rs`, on
   `panel_measured()`. Owner ruling 2026-08-22: *"any colour inside the gamut
   should be accurate"* — a census beats a colour list somebody picked.

   Enumerate the sRGB cube on a grid of 16 and keep what `Hull::contains`
   admits: **677 in-gamut colours**. Coarser grids collapse (78 at step 32, 11
   at step 64) because this panel's gamut is only 16.5% of the cube. Dither
   each as a 64×64 patch, discarding the first 8 rows where error diffusion is
   still settling.

   Two independent gates over that set:

   - **Accuracy.** dE below a bound set from the pre-change measurement. This
     is a no-regression guard: the fix must not make any reachable colour less
     accurate than it already is.
   - **No field colour.** For a target whose OKLab chroma is below the
     *dullest* chromatic ink's chroma, the largest single chromatic ink must
     stay at or below 50%. The threshold is **derived from the palette, not
     chosen**: a target duller than every ink cannot legitimately be mostly one
     of them. On `panel_measured()` the dullest ink is green at chroma 0.068,
     which covers every grey (0.000) and every skin tone (0.03–0.06) while
     exempting saturated colours that genuinely are one ink.

   **A dE gate alone would not catch the reported bug.** Grey 128 dithers to
   77.6% green at dE 0.063; `skin warm` to 52.8% green at dE 0.037. The average
   is right and the area looks wrong, because the eye reads the majority ink as
   the colour of the region rather than averaging. That is why there are two
   gates and not one.

   This supersedes an earlier draft of this section, which proposed a
   hand-picked list of muted colours.
3. **`find_nearest_biased(c, m, 0.0, bias) == find_nearest(c, m)`** over a
   colour sweep with arbitrary bias vectors.
4. **Continuity of the weight field.** Walk lightness ramps at several hues and
   hue sweeps at several lightnesses; bound the largest single-step change in
   any ink's weight. This is the measurement that condemned the earlier spike
   (0.090, against its own *"a smooth field would stay well under 0.1"*). The
   bound is set from a first measurement, not guessed.

### 5.2 Gates that must not move

- `test_dither_perceptual_accuracy_photo` (idealised palette) — bounds unchanged.
- `test_photo_muted_color_accuracy`.
- The other 227 passing `eink-dither` tests, and `cargo test --workspace`.
- `#[ignore]`d diagnostics `test_dither_versus_gamut_bound` and
  `test_ink_histogram_versus_optimal_recipe`, compared before and after by hand.

### 5.3 Choosing `lambda`

An `#[ignore]`d diagnostic sweeps `lambda` from 0.0 to ~1.2 across both
`panel_measured()` and the idealised palette, over a neutral ramp, the
muted/skin set and the saturated primaries, printing:

| column | question it answers |
|---|---|
| mean and max dE | did accuracy survive |
| max single chromatic ink % | did the field colour go away |
| black + white share | the direct measure of the defect in §1 |
| max single-step weight change on a gradient | did banding appear |

It also writes renders to `target/dither-compare/`, as `spike_simplex.rs` does.

**Prediction:** §3.3 puts the useful range at roughly 0.3–0.8 (black loses to
green by ~0.35 dE at a black weight near 0.7). If the measurements land far
outside that, the model is wrong and must be said so, not tuned until it fits.

**`best_reachable()` is not an oracle for ink shares.** Because the exact
solutions form a two-parameter family, the recipe it returns is an arbitrary
member of that family. Its black share is not a target and no gate uses it. It
remains valid as the **dE bound** it was written to be.

### 5.4 The real-panel check

Numbers decide accuracy; the owner decides the look, from renders. Through the
`byonk-g18` MCP `render_screen`, with explicit `width`/`height` so the output
is 1:1 — `image_max_width` resamples, which averages away the very clumping
under test:

- the calibration photo with the face,
- a neutral ramp,
- a smooth gradient,
- a text-and-chart dashboard screen.

Before and after, at the chosen `lambda`.

### 5.5 If photographs regress

Lower `lambda`. It is continuous and `0.0` reproduces today exactly. That
retreat is the reason this approach was chosen over the hard argmax.

### 5.6 Verification commands

`make check` has reported exit 0 while tests failed. Run these directly:

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd docs && mdbook build
```

## 6. Cleanup

`crates/eink-dither/tests/probe_skin_throwaway.rs` — the throwaway probe that
produced §1's table — is deleted. Its colours move into the new muted-colour
test, where they earn their keep.

## 7. References

- Ostromoukhov, *"Chromaticity Gamut Enhancement by Heptatone Multi-Color
  Printing"*, SPIE 1909 (1993) — the wedge construction.
- Chai Wah Wu, *"Error Diffusion: Recent Developments in Theory and
  Applications"*, IS&T NIP20 (2004) — bounded error iff the input gamut is
  inside the convex hull of the output set; the near-180° counterexample.
- Crounse, E Ink, US 10,554,854 / 10,771,652 (priority 2016-05-24) —
  barycentric argmax for exactly this hardware.
- Buckley, Crounse, Telfer, Sainis, E Ink, US 11,527,216 (2017) — the vendor's
  own reversal, inside the hull.
- Shaked, Arad, Fitzhugh, Sobel, HP Labs HPL-96-128R1 / US 5,991,438 — MBVQ:
  black+white dots have maximum luminance contrast and are the grainiest
  neutral. Assumes complementary inks, which this palette lacks.
- Morovič, Morovič, Gondek, IEEE TIP 21(2) (2012); HP US 8,213,055 — HANS.
  Companion halftoner PARAWACS, CIC24 (2016). The follow-up in §2.
- Zhigang Fan, Xerox, US 9,848,105 — *"a minimum error in a perceptual space
  may imply a large error in an output device color space."*
