-- Mandelbrot screen
-- Renders a random, beautiful region of the Mandelbrot set.
-- Computes a low-resolution escape-time grid in Lua, run-length encodes
-- horizontal runs of equal colour, and emits them as SVG <rect>s.

local now = time_now()
math.randomseed(now)

local width = layout.width
local height = layout.height
local aspect = height / width

-- Curated "beautiful" regions. Each has a center, an extent (half-width in
-- the complex plane) and a sensible iteration budget for that zoom.
local locations = {
  { name = "Seahorse Valley",   cx = -0.74364388703, cy = 0.13182590421,  extent = 0.0060, iter = 400 },
  { name = "Elephant Valley",   cx =  0.28693186889, cy = 0.01456333250,  extent = 0.0040, iter = 400 },
  { name = "Triple Spiral",     cx = -0.08807744150, cy = 0.65467275200,  extent = 0.0050, iter = 350 },
  { name = "Mini Mandelbrot",   cx = -1.76938317919, cy = 0.00423684791,  extent = 0.0020, iter = 500 },
  { name = "Feigenbaum Point",  cx = -1.40115518909, cy = 0.00000000000,  extent = 0.0050, iter = 500 },
  { name = "Starfish",          cx = -0.77568377000, cy = 0.13646737000,  extent = 0.0040, iter = 400 },
  { name = "Double Spiral",     cx = -0.74529000000, cy = 0.11307000000,  extent = 0.0060, iter = 400 },
  { name = "Julia Island",      cx = -1.76877883254, cy = 0.00173891149,  extent = 0.0015, iter = 600 },
  { name = "Scepter Valley",    cx = -1.36022687000, cy = 0.00500000000,  extent = 0.0080, iter = 350 },
  { name = "Main Cardioid",     cx = -0.50000000000, cy = 0.00000000000,  extent = 1.5000, iter = 150 },
  { name = "North Bulb",        cx = -0.15652017000, cy = 1.03224710000,  extent = 0.0080, iter = 400 },
  { name = "West Needle",       cx = -1.75000000000, cy = 0.00000000000,  extent = 0.0300, iter = 300 },
}

