-- Font rendering comparison: the same text, side by side, at the small sizes
-- where rendering choices decide whether type is crisp or mush.
--
-- This is the worked example for the font screenshots in the docs.
--
-- Params:
--   topic = "hinting": Outfit three ways — unhinted and anti-aliased,
--           unhinted and drawn 1-bit, and byonk's own default for the panel.
--   topic = "strikes": X11Helv at its strike sizes and at the sizes between
--           them, next to Outfit (an outline font) at the same size.

local width = layout.width
local height = layout.height

local topic = params.topic or "hinting"

-- Deliberately hostile: bare stems and tight verticals are where rendering
-- differences show. A flattering string would hide them.
local sample = "illiIL1 Minimum 0123"

local margin = 8
local header_h = 22
local col_header_h = 22
local label_w = 64

local columns = {}
local rows = {}
local variants = {}

if topic == "hinting" then
  -- Variant names are hooks byonk intercepts during font selection, so they
  -- must not be real families. The third column names no variant at all: it
  -- is plain Outfit, which gets byonk's adaptive default for this panel.
  variants["Compare Unhinted"] = { font = "Outfit", hinting = false }
  columns = {
    { label = "hinting off", family = "'Compare Unhinted', Outfit",
      text_rendering = "optimizeLegibility" },
    { label = "hinting off, 1-bit", family = "'Compare Unhinted', Outfit",
      text_rendering = "optimizeSpeed" },
    { label = "byonk default", family = "Outfit", text_rendering = nil },
  }
  for _, sz in ipairs({ 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 24 }) do
    table.insert(rows, { size = sz, label = sz .. " px" })
  end
elseif topic == "strikes" then
  -- X11Helv carries strikes at 8, 10, 11, 12, 14, 17, 18, 20, 24, 25 and 34 px
  -- and no outlines: at any other size the nearest strike is scaled.
  local strikes = { [10] = true, [11] = true, [12] = true, [14] = true,
                    [17] = true, [18] = true, [20] = true, [24] = true,
                    [25] = true, [34] = true }
  columns = {
    { label = "X11Helv (bitmap)", family = "X11Helv" },
    { label = "Outfit (outline)", family = "Outfit" },
  }
  for _, sz in ipairs({ 10, 12, 13, 14, 15, 16, 17, 19, 20, 22, 24, 25, 28, 34 }) do
    table.insert(rows, {
      size = sz,
      label = sz .. " px",
      note = strikes[sz] and "strike" or "scaled",
    })
  end
else
  error("Unknown topic: " .. topic)
end

-- Whole pixels, deliberately: hinting fits outlines to the pixel grid, and a
-- fractional origin slides the fitted glyph back off it.
local col_w = math.floor((width - margin * 2 - label_w) / #columns)

local col_headers = {}
for ci, col in ipairs(columns) do
  table.insert(col_headers, {
    label = col.label,
    x = margin + label_w + (ci - 1) * col_w + 4,
  })
end

local lines = {}
local y = header_h + col_header_h + 4
for _, row in ipairs(rows) do
  y = y + row.size + 6
  if y > height - margin then break end
  local cells = {}
  for ci, col in ipairs(columns) do
    table.insert(cells, {
      x = margin + label_w + (ci - 1) * col_w + 4,
      family = col.family,
      text_rendering = col.text_rendering,
    })
  end
  table.insert(lines, {
    y = y,
    size = row.size,
    label = row.label,
    note = row.note,
    cells = cells,
  })
end

local col_lines = {}
for ci = 1, #columns do
  table.insert(col_lines, { x = margin + label_w + (ci - 1) * col_w })
end

local result = {
  data = {
    width = width,
    height = height,
    title = topic == "hinting" and "Hinting: off vs byonk's default (Outfit)"
      or "Bitmap strikes: X11Helv on and between its strike sizes",
    sample = sample,
    margin = margin,
    header_h = header_h,
    col_header_y = header_h + 16,
    grid_top = header_h + col_header_h,
    col_headers = col_headers,
    col_lines = col_lines,
    lines = lines,
  },
  refresh_rate = 3600,
}

if next(variants) then
  result.font_hinting = { variants = variants }
end

return result
