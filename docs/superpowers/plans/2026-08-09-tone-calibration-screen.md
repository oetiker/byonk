# Tone Calibration Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a builtin screen `byonk-builtin/calibration/tone` that renders a photograph, a hue sweep and a patch grid twice side by side, with only the right column marked `data-byonk-tone="continuous"`, so the gamut mapper's effect is visible on a real panel.

**Architecture:** A standard builtin screen directory — `meta.yaml` + `script.lua` + `screen.svg` + a JPEG asset. The Lua script precomputes **absolute** coordinates for both columns (no arithmetic in the template), and the SVG wraps only the right column's three content bands in a single `<g data-byonk-tone="continuous">`. A render test rasterizes the resulting tone mask and asserts it covers the right column and nothing else.

**Tech Stack:** Lua 5.4 via `mlua`, Tera templating, `resvg`/`usvg`/`tiny-skia` 0.46 for the test's mask rasterization, Rust 2021.

## Global Constraints

- **Spec of record:** `docs/superpowers/specs/2026-08-09-tone-calibration-screen-design.md`. Read it before starting.
- **No gamut knobs.** The script must NOT return a `gamut` table. `knee`, `amount` and `max_compression` are deliberately not exposed — a default restated in YAML would silently drift from the Rust constants.
- **Both columns must render identical content.** The adaptation factor `R` is derived from the marked pixels only; if the columns differed, the mapped side would be adapting to pixels the control does not show, and the comparison would be meaningless.
- **Only the right column's three content bands are marked.** Header text stays outside the marked group.
- **Never `git add -A`** in this repository. Add by explicit path and check `git diff --cached` before committing.
- **Build commands:** `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib <filter>`. Do **not** run `make check` in the foreground — it takes ~10 minutes and the subagent watchdog fires at 600 s of silence.
- **Tera is the template engine**, so `{% for %}` / `{{ }}` are the syntax the other calibration screens already use.

---

### Task 1: The screen renders, both columns, unmarked

Deliverable: `byonk-builtin/calibration/tone` renders without error on an 800×480 six-colour panel, showing two visually identical columns. No tone marking yet — that is Task 2, so that the marking's effect is observable as a *change* rather than baked in from the start.

**Files:**
- Create: `screens/builtin/calibration/tone/photo.jpg`
- Create: `screens/builtin/calibration/tone/meta.yaml`
- Create: `screens/builtin/calibration/tone/script.lua`
- Create: `screens/builtin/calibration/tone/screen.svg`
- Test: `src/services/screen_store.rs` (tests module, near the existing `render_*` tests)

**Interfaces:**
- Consumes: the Lua globals `layout` (`.width`, `.height`), `params`, and `scale_pixel(n)`; all already provided by `lua_runtime.rs`.
- Produces: the screen ref string `byonk-builtin/calibration/tone`, used by Task 2's test.

- [ ] **Step 1: Create the photo asset**

The screen never displays the photo wider than about 600 px (half of the 1200 px E1004), so a copy of the 1.5 MB `photo.png` would be repository weight for nothing. Downscale it.

macOS (`sips` is built in):

```bash
mkdir -p screens/builtin/calibration/tone
sips -Z 640 \
  --setProperty format jpeg \
  --setProperty formatOptions 88 \
  screens/builtin/calibration/color/photo.png \
  --out screens/builtin/calibration/tone/photo.jpg
```

If ImageMagick is preferred or `sips` is unavailable:

```bash
magick screens/builtin/calibration/color/photo.png \
  -resize 640x640 -quality 88 \
  screens/builtin/calibration/tone/photo.jpg
```

Verify it is well under 300 KB:

```bash
ls -la screens/builtin/calibration/tone/photo.jpg
```

- [ ] **Step 2: Write `meta.yaml`**