local loc = locations[math.random(#locations)]

-- Gentle jitter around the base point and a random zoom factor so every
-- refresh produces a different composition.
local jitter = (math.random() - 0.5) * loc.extent * 0.4
local jitter_y = (math.random() - 0.5) * loc.extent * 0.4
local zoom_factor = 0.6 + math.random() * 0.9  -- 0.6x .. 1.5x
local extent_x = loc.extent * zoom_factor
local extent_y = extent_x * aspect
local cx = loc.cx + jitter
local cy = loc.cy + jitter_y
local max_iter = loc.iter

-- Compute grid resolution: ~6 device pixels per cell, capped so the render
-- stays fast and the SVG stays small.
local W = math.min(320, math.floor(width / 6))
if W < 80 then W = 80 end
local H = math.max(1, math.floor(W * aspect))

-- Quantise to a fixed number of output levels so adjacent pixels often share
-- a colour (good RLE compression) while still looking smooth after dither.
local levels = 48
local level_max = levels - 1

local log = math.log
local sqrt = math.sqrt
local floor = math.floor
local log2 = log(2)
local log_log2 = log(log2)

local x_step = extent_x * 2 / W
local y_step = extent_y * 2 / H
local x_origin = cx - extent_x
local y_origin = cy + extent_y  -- top row → maximum cy (math orientation)

-- Build the escape-time gradient from the panel's own palette, sorted by
-- perceived luminance. This way a greyscale panel gets a smooth black→white
-- ramp, while a 4-colour panel (e.g. black/white/red/yellow) gets a warm
-- black→red→yellow→white flame ramp that the dither engine can match
-- cleanly to the actual panel colours.
local palette = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local stops = {}
for _, c in ipairs(palette) do
  local hex = c:gsub("#", "")
  local r = tonumber(hex:sub(1, 2), 16) or 0
  local g = tonumber(hex:sub(3, 4), 16) or 0
  local b = tonumber(hex:sub(5, 6), 16) or 0
  -- Rec. 709 luminance — monotonic in perceived brightness.
  local lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
  stops[#stops + 1] = { r = r, g = g, b = b, lum = lum }
end
table.sort(stops, function(a, b) return a.lum < b.lum end)
local stop_count = #stops
local last_seg = stop_count - 1

-- Pre-compute the hex colour for each gradient level by linear-interpolating
-- between adjacent palette stops. RGB lerp is fine here because the dither
-- engine does the final perceptual matching to the actual panel colours.
local level_colors = {}
for i = 0, level_max do
  local t = i / level_max
  local seg = t * last_seg
  local idx = floor(seg)
  if idx >= last_seg then
    idx = last_seg - 1
    if idx < 0 then idx = 0 end
  end
  local frac = seg - idx
  local a = stops[idx + 1]
  local b = stops[idx + 2] or a
  local r = floor(a.r + (b.r - a.r) * frac + 0.5)
  local g = floor(a.g + (b.g - a.g) * frac + 0.5)
  local bb = floor(a.b + (b.b - a.b) * frac + 0.5)
  level_colors[i] = string.format("#%02x%02x%02x", r, g, bb)
end

-- Interior of the set uses the panel's darkest colour (almost always black).
local darkest = stops[1]
local black_color = string.format("#%02x%02x%02x", darkest.r, darkest.g, darkest.b)

-- Compute the fractal and run-length encode each row on the fly.
local pieces = {}
local pixel_w = width / W
local pixel_h = height / H
-- Overlap by half a pixel so adjacent rects have no visible seams.
local rect_w_pad = pixel_w * 0.5
local rect_h_pad = pixel_h * 0.5

local fmt = string.format

for py = 0, H - 1 do
  local y0 = y_origin - py * y_step
  local row_y = py * pixel_h
  local rect_h = pixel_h + rect_h_pad

  local run_start = 0
  local run_color  -- set below on first pixel

  for px = 0, W - 1 do
    local x0 = x_origin + px * x_step
    local x, y = 0.0, 0.0
    local x2, y2 = 0.0, 0.0
    local iter = 0
    while x2 + y2 <= 4.0 and iter < max_iter do
      y = 2.0 * x * y + y0
      x = x2 - y2 + x0
      x2 = x * x
      y2 = y * y
      iter = iter + 1
    end

    local color
    if iter >= max_iter then
      color = black_color
    else
      -- Smooth iteration count: nu = log2(log2(|z|)) with |z|^2 = x2 + y2
      local mag = x2 + y2
      local nu = (log(0.5 * log(mag)) - log_log2) / log2
      local smooth = iter + 1 - nu
      if smooth < 0 then smooth = 0 end
      -- sqrt-ramp: stretches detail near the boundary, pushes the
      -- far exterior to bright greys. Interior is already black.
      local t = sqrt(smooth / max_iter)
      if t > 1.0 then t = 1.0 end
      local lvl = floor(t * level_max + 0.5)
      color = level_colors[lvl]
    end

    if px == 0 then
      run_color = color
    elseif color ~= run_color then
      local rx = run_start * pixel_w
      local rw = (px - run_start) * pixel_w + rect_w_pad
      pieces[#pieces + 1] = fmt(
        '<rect x="%.2f" y="%.2f" width="%.2f" height="%.2f" fill="%s"/>',
        rx, row_y, rw, rect_h, run_color
      )
      run_start = px
      run_color = color
    end
  end

  -- Close the final run on this row.
  local rx = run_start * pixel_w
  local rw = (W - run_start) * pixel_w + rect_w_pad
  pieces[#pieces + 1] = fmt(
    '<rect x="%.2f" y="%.2f" width="%.2f" height="%.2f" fill="%s"/>',
    rx, row_y, rw, rect_h, run_color
  )
end

local svg_pixels = table.concat(pieces)

-- Caption with the location name and a zoom indicator. Kept tiny so it
-- does not dominate the image.
local zoom_level = 1.0 / extent_x
local caption = string.format("%s  -  zoom %.0fx", loc.name, zoom_level)
local coords = string.format("c = %.6f %+0.6fi", cx, cy)

return {
  data = {
    width = width,
    height = height,
    svg_pixels = svg_pixels,
    caption = caption,
    coords = coords,
    pad = scale_pixel(6),
    font_caption = scale_font(12),
    font_coords = scale_font(10),
    caption_y = height - scale_pixel(18),
    coords_y = height - scale_pixel(6),
  },
  -- Floyd-Steinberg gives a soft, painterly feel on smooth gradients.
  dither = "floyd-steinberg",
  refresh_rate = 600,  -- 10 minutes
}
