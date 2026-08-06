# Design — Measured Colours in Lua + Image Processing for E-Ink

_Status: draft for review._

## Purpose

Give screen authors — human and LLM alike — the two things missing when a screen has to
look good on a real e-ink panel rather than on a monitor:

1. **Knowledge of, and control over, the panel's measured colours** (`colors_actual`) from
   inside `script.lua`.
2. **A tone-mapping and geometry pipeline for photographs** applied *before* the image is
   embedded in the SVG, so a photo survives being reduced to a six-colour,
   low-dynamic-range display.

A third, small piece falls out of the first two: `render_screen` (MCP) gains explicit
control over whether the returned PNG is drawn in the panel's measured colours or in its
spec colours — the same choice `/dev` already offers a human.

These three ship together because they are one idea: **the panel's real colour behaviour
should be visible to the author, controllable by the author, and usable by the image
pipeline.**

## Background — current state (verified in tree)

### Palette resolution

Byonk carries two palettes per render:

- **Official colours** — what the device spec says (`palette`), used for the bytes sent to
  the panel.
- **Measured / actual colours** (`colors_actual`) — what the panel really shows. Used for
  colour *matching* during dithering, and, in dev preview, for the PNG's `PLTE` so a human
  sees what the panel will look like.

`eink_dither::Palette::new(official, actual)` (`crates/eink-dither/src/palette/palette.rs`)
holds both, precomputed in sRGB / linear / OKLab, and errors on a length mismatch.

The two chains resolve separately, both in `src/api/display.rs`:

- **Official palette** — `resolve_render_params` (`display.rs:179`):
  `script_colors > device_config_colors > panel_colors > fallback`. A script can already
  return `colors = { … }` and win.
- **Measured colours** (`display.rs:575`):
  `dev override > panel.colors_actual > Measured-Colors header > none`.
  **There is no script layer.** `resolve_render_params` receives `measured_colors` already
  resolved and passes it through untouched.

A `measured_source` string is computed alongside and logged, so the origin of the measured
values is already observable.

### Two existing defects this design touches

- **Silent calibration loss.** `src/rendering/svg_to_png.rs:341` sets `eink_actual = None`
  whenever the post-dedup measured list is empty or a different length from the official
  list. This is a deliberate "never fail a device render" fallback, but it is **silent**:
  a script that returns its own `colors` of a different length than the panel's
  `colors_actual` loses the calibration entirely with no diagnostic anywhere.
- **`use_actual` is reachable only by accident.** `src/services/screen_store.rs:1114` does
  `let use_actual = measured_colors.is_some()`. So an MCP `render_screen` call returns
  measured-colour output if and only if the caller happened to pass a `panel` that has
  `colors_actual` configured. The agent cannot ask for it, cannot turn it off, and the
  behaviour is undocumented. `/dev` exposes the same switch explicitly
  (`src/api/dev.rs:67`).

### The Lua environment

`src/services/lua_runtime.rs` (1567 lines) sets up globals before running a script:
`params`, `device` (including `device.colors` and `device.dither.*`), `layout`, `greys`,
`scale_font`, `scale_pixel`, `base64_encode`, `read_asset`, `http_get`/`http_post`,
`html_parse`, `json_*`, `time_*`, `log_*`, and a sandboxed `require`.

A script returns a table; `run_script` (`lua_runtime.rs:168`) extracts `data`,
`refresh_rate`, `skip_update`, `colors`, `dither`, `preserve_exact`, and the dither tuning
values. **There is no image manipulation of any kind.** The `gphoto` example
(`screens/examples/gphoto/script.lua:109`) fetches a JPEG over HTTP and embeds it verbatim
as `"data:image/jpeg;base64," .. base64_encode(img_data)`, relying on Google's `=w…-h…`
URL parameters for sizing and on nothing at all for tone.

### Image decoding

The main crate has no image-decoding dependency. `resvg` pulls `png`, `zune-jpeg` and
`image-webp` into the lockfile transitively, but the `image` façade crate is absent.
`crates/eink-dither` has **zero** runtime dependencies.