```yaml
title: Tone Marker A/B
description: The same photograph, hue sweep and patch grid rendered twice — left untouched, right marked as continuous-tone so the gamut mapper compresses it into the panel's reachable colours. Shows what the tone marker actually changes.
byonk: "0.17"
refresh: 3600
params:
  hues:
    type: int
    label: "Hue columns"
    description: "Hue columns in the patch grid. Lower than the full-width Gamut Patches screen because each column here is half as wide."
    default: 12
    min: 2
    max: 48
    mode: box
  levels:
    type: int
    label: "Lightness rows"
    description: "Lightness rows in the patch grid"
    default: 5
    min: 1
    max: 12
    mode: box
```

- [ ] **Step 3: Write `script.lua`**

Note the two deliberate choices: absolute coordinates are computed for both columns so the template needs no arithmetic, and the photo band absorbs the leftover height so the layout stays valid on a taller panel.

```lua
-- Tone marker A/B.
--
-- The same three bands twice: the left column untouched, the right column
-- wrapped in `data-byonk-tone="continuous"` so the gamut mapper acts on it.
-- Everything else about the two columns is identical, and that is what makes
-- the comparison fair. The mask is frame-level -- there is exactly one
-- adaptation group -- so the adaptation factor R is derived from the marked
-- pixels alone. The mapped column therefore adapts to precisely the content
-- it displays.
--
-- The bands answer different questions: the photograph shows the everyday
-- benefit on real content, the hue sweep shows banding and tail separation
-- across a controlled gradient, and the patch grid shows ink survival and
-- which hues collapse onto a single palette entry.
--
-- Params:
--   hues   hue columns in the patch grid (default 12)
--   levels lightness rows in the patch grid (default 5)

local width = layout.width
local height = layout.height

local hues = tonumber(params.hues or 12)
if hues < 2 then hues = 2 end
if hues > 48 then hues = 48 end

local levels = tonumber(params.levels or 5)
if levels < 1 then levels = 1 end
if levels > 12 then levels = 12 end

local grid = scale_pixel(2)
local margin = scale_pixel(4)
local gutter = scale_pixel(6)
local band_gap = scale_pixel(4)
local font_size = scale_pixel(10)
local header_h = font_size + scale_pixel(4)

-- HSL -> sRGB. Same convention as the color and gamut calibrators, so a hue
-- number means the same thing on all three screens.
local function hsl_to_rgb(h, s, l)
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r, g, b
  if     h < 60  then r, g, b = c, x, 0
  elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c
  else                r, g, b = c, 0, x
  end
  return math.floor((r + m) * 255 + 0.5),
         math.floor((g + m) * 255 + 0.5),
         math.floor((b + m) * 255 + 0.5)
end

-- Two equal columns either side of a neutral gutter.
local col_w = math.floor((width - 2 * margin - gutter) / 2)

-- Band heights. The sweep and patch bands are fixed; the photo takes whatever
-- is left, so a taller panel grows the photograph rather than overflowing.
local body_y = margin + header_h
local body_h = height - body_y - margin
local sweep_h = scale_pixel(55)
local patch_h = scale_pixel(205)
local photo_h = body_h - sweep_h - patch_h - 2 * band_gap

-- On a short panel the fixed bands can leave nothing for the photo. Give the
-- photo a floor and take the difference out of the patch grid, which degrades
-- gracefully; a zero-height photo does not.
local photo_min = scale_pixel(60)
if photo_h < photo_min then
  patch_h = patch_h - (photo_min - photo_h)
  photo_h = photo_min
  if patch_h < scale_pixel(30) then patch_h = scale_pixel(30) end
end

-- Patch cell geometry, shared by both columns. Remainders are spread one pixel
-- at a time across the leading cells so the grid stays flush with the column.
local avail_w = col_w - (hues + 1) * grid
local cell_w = math.floor(avail_w / hues)
local w_rem = avail_w - cell_w * hues

local avail_h = patch_h - (levels + 1) * grid
local cell_h = math.floor(avail_h / levels)
local h_rem = avail_h - cell_h * levels

-- Build one column's absolute geometry at origin `x`.
local function column(x, label)
  local photo_y = body_y
  local sweep_y = photo_y + photo_h + band_gap
  local patch_y = sweep_y + sweep_h + band_gap

  local patches = {}
  local py = patch_y + grid
  for row = 1, levels do
    local ch = cell_h + ((row <= h_rem) and 1 or 0)
    -- Spread lightness over the usable middle: pure 0 and 1 are black and
    -- white at every hue and would waste two rows saying so.
    local l = 0.2 + 0.6 * ((row - 1) / math.max(levels - 1, 1))
    if levels == 1 then l = 0.5 end

    local px = x + grid
    for col = 1, hues do
      local cw = cell_w + ((col <= w_rem) and 1 or 0)
      local hue = (col - 1) * 360 / hues
      local r, g, b = hsl_to_rgb(hue, 1.0, l)
      table.insert(patches, {
        x = px, y = py, width = cw, height = ch,
        color = string.format("rgb(%d,%d,%d)", r, g, b),
      })
      px = px + cw + grid
    end
    py = py + ch + grid
  end

  return {
    label = label,
    label_x = x,
    photo = { x = x, y = photo_y, width = col_w, height = photo_h },
    sweep = { x = x, y = sweep_y, width = col_w, height = sweep_h },
    patch_bg = { x = x, y = patch_y, width = col_w, height = patch_h },
    patches = patches,
  }
end

-- Hue sweep gradient stops, shared by both columns.
local hue_stops = {}
for i = 0, 12 do
  local hue = i * 360 / 12
  local r, g, b = hsl_to_rgb(hue % 360, 1.0, 0.5)
  table.insert(hue_stops, {
    offset = string.format("%.4f", i / 12),
    color = string.format("rgb(%d,%d,%d)", r, g, b),
  })
end

return {
  data = {
    width = width,
    height = height,
    font_size = font_size,
    label_y = margin + font_size,
    hue_stops = hue_stops,
    left = column(margin, "UNMAPPED (control)"),
    right = column(margin + col_w + gutter, "GAMUT MAPPED"),
  },
  refresh_rate = 3600,
}
```

