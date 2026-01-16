-- Public transport departure display
-- Fetches real-time departures from Swiss OpenData Transport API

local station = params.station or "Olten, Südwest"
local limit = params.limit or 8

log_info("Fetching departures for: " .. station)

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
    destination = dep.to or "Unknown",
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

return {
  data = {
    station = station_name,
    departures = departures,
    updated_at = time_format(now, "%H:%M"),
    total_count = #departures
  },
  refresh_rate = refresh_rate
}
