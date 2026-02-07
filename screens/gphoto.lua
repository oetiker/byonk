-- Google Photos shared album display
-- Fetches random photos from a shared Google Photos album
-- Uses HTML scraping of the shared album page (no OAuth required)

local width = layout.width
local height = layout.height
local show_status = params.show_status or false
local refresh_rate = params.refresh_rate or 3600

-- Helper for error returns (always include width/height for template)
local function error_return(msg, retry)
  return {
    data = { error = msg, width = width, height = height },
    refresh_rate = retry or 300
  }
end

local album_url = params.album_url
if not album_url then
  -- No album configured - show registration code (useful as default screen)
  local reg_box_width = scale_pixel(520)
  local reg_box_height = scale_pixel(104)
  local font_reg_code = scale_font(62)
  local font_title = scale_font(48)

  return {
    data = {
      show_registration = true,
      width = width,
      height = height,
      center_x = math.floor(width / 2),
      center_y = math.floor(height / 2),
      reg_box_x = math.floor(width / 2) - math.floor(reg_box_width / 2),
      reg_box_y = math.floor(height / 2) - math.floor(reg_box_height / 2),
      reg_box_width = reg_box_width,
      reg_box_height = reg_box_height,
      reg_text_y = math.floor(height / 2) + math.floor(reg_box_height * 0.18),
      font_reg_code = font_reg_code,
      title_y = math.floor(height / 2) - math.floor(reg_box_height / 2) - scale_pixel(40),
      font_title = font_title,
    },
    refresh_rate = 60
  }
end

-- Fetch album page (no caching - Google responses are inconsistent with caching enabled)
-- Note: Do NOT add User-Agent header - Google serves a JS-heavy page to browsers
-- but includes image URLs in the HTML for simpler clients
local ok, html = pcall(function()
  return http_get(album_url)
end)

if not ok then
  log_error("Failed to fetch album: " .. tostring(html))
  return error_return("Failed to fetch album")
end

-- Verify we got content (empty response = likely cached error)
if not html or #html < 1000 then
  log_error("Album response too short: " .. (html and #html or 0) .. " bytes")
  return error_return("Failed to load album")
end

-- Extract image URLs from lh3.googleusercontent.com
-- First unescape JavaScript-escaped URLs (\/\/ -> //)
local unescaped = html:gsub("\\/", "/")

local image_urls = {}
local seen = {}
for url in unescaped:gmatch('(https://lh3%.googleusercontent%.com/pw/[^"\'%s%)%;]+)') do
  -- Remove existing size parameters and deduplicate
  local base_url = url:gsub("=[whs]%d+[^\"'%s%)%;]*", "")
  if not seen[base_url] then
    seen[base_url] = true
    table.insert(image_urls, base_url)
  end
end

if #image_urls == 0 then
  log_warn("No images found in album (html len=" .. #html .. ")")
  return error_return("No images found in album")
end

-- Select random image using real system time (not overrideable time_now)
local seed = os.time()
math.randomseed(seed)
local idx = math.random(#image_urls)
local selected_url = image_urls[idx]
log_info("Random: seed=" .. seed .. " idx=" .. idx .. "/" .. #image_urls)

-- Append size parameters for device dimensions
local sized_url = selected_url .. "=w" .. width .. "-h" .. height .. "-no"

log_info("Selected photo from " .. #image_urls .. " available")

-- Fetch image (cached for 24 hours)
local img_ok, img_data = pcall(function()
  return http_get(sized_url, { cache_ttl = 86400 })
end)

if not img_ok then
  log_error("Failed to fetch image: " .. tostring(img_data))
  return error_return("Failed to fetch image")
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
  -- dither = "floyd-steinberg",
  dither = "jarvis-judice-ninke",
  refresh_rate = refresh_rate
}