- [ ] **Step 4: Write `screen.svg` (unmarked for now)**

Both columns are written out in full rather than looped, because in Task 2 only one of them gets wrapped in the tone group — a loop would have to carry a conditional and would obscure the one thing this screen is about.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {{ data.width }} {{ data.height }}" width="{{ data.width }}" height="{{ data.height }}">
  <defs>
    <style>
      text {
        {% include "byonk-base-v1/hinting.svg" %}
      }
    </style>
    <linearGradient id="huesweep" x1="0%" y1="0%" x2="100%" y2="0%">
      {% for stop in data.hue_stops %}
      <stop offset="{{ stop.offset }}" style="stop-color:{{ stop.color }}"/>
      {% endfor %}
    </linearGradient>
  </defs>

  <!-- White ground; the gutter between the columns is simply background, and
       it is load-bearing: error diffusion runs across the whole frame after
       mapping, so the gutter absorbs the seam's error instead of letting the
       mapped column bleed into the control. -->
  <rect width="{{ data.width }}" height="{{ data.height }}" fill="#FFFFFF"/>

  <!-- Column headers. Text is graphic content and is deliberately left
       outside the tone group. -->
  <text x="{{ data.left.label_x }}" y="{{ data.label_y }}" font-family="X11Misc6x" font-size="{{ data.font_size }}" fill="#000000">{{ data.left.label }}</text>
  <text x="{{ data.right.label_x }}" y="{{ data.label_y }}" font-family="X11Misc6x" font-size="{{ data.font_size }}" fill="#000000">{{ data.right.label }}</text>

  <!-- LEFT COLUMN — the control, never marked -->
  <image x="{{ data.left.photo.x }}" y="{{ data.left.photo.y }}" width="{{ data.left.photo.width }}" height="{{ data.left.photo.height }}" href="photo.jpg" preserveAspectRatio="xMidYMid slice"/>
  <rect x="{{ data.left.sweep.x }}" y="{{ data.left.sweep.y }}" width="{{ data.left.sweep.width }}" height="{{ data.left.sweep.height }}" fill="url(#huesweep)"/>
  <rect x="{{ data.left.patch_bg.x }}" y="{{ data.left.patch_bg.y }}" width="{{ data.left.patch_bg.width }}" height="{{ data.left.patch_bg.height }}" fill="#000000"/>
  {% for p in data.left.patches %}
  <rect x="{{ p.x }}" y="{{ p.y }}" width="{{ p.width }}" height="{{ p.height }}" fill="{{ p.color }}" shape-rendering="crispEdges"/>
  {% endfor %}

  <!-- RIGHT COLUMN — Task 2 wraps these three bands in the tone group -->
  <image x="{{ data.right.photo.x }}" y="{{ data.right.photo.y }}" width="{{ data.right.photo.width }}" height="{{ data.right.photo.height }}" href="photo.jpg" preserveAspectRatio="xMidYMid slice"/>
  <rect x="{{ data.right.sweep.x }}" y="{{ data.right.sweep.y }}" width="{{ data.right.sweep.width }}" height="{{ data.right.sweep.height }}" fill="url(#huesweep)"/>
  <rect x="{{ data.right.patch_bg.x }}" y="{{ data.right.patch_bg.y }}" width="{{ data.right.patch_bg.width }}" height="{{ data.right.patch_bg.height }}" fill="#000000"/>
  {% for p in data.right.patches %}
  <rect x="{{ p.x }}" y="{{ p.y }}" width="{{ p.width }}" height="{{ p.height }}" fill="{{ p.color }}" shape-rendering="crispEdges"/>
  {% endfor %}
