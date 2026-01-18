-- Default screen script
-- TV test pattern with device info

local now = time_now()
local time_str = time_format(now, "%H:%M")
local date_str = time_format(now, "%Y-%m-%d")

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height
local grey_levels = layout.grey_levels

-- Layout calculations (scaled)
local swatch_x = layout.margin
local swatch_width = layout.margin_lg
local swatch_area_height = height * 0.5  -- 50% of screen height for swatches
local swatch_height = math.floor(swatch_area_height / grey_levels)
local swatch_start_y = scale_pixel(height * 0.375 / layout.scale)  -- Start at 37.5% down

local gradient_x = width - scale_pixel(60)
local gradient_width = layout.margin_lg

local info_bar_height = scale_pixel(32)
local info_bar_y = height - info_bar_height

-- Font sizes (scaled) - use scale_font for precision
local font_sm = scale_font(15)
local font_md = scale_font(17)
local font_lg = scale_font(18)
local font_hero = scale_font(96)
local font_tagline = scale_font(39.8)
-- Swatch font: proportional to swatch height, capped for readability
local font_swatch = math.min(math.floor(swatch_height * 0.5), scale_pixel(12))

-- Generate grey level swatches with positions using greys() helper
local palette = greys(grey_levels)
local grey_swatches = {}
for i, entry in ipairs(palette) do
  -- Reverse order: white at top, black at bottom
  local reversed_idx = grey_levels - i
  local y = swatch_start_y + (reversed_idx * swatch_height)
  table.insert(grey_swatches, {
    value = entry.value,
    color = entry.color,
    text_color = entry.text_color,
    y = y,
    height = swatch_height,
    text_y = y + math.floor(swatch_height * 0.7),
  })
end

return {
  data = {
    time = time_str,
    date = date_str,
    greys = grey_swatches,
    grey_count = grey_levels,
    -- Layout
    width = width,
    height = height,
    scale = layout.scale,
    swatch_x = swatch_x,
    swatch_width = swatch_width,
    swatch_height = swatch_height,
    swatch_center_x = swatch_x + math.floor(swatch_width / 2),
    gradient_x = gradient_x,
    gradient_width = gradient_width,
    gradient_y = swatch_start_y,
    gradient_height = math.floor(swatch_area_height),
    info_bar_y = info_bar_y,
    info_bar_height = info_bar_height,
    info_text_y = info_bar_y + math.floor(info_bar_height * 0.66),
    -- Title positions
    title_y = scale_pixel(height * 0.27 / layout.scale),
    tagline_y = scale_pixel(height * 0.27 / layout.scale) + scale_pixel(40),
    center_x = layout.center_x,
    -- Info bar text positions
    info_battery_x = math.floor(width / 12),
    info_mac_x = math.floor(width / 3.3),
    info_time_x = math.floor(width / 1.6),
    info_rssi_x = math.floor(width / 1.1),
    -- Font sizes
    font_sm = font_sm,
    font_md = font_md,
    font_lg = font_lg,
    font_hero = font_hero,
    font_tagline = font_tagline,
    font_swatch = font_swatch,
  },
  refresh_rate = 300  -- 5 minutes
}
