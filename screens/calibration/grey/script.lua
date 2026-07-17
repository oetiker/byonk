-- E-ink color test pattern
-- Adapts to device dimensions and display palette

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height
local colors = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local color_count = #colors

-- Layout calculations using scale_pixel for pixel alignment
local bar_width = math.floor(width / color_count)
local bar_height = math.floor(height * 0.38)  -- ~38% for color bars
local gradient_y = bar_height
local gradient_height = scale_pixel(40)
local pattern_y = gradient_y + gradient_height
local pattern_height = scale_pixel(40)
local info_bar_y = height - scale_pixel(100)
local info_bar_height = height - info_bar_y

-- Generate color bars from display palette
local bars = {}
local labels
if color_count == 4 and not layout.colors then
  labels = {"WHITE", "LIGHT", "DARK", "BLACK"}
else
  labels = {}
  for i = 1, color_count do
    labels[i] = colors[color_count - i + 1]
  end
end

-- Reverse palette order for display (lightest on left, darkest on right)
for i = 1, color_count do
  local color = colors[color_count - i + 1]  -- Reverse order
  local x = (i - 1) * bar_width
  local center_x = x + math.floor(bar_width / 2)
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
    width = bar_width,
    color = color,
    text_color = text_color,
    center_x = center_x,
    label = label,
    value = color,
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
    -- Text positions
    label_y = math.floor(bar_height * 0.35),
    value_y = math.floor(bar_height * 0.35) + scale_pixel(20),
    circle_y = math.floor(bar_height * 0.72),
    circle_r = scale_pixel(30),
    gradient_text_y = gradient_y + scale_pixel(26),
    title_y = info_bar_y + scale_pixel(40),
    subtitle_y = info_bar_y + scale_pixel(75),
    -- Font sizes - use scale_font for precision
    font_label = scale_font(20),
    font_value = scale_font(16),
    font_gradient = scale_font(14),
    font_title = scale_font(28),
    font_subtitle = scale_font(16),
    -- Corner marker size
    corner_size = scale_pixel(40),
  },
  refresh_rate = 3600
}
