-- Floerli room booking display
-- Fetches schedule from floerli-olten.ch and shows current/upcoming bookings

local room_name = params.room or "Rosa"

-- Optional test timestamp (for testing specific dates)
-- Use: params.test_timestamp = 1768390800 for Jan 14, 2026 at 12:00
local now
if params.test_timestamp then
  now = tonumber(params.test_timestamp)
  log_info("Using test timestamp: " .. now)
else
  now = time_now()
end

-- Room ID mapping (from HTML: room01=Flora, room02=Salon, etc.)
local room_ids = {
  Flora = "room01", Salon = "room02", ["Küche"] = "room03",
  Bernsteinzimmer = "room05", Rosa = "room06", Clara = "room07",
  Cosy = "room08", Sofia = "room09"
}
local room_id = room_ids[room_name] or "room06"

log_info("Fetching Floerli calendar for room: " .. room_name .. " (id: " .. room_id .. ")")

-- Calculate today's midnight timestamp for the URL
-- Get start of today (midnight) - approximate by subtracting seconds since midnight
local hour = tonumber(time_format(now, "%H"))
local min = tonumber(time_format(now, "%M"))
local sec = tonumber(time_format(now, "%S"))
local today_start = now - (hour * 3600 + min * 60 + sec)

-- Fetch the calendar page
local url = "https://floerli-olten.ch/index.cgi?rm=calendar&start=" .. math.floor(today_start)
log_info("Fetching URL: " .. url)

local ok, html = pcall(function()
  return http_get(url)
end)

if not ok then
  log_error("Failed to fetch calendar: " .. tostring(html))
  return {
    data = {
      room = room_name,
      error = "Failed to fetch calendar",
      current = nil,
      upcoming = {},
      updated_at = time_format(now, "%H:%M"),
      date_str = time_format(now, "%d.%m.%Y")
    },
    refresh_rate = 60
  }
end

local doc = html_parse(html)

