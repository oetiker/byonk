-- Tone marker A/B.
--
-- The same three bands twice: the left column unmarked, the right column
-- wrapped in `data-byonk-tone="continuous"`.
--
-- THIS IS NOT A GAMUT-MAPPER CONTROL, and the labels must not say it is.
-- One mask drives THREE things at once (ruling 22), so the two columns differ
-- in all of them: the left is matched against the OFFICIAL palette, is not
-- gamut mapped, and IS exact-match pinned; the right is matched against the
-- MEASURED inks, is mapped, and is not pinned. Nothing here isolates the
-- mapper, and the gap the eye sees is dominated by the colour model rather
-- than by mapping -- measured at roughly 14x the mapper's own contribution
-- (Task 8: block-averaged dE 0.107 vs 0.008). Labelling the left column
-- "UNMAPPED (control)" overstated the mapper by about that factor.
--
-- What the screen DOES show, honestly, is what byonk does to marked versus
-- unmarked content -- which is exactly what an author has to understand in
-- order to mark their own screens correctly.
--
-- Everything else about the two columns is identical. The mask is frame-level
-- -- there is exactly one adaptation group -- so the adaptation factor R is
-- derived from the marked pixels alone. The marked column therefore adapts to
-- precisely the content it displays.
--
-- The bands answer different questions: the photograph shows the everyday
-- benefit on real content, the hue sweep shows banding and tail separation
-- across a controlled gradient, and the patch grid shows ink survival and
-- which hues collapse onto a single palette entry.
--
-- Params:
--   hues   hue columns in the patch grid (default 12)
--   levels lightness rows in the patch grid (default 5)

local width = layout.width
local height = layout.height

local hues = tonumber(params.hues or 12)
if hues < 2 then hues = 2 end
if hues > 48 then hues = 48 end

local levels = tonumber(params.levels or 5)
if levels < 1 then levels = 1 end
if levels > 12 then levels = 12 end

local grid = scale_pixel(2)
local margin = scale_pixel(4)
local gutter = scale_pixel(6)
local band_gap = scale_pixel(4)
local font_size = scale_pixel(10)
local header_h = font_size + scale_pixel(4)

-- HSL -> sRGB. Same convention as the color and gamut calibrators, so a hue
-- number means the same thing on all three screens.
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

-- Two equal columns either side of a neutral gutter.
local col_w = math.floor((width - 2 * margin - gutter) / 2)

-- Band heights. The sweep and patch bands are fixed; the photo takes whatever
-- is left, so a taller panel grows the photograph rather than overflowing.
local body_y = margin + header_h
local body_h = height - body_y - margin
local sweep_h = scale_pixel(55)
local patch_h = scale_pixel(205)
local photo_h = body_h - sweep_h - patch_h - 2 * band_gap

-- On a short panel the fixed bands can leave nothing for the photo. Give the
-- photo a floor and take the difference out of the patch grid, which degrades
-- gracefully; a zero-height photo does not.
local photo_min = scale_pixel(60)
if photo_h < photo_min then
  patch_h = patch_h - (photo_min - photo_h)
  photo_h = photo_min
  if patch_h < scale_pixel(30) then patch_h = scale_pixel(30) end
end

-- Patch cell geometry, shared by both columns. Remainders are spread one pixel
-- at a time across the leading cells so the grid stays flush with the column.
local avail_w = col_w - (hues + 1) * grid
local cell_w = math.floor(avail_w / hues)
local w_rem = avail_w - cell_w * hues

local avail_h = patch_h - (levels + 1) * grid
local cell_h = math.floor(avail_h / levels)
local h_rem = avail_h - cell_h * levels

-- Build one column's absolute geometry at origin `x`.
local function column(x, label)
  local photo_y = body_y
  local sweep_y = photo_y + photo_h + band_gap
  local patch_y = sweep_y + sweep_h + band_gap

  local patches = {}
  local py = patch_y + grid
  for row = 1, levels do
    local ch = cell_h + ((row <= h_rem) and 1 or 0)
    -- Spread lightness over the usable middle: pure 0 and 1 are black and
    -- white at every hue and would waste two rows saying so.
    local l = 0.2 + 0.6 * ((row - 1) / math.max(levels - 1, 1))
    if levels == 1 then l = 0.5 end

    local px = x + grid
    for col = 1, hues do
      local cw = cell_w + ((col <= w_rem) and 1 or 0)
      local hue = (col - 1) * 360 / hues
      local r, g, b = hsl_to_rgb(hue, 1.0, l)
      table.insert(patches, {
        x = px, y = py, width = cw, height = ch,
        color = string.format("rgb(%d,%d,%d)", r, g, b),
      })
      px = px + cw + grid
    end
    py = py + ch + grid
  end

  return {
    label = label,
    label_x = x,
    photo = { x = x, y = photo_y, width = col_w, height = photo_h },
    sweep = { x = x, y = sweep_y, width = col_w, height = sweep_h },
    patch_bg = { x = x, y = patch_y, width = col_w, height = patch_h },
    patches = patches,
  }
end

-- Hue sweep gradient stops, shared by both columns.
local hue_stops = {}
for i = 0, 12 do
  local hue = i * 360 / 12
  local r, g, b = hsl_to_rgb(hue % 360, 1.0, 0.5)
  table.insert(hue_stops, {
    offset = string.format("%.4f", i / 12),
    color = string.format("rgb(%d,%d,%d)", r, g, b),
  })
end

return {
  data = {
    width = width,
    height = height,
    font_size = font_size,
    label_y = margin + font_size,
    hue_stops = hue_stops,
    left = column(margin, "UNMARKED - nominal, pinned"),
    right = column(margin + col_w + gutter, "MARKED - measured, mapped"),
  },
  refresh_rate = 3600,
}