## Non-goals

- No changes to the dithering algorithms or to `eink-dither`'s colour science. Every
  finding recorded in project memory about HyAB vs. Euclidean stands untouched.
- No declarative image operations on the SVG `<image>` element. Considered and rejected:
  it invents a non-standard SVG extension, requires an SVG rewriting pass, and cannot react
  to fetched data.
- No new HTTP endpoints. `/dev` keeps its current surface.
- No per-colour HSL, split toning, vignette, grain, lens correction, or denoise. A
  six-colour panel cannot show the difference, and noise actively helps dithering.

---

## Part 1 — `colors_actual` in Lua

### 1.1 Read side

`DeviceContext` gains `colors_actual: Option<Vec<String>>`, populated from the measured
chain that `handle_display` already resolves **before** the script runs (`display.rs:575`).
`ScreenStore::render` populates it the same way from the resolved panel.

Lua sees:

```lua
device.colors         -- { "#000000", "#ffffff", "#ff0000", ... }  official / spec
device.colors_actual  -- { "#0a0a0a", "#e8e6e0", "#a83a30", ... }  measured, or nil
```

`device.colors_actual` is **index-parallel** to `device.colors`, and is **`nil` when no
measured colours were resolved** — deliberately *not* mirrored from `device.colors`, so a
script can distinguish "this panel is uncalibrated" from "this panel measures exactly to
spec". Scripts that want the simple thing write
`local shown = device.colors_actual or device.colors`.

### 1.2 Write side

A script may return `colors_actual`:

```lua
return {
  data = { ... },
  colors        = { "#000000", "#ffffff", "#ff0000", "#00ff00" },
  colors_actual = { "#0a0a0a", "#e8e6e0", "#a83a30", "#3f7a45" },
}
```

`ScriptResult` gains `colors_actual: Option<Vec<String>>`, parsed exactly like the existing
`colors` field (positive-integer keys, non-empty, else `None`).

The measured chain becomes:

```
script > dev override > panel.colors_actual > Measured-Colors header > none
```

This is **symmetric with the official palette chain**, where `script_colors` already beats
everything. One rule to remember: the screen author has the last word.

`resolve_render_params` gains a `script_colors_actual: Option<&[String]>` parameter and
takes over resolving the measured value, rather than receiving it pre-resolved. The
pre-script portion of the chain is still computed early (the script needs to *read* it),
and is passed in as the fallback.

`measured_source` gains the value `"script"`. This is the mitigation for the dev
colour-tuning popup appearing inert on such a screen: the dev UI and the existing
`tracing::debug!` both already surface `measured_source`, so the reason is visible rather
than mysterious.

### 1.3 Length rule and failure behaviour

A script-supplied `colors_actual` must have the same length as the **resolved official
palette**. Scripts that return `colors` and `colors_actual` together satisfy this by
construction.

On mismatch, the render does **not** fail. Byonk:

1. writes a line into the script log (`ScriptResult::logs`, which surfaces in
   `render_screen`'s `log` array and in dev mode), naming both lengths;
2. discards the script value and falls through to the next source in the chain.

Rationale: a device fetching its screen must never be denied content over a calibration
mistake, and the authoring path already has a channel that makes the mistake loud.

### 1.4 Fixing the silent drop

`svg_to_png.rs:341` keeps its fallback — `eink_actual = None` on a length disagreement —
but gains a `tracing::warn!` naming the official and measured lengths. This is the
last-ditch guard behind the check in 1.3; it should now be unreachable from the script
path, and if it fires, that is information worth having.

### 1.5 Testing

- `device.colors_actual` is present and index-parallel when a panel has measured colours;
  `nil` when it does not.
- A script-returned `colors_actual` beats a configured `panel.colors_actual`, and
  `measured_source` reports `"script"`.
- A length mismatch logs and falls back, and the *next* source in the chain is used —
  asserted against the resolved value, not merely against the absence of a panic.
- The `svg_to_png` warning fires on a genuine post-dedup mismatch.

Each of these must be written so that it **fails against the current code**. (Handover:
"the single most valuable catch remains a test that could not fail.")

---

## Part 2 — `image_process`

### 2.1 Why

An e-ink panel is a ~6-colour, low-dynamic-range, non-linear output device. A photograph
sent to it straight loses its shadows into a black sink, blows its highlights to paper
white, and desaturates until nothing reaches a chromatic palette entry — the image dithers
into greys. Lightroom's Basic and Presence panels are almost exactly the right controls for
fixing that, so the operation set is Lightroom's, minus what six colours cannot express.

### 2.2 Where it lives

A new **`crates/eink-photo/`** — a pure pipeline with no byonk dependencies, mirroring how
`crates/eink-dither` is structured. It takes decoded pixels plus a params struct and returns
pixels; it knows nothing about Lua, SVG, or config, and is unit-testable in isolation.

`src/services/lua_runtime.rs` gets only the thin binding: params-table parsing, the call,
and encoding to a data URI. This matters because that file is already 1567 lines.

**Dependency:** `image` with `default-features = false, features = ["jpeg", "png", "webp"]`.
The underlying decoders (`png`, `zune-jpeg`, `image-webp`) are already in the lockfile via
`resvg`, so this adds the façade rather than a new decoder stack.

### 2.3 Surface

One call, all parameters, fixed order. Explicitly **not** a chainable object: a chain reads
as sequential but would either mislead (if it recorded settings and baked later) or accept
wrong orderings (if it processed immediately).

```lua
local src, w, h = image_process(bytes, {
  -- geometry
  crop  = { x = 0.1, y = 0, w = 0.8, h = 1.0 },  -- normalised 0..1
  fit   = "cover",                                -- cover | contain | stretch | none
  width = 800, height = 480,

  -- tone (Lightroom "Basic")
  auto_levels = true,
  exposure    = 0.3,    -- EV, -5..5
  contrast    = 15,     -- -100..100
  highlights  = -20,    -- -100..100
  shadows     = 25,     -- -100..100
  whites      = 0,      -- -100..100
  blacks      = 0,      -- -100..100
  curve       = { {0,0}, {0.5,0.55}, {1,1} },     -- point tone curve, escape hatch

  -- presence
  clarity    = 25,      -- -100..100, large-radius local contrast
  vibrance   = 30,      -- -100..100, weighted toward less-saturated pixels
  saturation = 0,       -- -100..100, global

  -- colour
  temperature = 0,      -- -100..100
  tint        = 0,      -- -100..100
  grayscale   = false,
  invert      = false,

  -- detail
  sharpen = { amount = 40, radius = 1.0 },

  -- byonk-specific
  preset        = "eink",
  palette_aware = true,

  -- output
  format  = "png",      -- png | jpeg
  quality = 90,         -- jpeg only
})
```

Returns three values: the `data:` URI, and the result's pixel width and height — the caller
needs the latter two for the `<image>` box.

Every key is optional. `image_process(bytes, {})` decodes and re-encodes without changing
anything; `image_process(bytes, { preset = "eink" })` is the one-liner a simple photo screen
should use.

**Geometry defaults.** `image_process` knows nothing about the SVG or the device, so there
is no implicit "target box": with `width` and `height` both omitted the image keeps its
source dimensions (after `crop`), and the author passes `layout.width` / `layout.height`
explicitly. `fit` defaults to `"cover"` and is consulted only when at least one of
`width`/`height` is given; `fit = "none"` disables resizing even when they are. Giving
exactly one of `width`/`height` scales the other to preserve aspect ratio, in every `fit`
mode.

### 2.4 Canonical order

Fixed, documented, and not author-controllable:

1. decode, honouring EXIF orientation
2. crop
3. fit / resize to target
4. → **linear light**
5. exposure
6. white balance (temperature, tint)
7. blacks / whites (endpoint placement — where `palette_aware` acts)
8. highlights / shadows recovery
9. contrast
10. tone curve
11. → **back to sRGB**
12. clarity
13. vibrance
14. saturation
15. grayscale / invert
16. sharpen
17. encode

Geometry first is what keeps a 24 MP source cheap. Sharpening last is what makes it mean
anything. `auto_levels`, when set, computes its endpoints after step 3 and feeds step 7.

### 2.5 `preset`

`preset = "eink"` is a **base layer**: it seeds a tuned set of values, and any key the
author sets explicitly overrides it. So `{ preset = "eink", clarity = 0 }` means "the eink
treatment, but no clarity". This is the only rule that stays predictable as the preset's
values are retuned.

The preset's initial values are chosen by measurement against the calibration screens, not
guessed, and the spec deliberately does not freeze them — they are an implementation
detail that will move. The starting point is: `auto_levels = true`, `shadows = +20`,
`highlights = -20`, `clarity = +25`, `vibrance = +30`, `sharpen = { amount = 30, radius = 1.0 }`.

`preset = "none"` (or omitting it) applies nothing. Unknown preset names raise an error
rather than silently doing nothing.

### 2.6 `palette_aware`

`palette_aware = true` places the black and white points at the panel's **real** darkest and
lightest luminance rather than at 0 and 255, so the tone mapping does not spend range the
panel physically cannot show.

It sources the palette in this order: the resolved measured colours (`device.colors_actual`),
then `device.colors`, then nothing. With nothing available it is a **logged no-op**, not an
error — a screen using the preset must still render on an uncalibrated panel.

This is the one operation here that no photo editor has, because no photo editor knows the
output device this precisely, and it is expressible only because of Part 1.

**V1 does endpoints only.** Steering chroma toward reachable palette entries is noted as
future work and is not built: it interacts with the dithering colour science recorded in
project memory, and that interaction needs its own measurement.

### 2.7 Errors and guards

`image_process` **raises a Lua error** on failure, consistent with `http_get` and the
`pcall` idiom the `gphoto` example already uses. Failure cases: undecodable input,
unsupported format, an out-of-range parameter, an unknown `preset` or `fit` value, and the
limits below.

Decompression-bomb guards, enforced via `image::Limits` **before** allocation:

- a cap on source megapixels;
- a cap on source encoded bytes;
- a cap on output dimensions.

Out-of-range slider values are an error rather than a silent clamp, so a typo (`exposure =
30` for `3.0`) is caught rather than saturating.

### 2.8 Output format

Default `png`. `format = "jpeg", quality = 90` is offered because a full-bleed 800×480
photograph is roughly four times smaller as JPEG, and the resulting SVG is cached and
re-parsed on every render. At q90 the artefacts are far below what dithering to six colours
does anyway.

### 2.9 Testing

**Property assertions in `eink-photo`, not golden PNGs** — golden images rot and their
failures are unreadable. Against synthetic gradients and colour ramps:

- `exposure = 1.0` doubles linear-light values;
- `shadows > 0` raises the low-quartile mean while leaving the top decile within ε;
- `highlights < 0` does the mirror;
- `clarity > 0` raises local variance without moving the global mean;
- `vibrance > 0` raises saturation of low-saturation pixels more than of high-saturation
  ones — the property that distinguishes it from `saturation`;
- `crop` + `fit` produce the requested dimensions for every `fit` mode;
- ordering is observable: `sharpen` with a downscale produces a different (sharper) result
  than sharpening a pre-downscaled image, confirming step 16 runs after step 3.

**Limits**: an oversized source is rejected before allocation, asserted by the error, not by
watching memory.

**End-to-end, tying it to the actual goal**: a fixture screen `read_asset`s a small bundled
JPEG, processes it, and renders. The test asserts the **dithered** output's palette
histogram moves the expected way — `vibrance` increases the share of pixels landing on
chromatic palette entries. This is the assertion that proves the feature does what it
exists to do, rather than merely that it runs.

**Integration with Part 1**: `palette_aware` with measured colours yields different
endpoints than `palette_aware` without them.

---

## Part 3 — `render_screen` colour controls

`RenderOpts` (`screen_store.rs:117`) and the `render_screen` MCP tool gain two fields:

- **`use_actual: Option<bool>`** — explicit control over whether the returned PNG is drawn
  in measured colours (what the panel will look like) or in spec colours (what is sent to
  the panel). `None` preserves today's behaviour, `measured_colors.is_some()`, so no
  existing caller changes.
- **`colors_actual: Option<String>`** — comma-separated hex, the same format
  `panel.colors_actual` uses. Without this, `use_actual` is only reachable by first defining
  a panel in `config.yaml`, which an authoring agent has no business doing just to preview.

  Its place in the chain: it occupies the **dev-override slot**, i.e.
  `script > RenderOpts.colors_actual > panel.colors_actual > none`. It is the authoring
  path's equivalent of the dev colour-tuning popup, and like that popup it does not
  override a script that has made its own decision. `measured_source` reports
  `"render_opts"` when it is used.

Both are described in the tool's own description with their exact semantics, including that
`use_actual` changes only the returned PNG's palette and never the dithering decisions.
Tool descriptions are the only contract an MCP client sees, and an agent acts on them.

`/dev` is unchanged — it already has this switch.

### 3.1 Testing

- `use_actual = true` with measured colours produces a PNG whose `PLTE` carries the
  measured values; `use_actual = false` with the same input produces the spec values.
- `colors_actual` passed directly, with no panel configured, reaches the output.
- The default with no `use_actual` matches the pre-change behaviour exactly.

---

## Interaction between the three parts

```
panel.colors_actual ──┐
dev override ─────────┼──► resolved measured colours ──► device.colors_actual (Lua reads)
Measured-Colors hdr ──┘                                          │
                                                                 ▼
                                          script returns colors_actual (Lua writes, wins)
                                                                 │
                        ┌────────────────────────────────────────┤
                        ▼                                        ▼
       image_process{ palette_aware = true }            eink_dither::Palette
       places tone endpoints at the panel's             matches pixels against
       real black and white                             the measured colours
                        │                                        │
                        └──────────────► SVG ──► raster ─────────┘
                                                     │
                                                     ▼
                                        render_screen{ use_actual }
                                        decides which palette the
                                        returned PNG is drawn in
```

## Scope — how this decomposes into plans

Parts 1 and 3 are small and touch the same resolution chain; Part 2 is a new crate with its
own dependency and its own test strategy. They are therefore **two implementation plans**,
executed in order:

- **Plan A — measured colours end to end** (Parts 1 and 3). Self-contained and shippable on
  its own: it makes the panel's real colours readable, overridable, and previewable.
- **Plan B — `image_process`** (Part 2). Depends on Plan A only for `palette_aware`, which
  reads `device.colors_actual`. Everything else in Plan B is independent.

They are specified together because Plan B's most valuable feature (`palette_aware`) exists
only because of Plan A, and splitting the spec would hide that.

## Rollout

Additive throughout. No existing script, config, panel, or MCP call changes behaviour:

- `device.colors_actual` is a new global field.
- `colors_actual` as a script return value is new; scripts that do not return it are
  unaffected.
- `image_process` is a new global.
- `use_actual` and `colors_actual` on `render_screen` default to today's behaviour.

The only behavioural change to existing installations is the new `tracing::warn!` in
`svg_to_png` (§1.4), which is diagnostic output only.

## Documentation

- `docs/src/api/lua-api.md` — `device.colors_actual`, the `colors_actual` return value, and
  a full `image_process` section with the canonical order and the parameter table.
- The `gphoto` example upgraded to use `image_process` with the `eink` preset — the
  motivating case, and the one users copy.
- MCP tool description for `render_screen`.
- `CHANGES.md` under Unreleased, user-facing wording only.

## Open questions

None. Every fork raised during design was decided:

- **Read + override**, not read-only. (User.)
- **Script wins** the measured chain, symmetric with `colors`. (User.)
- **E-ink survival**, not a general editing toolkit. (User.)
- **One parametric call**, not a chainable object — "the pseudo chainable design is
  non-obvious". (User, after initially choosing the chain.)
- Length mismatch **logs and falls back**, never fails a device render.
- `palette_aware` v1 does **endpoints only**.
