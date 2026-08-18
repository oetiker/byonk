-- Terminus TTF font showcase
-- Shows all embedded bitmap sizes in all 4 styles

local width = layout.width
local height = layout.height
local margin = 6
local header_height = 20

local family = "Terminus (TTF)"

local sizes = fonts[family][1].bitmap_strikes
local styles = { {}, {bold=true}, {italic=true}, {bold=true, italic=true} }

local col_gap = 8
local split_size = 20
local col_split = math.floor(width * 2 / 5)

-- Build all entries
local entries = {}
for _, px in ipairs(sizes) do
  for _, style in ipairs(styles) do
    local parts = {}
    if style.bold then table.insert(parts, "bold") end
    if style.italic then table.insert(parts, "oblique") end
    table.insert(parts, px .. "px")
    table.insert(entries, {
      size = px,
      family = family,
      label = table.concat(parts, " "),
      bold = style.bold or false,
      italic = style.italic or false,
    })
  end
end

-- Check if we need two columns
local has_small, has_large = false, false
for _, e in ipairs(entries) do
  if e.size < split_size then has_small = true else has_large = true end
end

local use_two_cols = has_small and has_large
local col_x1 = margin
local col_x2 = col_split + col_gap

-- Place entries into lines list with column x coordinate
local lines = {}
local y = { header_height + 4, header_height + 4 }

for _, e in ipairs(entries) do
  local col
  if use_two_cols then
    col = e.size < split_size and 1 or 2
  else
    col = 1
  end
  local x = (col == 1) and col_x1 or col_x2
  if y[col] + e.size + 1 < height - 2 then
    table.insert(lines, {
      x = x,
      y = y[col] + e.size,
      size = e.size,
      family = e.family,
      label = e.label,
      bold = e.bold,
      italic = e.italic,
    })
    y[col] = y[col] + e.size + 1
  end
end

return {
  data = {
    width = width,
    height = height,
    lines = lines,
    margin = margin,
    header_height = header_height,
    title = "Terminus (TTF)",
    grey_count = layout.grey_count or 4,
  },
  refresh_rate = 3600,
}