</svg>
```

- [ ] **Step 5: Write the failing render test**

Add to the tests module of `src/services/screen_store.rs`, alongside the existing `render_*` tests.

```rust
#[test]
fn the_tone_screen_renders_both_columns() {
    let (store, _root) = test_store_with_local();
    let res = store.render(
        "byonk-builtin/calibration/tone",
        RenderOpts {
            width: Some(800),
            height: Some(480),
            include_svg: true,
            ..Default::default()
        },
    );
    assert!(res.error.is_none(), "{:?}", res.error);
    assert!(!res.png.is_empty(), "no PNG produced");

    let svg = res.svg.expect("include_svg was requested");

    // `TemplateService::resolve_image_refs` rewrites relative <image href>
    // values to base64 data URIs before the SVG is captured, so the literal
    // string "photo.jpg" is gone by this point. Count the inlined images.
    assert_eq!(
        svg.matches("data:image/jpeg;base64,").count(),
        2,
        "expected the photograph in both columns"
    );
    assert!(svg.contains("UNMAPPED (control)"));
    assert!(svg.contains("GAMUT MAPPED"));

    // Both columns must carry identical content: the adaptation factor is
    // derived from the marked column alone, so a comparison against a
    // different control would be meaningless. Asserted on the script's
    // returned data, where the two columns are directly comparable, rather
    // than by string-matching the expanded SVG.
    let colours = |col: &serde_json::Value| -> Vec<String> {
        col["patches"]
            .as_array()
            .expect("patches must be an array")
            .iter()
            .map(|p| p["color"].as_str().expect("color must be a string").to_string())
            .collect()
    };
    let left = &res.data["left"];
    let right = &res.data["right"];
    assert!(!colours(left).is_empty(), "no patches were generated");
    assert_eq!(
        colours(left),
        colours(right),
        "the two columns must request the same colours in the same order"
    );
    assert_eq!(left["photo"]["width"], right["photo"]["width"]);
    assert_eq!(left["photo"]["height"], right["photo"]["height"]);
    assert_ne!(
        left["photo"]["x"], right["photo"]["x"],
        "the columns must sit at different x offsets"
    );
}
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib the_tone_screen_renders_both_columns`

Expected before the files exist: FAIL with a render error about the screen not resolving. If it fails for a *different* reason — a Lua syntax error, a Tera error — fix that first; a test that fails for the wrong reason proves nothing.

- [ ] **Step 7: Run the test to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib the_tone_screen_renders_both_columns`

