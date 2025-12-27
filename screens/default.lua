-- Default screen script
-- Returns static data with current time

local now = time_now()
local time_str = time_format(now, "%H:%M")
local date_str = time_format(now, "%Y-%m-%d")

return {
  data = {
    title = "TRMNL BOYS",
    subtitle = "Dynamic Content Server",
    time = time_str,
    date = date_str,
    message = params.message or "Ready for scripted content!"
  },
  refresh_rate = 300  -- 5 minutes
}
