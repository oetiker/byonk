-- Google Photos shared album display
-- Fetches random photos from a shared Google Photos album
-- Uses HTML scraping of the shared album page (no OAuth required)

local album_url = params.album_url
if not album_url then
  return {
    data = { error = "Missing album_url parameter" },
    refresh_rate = 60
  }
end

local width = layout.width
local height = layout.height
local show_status = params.show_status or false
local refresh_rate = params.refresh_rate or 3600

-- Fetch album page (cached for 1 hour)
local ok, html = pcall(function()
  return http_get(album_url, { cache_ttl = 3600 })
end)

if not ok then
  log_error("Failed to fetch album: " .. tostring(html))
  return {
    data = { error = "Failed to fetch album" },
    refresh_rate = 300
  }
end

-- Extract image URLs from lh3.googleusercontent.com
local image_urls = {}
local seen = {}
for url in html:gmatch('(https://lh3%.googleusercontent%.com/pw/[^"\'%s]+)') do
  -- Remove existing size parameters and deduplicate
  local base_url = url:gsub("=w%d+%-h%d+[^\"']*", "")
  if not seen[base_url] then
    seen[base_url] = true
    table.insert(image_urls, base_url)
  end
end

if #image_urls == 0 then
  log_warn("No images found in album")
  return {
    data = { error = "No images found in album" },
    refresh_rate = 300
  }
end

-- Select random image
math.randomseed(time_now())
local selected_url = image_urls[math.random(#image_urls)]

-- Append size parameters for device dimensions
local sized_url = selected_url .. "=w" .. width .. "-h" .. height .. "-no"

log_info("Selected photo " .. #image_urls .. " available: " .. sized_url)

-- Fetch image (cached for 24 hours)
local img_ok, img_data = pcall(function()
  return http_get(sized_url, { cache_ttl = 86400 })
end)

if not img_ok then
  log_error("Failed to fetch image: " .. tostring(img_data))
  return {
    data = { error = "Failed to fetch image" },
    refresh_rate = 300
  }
end

local image_src = "data:image/jpeg;base64," .. base64_encode(img_data)

return {
  data = {
    image_src = image_src,
    width = width,
    height = height,
    show_status = show_status,
    info_bar_height = scale_pixel(32),
    info_bar_y = height - scale_pixel(32),
    font_sm = scale_font(14),
  },
  refresh_rate = refresh_rate
}