Expected: PASS.

- [ ] **Step 8: Look at it**

A calibration screen that has never been looked at is not finished. Render it to a file and open it.

```bash
cp config.yaml /tmp/tone-check.yaml
```

Add a throwaway device to `/tmp/tone-check.yaml` under the devices map — **the `panel:` line is mandatory; without it the render silently comes out greyscale**:

```yaml
  "AA:BB:CC:DD:EE:01":
    panel: reterminal_e1002
    screen: byonk-builtin/calibration/tone
```

```bash
CONFIG_FILE=/tmp/tone-check.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/tone.png
```

Open `/tmp/tone.png`. Expect: two columns that look **identical** (nothing is marked yet), three legible bands each, patch cells around 31×39 px, no clipping at the frame edges. Note that a downscaled PNG reads about 30% too dark in most viewers — that is a viewer artifact, not the render.

- [ ] **Step 9: Commit**

```bash
git add screens/builtin/calibration/tone/meta.yaml \
        screens/builtin/calibration/tone/script.lua \
        screens/builtin/calibration/tone/screen.svg \
        screens/builtin/calibration/tone/photo.jpg \
        src/services/screen_store.rs
git diff --cached --stat
git commit -m "feat(screens): add the calibration/tone A/B layout, unmarked

Two mirrored columns -- photograph, hue sweep, patch grid -- with a neutral
gutter between them. Nothing is marked yet, so both columns render
identically; the tone group lands in the next commit so its effect shows up
as a change rather than as the initial state.

The photo is a 640px JPEG rather than a copy of calibration/color's 1.5 MB
PNG: screens cannot share assets (screen_store rejects \`..\`) and the screen
never displays it wider than ~600px."
```

---

### Task 2: Mark the right column, and guard the mask geometry

Deliverable: the right column's three bands are wrapped in `<g data-byonk-tone="continuous">`, and a test rasterizes the resulting mask and asserts it covers the right column and nothing else.