-- Debug: check if we got any content
log_info("HTML length: " .. #html)

-- Find today's date row by looking for date cells
-- Format: <td id="date1768345200" class="date" colspan="9">Mittwoch 14. Januar 2026</td>
local bookings = {}
local today_date_str = time_format(now, "%d.%m.%Y")

-- Parse all rows with class "hour"
local hour_rows = doc:select("tr.hour")
log_info("Found hour rows: " .. (hour_rows and #hour_rows or 0))

local current_hour = tonumber(time_format(now, "%H"))

-- Track active rowspan bookings for each room
local active_booking = nil
local active_until_hour = 0

local rows_processed = 0
hour_rows:each(function(row)
  rows_processed = rows_processed + 1

  -- Get text from the row - the time slot is the first part before the cells
  local row_text = row:text()
  -- Extract the time pattern (e.g., "7 - 8")
  local start_hour, end_hour_str = row_text:match("^%s*(%d+)%s*%-%s*(%d+)")

  if not start_hour then return end
  local time_text = start_hour .. " - " .. end_hour_str
  local start_hour = tonumber(time_text:match("^(%d+)"))
  if not start_hour then return end

  -- Check if we have an active booking spanning this hour
  local booking_text = nil
  local is_free = true
  local end_hour = start_hour + 1

  -- Look for our room in the row HTML using pattern matching
  local row_html = row:html()

  -- Find our room's td by iterating through all td tags in the row
  local td_tag = nil
  for td in row_html:gmatch('<td[^>]+>') do
    if td:find('headers="' .. room_id .. ' ') or td:find('headers="' .. room_id .. '"') then
      td_tag = td
      break
    end
  end

  -- Check if this row is for today by extracting date from headers
  -- Format: headers="room06 date1768345200 time1768377600"
  local row_date = td_tag and td_tag:match('date(%d+)')
  local is_today = row_date and tonumber(row_date) == today_start

  -- Skip rows that are not for today
  if not is_today then return end

  -- Extract class from the td tag
  local room_class = td_tag and td_tag:match('class="([^"]*)"')

  if room_class then
    if room_class:find("occupied") then
      is_free = false
      -- Extract booking text - find the full td..../td content for our room
      -- Pattern: <td ...headers="room06..."...>CONTENT</td>
      local cell_pattern = '<td[^>]*headers="' .. room_id .. '[^"]*"[^>]*>([^<]*)'
      booking_text = row_html:match(cell_pattern)
      if booking_text then
        booking_text = booking_text:gsub("^%s+", ""):gsub("%s+$", "")
      end
      -- Check for rowspan in the td tag
      if td_tag then
        local rowspan = td_tag:match('rowspan="(%d+)"')
        if rowspan then
          end_hour = start_hour + tonumber(rowspan)
        end
      end
    elseif room_class:find("available") then
      is_free = true
      booking_text = nil
    end
  end

  -- Only track bookings from current hour onwards, and limit to today (hour < 24)
  if start_hour >= 7 and start_hour <= 23 then
    table.insert(bookings, {
      start_hour = start_hour,
      end_hour = end_hour,
      time = string.format("%02d:00", start_hour),
      time_range = string.format("%d - %d", start_hour, end_hour),
      title = booking_text,
      is_free = is_free
    })
  end
end)

log_info("Parsed " .. #bookings .. " time slots")

-- Find current booking (the slot containing current hour)
local current_booking = nil
local current_end_hour = nil

for i, booking in ipairs(bookings) do
  if booking.start_hour <= current_hour and current_hour < booking.end_hour then
    current_booking = booking
    current_end_hour = booking.end_hour
    break
  end
end

-- If no current booking found but we're in a valid hour, mark as free
if not current_booking and current_hour >= 7 and current_hour < 24 then
  current_booking = {
    start_hour = current_hour,
    end_hour = current_hour + 1,
    time = string.format("%02d:00", current_hour),
    title = nil,
    is_free = true
  }
  current_end_hour = current_hour + 1
end

-- Collect upcoming bookings (after current slot, non-free)
local upcoming = {}
for i, booking in ipairs(bookings) do
  if booking.start_hour > current_hour and not booking.is_free and #upcoming < 5 then
    -- Avoid duplicates (from rowspan)
    local dominated = false
    for _, existing in ipairs(upcoming) do
      if booking.start_hour >= existing.start_hour and booking.start_hour < existing.end_hour then
        dominated = true
        break
      end
    end
    if not dominated then
      table.insert(upcoming, booking)
    end
  end
end

-- Calculate refresh rate
-- Refresh when current booking ends, or in 15 min if room is free
local refresh_rate = 900  -- default 15 min

if current_booking then
  if not current_booking.is_free and current_end_hour then
    -- Refresh when current booking ends
    local end_timestamp = today_start + (current_end_hour * 3600)
    local seconds_until_end = end_timestamp - now
    refresh_rate = math.max(60, seconds_until_end + 30)
  else
    -- Room is free - refresh in 15 min or when next booking starts
    if #upcoming > 0 then
      local next_start = today_start + (upcoming[1].start_hour * 3600)
      local seconds_until_next = next_start - now
      refresh_rate = math.max(60, math.min(seconds_until_next + 30, 900))
    end
  end
end

refresh_rate = math.min(refresh_rate, 3600)  -- max 1 hour

log_info(string.format("Current: %s, upcoming: %d, refresh in %d sec",
  current_booking and (current_booking.is_free and "free" or current_booking.title) or "none",
  #upcoming, refresh_rate))

-- Use layout helpers for responsive design
local width = layout.width
local height = layout.height

return {
  data = {
    room = room_name,
    current = current_booking,
    upcoming = upcoming,
    updated_at = time_format(now, "%H:%M"),
    date_str = time_format(now, "%A, %d. %B %Y"),
    total_bookings = #bookings,
    -- Layout
    width = width,
    height = height,
    scale = layout.scale,
    -- Dimensions (scaled) - use scale_pixel for pixel alignment
    header_height = scale_pixel(70),
    header_text_y = scale_pixel(48),
    status_y = scale_pixel(90),
    status_height = scale_pixel(130),
    status_width = width - layout.margin_lg,
    status_label_y = scale_pixel(90 + 35),
    status_name_y = scale_pixel(90 + 75),
    status_time_y = scale_pixel(90 + 110),
    status_center_y = scale_pixel(90 + 65),
    status_center_time_y = scale_pixel(90 + 105),
    section_y = scale_pixel(265),
    line_y = scale_pixel(275),
    booking_start_y = scale_pixel(310),
    booking_spacing = scale_pixel(38),
    booking_name_x = scale_pixel(150),
    footer_y = height - layout.margin_sm,
    margin = layout.margin,
    text_margin = layout.margin_lg,
    right_margin = width - layout.margin,
    line_end_x = width - layout.margin_lg,
    center_x = layout.center_x,
    muted_y = scale_pixel(340),
    -- Font sizes (scaled) - use scale_font for precision
    font_title = scale_font(32),
    font_time = scale_font(14),
    font_label = scale_font(16),
    font_current_name = scale_font(26),
    font_current_time = scale_font(18),
    font_available = scale_font(36),
    font_available_time = scale_font(18),
    font_section = scale_font(20),
    font_booking_time = scale_font(18),
    font_booking_name = scale_font(18),
    font_muted = scale_font(18),
    font_footer = scale_font(11),
  },
  refresh_rate = refresh_rate
}
