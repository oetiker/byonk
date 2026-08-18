-- Default screen script
-- TV test pattern with device info

local now = time_now()
local time_str = time_format(now, "%H:%M")
local date_str = time_format(now, "%Y-%m-%d")

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height
local colors = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local color_count = #colors
local grey_count = layout.grey_count or 4

-- Layout calculations (scaled)
local small_screen = (width < 400)
local info_bar_height = scale_pixel(small_screen and 64 or 32)
local info_bar_y = height - info_bar_height

local swatch_x = layout.margin
local swatch_width = scale_pixel(small_screen and 144 or 80)
local swatch_height = math.floor((info_bar_y - 2 * layout.margin) / (small_screen and 9 or 16))  -- fixed height per swatch
local swatch_total_height = swatch_height * color_count
local swatch_start_y = info_bar_y - layout.margin - swatch_total_height  -- stack from bottom

local gradient_x = width - swatch_width - layout.margin
local gradient_width = swatch_width

-- Font sizes (scaled) - use scale_font for precision
local font_sm = scale_font(15)
local font_md = scale_font(17)
local font_lg = scale_font(small_screen and 36 or 18)
local font_hero = small_screen and scale_font(192) or scale_font(96)
local font_tagline = small_screen and scale_font(80) or scale_font(40)
local font_reg_code = scale_font(62)
local font_swatch = scale_font(small_screen and 20 or 14)

-- Registration box dimensions (centered in lower portion of screen)
local reg_box_width = scale_pixel(520)
local reg_box_height = scale_pixel(104)

-- Generate color swatches from the display palette
local swatches = {}
for i, color in ipairs(colors) do
  -- Reverse order: lightest at top, darkest at bottom
  local reversed_idx = color_count - i
  local y = swatch_start_y + (reversed_idx * swatch_height)
  -- Pick contrasting text color based on luminance of the swatch color
  local hex = color:gsub("#", "")
  local cr = tonumber(hex:sub(1, 2), 16) or 0
  local cg = tonumber(hex:sub(3, 4), 16) or 0
  local cb = tonumber(hex:sub(5, 6), 16) or 0
  local lum = 0.2126 * cr + 0.7152 * cg + 0.0722 * cb
  local text_color = (lum < 128) and "#FFFFFF" or "#000000"
  table.insert(swatches, {
    color = color,
    text_color = text_color,
    label = color,
    y = y,
    height = swatch_height,
    text_y = y + math.floor(swatch_height * 0.7),
  })
end

-- Calculate title/tagline positions
local title_y = scale_pixel(height * (small_screen and 0.38 or 0.27) / layout.scale)
local tagline_y = title_y + (small_screen and scale_pixel(80) or scale_pixel(40))
-- Registration box positioned in lower center of screen (above info bar)
local reg_box_y = height - info_bar_height - reg_box_height - scale_pixel(40) - math.floor(reg_box_height / 2)

return {
  data = {
    time = time_str,
    date = date_str,
    swatches = swatches,
    color_count = color_count,
    grey_count = grey_count,
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
    gradient_height = swatch_total_height,
    info_bar_y = info_bar_y,
    info_bar_height = info_bar_height,
    info_text_y = info_bar_y + math.floor(info_bar_height * 0.66),
    -- Title positions
    title_y = title_y,
    tagline_y = tagline_y,
    center_x = layout.center_x,
    -- Registration box (centered below tagline)
    reg_box_x = layout.center_x - math.floor(reg_box_width / 2),
    reg_box_y = reg_box_y,
    reg_box_width = reg_box_width,
    reg_box_height = reg_box_height,
    reg_text_y = reg_box_y + math.floor(reg_box_height * 0.68),
    font_reg_code = font_reg_code,
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
  -- dither = "atkinson",
  dither = "floyd-steinberg",
  refresh_rate = 300  -- 5 minutes
}