**Files:**
- Modify: `screens/builtin/calibration/tone/screen.svg` (wrap the right column's bands)
- Test: `src/services/screen_store.rs` (tests module)

**Interfaces:**
- Consumes: `byonk-builtin/calibration/tone` from Task 1; `crate::rendering::tone_mask::{has_tone_markup, build_mask_svg}` (both `pub`, in a `pub mod`).
- Produces: nothing further depends on this.

- [ ] **Step 1: Write the failing mask-geometry test**

This is the guard that earns its place: it catches the marking being dropped, inverted, or hoisted — the failure mode where the screen still renders something plausible while comparing nothing.

Add to the tests module of `src/services/screen_store.rs`:

```rust
#[test]
fn the_tone_screen_marks_its_right_column_and_nothing_else() {
    // `tiny_skia` is a direct dependency and is what `svg_to_png.rs` uses.
    use tiny_skia::{Pixmap, Transform};

    let (store, _root) = test_store_with_local();
    let res = store.render(
        "byonk-builtin/calibration/tone",
        RenderOpts {
            // Pinned rather than left to the default: the assertions below are
            // arithmetic on an 800x480 frame.
            width: Some(800),
            height: Some(480),
            include_svg: true,
            ..Default::default()
        },
    );
    assert!(res.error.is_none(), "{:?}", res.error);
    let svg = res.svg.expect("include_svg was requested");

    assert!(
        crate::rendering::tone_mask::has_tone_markup(svg.as_bytes()),
        "the screen must mark a continuous-tone region"
    );

    // Rasterize the mask document the renderer would build: white inside the
    // marked subtree, black outside, over a black ground.
    let mask_svg = crate::rendering::tone_mask::build_mask_svg(svg.as_bytes())
        .expect("mask rewrite must succeed");
    let tree = usvg::Tree::from_data(&mask_svg, &usvg::Options::default())
        .expect("mask must parse");
    let (w, h) = (800u32, 480u32);
    let size = tree.size();
    let scale = (w as f32 / size.width()).min(h as f32 / size.height());
    let transform = Transform::from_scale(scale, scale);
    let mut pixmap = Pixmap::new(w, h).unwrap();
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Edge antialiasing produces greys; threshold at 0.5 as the renderer does.
    let px = pixmap.data();
    let mut marked = 0usize;
    let mut marked_left = 0usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 4;
            if px[i] > 127 {
                marked += 1;
                if x < (w as usize) / 2 {
                    marked_left += 1;
                }
            }
        }
    }

    assert!(marked > 0, "nothing was marked");

    // The marked region must not reach into the control column. A handful of
    // pixels at the gutter is antialiasing; a percentage is a bug.
    let leak = marked_left as f64 / marked as f64;
    assert!(
        leak < 0.001,
        "{:.3}% of marked pixels fall in the left column",
        leak * 100.0
    );

    // At 800x480 a correct mask measures 0.4605: one 393px column covering
    // the 450px of content below the header. The band is wide on purpose —
    // this guards the marking, not the layout, and a tight bound would fail
    // every time a band height is adjusted.
    //
    // What it discriminates against, measured: marking dropped = 0.0, both
    // columns marked = 0.921, group hoisted to the root = ~1.0. It does NOT
    // discriminate "the header got swallowed" (0.483) from correct, and does
    // not try to — the leak assertion above is what catches misplacement.
    let fraction = marked as f64 / (w as f64 * h as f64);
    assert!(
        (0.35..=0.55).contains(&fraction),
        "marked fraction {fraction:.4} is outside the plausible band \
         (expected ~0.46) — 0 means the marking was dropped, ~0.92 means both \
         columns are marked, ~1.0 means the group was hoisted to the root"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib the_tone_screen_marks_its_right_column`

Expected: FAIL on the first assertion — `the screen must mark a continuous-tone region` — because Task 1 left the document unmarked. **Watch this specific failure.** If it fails later in the test instead, something is already marked and the premise is wrong.

- [ ] **Step 3: Wrap the right column's bands in the tone group**

In `screens/builtin/calibration/tone/screen.svg`, replace the `<!-- RIGHT COLUMN ... -->` block from Step 4 of Task 1 with:

```xml
  <!-- RIGHT COLUMN — the three content bands, marked as continuous-tone.
       The header text above stays outside: text is graphic content, and
       pushing glyph edges through the mapper buys nothing.

       One group, not three, because the mask is frame-level: there is exactly
       one adaptation group, so R is derived from all of these pixels
       together. `data-byonk-tone-group` exists as an attribute but is not
       implemented, and this screen must not become the reason it gets built. -->
  <g data-byonk-tone="continuous">
    <image x="{{ data.right.photo.x }}" y="{{ data.right.photo.y }}" width="{{ data.right.photo.width }}" height="{{ data.right.photo.height }}" href="photo.jpg" preserveAspectRatio="xMidYMid slice"/>
    <rect x="{{ data.right.sweep.x }}" y="{{ data.right.sweep.y }}" width="{{ data.right.sweep.width }}" height="{{ data.right.sweep.height }}" fill="url(#huesweep)"/>
    <rect x="{{ data.right.patch_bg.x }}" y="{{ data.right.patch_bg.y }}" width="{{ data.right.patch_bg.width }}" height="{{ data.right.patch_bg.height }}" fill="#000000"/>
    {% for p in data.right.patches %}
    <rect x="{{ p.x }}" y="{{ p.y }}" width="{{ p.width }}" height="{{ p.height }}" fill="{{ p.color }}" shape-rendering="crispEdges"/>
    {% endfor %}
  </g>
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib the_tone_screen_marks_its_right_column -- --nocapture`

Expected: PASS, with the marked fraction landing near **0.4605** — one 393 px column over the 450 px of content below the header, out of 800×480.

If it lands outside `0.35..=0.55`, do not widen the band — read the number first. Near 0.92 means both columns got marked. Near 1.0 means the group was hoisted to the root. Well below 0.35 means a band was left out of the group.

- [ ] **Step 5: Run both screen tests together**

Run: `CARGO_BUILD_JOBS=2 cargo test -p byonk --lib tone_screen`

Expected: both PASS.

- [ ] **Step 6: Look at it — this is the point of the whole screen**

Re-render with the same throwaway config from Task 1 Step 8:

```bash
CONFIG_FILE=/tmp/tone-check.yaml cargo run -- render --mac AA:BB:CC:DD:EE:01 --output /tmp/tone-marked.png
```

Compare `/tmp/tone.png` (Task 1, unmarked) against `/tmp/tone-marked.png`. Expect:

- the **left** column pixel-identical between the two files apart from a thin band at the seam,
- the **right** column visibly changed: saturated regions of the photograph holding more colour, the hue sweep's vivid arcs less collapsed,
- **no** lightness crush in the photograph's highlights — if the near-whites have visibly darkened, stop and report it, because that is the ray geometry's known liability and it should be bounded by the 0.99 knee.

- [ ] **Step 7: Commit**

```bash
git add screens/builtin/calibration/tone/screen.svg src/services/screen_store.rs
git diff --cached --stat
git commit -m "feat(screens): mark the tone screen's right column

Wraps the right column's photograph, hue sweep and patch grid in a single
data-byonk-tone=\"continuous\" group -- the first shipping markup to mark a
region. The header text stays outside; text is graphic content.

One group rather than three because the mask is frame-level: R is derived
from all marked pixels together, so the mapped column adapts to exactly the
content it displays.

Guarded by a test that rasterizes the mask and asserts it covers the right
column and only the right column. That catches the marking being dropped,
inverted or hoisted -- the failure mode where the screen still renders
something plausible while comparing nothing."
```

---

### Task 3: Document the screen

Deliverable: the new screen is described where users will look for it, and recorded in the changelog.

**Files:**
- Modify: `CHANGES.md`
- Modify: `docs/src/` — the page listing builtin screens, if one exists

**Interfaces:**
- Consumes: the finished screen from Task 2.
- Produces: nothing.

- [ ] **Step 1: Find where builtin screens are documented**

```bash
grep -rn "calibration/gamut\|Gamut Patches\|calibration/color" docs/src/ | head
```

If a page lists the calibration screens, add `calibration/tone` alongside them in the same style. If no such page exists, skip the docs edit — do **not** invent a new documentation page for one screen.

- [ ] **Step 2: Add the CHANGES.md entry**

Under the `Unreleased` section, in the added/new subsection. Keep it user-facing: what a user can now do. No task numbers, no internal mechanics.

```markdown
- New builtin calibration screen **Tone Marker A/B** (`byonk-builtin/calibration/tone`):
  renders a photograph, hue sweep and colour patch grid twice side by side, with the
  right half marked as continuous-tone. Shows on a real panel what gamut mapping
  changes.
```

- [ ] **Step 3: Verify the changelog edit is scoped**

```bash
git diff CHANGES.md
```

Expected: one added bullet under `Unreleased`. Nothing else touched.

- [ ] **Step 4: Commit**

```bash
git add CHANGES.md
# add the docs page too, only if step 1 found one
git diff --cached --stat
git commit -m "docs: record the Tone Marker A/B calibration screen"
```

---

## Final gate (controller, not the task implementer)

Run the full check in a **backgrounded** shell and poll — it takes about ten minutes and a foreground run will trip the subagent watchdog:

```bash
CARGO_BUILD_JOBS=2 make check
```

Expected: `All checks passed!`

Then re-read `docs/HANDOVER.md` and update it: the screen is the first shipping markup to mark a region, which changes the standing statement that the gamut feature "reaches nothing".
