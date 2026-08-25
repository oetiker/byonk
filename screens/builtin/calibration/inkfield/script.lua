-- Ink Field — a Latin-square calibration target.
--
-- The problem this solves: a ramp that runs left to right is spatially
-- ordered, so any left-to-right change in the light falling on the panel is
-- indistinguishable from a change in the panel's tone curve. Measuring a real
-- panel that way produced a "measured" curve that was mostly a photograph of
-- the room.
--
-- Here every ink appears exactly once per row and once per column (a Latin
-- square), so averaging an ink's cells cancels any lighting field that is a
-- product of a row term and a column term -- which is what a lamp to one side
-- and a lens vignette both look like. It cancels by construction, not on
-- average.
--
-- Black and white are inks like any other, so they too land near every
-- position. That is the useful part: each test cell can be normalised against
-- black and white measured *next to it*, which is dark-frame plus flat-field
-- correction from one hand-held photograph.
--
-- Nothing here is grey-specific. It reads `layout.colors`, so a colour panel
-- gets its own inks laid out the same way, and the white cells give the
-- illuminant needed to measure the others.

local width = layout.width
local height = layout.height
local colors = layout.colors or {"#000000", "#555555", "#AAAAAA", "#FFFFFF"}
local n = #colors

local function gcd(a, b)
  while b ~= 0 do
    a, b = b, a % b
  end
  return a
end

-- Step between rows. With the obvious `(row + col) % n` an ink lands on one
-- straight diagonal, so black and white cluster along two lines and the
-- interpolation between them has to reach a long way in the corners. Any
-- multiplier coprime to n keeps the Latin property while walking each ink
-- across the columns instead, which spreads the references over the panel.
local stride = 1
for a = 2, n - 1 do
  if gcd(a, n) == 1 then
    stride = a
    break
  end
end

-- Cells near 120 px read cleanly in a photo and are big enough that the
-- middle of a cell is unaffected by its neighbours, which matters on e-ink
-- where a hard edge between black and white bleeds a little.
local tiles = math.floor(tonumber(params.tiles) or 0)
if tiles < 1 then
  tiles = math.max(1, math.floor(math.min(width, height) / (120 * n) + 0.5))
end
local offset = math.floor(tonumber(params.offset) or 0) % n

local cols = n * tiles
local rows = n * tiles

-- Integer pixel edges, shared between neighbours, so no cell is a fraction of
-- a pixel wide and no seam of background shows between two cells.
local xs, ys = {}, {}
for i = 0, cols do
  xs[i] = math.floor(width * i / cols + 0.5)
end
for i = 0, rows do
  ys[i] = math.floor(height * i / rows + 0.5)
end

local cells = {}
for r = 0, rows - 1 do
  for c = 0, cols - 1 do
    local ink = (stride * r + c + offset) % n
    cells[#cells + 1] = {
      x = xs[c],
      y = ys[r],
      width = xs[c + 1] - xs[c],
      height = ys[r + 1] - ys[r],
      color = colors[ink + 1],
      ink = ink,
    }
  end
end

log_info(string.format("inkfield: %d inks, %dx%d cells, stride=%d offset=%d, cell %dx%d px",
  n, cols, rows, stride, offset, xs[1] - xs[0], ys[1] - ys[0]))

return {
  data = {
    width = width,
    height = height,
    cells = cells,
    inks = n,
    cols = cols,
    rows = rows,
    stride = stride,
    offset = offset,
  },
  refresh_rate = 3600,
}
