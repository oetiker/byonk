-- Default screen script
-- TV test pattern with device info

local now = time_now()
local time_str = time_format(now, "%H:%M")
local date_str = time_format(now, "%Y-%m-%d")

return {
  data = {
    time = time_str,
    date = date_str,
  },
  refresh_rate = 300  -- 5 minutes
}
