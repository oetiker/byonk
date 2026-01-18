-- E-ink grayscale test pattern
-- Adapts to device dimensions and grey levels

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height
local grey_levels = layout.grey_levels

-- Layout calculations using scale_pixel for pixel alignment
local bar_width = math.floor(width / grey_levels)
local bar_height = math.floor(height * 0.58)  -- ~58% for grey bars
local gradient_y = bar_height
local gradient_height = scale_pixel(60)
local pattern_y = gradient_y + gradient_height
local pattern_height = scale_pixel(40)
local info_bar_y = height - scale_pixel(100)
local info_bar_height = height - info_bar_y

-- Generate grey level bars with labels using greys() helper
local palette = greys(grey_levels)
local bars = {}
local labels = {"WHITE", "LIGHT", "DARK", "BLACK"}
if grey_levels > 4 then
  labels = {}
  for i = 1, grey_levels do
    table.insert(labels, "L" .. (grey_levels - i))
  end
end

-- Reverse palette order for display (white on left, black on right)
for i = 1, grey_levels do
  local entry = palette[grey_levels - i + 1]  -- Reverse order
  local x = (i - 1) * bar_width
  local center_x = x + math.floor(bar_width / 2)
  local label = labels[i] or ("L" .. (grey_levels - i))

  table.insert(bars, {
    x = x,
    width = bar_width,
    color = entry.color,
    value = entry.value,
    text_color = entry.text_color,
    center_x = center_x,
    label = label,
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

-- Generate step wedge using greys() helper
local step_width = math.floor((width - scale_pixel(220)) / grey_levels)
local steps = {}
for i, entry in ipairs(palette) do
  table.insert(steps, {
    x = scale_pixel(220) + (i - 1) * step_width,
    width = step_width,
    color = entry.color,
  })
end

return {
  data = {
    width = width,
    height = height,
    scale = layout.scale,
    grey_levels = grey_levels,
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
    vgradient_width = scale_pixel(60),
    vgradient_height = info_bar_y - pattern_y,
    info_bar_y = info_bar_y,
    info_bar_height = info_bar_height,
    center_x = layout.center_x,
    -- Text positions
    label_y = math.floor(bar_height * 0.5),
    value_y = math.floor(bar_height * 0.5) + scale_pixel(25),
    circle_y = math.floor(bar_height * 0.78),
    circle_r = scale_pixel(40),
    gradient_text_y = gradient_y + scale_pixel(40),
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
