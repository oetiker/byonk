-- Calibrator screen
-- Shows white-to-color gradients for each palette color, a hue sweep, and solid patches

local width = layout.width
local height = layout.height
local colors = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local color_count = #colors

-- For the solid patches, draw the panel's OFFICIAL/nominal colour.
-- The patches exist to show what each ink actually looks like, and on the
-- unmapped path a nominal colour IS that ink: unmarked content is matched
-- against the official palette, so #00FF00 matches official green at distance
-- zero, pins, and comes out as the green ink and nothing else.
--
-- Drawing the MEASURED colour instead -- which this screen used to do, back
-- when unmarked content was matched against the measured palette -- now makes
-- the patches dither. #B50303 is not an official entry, so it cannot match
-- exactly, pinning never fires, and the patch breaks up (measured at HEAD:
-- the blue patch came back 48% black / 38% blue / 11% green). Correct for the
-- palette it was matched against, but useless for judging the ink.
local ink = colors

-- Uniform grid line width
local grid = scale_pixel(4)

-- Filter out white from gradient list
local grad_colors = {}
for i = 1, color_count do
  local hex = colors[i]:gsub("#", ""):upper()
  if hex ~= "FFFFFF" then
    table.insert(grad_colors, colors[i])
  end
end
local grad_count = #grad_colors
local bar_count = grad_count + 1  -- palette gradients + rainbow

-- Unified vertical layout (black background = grid):
-- grid | bar | grid | bar | ... | bar | grid | patches | grid
-- Total grid lines = bar_count + 2
local total_grids = bar_count + 2
local nominal_patch = math.floor(height * 0.25)
local available = height - total_grids * grid - nominal_patch
local bar_h = math.floor(available / bar_count)
-- Absorb remainder into patch area to keep all bars equal height
local patch_inner = nominal_patch + (available - bar_count * bar_h)

-- Build gradient for each non-white palette color (white -> color)
local gradients = {}
for i, color in ipairs(grad_colors) do
  local hex = color:gsub("#", "")
  local r = tonumber(hex:sub(1, 2), 16) or 0
  local g = tonumber(hex:sub(3, 4), 16) or 0
  local b = tonumber(hex:sub(5, 6), 16) or 0

  local gy = grid + (i - 1) * (bar_h + grid)
  table.insert(gradients, {
    id = "grad_" .. i,
    name = color,
    y = gy,
    label_x = grid + 2,
    label_y = gy + bar_h - 3,
    r2 = r,
    g2 = g,
    b2 = b,
  })
end

-- Hue sweep (smooth walk through all hues at full saturation, 50% lightness)
local hue_stops = {}
local hue_step = 5
for h = 0, 360, hue_step do
  local s, l = 1.0, 0.5
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r1, g1, b1
  if     h < 60  then r1, g1, b1 = c, x, 0
  elseif h < 120 then r1, g1, b1 = x, c, 0
  elseif h < 180 then r1, g1, b1 = 0, c, x
  elseif h < 240 then r1, g1, b1 = 0, x, c
  elseif h < 300 then r1, g1, b1 = x, 0, c
  else                r1, g1, b1 = c, 0, x
  end
  table.insert(hue_stops, {
    offset = string.format("%.1f%%", h / 360 * 100),
    color = string.format("rgb(%d,%d,%d)",
      math.floor((r1 + m) * 255 + 0.5),
      math.floor((g1 + m) * 255 + 0.5),
      math.floor((b1 + m) * 255 + 0.5)),
  })
end

-- Hue sweep bar (last slot)
local rainbow_y = grid + grad_count * (bar_h + grid)

-- Patch area starts after all bars + grid
local patch_y = grid + bar_count * (bar_h + grid)

-- Horizontal layout for patches:
-- grid | patch | grid | patch | ... | patch | grid
local avail_w = width - (color_count + 1) * grid
local patch_w = math.floor(avail_w / color_count)
local w_remainder = avail_w - color_count * patch_w
local patches = {}
local px = grid
for i = 1, color_count do
  local color = ink[i]
  local w = patch_w + ((i <= w_remainder) and 1 or 0)
  local center_x = px + math.floor(w / 2)

  -- Pick contrasting text color from the MEASURED ink, not the nominal fill.
  -- The patch is filled nominal so it pins, but it RENDERS as the measured
  -- ink, and that is what the label has to be legible against. Judging
  -- contrast on the fill would put black text on nominal #00FF00 (bright,
  -- lum 182) and then paint it onto the green ink (#0D876B, lum 107).
  local shown = (device and device.colors_actual and device.colors_actual[i]) or color
  local hex = shown:gsub("#", "")
  local cr = tonumber(hex:sub(1, 2), 16) or 0
  local cg = tonumber(hex:sub(3, 4), 16) or 0
  local cb = tonumber(hex:sub(5, 6), 16) or 0
  local lum = 0.2126 * cr + 0.7152 * cg + 0.0722 * cb
  local text_color = (lum < 128) and "#FFFFFF" or "#000000"

  table.insert(patches, {
    x = px,
    width = w,
    color = color,
    text_color = text_color,
    label_x = px + 2,
    -- Label with the OFFICIAL colour: that is the value a screen author
    -- writes in their SVG, which is what they need to look up.
    label = colors[i],
  })
  px = px + w + grid
end

-- Image area: right half of the gradient zone
local bar_w = math.floor((width - 2 * grid) / 2)
local img_x = grid + bar_w + grid
local img_y = grid
local img_w = width - img_x - grid
local img_h = rainbow_y + bar_h - grid

return {
  data = {
    width = width,
    height = height,
    grid = grid,
    gradients = gradients,
    bar_h = bar_h,
    bar_w = bar_w,
    rainbow_y = rainbow_y,
    hue_stops = hue_stops,
    patches = patches,
    patch_y = patch_y,
    patch_inner = patch_inner,
    patch_label_y = patch_y + patch_inner - 3,
    font_size = 10,
    img_x = img_x,
    img_y = img_y,
    img_w = img_w,
    img_h = img_h,
  },
  refresh_rate = 3600,
  dither = "atkinson",
}
