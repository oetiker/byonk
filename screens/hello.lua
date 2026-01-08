-- Hello World screen
-- Displays a greeting with the current time

local now = time_now()

return {
  data = {
    greeting = "Hello, World!",
    time = time_format(now, "%H:%M:%S"),
    date = time_format(now, "%A, %B %d, %Y"),
    -- Generate a QR code anchored to bottom-right corner with 10px margin
    qr_code = qr_svg("https://www.youtube.com/watch?v=dQw4w9WgXcQ", {
      x = 790, y = 470,
      anchor = "bottom-right",
      module_size = 4
    })
  },
  refresh_rate = 60  -- Refresh every minute
}
