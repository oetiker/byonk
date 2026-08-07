-- Gamut patch grid.
--
-- Companion to the `calibration/color` screen. That one draws the hue sweep
-- as a smooth gradient, which is the right shape for judging banding but the
-- wrong shape for this question: neighbouring hues bleed into each other, so
-- a hue the panel cannot mix still looks like it is doing something.
--
-- Here every hue is an isolated flat patch instead. A patch that comes back
-- as one solid colour is a hue where the ditherer gave up and picked a single
-- palette entry for every pixel; a patch that comes back speckled is one it
-- actually mixed. Rows vary lightness, because whether a hue is reachable
-- depends strongly on how light it is -- the 6-colour panels have a dark blue
-- and a dark green, so bright blues and greens fall outside what they can mix.
--
-- Params:
--   hues   number of hue columns around the full circle (default 24)
--   levels number of lightness rows (default 6)

local width = layout.width
local height = layout.height

local hues = tonumber(params.hues or 24)
if hues < 2 then hues = 2 end
if hues > 48 then hues = 48 end

local levels = tonumber(params.levels or 6)
if levels < 1 then levels = 1 end
if levels > 12 then levels = 12 end

local grid = scale_pixel(2)
local font_size = scale_pixel(10)
-- Room for the hue labels along the top and the lightness labels down the left.
local label_h = font_size + scale_pixel(3)
local label_w = scale_pixel(22)

-- HSL -> sRGB. Matches the convention used by the color calibrator so the two
-- screens address the same hue by the same number.
local function hsl_to_rgb(h, s, l)
  local c = (1 - math.abs(2 * l - 1)) * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = l - c / 2
  local r, g, b
  if     h < 60  then r, g, b = c, x, 0
  elseif h < 120 then r, g, b = x, c, 0
  elseif h < 180 then r, g, b = 0, c, x
  elseif h < 240 then r, g, b = 0, x, c
  elseif h < 300 then r, g, b = x, 0, c
  else                r, g, b = c, 0, x
  end
  return math.floor((r + m) * 255 + 0.5),
         math.floor((g + m) * 255 + 0.5),
         math.floor((b + m) * 255 + 0.5)
end

-- Patch geometry. Remainders are spread one pixel at a time across the leading
-- cells rather than left at the edge, so the grid stays flush with the frame.
local avail_w = width - label_w - (hues + 1) * grid
local cell_w = math.floor(avail_w / hues)
local w_rem = avail_w - cell_w * hues

local avail_h = height - label_h - (levels + 1) * grid
local cell_h = math.floor(avail_h / levels)
local h_rem = avail_h - cell_h * levels

local patches = {}
local hue_labels = {}
local level_labels = {}

local py = label_h + grid
for row = 1, levels do
  local ch = cell_h + ((row <= h_rem) and 1 or 0)
  -- Spread lightness over the usable middle of the range: pure 0 and 1 are
  -- black and white for every hue and would waste two rows saying so.
  local l = 0.2 + 0.6 * ((row - 1) / math.max(levels - 1, 1))
  if levels == 1 then l = 0.5 end

  table.insert(level_labels, {
    y = py + math.floor(ch / 2) + math.floor(font_size / 3),
    text = string.format("%d", math.floor(l * 100 + 0.5)),
  })

  local px = label_w + grid
  for col = 1, hues do
    local cw = cell_w + ((col <= w_rem) and 1 or 0)
    local hue = (col - 1) * 360 / hues
    local r, g, b = hsl_to_rgb(hue, 1.0, l)

    table.insert(patches, {
      x = px,
      y = py,
      width = cw,
      height = ch,
      color = string.format("rgb(%d,%d,%d)", r, g, b),
    })

    if row == 1 then
      table.insert(hue_labels, {
        x = px + math.floor(cw / 2),
        text = string.format("%d", math.floor(hue + 0.5)),
      })
    end

    px = px + cw + grid
  end
  py = py + ch + grid
end

return {
  data = {
    width = width,
    height = height,
    patches = patches,
    hue_labels = hue_labels,
    level_labels = level_labels,
    label_y = font_size,
    label_w = label_w,
    font_size = font_size,
  },
  refresh_rate = 3600,
  -- Exact-match passthrough forces any pixel that equals an official palette
  -- colour to that entry and throws its error away. Four of the hues on this
  -- grid land exactly on a primary, so with it on they would be flat by
  -- construction and tell us nothing about whether mixing works. Off here so
  -- every patch is dithered on equal terms.
  preserve_exact = false,
}
