-- Hello World screen
-- Displays a greeting with the current time

local now = time_now()

return {
  data = {
    greeting = "Hello, World!",
    time = time_format(now, "%H:%M:%S"),
    date = time_format(now, "%A, %B %d, %Y")
  },
  refresh_rate = 60  -- Refresh every minute
}
