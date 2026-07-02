-- Hinting demo: 9-cell grid comparing hinting engines × targets
-- Rows: auto, native, none (no hinting)
-- Columns: mono, normal, light

local width = layout.width
local height = layout.height

local sizes = {10, 12, 14, 17, 20, 24}
local sample = "Hinting 0123"

local engines = {
  { name = "auto",   engine = "auto",   hinted = true },
  { name = "native", engine = "native", hinted = true },
  { name = "none",   engine = "auto",   hinted = false },
}

local targets = {
  { name = "mono",   target = "mono",   mode = "normal" },
  { name = "normal", target = "smooth", mode = "normal" },
  { name = "light",  target = "smooth", mode = "light"  },
}

local margin = 4
local top = 18
local row_header_w = 22
local col_header_h = 18
local pad = 6
local col_count = #targets
local row_count = #engines
local cell_w = (width - margin * 2 - row_header_w) / col_count
local cell_h = (height - top - margin - col_header_h) / row_count

-- Build cells with pre-computed positions
local cells = {}
local cell_idx = 0
for ri, eng in ipairs(engines) do
  for ci, tgt in ipairs(targets) do
    local cx = margin + row_header_w + (ci - 1) * cell_w + pad
    local cy = top + col_header_h + (ri - 1) * cell_h + pad
    local lines = {}
    local ly = 0
    for _, sz in ipairs(sizes) do
      ly = ly + sz
      if cy + ly < top + col_header_h + ri * cell_h - pad then
        table.insert(lines, {
          size = sz,
          text = sz .. "px " .. sample,
          x = cx,
          y = cy + ly,
        })
      end
      ly = ly + 2
    end
    table.insert(cells, {
      idx = cell_idx,
      engine = eng.engine,
      hinted = eng.hinted,
      target = tgt.target,
      mode = tgt.mode,
      is_mono = tgt.target == "mono" and eng.hinted,
      lines = lines,
    })
    cell_idx = cell_idx + 1
  end
end

-- Column header positions
local col_headers = {}
for ci, tgt in ipairs(targets) do
  table.insert(col_headers, {
    label = tgt.name,
    x = margin + row_header_w + (ci - 1) * cell_w + cell_w / 2,
  })
end

-- Row header positions (vertical text, centered in cell)
local row_headers = {}
for ri, eng in ipairs(engines) do
  local row_y = top + col_header_h + (ri - 1) * cell_h
  table.insert(row_headers, {
    label = eng.name,
    x = margin + row_header_w / 2 + 1,
    y = row_y + cell_h / 2,
    line_y = row_y,
    show_line = ri > 1,
  })
end

-- Column grid line positions
local col_lines = {}
for ci = 2, col_count do
  table.insert(col_lines, {
    x = margin + row_header_w + (ci - 1) * cell_w,
  })
end

-- Grid boundaries
local grid_x1 = margin + row_header_w
local grid_x2 = width - margin
local grid_y1 = top + col_header_h
local grid_y2 = height - margin

return {
  data = {
    width = width,
    height = height,
    cells = cells,
    col_headers = col_headers,
    row_headers = row_headers,
    col_lines = col_lines,
    margin = margin,
    top = top,
    row_header_w = row_header_w,
    col_header_h = col_header_h,
    grid_x1 = grid_x1,
    grid_x2 = grid_x2,
    grid_y1 = grid_y1,
    grid_y2 = grid_y2,
    grey_count = layout.grey_count or 4,
  },
  refresh_rate = 3600,
}
