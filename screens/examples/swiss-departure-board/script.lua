-- Public transport departure display
-- Fetches real-time departures from Swiss OpenData Transport API

local station = params.station or "Olten, Südwest"

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height

-- Calculate layout dimensions using scale_pixel for pixel alignment
local header_height = scale_pixel(70)
local col_header_height = scale_pixel(35)
local footer_height = scale_pixel(30)
local row_height = scale_pixel(45)
local available_height = height - header_height - col_header_height - footer_height
local rows_per_column = math.floor(available_height / row_height)

-- Use 2 columns for wider screens (X device has aspect ratio ~1.33, OG has ~1.67)
local use_two_columns = (width / height) < 1.5
local num_columns = use_two_columns and 2 or 1
local max_rows = rows_per_column * num_columns
local limit = params.limit or math.min(max_rows, 30)  -- Cap at 30 max

-- Truncate destination names based on layout
local dest_max_chars = use_two_columns and 16 or 30
local function truncate_dest(dest)
  if #dest > dest_max_chars then
    return dest:sub(1, dest_max_chars - 1) .. "…"
  end
  return dest
end

log_info("Fetching departures for: " .. station .. " (limit: " .. limit .. ")")

-- URL encode the station name
local encoded_station = url_encode(station)

local url = "https://transport.opendata.ch/v1/stationboard?station=" .. encoded_station .. "&limit=" .. limit

local ok, response = pcall(function()
  return http_get(url)
end)

if not ok then
  log_error("Failed to fetch departures: " .. tostring(response))
  return {
    data = {
      station = station,
      error = "Failed to fetch departures",
      departures = {},
      updated_at = time_format(time_now(), "%H:%M")
    },
    refresh_rate = 60
  }
end

-- Parse JSON response
local json = json_decode(response)

if not json or not json.stationboard then
  log_error("Invalid response from API")
  return {
    data = {
      station = station,
      error = "Invalid API response",
      departures = {},
      updated_at = time_format(time_now(), "%H:%M")
    },
    refresh_rate = 60
  }
end

-- Get actual station name from response
local station_name = station
if json.station and json.station.name then
  station_name = json.station.name
end

-- Process departures
local departures = {}
local now = time_now()

for i, dep in ipairs(json.stationboard) do
  local departure_time = dep.stop and dep.stop.departure or dep.departure
  local delay = dep.stop and dep.stop.delay or 0

  -- Parse departure time (ISO 8601 format: 2024-12-26T14:30:00+0100)
  local hour, min = departure_time:match("T(%d+):(%d+)")
  local time_str = hour .. ":" .. min

  -- Add delay indicator
  local delay_str = nil
  if delay and delay > 0 then
    delay_str = "+" .. delay
  end

  -- Get category icon (B=Bus, S=S-Bahn, T=Tram, etc.)
  local category = dep.category or "?"
  local line = dep.number or dep.name or ""

  -- Combine category and line for display
  local line_display = category
  if line ~= "" then
    line_display = category .. line
  end

  table.insert(departures, {
    time = time_str,
    delay = delay_str,
    line = line_display,
    destination = truncate_dest(dep.to or "Unknown"),
    category = category
  })
end

-- Calculate refresh rate
-- Refresh shortly after the first bus departs so the display updates
local refresh_rate = 300
local next_departure_info = nil

if #departures > 0 then
  local first_dep = json.stationboard[1]
  if first_dep and first_dep.stop and first_dep.stop.departureTimestamp then
    local dep_timestamp = first_dep.stop.departureTimestamp
    local delay_seconds = (first_dep.stop.delay or 0) * 60

    -- Actual departure time including delay
    local actual_departure = dep_timestamp + delay_seconds
    local seconds_until = actual_departure - now

    -- Refresh 30 seconds after the bus departs (so it disappears from list)
    -- Minimum 30s, maximum 15 minutes
    if seconds_until > 0 then
      refresh_rate = seconds_until + 30
    else
      -- Bus already departed, refresh soon
      refresh_rate = 30
    end
    refresh_rate = math.max(30, math.min(refresh_rate, 900))

    next_departure_info = string.format("%s to %s in %d sec",
      departures[1].line, first_dep.to or "?", seconds_until)
  end
end

if next_departure_info then
  log_info(string.format("Found %d departures, next: %s, refresh in %d sec",
    #departures, next_departure_info, refresh_rate))
else
  log_info(string.format("Found %d departures, refresh in %d sec", #departures, refresh_rate))
end

-- Split departures into columns for two-column layout
local left_departures = {}
local right_departures = {}
if use_two_columns then
  for i, dep in ipairs(departures) do
    if i <= rows_per_column then
      table.insert(left_departures, dep)
    else
      table.insert(right_departures, dep)
    end
  end
else
  left_departures = departures
end

-- Column width for two-column layout
local column_width = use_two_columns and math.floor(width / 2) or width
local column_gap = layout.margin

return {
  data = {
    station = station_name,
    departures = departures,
    left_departures = left_departures,
    right_departures = right_departures,
    updated_at = time_format(now, "%H:%M"),
    total_count = #departures,
    -- Layout
    width = width,
    height = height,
    scale = layout.scale,
    use_two_columns = use_two_columns,
    column_width = column_width,
    rows_per_column = rows_per_column,
    -- Dimensions (scaled)
    header_height = header_height,
    header_text_y = scale_pixel(48),
    col_header_height = col_header_height,
    col_header_y = header_height + scale_pixel(25),
    row_start_y = header_height + col_header_height,
    row_height = row_height,
    badge_y_offset = scale_pixel(7),
    badge_height = scale_pixel(30),
    badge_width = scale_pixel(70),
    text_y_offset = scale_pixel(29),
    margin = layout.margin,
    -- Left column positions
    left_badge_x = layout.margin,
    left_badge_center_x = scale_pixel(55),
    left_dest_x = scale_pixel(120),
    left_time_x = column_width - scale_pixel(30),
    left_delay_x = column_width - layout.margin_sm,
    -- Right column positions (offset by column_width)
    right_badge_x = column_width + layout.margin,
    right_badge_center_x = column_width + scale_pixel(55),
    right_dest_x = column_width + scale_pixel(120),
    right_time_x = width - scale_pixel(30),
    -- General positions
    right_margin = width - layout.margin,
    center_x = layout.center_x,
    muted_y = scale_pixel(280),
    footer_y = height - scale_pixel(7),
    -- Font sizes (scaled) - use scale_font for precision
    font_title = scale_font(28),
    font_time = scale_font(14),
    font_col_header = scale_font(14),
    font_line = scale_font(16),
    font_dest = scale_font(18),
    font_dep_time = scale_font(22),
    font_delay = scale_font(14),
    font_muted = scale_font(24),
    font_footer = scale_font(11),
  },
  refresh_rate = refresh_rate
}
