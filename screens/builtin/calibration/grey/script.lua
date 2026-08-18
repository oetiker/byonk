-- E-ink color test pattern
-- Adapts to device dimensions and display palette

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height
local colors = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local color_count = #colors

-- Swatch grid.
--
-- One row per palette entry is right while a label still fits its column.
-- Past that it does not: a 16-level panel gives each `#RRGGBB` label a
-- sixteenth of the width, roughly a third of what the text needs, and the
-- labels run together into an unreadable smear. Wrapping into a squarish
-- grid gives each label four times the width at 16 levels, and leaves the
-- 4- and 6-entry palettes on the single row they already fit.
local grid_cols = color_count
local grid_rows = 1
if color_count > 8 then
  grid_cols = math.ceil(math.sqrt(color_count))
  grid_rows = math.ceil(color_count / grid_cols)
end

-- Layout calculations using scale_pixel for pixel alignment. Everything
-- below the swatches has a fixed height, so measure that first and give the
-- swatches whatever is left.
local gradient_height = scale_pixel(40)
local pattern_height = scale_pixel(40)
local info_bar_y = height - scale_pixel(100)
local info_bar_height = height - info_bar_y

-- 38% of the panel is one row of swatches, deep enough for a label and a
-- registration mark. A grid needs that depth *per row*, so it claims more —
-- which also fills the dead band a 16-level panel used to leave between the
-- test patterns and the info bar. The cap is what the patterns and the info
-- bar need, so the swatches can never grow over them.
local max_bar_height = info_bar_y - gradient_height - pattern_height
local bar_height = math.min(
  math.floor(height * (grid_rows > 1 and 0.30 + 0.16 * grid_rows or 0.38)),
  max_bar_height
)
local gradient_y = bar_height
local pattern_y = gradient_y + gradient_height

local cell_w = math.floor(width / grid_cols)
local cell_h = math.floor(bar_height / grid_rows)

-- Type and marks are placed from the cell, not from the panel: in a grid the
-- cell is a fraction of the height the single-row layout took for granted.
local font_label = scale_font(20)
local font_value = scale_font(16)
local pad = scale_pixel(8)

-- The second text line only exists for the named 4-grey palette, where the
-- label is a word and the hex below it adds something. Every other palette
-- labels each swatch with its own hex, so a second line would repeat it.
local has_value = (color_count == 4 and not layout.colors)

-- Label baseline. The proportional placement alone clipped the first grid
-- row against the top of the screen, because 30% of a grid cell is less than
-- one cap height; the max() is the floor that stops that.
local label_offset = math.max(font_label + pad, math.floor(cell_h * 0.30))
local value_offset = label_offset + font_value + math.floor(pad / 2)
local text_bottom = (has_value and value_offset or label_offset)
  + math.floor(font_label * 0.25) -- descender room

-- The registration mark is centred in whatever the cell has left below the
-- text, and sized to fit it. A flat scale_pixel(30) radius was wider than a
-- 16-level cell, so the circles overlapped each other and the leftmost one
-- hung off the edge of the screen.
local mark_top = text_bottom + pad
local mark_space = cell_h - mark_top - pad
local circle_r = math.max(
  scale_pixel(4),
  math.min(scale_pixel(30), math.floor(cell_w * 0.24), math.floor(mark_space / 2))
)
local circle_offset = mark_top + math.floor(mark_space / 2)

-- Generate color bars from display palette
local bars = {}
local labels
if has_value then
  labels = {"WHITE", "LIGHT", "DARK", "BLACK"}
else
  labels = {}
  for i = 1, color_count do
    labels[i] = colors[color_count - i + 1]
  end
end

-- Reverse palette order for display (lightest first, darkest last)
for i = 1, color_count do
  local color = colors[color_count - i + 1]  -- Reverse order
  local col = (i - 1) % grid_cols
  local row = math.floor((i - 1) / grid_cols)
  local x = col * cell_w
  local y = row * cell_h
  local label = labels[i] or color
  -- Pick contrasting text color based on luminance
  local hex = color:gsub("#", "")
  local cr = tonumber(hex:sub(1, 2), 16) or 0
  local cg = tonumber(hex:sub(3, 4), 16) or 0
  local cb = tonumber(hex:sub(5, 6), 16) or 0
  local lum = 0.2126 * cr + 0.7152 * cg + 0.0722 * cb
  local text_color = (lum < 128) and "#FFFFFF" or "#000000"

  table.insert(bars, {
    x = x,
    y = y,
    width = cell_w,
    height = cell_h,
    color = color,
    text_color = text_color,
    center_x = x + math.floor(cell_w / 2),
    label = label,
    value = color,
    -- A second line is only worth its space when it says something the label
    -- does not. For any palette but the named 4-grey one the label already
    -- *is* the hex value, and printing it twice is what put two identical
    -- rows of text across the 16-level render.
    show_value = has_value,
    label_y = y + label_offset,
    value_y = y + value_offset,
    circle_y = y + circle_offset,
  })
end

-- Generate resolution test bars (alternating black/white)
local res_bars = {}
local widths = {10, 10, 10, 10, 10, 10, 10, 10, 20, 20, 20, 20}
local res_x = 0
for i, w in ipairs(widths) do
  local scaled_w = scale_pixel(w)
  table.insert(res_bars, {
    x = res_x,
    width = scaled_w,
    color = (i % 2 == 1) and "#000000" or "#ffffff",
  })
  res_x = res_x + scaled_w
end

-- Generate step wedge from display palette
local step_width = math.floor((width - scale_pixel(220)) / color_count)
local steps = {}
for i, color in ipairs(colors) do
  table.insert(steps, {
    x = scale_pixel(220) + (i - 1) * step_width,
    width = step_width,
    color = color,
  })
end

return {
  data = {
    width = width,
    height = height,
    scale = layout.scale,
    color_count = color_count,
    bars = bars,
    res_bars = res_bars,
    steps = steps,
    -- Layout positions
    bar_height = bar_height,
    gradient_y = gradient_y,
    gradient_height = gradient_height,
    pattern_y = pattern_y,
    pattern_height = pattern_height,
    vgradient_x = scale_pixel(160),
    vgradient_width = scale_pixel(40),
    vgradient_height = info_bar_y - pattern_y,
    info_bar_y = info_bar_y,
    info_bar_height = info_bar_height,
    center_x = layout.center_x,
    -- Text positions. The per-swatch ones (label_y, value_y, circle_y) live
    -- on each bar now, because they depend on which grid row it landed in.
    circle_r = circle_r,
    gradient_text_y = gradient_y + scale_pixel(26),
    title_y = info_bar_y + scale_pixel(40),
    subtitle_y = info_bar_y + scale_pixel(75),
    -- Font sizes - use scale_font for precision
    font_label = font_label,
    font_value = font_value,
    font_gradient = scale_font(14),
    font_title = scale_font(28),
    font_subtitle = scale_font(16),
    -- Corner marker size
    corner_size = scale_pixel(40),
  },
  refresh_rate = 3600
}
