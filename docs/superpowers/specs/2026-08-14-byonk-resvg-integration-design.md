# Integrating byonk-base into byonk

**Date:** 2026-08-14
**Repo:** `oetiker/byonk`
**Lands on:** `feat/screen-store-authoring-core` (PR #30)
**Prerequisite:** a ready `byonk-base` — see
`2026-08-14-resvg-byonk-base-design.md`

One integration, not two. byonk takes the port jump and the hinting features
together, against a `byonk-base` that already carries the full stack.

## Problem

byonk pins `resvg`, `usvg`, and `fontdb` to the fork's `skrifa` branch
(`Cargo.toml:104-106`), freezing it at resvg v0.46 while upstream shipped v0.47,
v0.48, and v0.48.1. The fork branch is obsolete: upstream adopted the
harfrust/skrifa port itself in v0.48.0.

Separately, byonk configures no font hinting at all, and its screens mix bitmap
pixel fonts such as X11Helv with outline and variable fonts — font kinds that
want opposite hinting treatment.

## Scope

1. A dependency audit of every crate byonk shares across the resvg boundary.
2. Bump `resvg` 0.46 -> 0.48.1, `tiny-skia` 0.11 -> 0.12, `fontdb` 0.23 -> 0.24;
   point `resvg`/`usvg` at `byonk-base`; drop the `fontdb` pin.
3. Reimplement `bitmap_strikes` in byonk using skrifa.
4. A Lua `fonts` directive covering per-font hinting and bitmap strike choice.
5. A render-scale warning.
6. A three-state render capture so #30's manual assessment can attribute changes.

`byonk-base` is built and green as of `b67da7c0` — see the companion spec's
status section. Nothing here is blocked.

## Step 0: dependency audit

**Do this before touching code.** byonk shares types with resvg across the API
boundary, so every shared crate must resolve to exactly the version resvg 0.48.1
resolves. A mismatch produces two distinct copies of the same type and does not
compile.

Two are already known:

| Crate | byonk today | Required | Why it is load-bearing |
|---|---|---|---|
| `tiny-skia` | `0.11` (`Cargo.toml:21`) | `0.12` | byonk builds the `Pixmap` and passes `pixmap.as_mut()` into `resvg::render` (`svg_to_png.rs:18,229,232`). resvg bumped this in **0.47.0** (resvg CHANGELOG.md:76) — a *major* bump, so expect API changes, not just a version bump. |
| `fontdb` | `0.23` (`Cargo.toml:31`) | `0.24` | byonk passes `Arc<fontdb::Database>` into `usvg::Options.fontdb` (`svg_to_png.rs:210-212`). |

The audit enumerates the rest and checks each one's breaking changes. Do not
assume this table is complete — that assumption is what made an earlier draft of
this spec miss `tiny-skia`.

Also confirm byonk still gets what it needs from default features: resvg 0.48.0
added `svgz` and `writer` feature gates (#1088) to reduce required dependencies.
MSRV for resvg 0.48.1 is 1.85.0.

## Dependency changes

```toml
resvg     = "0.48.1"   # was 0.46   (Cargo.toml:20)
tiny-skia = "0.12"     # was 0.11   (Cargo.toml:21)
fontdb    = "0.24"     # was 0.23   (Cargo.toml:31)

[patch.crates-io]
resvg = { git = "https://github.com/oetiker/resvg.git", branch = "byonk-base" }
usvg  = { git = "https://github.com/oetiker/resvg.git", branch = "byonk-base" }
# fontdb patch removed
```

## bitmap_strikes moves into byonk

byonk exposes a per-face list of bitmap strike sizes to Lua (`lua_runtime.rs:861`,
via `FontFaceInfo.bitmap_strikes` at `lua_runtime.rs:55`, populated at
`content_pipeline.rs:115` from `svg_to_png.rs:62`). On the `skrifa` branch this
came from a `bitmap_strikes` field added to fontdb's `FaceInfo`, which is why
fontdb had to be vendored into the resvg workspace and why byonk pins fontdb to
the resvg repo at all.

crates.io `fontdb 0.24` has no such field, and fontdb is a separate upstream
project, so no resvg PR could ever carry this.

It does not need one. The field was never new capability, only a cached
convenience. skrifa exposes the data directly (`skrifa-0.44/src/bitmap.rs`):

```rust
BitmapStrikes::new(&font_ref)   // :23
    .iter()                      // :123 -> Iterator<Item = BitmapStrike>
    .map(|s| s.ppem())           // :143 -> f32
// also .len(), .is_empty(), .format() -> Option<BitmapFormat>
```

Upstream's own `flatten.rs:302` already calls `BitmapStrikes::new` internally.

**Decision:** byonk computes the strike list itself, with skrifa, over the font
bytes it already owns (`svg_to_png.rs:35-38` loads them from its asset loader).
The fontdb pin disappears, and nothing un-upstreamable is left on the resvg side.

Keep `FontFaceInfo.bitmap_strikes: Vec<u16>` sorted ascending — the existing test
asserts sortedness (`svg_to_png.rs:463`) and the Lua table is 1-indexed
(`lua_runtime.rs:861`).

## The font_hinting Lua directive

byonk's Lua contract is that a screen script returns a table of render
directives, and `run_script` parses optional keys off it into `ScriptResult`
(`lua_runtime.rs:110-141`): `colors`, `dither`, `preserve_exact`, `error_clamp`,
`noise_scale`, `chroma_clamp`, `strength`. Font configuration is one more such
key. No new mechanism.

`byonk-base` exposes two per-font decisions, and they are entangled: declining a
font's bitmap strikes is what sends its glyphs to the outline, and an outline is
the only thing hinting can act on. A family's entry therefore has to be able to
say both things at once, which rules out a directive shaped only like
`FontHintingOptions`.

```lua
font_hinting = {
  -- The document-wide default, a structural mirror of FontHintingOptions.
  engine = "auto_fallback",        -- interpreter | auto | auto_fallback
  target = "mono",                 -- or a table:
  -- target = {
  --   mode = "normal",            -- normal | light | lcd | vertical_lcd
  --   symmetric_rendering = true,
  --   preserve_linear_metrics = false,
  -- },

  -- Per family. Drives select_hinting and select_bitmap together.
  fonts = {
    -- Keep the pixels: use the strikes, and hinting is moot for them anyway.
    X11Helv = { strikes = true },

    -- Same font, opposite intent: ignore the strikes and hint the outline.
    -- `hinting` accepts the same shape as the top level, or false for none.
    ["X11Helv Outline"] = { strikes = false, hinting = { target = "mono" } },

    Inter = { hinting = { target = { mode = "light" } } },
    Noto = { hinting = false },
  },
}
```

Every field is optional. An absent `font_hinting` means hinting off, matching
`Options::font_hinting: None`; an absent `strikes` means the resvg default,
which is to use them. A family entry may still be given as a bare hinting value
(`Inter = false`) for the common case, with the table form used when strikes
matter.

The `fonts` sub-table resolves families through the same `fontdb` query byonk
already performs, and is installed as both selectors.

This composes with the existing `fonts` global, which exposes each face's
`bitmap_strikes`: a script can decide *from introspection* — a face with strikes
at the size being drawn wants them used and needs no hinting, a face without
wants hinting on — rather than hardcoding family names.

One thing the directive cannot express, because resvg does not offer it:
scaling a strike to a size it was not drawn for. #1115's rule stands — a strike
is used only at its own size, and every other size falls back to the outline
whatever `strikes` says. A screen wanting crisp pixels at an arbitrary size must
pick a size the font actually carries, which is exactly what the `fonts` global
is there to tell it.

## Hinting is effective at byonk's render scale

`FontHintingOptions`' docs note that hinted output lands on whole pixels only
when a tree is rendered without scaling or at an integer zoom. byonk satisfies
this by construction.

`layout.width`/`layout.height` given to Lua are the device's native pixel
dimensions (`lua_runtime.rs:238-243`). Screens author their SVG at those
dimensions, so in `rasterize_svg` the SVG size equals the panel size and

```rust
let scale = scale_x.min(scale_y);   // svg_to_png.rs:212-219  ->  exactly 1.0
```

Scale 1.0 is the ideal case. Note that `layout.scale` (`lua_runtime.rs:240`,
`width/800.0`) is a *design* scale scripts use to size fonts; it is not the
render transform. Do not conflate them.

The exposure is narrow: a screen that **hardcodes 800x480** instead of using
`layout.width`/`layout.height` renders at a fractional scale on any other panel,
and gets degraded hinting along with the resampling blur it already suffers.

**Add a warning in `rasterize_svg` when the computed scale is not 1.0.** One
line, it turns an invisible authoring mistake into a visible one, and it is
useful whether or not hinting is enabled.

## Landing this with PR #30

PR #30 (`feat/screen-store-authoring-core`, +49225/-2069 over 179 files) already
changes rendered output and already requires a full manual assessment of every
screen. **The integration commits go onto that branch**, so one assessment pass
covers both changes instead of two.

This is a deliberate trade: it costs attribution. When a screen looks wrong,
nothing tells you whether #30 or the resvg change caused it — and #30 is large.

Recover attribution cheaply by automating the *rendering* even though the
*judging* stays manual. Capture every bundled screen at its real panel size in
three states:

1. `main` before #30 (current pinned `skrifa` build) — the baseline
2. #30 without the integration commits
3. #30 with the integration commits

Pixel-diff 2 against 3. Only the screens that differ were touched by the resvg
change, so manual review focuses there; everything else is being assessed for #30
reasons anyway.

This matters because byonk has **no assertions on rendered output today**.
`test_bitmap_font_render` (`svg_to_png.rs:470`) renders, writes a PNG to `/tmp`,
prints the size, and asserts nothing; `api_image_test.rs` only checks that the
bytes are a PNG with the right content-type and length. Meanwhile resvg 0.48.0's
changelog states plainly:

> May result in small rendering changes compared to older versions of resvg.

on top of `Glyph advances are now calculated correctly (#1043)`, `Text nodes now
inherit their absolute transform (#1040)`, and `Transforms are no longer applied
twice in abs_transform (#1056)` — three text-positioning fixes, against
hand-tuned fixed-panel layouts.

While here: give `test_bitmap_font_render` a real assertion or delete it. A test
that only prints looks like coverage during exactly the change where coverage
matters, and its hardcoded `/tmp` output path is a second reason to revisit it.

## Testing

- `test_bitmap_strikes_exposed` and `test_bitmap_font_families`
  (`svg_to_png.rs:427-467`) must pass **unchanged**. They are the contract for the
  fontdb substitution — non-empty and ascending strike lists for X11Helv.
- Lua directive parsing: each field, omitted fields falling back to defaults,
  `false` for a family, the bare-value shorthand, `strikes` with and without a
  sibling `hinting`, and malformed input rejected with a useful error.
- A render test through the Lua path proving both per-font hinting and the
  strike choice reach the renderer.
- The scale-not-1.0 warning fires for a hardcoded-dimension screen on a
  differently sized panel.
- The full existing suite passes.
- The three-state capture is the rendering check; there is no automated pixel
  assertion and this spec does not add one.

## Risks

**Attribution is deliberately traded away** (see above), mitigated by the
three-state capture rather than eliminated.

**`tiny-skia` 0.11 -> 0.12 is a major bump.** byonk's use is small — `Pixmap`,
`Color::WHITE`, `as_mut()` — but the audit exists because "small use" is what
made this get missed once already.

**The Lua surface follows upstream.** If a maintainer reshapes `FontHintingOptions`
before #1116 lands, the Lua mapping reshapes with it.

**Nothing here has been compiled.** All claims come from reading code and
changelogs.

## Sequence

0. Dependency audit. Enumerate shared crates, pin versions, read breaking changes.
1. Capture the pre-#30 baseline renders (state 1) and the #30-only renders
   (state 2).
2. On `feat/screen-store-authoring-core`: bump dependencies, point at
   `byonk-base`, reimplement `bitmap_strikes` with skrifa, drop the fontdb pin.
   Get it compiling and green before adding anything.
3. Add the `font_hinting` Lua directive (hinting + strikes) and the
   render-scale warning.
4. Capture state 3; pixel-diff against state 2; hand the differing screens to the
   manual assessment.
5. Fix or delete `test_bitmap_font_render`.

Step 0 is first because it is where the surprises are, and it needs no code.
Step 2 is deliberately separated from step 3: get byonk running on the new resvg
before layering a new feature on top.
