# Panel-colour pinning with distance-decayed error carry — design

_Status: spike. Written 2026-08-10 (session 10), branch `feat/screen-store-authoring-core`._

## The defect

In the `calibration/tone` screen's **unmapped control column** — no gamut mapping
involved — the 2 px pure-black grid lines between the patches come back as:

| ink | share of grid pixels |
|---|---|
| black `#000000` | **73.2%** |
| red `#B50303` | 10.7% |
| blue `#205497` | 8.6% |
| green `#0D876B` | 7.6% |

No white and no grey, so this is not antialiasing. It is chromatic error diffused
*out of* the neighbouring saturated patches *into* pixels that were already exactly
a panel ink. The mapped column measures 71.4%, so gamut mapping accounts for under
two points: **this is a dithering effect, not a tone-mapping one.**

The scope is not the calibration screen. Any black text or logo abutting saturated
content on any screen is being speckled the same way. That is the motive.

### The code claims this cannot happen

`crates/eink-dither/src/preprocess/preprocessor.rs:88` justifies the removal of
exact-match pinning by saying such a pixel "has zero quantisation error, so error
diffusion reproduces it exactly without a special case". That is true of the pixel's
**own** error and ignores the error diffused **into** it from its neighbours. The
comment is wrong and is corrected by this work regardless of what else lands.

The other half of that note is sound and must survive: pinning also *did* cause
seams across smooth ramps, by pinning pixels that merely coincided with a palette
entry and discarding their error. Any design that reintroduces pinning has to answer
for that, and this one does — see "Why the seam does not come back".

## What is being built

At the one site where accumulated error is added to the source pixel
(`crates/eink-dither/src/dither/mod.rs:319-324`), a pixel that is **eligible** for
pinning and **is** exactly a nominal palette entry:

- outputs that palette index, ignoring the accumulated error entirely;
- emits `λ · accumulated` to its neighbours instead of `(source + accumulated) −
  nearest`, because its own quantisation error is zero.

Everything downstream of that — the kernel, the blue-noise jitter, the serpentine
direction, `error_clamp` — is untouched. A pinned pixel is an ordinary pixel whose
output is forced and whose emitted error is substituted.

```rust
// dither/mod.rs, replacing the unconditional accumulate-then-match
if pinned {
    output[idx] = matched_idx;                     // exact ink
    carry = [accumulated[0] * lambda, ...];        // own error is zero
} else {
    // existing path unchanged
    carry = strength_error;
}
// existing kernel / jitter / serpentine loop consumes `carry`
```

### λ, and why decay is per pinned pixel

The error is multiplied by λ each time it crosses a pinned pixel. The count of
pinned pixels the error has crossed **is** its distance into the pinned region, and
it is the distance along the path the error actually travelled — not a Euclidean
distance from the nearest boundary. No distance transform, no extra pass, no
per-pixel buffer.

At depth *n* the surviving fraction is `λ^n`:

| λ | 2 px line | 20 px in | 100 px in |
|---|---|---|---|
| 0.90 | 0.81 | 0.12 | 3e-5 |

This is the property that makes the mechanism correct for both shapes at once. A
2 px grid line or a text stroke is crossed in one or two steps, so error passes
through it nearly intact and flows *around* the element — no seam, and an exact-match
pixel emits no error of its own, so a logo cannot smear outward. A large flat area
absorbs the incoming error within a few pixels of its edge, so nothing survives to
dump as a fringe at the far side.

**λ is a single knob spanning both variants originally proposed.** λ=0 is variant A
(absorb, drop the error), λ=1 is variant B (pure pass-through, error conserved). The
sweep therefore contains both endpoints, and the spike answers "is B actually better
than A" with a measurement rather than an argument.

Serpentine scanning reverses direction on alternate rows, so over a two-row period
the decay is symmetric across a region and introduces no left/right bias.

`error_clamp` continues to bound what a pinned pixel's neighbours receive, so the
carry cannot exceed the bound that applies today.

## Where pinning is allowed — owner ruling

**Pinning is eligible everywhere outside a `data-byonk-tone="continuous"` region, in
every document, including documents that carry no tone markup at all.**

This is the owner's ruling of 2026-08-10, taken over the narrower "only in documents
that carry tone markup" gate. It reaches every screen's text and logos immediately.
Its cost is that it also pins `calibration/color`'s photograph wherever a pixel
coincidentally lands exactly on an ink, which is the content the original seam bug
came from. The spike measures that directly — see measurement 2.

Eligibility is therefore the **inverse of the tone mask**. On an unmarked document
the eligibility mask is all-true and no second rasterization is needed.

### The authoring principle this establishes

