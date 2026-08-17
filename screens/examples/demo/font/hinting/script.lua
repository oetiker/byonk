-- Hinting demo: a 3x3 grid of `font_hinting` variants over a single family.
--
-- Rows are the hinting engine, columns are the target. Every cell is a
-- variant of one family (Outfit), so the grid reads out directly what the
-- `font_hinting` directive does — this is the worked example for the
-- Font Hinting page in the docs.
--
-- The third column turns hinting off: the control every other cell is read
-- against.
--
-- What the grid actually shows, measured rather than assumed. Read the mono
-- column first — it is drawn 1-bit, so it carries no anti-aliased greys for
-- the ditherer to perturb, and is the only column where cells can be
-- compared exactly:
--
--   * `auto` and `auto_fallback` are the same picture. None of the fonts
--     byonk ships carries a hinting program — every one has an empty `fpgm`
--     and zero instruction bytes across every glyph — so there is nothing to
--     fall back TO and the fallback engine lands on the automatic hinter.
--     If this pair ever stops matching, byonk's `resolve_auto_fallback` has
--     regressed and `auto_fallback` is silently rendering unhinted.
--   * `interpreter` is visibly worse: stem spacing goes uneven and the glyphs
--     gain ink. It is executing a font program that does not exist, so the
--     outline reaches the rasterizer unfitted. That is the row to point at
--     when someone asks why byonk defaults to `auto`.
--   * In the smooth and off columns the two differ far less, because both are
--     anti-aliased and the difference lands in grey edge pixels rather than
--     in which pixels are covered at all.
--
-- `mode = "light"` has no column: it is byte-identical to "normal".
--
-- Careful when measuring this screen: it is dithered, and error diffusion
-- varies with position, so two cells with identical settings at different
-- places on the page do NOT come out byte-identical. That is the measurement
-- confound, not a rendering difference. Compare within the mono column, or
-- render one variant per image.

local base = "Outfit"

local width = layout.width
local height = layout.height

local sizes = { 10, 12, 14, 17, 20, 24 }
-- Deliberately hostile: bare stems and tight verticals are where hinting
-- shows. A flattering string would hide the differences this grid exists
-- to display.
local sample = "illiIL1 xXHv"

local engines = {
  { name = "auto",          engine = "auto" },
  { name = "interpreter",   engine = "interpreter" },
  { name = "auto_fallback", engine = "auto_fallback" },
}

-- `mode = "light"` is deliberately absent: it renders byte-identically to
-- "normal", so a column for it would be three more duplicate cells.
-- `text_rendering` is the other half of each treatment, and the grid is wrong
-- without it. The Lua `aliased` flag is document-level, so a variant cannot
-- carry it — but aliasing is an ordinary inheritable SVG property, so the
-- element using the variant can ask for it directly. Measured: a mono variant
-- plus `optimizeSpeed` renders byte-identically to the document-level
-- `target = { mode = "mono", aliased = true }`.
--
-- Stating it on every cell also makes the grid mean the same thing on a
-- black-and-white panel as on a grey one, instead of inheriting whichever
-- default the panel happened to give it.
local targets = {
  -- Mono hinting is what makes aliasing safe: the rasterizer has no dropout
  -- control, so an un-hinted outline drawn 1-bit loses stems.
  { name = "mono",   target = "mono",   hinted = true,  text_rendering = "optimizeSpeed" },
  { name = "smooth", target = "smooth", hinted = true,  text_rendering = "optimizeLegibility" },
  { name = "off",    target = nil,      hinted = false, text_rendering = "optimizeLegibility" },
}

local margin = 4
local top = 18
local row_header_w = 22
local col_header_h = 18
local pad = 6
local col_count = #targets
local row_count = #engines
-- Whole pixels, deliberately. Hinting fits the outline to the pixel grid, so
-- a cell origin at x.667 slides the fitted glyph straight back off it and
-- undoes the fit — measured at 3-5% of the ink lost to dropped stems, which
-- is more than the difference between two engines. With fractional cells only
-- the top-left cell of this grid sat on the grid at all, and the rest were
-- being compared against it unfairly.
local cell_w = math.floor((width - margin * 2 - row_header_w) / col_count)
local cell_h = math.floor((height - top - margin - col_header_h) / row_count)

-- Build the variant table alongside the cells, so a cell and the variant it
-- names can never drift apart.
local variants = {}
local cells = {}

for ri, eng in ipairs(engines) do
  for ci, tgt in ipairs(targets) do
    -- The alias is a name byonk intercepts during font selection, so it must
    -- not be a real installed family. "Grid <engine> <target>" says what the
    -- cell is for and is plainly not a font anyone ships.
    local alias = "Grid " .. eng.name .. " " .. tgt.name

    if tgt.hinted then
      variants[alias] = {
        font = base,
        hinting = { engine = eng.engine, target = tgt.target },
      }
    else
      variants[alias] = { font = base, hinting = false }
    end

    local cx = margin + row_header_w + (ci - 1) * cell_w + pad
    local cy = top + col_header_h + (ri - 1) * cell_h + pad
    local lines = {}
    local ly = 0
    for _, sz in ipairs(sizes) do
      ly = ly + sz
      if cy + ly < top + col_header_h + ri * cell_h - pad then
        table.insert(lines, {
          size = sz,
          text = sz .. "px " .. sample,
          x = cx,
          y = cy + ly,
        })
      end
      ly = ly + 2
    end

    table.insert(cells, {
      family = alias,
      text_rendering = tgt.text_rendering,
      lines = lines,
    })
  end
end

local col_headers = {}
for ci, tgt in ipairs(targets) do
  table.insert(col_headers, {
    label = tgt.name,
    x = margin + row_header_w + (ci - 1) * cell_w + math.floor(cell_w / 2),
  })
end

local row_headers = {}
for ri, eng in ipairs(engines) do
  local row_y = top + col_header_h + (ri - 1) * cell_h
  table.insert(row_headers, {
    label = eng.name,
    x = margin + math.floor(row_header_w / 2) + 1,
    y = row_y + math.floor(cell_h / 2),
    line_y = row_y,
    show_line = ri > 1,
  })
end

local col_lines = {}
for ci = 2, col_count do
  table.insert(col_lines, {
    x = margin + row_header_w + (ci - 1) * cell_w,
  })
end

return {
  data = {
    width = width,
    height = height,
    -- The template names this after every alias, so the text still resolves
    -- sensibly if a variant is ever removed.
    base = base,
    cells = cells,
    col_headers = col_headers,
    row_headers = row_headers,
    col_lines = col_lines,
    margin = margin,
    top = top,
    row_header_w = row_header_w,
    col_header_h = col_header_h,
    grid_x1 = margin + row_header_w,
    grid_x2 = width - margin,
    grid_y1 = top + col_header_h,
    grid_y2 = height - margin,
  },
  font_hinting = {
    variants = variants,
  },
  refresh_rate = 3600,
}
