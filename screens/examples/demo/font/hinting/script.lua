-- Hinting demo: a 3x3 grid of `font_hinting` variants over a single family.
--
-- Rows are the hinting engine, columns are the target. Every cell is a
-- variant of one family (Outfit), so the grid reads out directly what the
-- `font_hinting` directive does — this is the worked example for the
-- Font Hinting page in the docs.
--
-- The third column turns hinting off. It is a CONTROL: because no engine
-- runs when hinting is off, those three cells must look identical to each
-- other, and any cell that differs from them is a cell where hinting
-- actually did something.
--
-- Several cells are EXPECTED to coincide. That is a property of the fonts
-- and the engines, not a fault in the grid, and it is the most useful thing
-- the grid teaches: for a font that carries no usable hinting program, the
-- target is what matters and the engine barely does. Measured on Outfit,
-- one line per variant, each rendered in its own image so that error
-- diffusion could not leak between them:
--
--   * `auto` and `auto_fallback` agree in every column — with no hints to
--     fall back to, the fallback engine lands on the automatic hinter.
--   * under `interpreter` the target has no effect, so that whole row is
--     one appearance, and it differs from "off" only marginally.
--   * `mode = "light"` is byte-identical to "normal"; it has no column here
--     for that reason.
--
-- So the grid holds three distinct appearances plus the control, and a cell
-- that stops matching its expected group is a real regression.

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
local targets = {
  { name = "mono",   target = "mono",   hinted = true },
  { name = "smooth", target = "smooth", hinted = true },
  { name = "off",    target = nil,      hinted = false },
}

local margin = 4
local top = 18
local row_header_w = 22
local col_header_h = 18
local pad = 6
local col_count = #targets
local row_count = #engines
local cell_w = (width - margin * 2 - row_header_w) / col_count
local cell_h = (height - top - margin - col_header_h) / row_count

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
      -- Anything that is not mono-hinted needs anti-aliasing restored on a
      -- black-and-white panel, where byonk draws the document 1-bit and an
      -- un-hinted outline would drop stems. optimizeLegibility restores it
      -- and keeps hinting; geometricPrecision would disable hinting.
      needs_aa = not (tgt.hinted and tgt.target == "mono"),
      lines = lines,
    })
  end
end

local col_headers = {}
for ci, tgt in ipairs(targets) do
  table.insert(col_headers, {
    label = tgt.name,
    x = margin + row_header_w + (ci - 1) * cell_w + cell_w / 2,
  })
end

local row_headers = {}
for ri, eng in ipairs(engines) do
  local row_y = top + col_header_h + (ri - 1) * cell_h
  table.insert(row_headers, {
    label = eng.name,
    x = margin + row_header_w / 2 + 1,
    y = row_y + cell_h / 2,
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