The mask is **not** "which region of the layout" — it is "which content is
continuous-tone". Structure — a grid, text, rules, a logo — stays unmarked *wherever
it sits*, including in the middle of an otherwise continuous area.

One mask now has two consumers, and they want the same answer: do not gamut-map
structure, and do pin it. Marking by layout region rather than by content type gets
both wrong at once. This is the guidance any future screen author needs, and it is
what makes a single opt-in attribute sufficient for both features.

### The tone screen is currently marked by region, and must be fixed

`screens/builtin/calibration/tone/screen.svg:45` puts the patch grid inside the
marked group:

```svg
<g data-byonk-tone="continuous">
  <image .../>                          <!-- photo -->
  <rect ... fill="url(#huesweep)"/>     <!-- sweep -->
  <rect ... fill="#000000"/>            <!-- the grid, INSIDE the group -->
  {% for p in data.right.patches %}<rect .../>{% endfor %}
</g>
```

The grid is not drawn as lines; it is that black backing rect showing through the
2 px gaps between the patches drawn over it. Inside the group it is ineligible for
pinning, so the marked column would keep a speckled grid while the unmarked column
got a crisp one — an artifact of the authoring, not of the design.

**The rect moves out of the group**, kept immediately before it so document order and
therefore z-order are unchanged. The patches stay marked; they are the content the
mapping exists to act on.

Pure black is in gamut, so this cannot change the mapped patches. It does remove
those pixels from the **adaptation group**, and `R` is a 99th-percentile over the
marked set (`PERCENTILE = 0.99`). The pixel count is small, but a percentile is
exactly the statistic that moves when the set changes. **Measure `R` before and
after; do not assume it is unchanged.**

### Why the seam does not come back

The seam arose because the old pinning **discarded** a coincidentally-matching
pixel's accumulated error. Under this design that error is not discarded; it is
passed on, attenuated by λ. A coincidental match mid-gradient is by construction
an isolated pinned pixel — depth 1 — so it retains `λ` of the incoming error, which
at λ=0.9 is 90%. Total error across the region is very nearly conserved, and the
neighbours compensate.

That is the reasoning. It is not the evidence. Measurement 2 is the evidence.

## Division of responsibility

- **eink-dither owns "is this a pure panel colour".** It holds the palette; the
  exact-match test is against the **nominal** palette entries, not the measured
  `actual` values, because an SVG author writes the nominal colour. The caller does
  not supply indices — the crate determines the match itself, so a caller cannot
  force a wrong index.
- **byonk owns "is pinning allowed here".** It builds the eligibility mask from the
  tone mask it already rasterizes.

### Plumbing

- `EinkDitherer::dither(&pixels, w, h)` keeps its signature and delegates to a new
  `dither_with_pinning(&pixels, w, h, pin_eligible: Option<&[bool]>)` passing `None`.
- **`None` means pinning is off entirely, not "eligible everywhere".** The inverse
  would silently change the output of every existing eink-dither caller and test,
  which is precisely the class of change that makes a green suite meaningless. A
  caller that wants frame-wide pinning passes an explicit all-true slice, and byonk
  does exactly that for unmarked documents.
- **The per-pixel mask must not live in `DitherOptions`.** That struct is `Clone` and
  is cloned on every `dither()` call (`crates/eink-dither/src/api/builder.rs:199`);
  a `Vec<bool>` there would be copied per frame.
- **λ does go in `DitherOptions`**, as a scalar with a builder method, so tests can
  sweep it. It is **not** exposed in panel YAML or `DitherTuning` in this spike. No
  user knob until there is a measured default worth defaulting to.
- byonk builds `pin_eligible` in `src/rendering/svg_to_png.rs`. The tone mask is
  currently rasterized only when `has_tone_markup()` **and** `gamut.amount != 0.0`
  (`svg_to_png.rs:134-137`). Pinning is independent of gamut, so that inner gate must
  move: the mask is now needed whenever the document carries markup, whichever
  consumer wants it. The existing length-mismatch hard error stays.
- A length mismatch between `pin_eligible` and the frame is a hard error, matching
  the existing mask handling. It cannot happen; it is loud rather than silent.

### Where the match is decided, and why not in the dither loop

`Preprocessor::process` runs between the caller's `Srgb` pixels and
`dither_with_kernel_noise`. Matching inside the dither loop would mean matching the
*preprocessed* `LinearRgb` value, and saturation or contrast at anything other than
identity moves a pure ink off its palette entry. Exact match would then never fire
and the feature would be a silent no-op that still passes every test written against
the mechanism in isolation.

**So the match is resolved before preprocessing, on the caller's `Srgb` bytes**, by
exact `[u8; 3]` equality against `Palette::official(i).to_bytes()`, AND-ed with the
eligibility mask. The result is a `Vec<Option<u8>>` — the resolved ink index, or
`None` — handed to the dither loop alongside the pixels.

