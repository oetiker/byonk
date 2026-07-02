-- Bitmap font showcase
-- Each line shows a font rendered at its native size with CSS shorthand as label
-- Params:
--   font_prefix: which family to show (default: "X11Helv")

local width = layout.width
local height = layout.height
local margin = 6
local header_height = 20

local prefix = params.font_prefix or "X11Helv"

-- Font family definitions
-- Each entry is a list of { family, sizes, styles } groups
local family_db = {
  X11Helv   = {{ family = "X11Helv",
                  sizes = {8, 10, 11, 12, 14, 17, 18, 20, 24, 25, 34},
                  styles = { {}, {bold=true}, {italic=true}, {bold=true, italic=true} } }},
  X11LuSans = {{ family = "X11LuSans",
                  sizes = {8, 10, 11, 12, 14, 17, 18, 19, 20, 24, 25, 26, 34},
                  styles = { {}, {bold=true}, {italic=true}, {bold=true, italic=true} } }},
  X11LuType = {{ family = "X11LuType",
                  sizes = {8, 10, 11, 12, 14, 17, 18, 19, 20, 24, 25, 26, 34},
                  styles = { {}, {bold=true} } }},
  X11Term   = {{ family = "X11Term",
                  sizes = {14, 18},
                  styles = { {}, {bold=true} } }},
  X11Misc   = {
    { family = "X11Misc5x",  sizes = {6, 7, 8},       styles = { {} } },
    { family = "X11Misc6x",  sizes = {9, 10, 12},     styles = { {} } },
    { family = "X11Misc6x",  sizes = {13},             styles = { {}, {bold=true}, {italic=true} } },
    { family = "X11Misc7x",  sizes = {13},             styles = { {}, {bold=true}, {italic=true} } },
    { family = "X11Misc7x",  sizes = {14},             styles = { {}, {bold=true} } },
    { family = "X11Misc8x",  sizes = {13},             styles = { {}, {bold=true}, {italic=true} } },
    { family = "X11Misc8x",  sizes = {16},             styles = { {} } },
    { family = "X11Misc9x",  sizes = {15, 18},         styles = { {}, {bold=true} } },
    { family = "X11Misc10x", sizes = {20},             styles = { {} } },
    { family = "X11Misc12x", sizes = {24},             styles = { {} } },
  },
}

local groups = family_db[prefix]
if not groups then
  error("Unknown font_prefix: " .. prefix)
end

local col_gap = 8
local split_size = 20  -- sizes < split go left, >= split go right
local col_split = math.floor(width * 2 / 5)  -- left column for small sizes

-- Build all entries
local entries = {}
for _, grp in ipairs(groups) do
  for _, px in ipairs(grp.sizes) do
    for _, style in ipairs(grp.styles) do
      local parts = {}
      if style.bold then table.insert(parts, "bold") end
      if style.italic then table.insert(parts, "oblique") end
      table.insert(parts, px .. "px/" .. grp.family)
      table.insert(entries, {
        size = px,
        family = grp.family,
        label = table.concat(parts, " "),
        bold = style.bold or false,
        italic = style.italic or false,
      })
    end
  end
end

-- Check if we need two columns (have both small and large sizes)
local has_small, has_large = false, false
for _, e in ipairs(entries) do
  if e.size < split_size then has_small = true else has_large = true end
end

local use_two_cols = has_small and has_large
local col_x1 = margin
local col_x2 = col_split + col_gap

-- Place entries into lines list with column x coordinate
local lines = {}
local y = { header_height + 4, header_height + 4 }

for _, e in ipairs(entries) do
  local col
  if use_two_cols then
    col = e.size < split_size and 1 or 2
  else
    col = 1
  end
  local x = (col == 1) and col_x1 or col_x2
  if y[col] + e.size + 1 < height - 2 then
    table.insert(lines, {
      x = x,
      y = y[col] + e.size,
      size = e.size,
      family = e.family,
      label = e.label,
      bold = e.bold,
      italic = e.italic,
    })
    y[col] = y[col] + e.size + 1
  end
end

return {
  data = {
    width = width,
    height = height,
    lines = lines,
    margin = margin,
    header_height = header_height,
    title = prefix,
    grey_count = layout.grey_count or 4,
  },
  refresh_rate = 3600,
}
