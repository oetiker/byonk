-- Hello World screen
-- Displays a greeting with the current time

local now = time_now()

return {
  data = {
    greeting = "Hello, World!",
    time = time_format(now, "%H:%M:%S"),
    date = time_format(now, "%A, %B %d, %Y"),
    -- Layout
    width = layout.width,
    height = layout.height,
    scale = layout.scale,
    -- Font sizes (scaled) - use scale_font for precision
    font_greeting = scale_font(48),
    font_time = scale_font(72),
    font_date = scale_font(24),
    font_footer = scale_font(14),
    -- Positions (scaled) - use scale_pixel for pixel alignment
    greeting_y = scale_pixel(120),
    time_y = scale_pixel(260),
    date_y = scale_pixel(320),
    footer_y = layout.height - scale_pixel(30),
    center_x = layout.center_x,
    -- Generate a QR code anchored to bottom-right corner with scaled margin
    qr_code = qr_svg("https://www.youtube.com/watch?v=dQw4w9WgXcQ", {
      anchor = "bottom-right",
      right = layout.margin_sm,
      bottom = layout.margin_sm,
      module_size = scale_pixel(4)
    })
  },
  refresh_rate = 60  -- Refresh every minute
}