Two consequences, both deliberate:

- **A pinned pixel is not enhanced.** It renders the colour the author wrote. This is
  the right answer for structure, which is what pinning is for, and it is moot in
  byonk's production path, which uses identity saturation and contrast.
- **Pinning requires no resize.** Resampling destroys exact matches and breaks the
  index correspondence between the caller's pixels and the preprocessed frame. When
  `target_width`/`target_height` are set, pinning is refused rather than silently
  producing a misaligned map. (`resize_lanczos` panics by design today, so this path
  is unreachable in practice; it is guarded so it stays that way.)

## Measurements

All in **linear light**. Every visual dither comparison in this tree reads about 30%
too dark when a viewer downscales a PNG without linearising; relative comparisons
between variants stay valid, absolute judgements need the undithered renders.

Whole-image means are not evidence here. Measure the pixels the change touches.

1. **Does it fix the reported defect?** Grid-line ink share in the `calibration/tone`
   screen, both columns, **after the backing rect has been moved out of the marked
   group**. Baseline 73.2% (unmarked) and 71.4% (marked) black. Success: **both**
   columns approach 100%. The grid is unmarked on both sides, so both are pinned, and
   the two columns continue to differ by exactly the mapping — which is the whole
   point of the screen.
   Also record `R` (the adaptation factor over the marked set) before and after the
   SVG change, and the mapped patch colours, to confirm removing the black pixels
   from the adaptation group moved neither.
2. **Does it seam the photograph?** Under the ruling above, `calibration/color`'s
   photo is eligible. First count **how many of its pixels are exact ink matches at
   all** — if the count is negligible the question is moot and the measurement is
   cheap. If it is material, measure at the **borders of pinned runs**, 8×8 block
   mean against source, never a whole-image mean.
3. **Does error dump at the far edge?** Synthetic: a wide pure-ink bar abutting
   saturated content. Compare ink share in the last 10 px against the middle. λ=1 is
   expected to show the fringe; the point is to confirm λ<1 removes it rather than
   assume it.
4. **λ sweep**: 0.0, 0.5, 0.8, 0.9, 0.95, 1.0 against measurements 1, 2 and 3
   together, to choose the default. The sweep's endpoints are variants A and B.
5. **Text on a real screen.** The motive is text and logos, not the calibration grid.
   Black text abutting saturated content, rendered before and after.
6. **Cost.** Expected negligible — one equality test per pixel — but stated rather
   than assumed.

### Test discipline

Every new guard is **mutation-verified in both directions**: a mutant that never pins
must fail the pinning guard, and a mutant that pins unconditionally (ignoring
eligibility) must fail the eligibility guard. Two inherited tests in this tree were
found to be guarding nothing precisely because they were never mutated.

Synthetic sweeps stay inside inputs that can actually occur. Flat-patch dE is
misleading on its own — every artifact that matters is at a boundary between colours.

## Scope

**This is a spike.** Measured, committed on `feat/screen-store-authoring-core`, λ not
wired to configuration. If it lands well it graduates to a proper task. If λ around
0.9 seams the photograph, the legitimate outcome is that the eligibility ruling needs
the narrower "documents with tone markup only" gate after all.

Two things are in scope despite being a spike, because without them the spike cannot
be judged:

- **The `calibration/tone` SVG change** — moving the backing rect out of the marked
  group. This alters a shipped screen's output. Without it the measurement is taken
  on a screen whose markup contradicts the design.
- **The authoring principle** — "mark content that is continuous-tone, not regions of
  the layout" — belongs in the tone-markup documentation, because it is now what makes
  one attribute serve two features. Written when the spike graduates, not before; a
  documented rule that then changes is worse than an undocumented one.

Out of scope: changing the default dither algorithm, the three open dithering
defects, widening the tone screen's patch gap (measured and rejected as
symptom-treatment: 2 px → 73.2% black, 3 px → 81.5%, 4 px → 86.1%, 6 px → 90.5%,
roughly one contaminated pixel per side regardless of width, and it would leave every
other screen's text unfixed).

## Verification

- `CARGO_BUILD_JOBS=2 cargo check --workspace --all-targets`, then
  `CARGO_BUILD_JOBS=2 cargo test -p eink-dither --lib` and `-p byonk --lib`.
- `make check` is the full gate and takes ~10 minutes — background it. It runs
  `cargo fmt`, not `cargo fmt --check`, so it rewrites files in place.
- Rendering a builtin screen needs a device with a `panel:` set, or the render is
  silently greyscale. Copy `config.yaml`, point `CONFIG_FILE` at the copy, and use
  `render --mac <MAC> --output <PATH>`. Never edit the tracked `config.yaml`.
