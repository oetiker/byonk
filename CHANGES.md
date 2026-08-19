# Changelog

All notable changes to Byonk will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### New

- **The Home Assistant device page now shows a picture of the screen.** Each
  device gets a *Screen preview* camera rendered by byonk from that device's own
  screen, parameters, panel and dither settings, so changing the Screen select
  shows you the result instead of making you walk over to the device. The
  reserved Default device has one too.

  It is free while you are not looking at it: Home Assistant only pulls frames
  while the picture is open on screen, and byonk re-serves a rendered image until
  the device's configuration changes or the screen's own refresh rate elapses.
  A screen whose data moves on its own — a clock, a forecast — can therefore sit
  still in the preview; the new *Refresh preview* button forces a render. A
  screen that fails to render shows the error image the panel itself would show.

  Two switches change how the preview is drawn, without touching what the
  device displays: *Preview dithering* off shows the screen before dithering,
  in full colour and at no palette restriction — the version to look at when
  you are checking a layout — and *Preview measured colors* off draws the spec
  colours byonk sends to the panel instead of the measured ones a calibration
  says it really produces.

- **New admin endpoint `GET /api/admin/devices/:key/preview`**, returning that
  PNG. `?force` skips the cache, `?dither=off` returns the undithered render
  and `?measured=off` the spec colours.

### Changed

### Fixed

## 0.18.0 - 2026-08-18

### New

- **New `http_response()` for Lua scripts**, which returns the whole reply —
  `ok`, `status`, `body`, `headers` — instead of just the body, and does not
  raise when the request fails. Until now a script could not tell a 404 or a
  500 from real data: `http_get` hands back whatever came, so an error page
  arrived looking exactly like the thing you asked for. What a failure means is
  the screen's business, so byonk now reports it and lets the script decide.
  Responses are also only cached when they succeed, so an error page is no
  longer served as data for the rest of a `cache_ttl` window.
- **Writable local screen repositories** via `screen_repos: { <name>: { path: … } }`,
  so your own screens live in their own handle (`local`) instead of being mixed into
  the `byonk-builtin` handle.
- **Shipped example screens** (hello, mandelbrot, webscrape, gphoto,
  swiss-departure-board, font demo) now install as an editable `examples`
  repository. Override where they're seeded with the new `EXAMPLES_DIR`
  environment variable.
- **New `image_process()` function for Lua scripts**: crop, resize, tone-map
  and sharpen a photograph before embedding it in a screen. A
  `preset = "eink"` one-liner handles the common case, and `palette_aware`
  tunes the result to what your panel can actually display.
- **The `gphoto` example now uses `image_process()`**, so photo screens look
  markedly better out of the box.
- **Author screens with an LLM over MCP.** Byonk now exposes a Model Context
  Protocol endpoint at `/mcp`, so an assistant like Claude Code can list, read,
  create, edit, validate and render screens on a running byonk — including one
  inside Home Assistant — over the network, with no file access needed. It is
  protected by the same admin token as the admin API, and is invisible (404)
  until you set one. The server also publishes its own authoring references
  (Lua API, SVG templates, the `meta.yaml` schema) so the assistant works from
  this server's rules rather than guesswork. `render_screen` lets the assistant
  choose what it gets back — which image (or none), scaled to a width it picks,
  whether to include the script's data table, and whether to include the fully
  expanded SVG that was rasterized — so reviewing a screen costs only as much
  of its context as the assistant decides to spend. Embedded base64 images are
  summarised rather than repeated verbatim by default, which takes a photo
  screen's render from 336 KB to 17 KB.
- **Lua scripts can now read and override a panel's measured colours.**
  `device.colors_actual` exposes what the panel really shows (`nil` when
  uncalibrated), and a script can return its own `colors_actual` to retune
  the render for this pass — so a screen can adapt to, or improve on, what
  its display actually renders.
- **The `render_screen` MCP tool gained `use_actual` and `colors_actual`**, so
  an authoring agent can preview a screen exactly as the panel will show it —
  including previewing a calibration by hand — without first configuring a
  panel. Its diagnostics also report `measured_source`, naming which layer
  supplied the measured colours the render actually dithered against.
- **`byonk render` now honours measured panel colours**, including a new
  `--use-actual` flag to draw the output PNG in those measured colours
  instead of the spec palette. Previously the CLI ignored measured colours
  entirely.
- **New `byonk-builtin/calibration/gamut` screen** draws the hue circle as
  isolated flat patches across several lightness levels, so you can see at a
  glance which colours your panel can actually mix and which it collapses to a
  single flat colour. Complements the existing colour calibrator, whose smooth
  gradient hides exactly this. Tune the grid with the `hues` and `levels`
  params.
- New builtin calibration screen **Tone Marker A/B** (`byonk-builtin/calibration/tone`):
  renders a photograph, hue sweep and colour patch grid twice side by side, with the
  right half marked as continuous-tone. Shows on a real panel what gamut mapping
  changes.
- **New `font_hinting` directive for `script.lua`**, for the screens that want
  something other than what byonk chose. It can pick the engine and target,
  turn hinting off outright, and declare *variants* — your own names for a font
  hinted a particular way, so one screen can render the same family two
  different ways. Byonk checks a variant's base family and name when the script
  runs and fails with a clear message, rather than silently rendering a
  different font. See the new "Font Hinting" page in the documentation.
### Changed

- **Byonk now tells you when a screen's SVG is not the size of the device.**
  Such a screen still renders — byonk scales it to fit — but the scaling is
  silent, and it costs more than sharpness: every dimension you chose is shown
  at the wrong size (text set at 10 px in a 400x240 SVG is 20 px on an 800x480
  panel), hinting stops helping because hinted outlines are fitted to the SVG's
  own pixel grid, and bitmap fonts are resampled instead of being drawn from
  their strikes. `byonk render` prints the warning to stderr and the authoring
  API returns it in the render log. Build your `viewBox`, `width` and `height`
  from `layout.width` and `layout.height` and it never fires. An exact 2x zoom
  is warned about too — an integer scale keeps glyphs on whole pixels, but it
  still shows your layout at twice the size you designed it.

- **Continuous-tone content is now marked, and it changes how a screen renders.**
  Byonk now treats a screen as two kinds of content. Anything you mark with
  `data-byonk-tone="continuous"` — photographs, hue gradients — is matched
  against your panel's *measured* colours and gamut-mapped into what the panel
  can physically show. Everything else is **structure**, and is matched against
  the *official* palette: `#FF0000` is simply red, so it pins to that one ink
  and renders as a flat block with no speckle. Text beside a saturated area
  stays black instead of picking up diffused colour error, and a flat fill of a
  palette colour comes out as that colour.

  **Upgrade note — mark your photographs.** This is the one change that can
  make an existing screen look worse. An *unmarked* photograph is now aimed at
  primaries the panel cannot produce, and it renders markedly darker: on
  byonk's own sample images the black ink share goes from roughly 56% to
  roughly 75%. If a screen of yours draws a photo or a hue gradient, add
  `data-byonk-tone="continuous"` to that element. Byonk's own bundled screens
  have already been updated — most needed no change, because most of them draw
  no continuous-tone content at all.

  Mark the element that *is* continuous-tone, not a `<g>` around it — a wrapper
  swallows neighbouring labels and turns text into continuous-tone content.
  Leave grey gradients unmarked: grey is always in gamut, so marking only costs
  you the pinning. See *Marking continuous-tone content* in the SVG templates
  guide for the full rules.

  **If your Lua script paints a measured colour, stop.** `device.colors_actual`
  values are no longer palette entries for unmarked content, so a shape filled
  with one can no longer match exactly and will dither instead of coming out
  flat. Decide with the measured value if you need to (contrast, say), but
  paint with the official one.
- **Dithered images no longer show weave patterns or stray lines.** Error
  diffusion on smooth content could settle into a repeating pattern instead of
  a random one, printing a herringbone texture across flat areas and, in
  gradients, drawing a clean solid line straight through the picture. Both are
  the kind of artifact the eye picks out immediately. Every dithering
  algorithm's blue-noise setting has been retuned to break the pattern up.
  Atkinson was the worst affected — it shipped with the noise turned off
  entirely — so screens using it change the most. Colour accuracy improves
  slightly at the same time, and fine detail is unaffected: thin strokes,
  small text and hard edges render exactly as crisply as before.
- **The reTerminal E1002 and E1004 panel presets no longer pin a noise value.**
  Both fixed it at a setting that measurement shows is past the useful point
  for the algorithm they set it on, so they now follow the tuned default. If
  you had copied either preset into your own `config.yaml`, delete the
  `dither: sierra-lite: noise_scale:` lines to pick this up.
- **The git client used to fetch screen repositories was updated** (`gix` 0.66 →
  0.86), picking up security fixes in git's object, pack and ref handling. This
  matters if you fetch screen repositories from sources you don't fully control.
- **HTTPS connections now trust your system's certificate store.** The HTTP
  client was updated (`reqwest` 0.12 → 0.13), and with it byonk switched from a
  bundled copy of the Mozilla root list to your platform's own trust store. If
  your network uses an internal or corporate certificate authority that your
  machine already trusts, `http_get()` in Lua scripts now works against it
  without passing `ca_cert`. The official container image is unaffected — it
  ships the same root certificates as before.
- **Home Assistant "add-ons" are now called "apps".** Home Assistant 2026.2 renamed
  add-ons to apps in its UI (**Settings → Apps → App store**). Byonk's Home Assistant
  documentation, the integration's messages, and the Supervisor repository name now
  use the new wording. Nothing functional changed — it's the same add-on/app.
- **Built-in screens are now a minimal, read-only set** (default + calibration);
  your own screens are no longer mixed into the `byonk-builtin` handle. Existing
  installs are migrated automatically on first start.
- Updated the SVG renderer to resvg 0.48.1, which brings a faster and more correct
  text engine. Text positioning and glyph advances are more accurate, so some screens
  may shift by a pixel or two.
- **Byonk now hints text for you, and screens no longer need to ask.** Which
  treatment a screen gets is chosen per render from the panel it is being drawn
  for: mono hinting with 1-bit glyphs on a black-and-white panel, smooth
  anti-aliased hinting once there are greys. Including
  `byonk-base-v1/hinting.svg` is no longer how this happens, and a screen that
  includes it is unaffected either way — the include is now inert and can be
  deleted. **The `-resvg-hinting-*` CSS properties no longer exist**, so a
  screen that set them directly must move that configuration into the new
  `font_hinting` directive; until it does, it silently gets byonk's own choice
  instead of what it asked for. The bundled screens no longer include the
  partial, and render identically without it.

### Fixed

- **`byonk render` could never render a screen that fetches anything.** Any
  screen calling `http_get` — the bundled `webscrape` and
  `swiss-departure-board` examples among them — failed on the command line with
  a tokio panic about dropping a runtime, then a confusing complaint about a
  missing `data.width`. The CLI drove the whole render from inside its async
  runtime, where the blocking HTTP client may not be used, while every other
  caller happened to step off it first. The request now always runs on a thread
  of its own, so a script can fetch no matter how byonk was started.
- **Two example screens hid the reason they failed.** `webscrape` and
  `swiss-departure-board` returned a half-filled data table when a fetch
  failed, holding an error message their templates never displayed — so any
  fetch failure ended in `Variable 'data.width' not found` rather than in
  anything a reader could act on. Both now report what went wrong and stop,
  which puts the message on the device's error screen and on the command line.
- **The font hinting demo showed nine cells that were meant to differ and
  didn't.** Its stylesheet set `font-family` on a bare `text` rule, and in SVG
  a CSS rule outranks a presentation attribute — so it overrode the
  `font-family` on every cell and the hinting variants were never selected.
  Its grid was also laid out on fractional pixel positions, which slides a
  hinted glyph back off the pixel grid it was just fitted to and costs 3-5% of
  the ink to dropped stems. Both are fixed, and the demo now draws its
  mono-hinted column 1-bit, so the treatments are plainly different.
- **The Terminus TTF font demo never actually showed Terminus.** The example
  wrote its family name into the template unquoted, and `Terminus (TTF)` is not
  a valid unquoted CSS family, so every line of the demo silently fell back to a
  serif — the one thing the demo exists to show. Both font demos now quote the
  family they interpolate. This is the same trap as quoting `Source Sans 3`: if
  a template writes a family name it got from a script, quote it.
- **Bitmap fonts render correctly.** The 26 bundled X11 bitmap faces, and Terminus,
  lost chunks out of their glyphs at every size and were unevenly spaced. Byonk was
  positioning them using the metrics of the font's outline rather than the metrics
  the pixel design carries for that exact size — a pixel font is drawn afresh per
  size, so the two only agree at one of them. Every glyph after the first therefore
  landed between pixels, where a picture of a pixel grid cannot be reproduced, and
  the renderer smeared its edges. Glyphs now sit on whole pixels by construction and
  keep their designed spacing, and a strike is also snapped to the pixel grid when a
  screen itself places text at a fractional position. Terminus at 14 px and 18 px is
  now 1 px per glyph wider, which is its real design: byonk was crowding it.
- **The X11 bitmap fonts are spaced as their designers drew them.** The 26 bundled
  X11 faces were converted from their originals in a way that threw the original
  pixel advances away and recomputed them from an autotraced outline, so 16 of 28
  fixed-pitch strikes rendered at the wrong pitch. `X11Misc7x` at 13 px was a pixel
  too narrow and its glyphs welded into a bar; `X11Misc9x` Bold at 18 px was five
  pixels too wide and read as if letter-spaced. Every face has been rebuilt from the
  original X.Org sources and now renders at exactly the pitch those sources declare.
  The rebuilt fonts are also bitmap-only, which halves their size — the bundle drops
  from 8.7 MB to 4.9 MB. One consequence worth knowing: a bitmap face asked for at a
  size it has no strike for now scales its nearest strike instead of falling back to
  a blurry traced outline. It stays the same typeface at the right width, but it is
  blocky. `fonts/FONTS.md` lists the sizes each family actually has.
- `X11Term` gained the plain ASCII apostrophe and backtick, which it never had: the
  previous conversion moved them to the typographic quotes at U+2018 and U+2019,
  leaving `'` and `` ` `` blank. It no longer has U+0152, U+0153, U+0178, U+2018,
  U+2019 or U+2212, none of which are in the ISO 8859-1 range the face is drawn for.
- **Using a bundled header, footer or status bar no longer kills the server.** A screen
  that included one of the ready-made `byonk-base-v1` components — exactly as their own
  usage notes describe — crashed byonk outright, taking every other screen down with it.
  Each of those three files carried its usage example inside an HTML comment, which the
  template engine does not treat as a comment, so the example told the file to include
  itself without end. The examples are now written as template comments and the components
  work as documented. The status bar had a second fault on top of that: it refused to
  render at all unless the screen supplied a WiFi state, although its notes call that
  optional. It now draws whichever indicators it was given and omits the rest. All three
  also show how to pass values in, which was never written down.
- **The bundled header and status bar no longer draw on top of each other.** A screen using
  both got the header's timestamp and the status icons stacked in the same top-right corner,
  and the icons were dark grey on the header's black bar, so they were barely visible. The
  status icons now own that corner and default to light ink; the header no longer draws a
  timestamp, which removes a duplicate, since `footer.svg` already prints one. To put the
  icons somewhere else, set `status_y` and the new `status_color`. **If you include
  `header.svg` and relied on it showing `updated_at`, include `footer.svg` as well.**
- **The grey calibration screen is readable on panels with many grey levels.** It laid
  every swatch out in a single row, so on a 16-grey panel each `#RRGGBB` label got a
  sixteenth of the width — about a third of what the text needs. The labels ran together
  into an unreadable smear, the registration circles overlapped, and the leftmost one hung
  off the edge of the screen. Swatches now wrap into a grid once there are more than eight
  of them (4x4 at sixteen levels, 3x3 at nine), and the labels and registration marks are
  sized and placed from the swatch rather than from the panel. The swatch block also grows
  to fill the space a tall panel used to leave empty. Palettes of eight or fewer keep the
  single row they already fit. Each swatch is now labelled once rather than printing its
  hex value twice.
- **`byonk render` now draws at the device's own panel size.** It used to take the
  output size only from `--device`, which offers just two values — `og` (800x480) and
  `x` (1872x1404). So a device configured for any other panel rendered at 800x480
  while correctly using that panel's *colours*: the reTerminal E1004 (1200x1600), the
  three 296x128 Xiao panels, and any panel you define yourself all came out the wrong
  size, in a picture that otherwise looked right. `--device` is now only the fallback
  for a device that has no `panel:` set. **If you scripted around the old behaviour by
  passing `--device x` to get a large render, drop it** — the panel decides now.
- **A device configured for a screen that does not exist is now an error.** A typo in a
  device's `screen:` silently rendered the DEFAULT screen instead and reported success, so
  the device showed a plausible-looking picture that was not the screen you asked for. Byonk
  now says which device names which missing screen — on the command line as a failure, and
  on the device as an on-screen message. Devices that have no configuration of their own
  still fall back to the DEFAULT screen, which is what that fallback is for.
- **A screen can no longer take the server down with a looping template.** A template that
  includes itself, two that include each other, or a macro that calls itself used to abort
  the whole process. Byonk now refuses such a screen with an ordinary error that names the
  loop, so the screen fails and everything else keeps running. Very deeply nested includes
  are refused the same way.
- **The generic font families work, and now get real text faces.** Text set in
  `sans-serif`, `serif`, `monospace`, `cursive` or `fantasy` used to resolve only
  to fonts byonk does not ship, so on the released container image — which
  contains no system fonts — such text was silently dropped and the screen came
  out blank. Byonk now bundles Source Sans 3, Source Serif 4 and Source Code Pro
  and points the three main generics at them, so a screen asking for a serif gets
  a serif and one asking for a monospace gets a monospace at any size. `cursive`
  and `fantasy` resolve to Outfit, and every screen that names Outfit is
  unchanged. **If you write one of these families by name, quote it**:
  `font-family="'Source Sans 3'"`. A name ending in a digit is not valid unquoted
  CSS and falls back without warning.
- **Automatic-fallback hinting now actually falls back.** Text set with the
  automatic-fallback engine came out unhinted on most fonts, including the bundled
  Outfit. The engine is meant to use a font's own hinting where it has some and the
  automatic hinter where it does not, but it treated the small preamble almost every
  modern font carries as real hinting, chose the font's own — which had nothing to
  apply — and left the outlines untouched. Such fonts now get the automatic hinter,
  as intended.
- **Text on black-and-white panels is crisp instead of speckled.** Small type used
  to come out with a fuzzy halo of stray dots around every letter. Screens on such
  panels now get sharp, solid glyphs with no extra work; a screen that wants
  smoother text for a particular element can ask with
  `text-rendering="optimizeLegibility"`, and panels with grey levels are unchanged.
- **Saturated colours are no longer rendered flat.** The dithering error cap
  (`error_clamp`) used to limit the resulting pixel value rather than the error
  itself, so how much error could accumulate depended on how close a colour
  already sat to full intensity. Saturated colours sit at that limit by
  definition, so they were starved of the very thing that lets the ditherer mix
  inks — the same colour won every pixel and whole areas came out as one flat
  block. On a 6-colour panel, 16 of 24 hues were affected. Muted colours got
  markedly more accurate too.

  If you set `error_clamp` in `config.yaml` or a script, its meaning has
  changed: it is now a cap on accumulated error, and the useful range is around
  1.0 rather than around 0.1. Old values will look flat. Remove the setting to
  take the new default.
- **The colour calibrator's patches show the panel's inks again** — each patch is
  one flat block of a single ink, so you can judge the ink itself instead of a
  dither pattern. The label still shows the official value you write in a screen.
- **Gradients no longer get a hard seam where they cross a palette colour.**
  Byonk detects pixels whose value exactly matches one of the panel's colours
  and pins them to that ink, discarding their dithering error, to keep text and
  logos crisp. That works for a deliberate flat fill but not for a gradient,
  where a pixel's value matches only by coincidence — the pinned pixels showed
  up as a visible stripe across an otherwise smooth ramp.

  Marking a gradient or photograph as continuous-tone (see *Continuous-tone
  content is now marked* above) switches pinning off across it, which removes
  the seam. Structure — text, logos, flat fills — is still pinned, and that is
  what keeps it crisp.

  **Breaking:** the `preserve_exact` key in a screen's Lua return value is
  removed. Scripts setting it should drop it; marking your continuous-tone
  content replaces it. Setting it now has no effect.
- **`sierra-light` now selects Sierra Lite instead of silently falling back to
  Atkinson.** The misspelling was listed by the admin API as if it were its own
  algorithm and shipped in the panel presets, but nothing understood it: any
  device configured with it was rendered with Atkinson and also lost its
  per-algorithm panel tuning, with nothing in the output to say so. The name is
  now an accepted alias for `sierra-lite`, and the effective algorithm is
  canonicalised once so the renderer and the tuning lookup cannot disagree. If
  you had a device on `sierra-light`, it will now genuinely render with Sierra
  Lite — which will look different.
- **Screens no longer show a bogus "requires byonk 0.15 but this engine is
  0.17.1" warning.** Every bundled screen still declared compatibility with an
  old engine series, so the screen list warned about all of them — including the
  built-in default screen. The bundled screens now declare the current series,
  and newly created screens inherit the running version instead of a fixed one.
- **Your own screens are no longer listed twice.** A screen in your screens
  directory appeared both under `local` and under `byonk-builtin`; it now
  appears only under `local`, where it belongs.
- **Reserved repository handles are rejected in the Home Assistant app
  options.** Naming a screen-repo row `local`, `examples`, or `byonk-builtin`
  used to silently make the screens in that repository unreachable. Such rows
  are now ignored with a warning in the log.
- Screen repositories no longer follow symbolic links that point outside the
  repository, so a screen repo cannot expose files elsewhere on the server.
- Validating a screen now reports an oversized or non-UTF-8 file for what it is,
  instead of misreporting it as a missing file or a Lua syntax error.
- Measured panel colours are no longer discarded without explanation when a
  screen's palette and its measured colours disagree in length; the mismatch is
  now reported.

## 0.17.1 - 2026-07-17

### New

- **The Home Assistant integration now ships its own icon and logo.** On Home
  Assistant 2026.3 and later the Byonk integration shows its proper brand icon
  (bundled in the integration), instead of the generic placeholder.
### Changed

### Fixed

## 0.17.0 - 2026-07-17

### New

- **Failing screen repos are now visible in Home Assistant.** When Byonk cannot
  fetch a screen repo, the integration raises a Home Assistant *Repair* issue
  showing the repo and the actual error, and clears it automatically once the
  repo updates successfully. Previously the error was only visible as an
  attribute on a diagnostic sensor.
### Changed

- **"Packages" are now called "screen repos".** A screen repo is a git
  repository of screens, so the clearer name is used everywhere: the admin API
  (`/api/admin/screen-repos`), the `config.yaml` keys (`screen_repos:`,
  `screen_repo_refresh_interval:`), the Home Assistant add-on options, and the
  integration UI (the "Update screen repos" button and per-repo status sensors).
  Existing add-on options set under `packages` / `package_refresh_interval` must
  be re-entered under the new `screen_repos` / `screen_repo_refresh_interval`
  keys.

### Fixed

- **Screen packages could never be fetched from the published container image.**
  The release image is built `FROM scratch` and has no `/tmp` directory, so the
  intermediate git clone (which used the system temp dir) failed with an opaque
  `Could not open data at '/tmp/…'` and every package showed status `error`. The
  clone now runs beside the package cache under the persistent data directory, so
  fetching works in the container. Fetch failures are also written to the log now,
  instead of only appearing in the per-package status.

## 0.16.0 - 2026-07-17

### New

- **Home Assistant integration** (`custom_components/byonk/`): run Byonk from Home
  Assistant with a zero-touch, Supervised/HAOS-only setup — it installs and starts the
  Byonk add-on for you, provisions the add-on's admin token with no user input, and
  keeps it in sync (re-provisioning automatically if it becomes invalid). New TRMNL
  devices appear as native Home Assistant **Discovered** cards; configuring one creates
  a per-device entry and writes its screen mapping to Byonk — Home Assistant is the
  source of truth. Each device exposes battery, signal, last-seen, firmware, and model
  sensors; screen, dither, panel, and refresh-interval controls; and a live control for
  every parameter of its current screen (text, switch, or select, applied instantly).
  Renaming a device in Home Assistant mirrors the name down to Byonk. A *Byonk Server*
  hub device carries a registration switch, an "Update packages" button, and per-package
  status sensors. Install through HACS as a custom repository (and via the default-store
  search once accepted). See *Getting Started → Home Assistant Integration*.
- **Home Assistant add-on**: run Byonk as a Home Assistant Supervisor add-on (references
  the prebuilt `ghcr.io/oetiker/byonk` image) with persistent, editable
  config/screens/fonts and a host port for TRMNL devices. The add-on's Configuration tab
  is the source of truth for Byonk's global settings (`auth_mode`,
  `package_refresh_interval`) and the screen-package registry; the integration shows
  these read-only. See *Getting Started → Home Assistant Add-on*.
- **Admin/management API** (`/api/admin/*`), gated by a bearer token
  (`BYONK_ADMIN_TOKEN` env or `admin.token` in config; disabled = returns 404): read
  device telemetry, pending/unregistered devices, effective config, screen lists
  (grouped by package), and screen parameter schemas; create/update/delete device
  mappings (including the reserved `DEFAULT` device); register/update/remove screen
  packages and trigger re-fetches; and update global settings. Admin writes patch
  `config.yaml` in place — preserving comments and formatting — and take effect without
  a restart.
- **Screen packages**: screens are now distributed as packages instead of loose files.
  A package is a directory tree with a `byonk-screens.yaml` manifest; every folder
  containing a `meta.yaml` is a screen, made of `meta.yaml` (title, description, `byonk:`
  engine compatibility, default `refresh:`, and a `params:` schema with UI hints),
  `script.lua`, and `screen.svg`. Screens are referenced by a qualified `handle/path`
  ref; the bundled screens ship in the embedded `byonk-builtin` package, and shared SVG
  comes from the versioned `byonk-base-v1` standard library. A `packages:` registry maps
  handles to git sources (`{ repo, pin, token? }`): a full-sha pin is immutable and
  cached forever, while a tag/branch pin is re-fetched on demand or every
  `package_refresh_interval` seconds; a failed refresh keeps serving the cached checkout
  rather than taking the package offline. Package tokens are never exposed in API
  responses.
- **Mandelbrot screen** (`mandelbrot`): renders a random, aesthetically-pleasing region
  of the Mandelbrot set on each refresh, chosen from curated locations (Seahorse Valley,
  Elephant Valley, …), with escape-time computed in Lua and the gradient built from the
  panel's own palette (greyscale panels get a black→white ramp, colour panels a natural
  through-the-palette ramp).
- **reTerminal E1004 panel profile** (`reterminal_e1004`): 1200×1600, 6-color Spectra 6
  palette. Added to the bundled `config.yaml`.
### Changed

- **Breaking — screens are now packages.** The old flat `<name>.lua` + `<name>.svg`
  screens, the `screens:` config block, and the `@params` Lua-comment schema are gone (a
  clean break — no legacy reader). Existing screens must be migrated to the package
  format, and every device's `screen` value must be a qualified `handle/path` ref (e.g.
  `byonk-builtin/example/hello`). See *Screen Packages*.
- **Reserved `DEFAULT` device replaces `default_screen` and `registration.screen`.** The
  screen assigned to `devices.DEFAULT` is shown by every un-onboarded or unassigned
  device, and the built-in default screen renders the pairing code for new devices. (In
  Home Assistant this is the **Byonk Default** device's screen-select.)
- **Google Photos screen now displays photos in album order** instead of picking a random
  one each refresh. The position advances by one per refresh interval and is derived
  statelessly from the clock, so it survives restarts and wraps at the end of the album.
- **Panel auto-detection now also matches the `Model` header**, not just `Board`, so
  panels that report their identity in `Model` (e.g. the reTerminal E1004) auto-detect
  correctly instead of falling back to greyscale.
- **Reported device model** is now the verbatim `Model` header the device sends, instead
  of being collapsed to `og`/`x`. Genuine TRMNL OG/X devices are unaffected.
- The **"device not registered" log line** now includes the device's `Board`, `Model`,
  and `Colors` headers and the resolved `width`/`height`, so you can author a matching
  panel profile straight from the log.

### Fixed

- **`gphoto` screen**: `album_url` is no longer marked `required`, so the screen can be
  selected (and used as the default) before an album URL is configured — it shows its
  registration code until one is set.

## 0.15.0 - 2026-04-28

### New

- Added **dither strength** parameter (`strength`) for error diffusion. Scales the diffused error before propagation: `0.0` = no diffusion (pure nearest-color), `0.5` = subtle, `1.0` = standard (default, backward compatible), `>1.0` = exaggerated. Available in config.yaml (`strength:`), Lua scripts (`device.dither.strength`), dev mode UI, and the eink-dither crate API.
- Added **Atkinson Hybrid** dithering algorithm (`"atkinson-hybrid"`). Uses the same 6-neighbor Atkinson kernel but with hybrid error propagation: 100% for the achromatic (brightness) component and 75% for the chromatic (color deviation) component. Fixes Atkinson's color drift on chromatic palettes while preserving its distinctive high-contrast character.
### Changed

### Fixed

## 0.14.0 - 2026-02-14

### New

- Added **Stucki** and **Burkes** dithering algorithms.
### Changed

- **Simplified dither system**: removed graphics/photo intent split and blue-noise/simplex ordered dithering. All algorithms are now error diffusion with configurable blue noise jitter (`noise_scale`). Plain and noise variants are unified — set `noise_scale: 0` for no jitter.
- **Default dither algorithm** is now `atkinson` (was `graphics` blue-noise ordered dithering).
- **Grey palette auto-detection**: greyscale palettes (R=G=B for all colors) automatically use `error_clamp: 0.6` for better tonal range, unless explicitly overridden.
- Removed `"graphics"`, `"photo"`, `"blue-noise"`, and `"simplex"` dither aliases. Use algorithm names directly: `"atkinson"`, `"floyd-steinberg"`, etc.

### Fixed

## 0.13.0 - 2026-02-09

### New

- **Perceptual dithering engine**: Vendored [eink-dither](crates/eink-dither/) crate with Oklab color matching, gamma-correct linear RGB processing, and automatic distance metric selection (HyAB+chroma for color palettes, Euclidean OKLab for greyscale). Eight dithering algorithms: `blue-noise` (default), `atkinson`/`photo`, `floyd-steinberg`, `jarvis-judice-ninke`/`jjn`, `sierra`, `sierra-two-row`, `sierra-lite`, `simplex`. All error diffusion algorithms use blue noise kernel jitter to break "worm" artifacts.
- **Panel profiles**: Define display panels in `config.yaml` with official and measured (actual) colors, per-algorithm dither tuning defaults, and auto-detection from firmware `Board` header. Panels can also be assigned per-device. Measured colors let the dithering engine model what the panel really shows. `Measured-Colors` firmware header can override panel profile.
- **Dither tuning**: Fine-tune `error_clamp`, `noise_scale`, and `chroma_clamp` per-panel, per-device, per-script, or via dev UI. Scripts can read pre-resolved tuning via `device.dither` table. Priority chain: dev UI > script > device config > panel defaults > algorithm defaults.
- **Dev mode overhaul**: Dither algorithm dropdown, tuning controls (serpentine, exact absorb, error clamp, noise scale, chroma clamp), panel preview with measured colors, click-to-tune HSL color popup, always-visible console. Dimensions and colors derive from panel profile. All changes auto-refresh. Dev overrides propagate to physical device renders.
- **`preserve_exact` option**: Disable exact color match preservation via Lua (`preserve_exact = false`) or dev UI — forces all pixels through enhancement + dithering.
- **Google Photos screen** (`gphoto`): Display random photos from a shared Google Photos album (HTML scraping, no OAuth).
- **Terminus font demo** (`fontdemo-terminus`): Showcases all 9 embedded bitmap sizes (12–32px) in regular, bold, italic, and bold-italic.
- **`fonts` global in Lua**: Discover available font families, styles, weights, and bitmap strike sizes at runtime.
### Changed

- **PNG output ~27% smaller**: oxipng post-processing with zopfli compression and adaptive filter selection.
- **Config errors are fatal**: `config.yaml` parse errors now abort startup with a clear message instead of silently falling back to defaults.
- **Documentation reorganized**: Consolidated duplicated sections, added panels, dither algorithms, dither tuning, and display calibration docs.

### Fixed

- Exact-match pixels (text, lines, borders) now absorb accumulated dither error, preventing artifacts from bleeding across hard boundaries.
- Device config entries work with auto-discovered screens (`.lua`+`.svg` files not listed in `config.yaml`).
- Dev mode clipboard copy works over plain HTTP.

## 0.11.0 - 2026-02-01

### New

- Added `layout.*` namespace to SVG template context — provides `layout.width`, `layout.height`, `layout.scale`, `layout.grey_count`, and other pre-computed layout values directly in templates without needing Lua to pass them through
- Reusable `components/hinting.svg` include — adaptive font hinting that switches between mono and smooth based on `layout.grey_count`. All built-in screens use it via `{% include "components/hinting.svg" %}`
- Added `layout.grey_count` to Lua API — counts palette colors where R=G=B, useful for conditional font hinting in SVG templates
- X11 bitmap fonts: 26 TTF files with embedded bitmap strikes and autotraced scalable outlines. Proportional families (X11Helv, X11LuSans, X11LuType, X11Term) and fixed-width families grouped by cell width (X11Misc5x–X11Misc12x). Use `font-family="X11Helv" font-size="14"` — the renderer selects the matching bitmap strike automatically.
- Hinting demo screen (`hintdemo`): 9-cell grid comparing hinting engines (auto, native, none) × targets (mono, normal, light)
- Dev mode screen selector now shows configured device IDs under their screens
### Changed

- Dev mode: removed separate Device ID input field, merged into screen selector dropdown
- Device lookup now supports case-sensitive IDs (e.g., `X11Helv`) in addition to MAC addresses

### Fixed

- Dev mode now auto-discovers new screens from the filesystem without requiring a server restart
- Dev mode: fix YAML-to-JSON param conversion that silently dropped string device parameters
- Bitmap font spacing: update resvg to use bitmap strike advances instead of hmtx, fixing character overlap in proportional bitmap fonts
- **HTTP binary responses**: `http_get`/`http_request` now correctly handle binary response data (e.g., images) instead of forcing UTF-8 text decoding, enabling `base64_encode()` to work with fetched images (#3)

## 0.10.0 - 2026-01-30

### New

- **Color Palette Override**: Display colors can now be set per-device in `config.yaml` or per-script via Lua return value
  - Priority chain: Lua script `colors` > device config `colors` > firmware `Colors` header > system default (`#000000,#555555,#AAAAAA,#FFFFFF`)
  - Device config: add `colors: "#000000,#FFFFFF,#FF0000"` to any device entry in `config.yaml`
  - Script return: add `colors = { "#000000", "#FFFFFF", "#FF0000" }` to the Lua return table
- **Device Registration System**: Optional security feature to require explicit approval of new devices before they can display content
  - Enable with `registration.enabled: true` in config.yaml (enabled by default)
  - New devices show a 10-character registration code on screen
  - Registration codes are derived from a SHA256 hash of the device's API key (deterministic)
  - Works with any API key format (TRMNL-issued, custom, etc.) - no WiFi reset required
  - Add the code to `config.devices` section using hyphenated format (e.g., `"ABCDE-FGHJK"`)
  - Registration codes can be used interchangeably with MAC addresses for device identification
  - Registration code available as `device.registration_code` and `device.registration_code_hyphenated` in Lua and templates
  - Optionally set `registration.screen` to use a dedicated screen instead of the default
- **CLI render command**: Added `--colors` option to override display palette and `--registration-code` now simulates enrollment (shows registration screen for unregistered devices)
- **Ed25519 Authentication**: Optional cryptographic device authentication using Ed25519 signatures
  - New `/api/time` endpoint returns server timestamp for signature generation
  - Devices sign `timestamp_ms || public_key` and send `X-Public-Key`, `X-Signature`, `X-Timestamp` headers
  - Server verifies signature and checks timestamp is within ±60 seconds
  - `/api/display` accepts both Ed25519 and API key authentication (dual-accept)
  - Set `auth_mode: ed25519` in config.yaml to advertise Ed25519 mode to devices via `/api/setup`
- **Palette-Aware Color Support**: Full support for color and multi-grey e-ink displays
  - Parse `Colors` and `Board` HTTP headers from firmware for display palette detection
  - Palette-aware blue-noise-modulated Floyd-Steinberg dithering in RGB space
  - BT.709 luminance pre-conversion for greyscale palettes (perceptually correct grey mapping)
  - Smart PNG format: greyscale palettes produce native greyscale PNG; color palettes produce indexed PNG with PLTE
  - Palette exposed to Lua as `layout.colors`, `layout.color_count`, `device.colors`, `device.board`
  - Supports arbitrary palettes: 2-color, 3-color, 4-grey, 6-color, 16-grey, etc.
- **Dev UI Improvements**:
  - Device model dropdown with all known boards and their color palettes
  - Colors input field replaces grey levels selector
  - Device ID field accepts both MAC addresses and registration codes
  - 3D device bezel frame with embossed "BYONK — {device name}" label
  - Magnification lens on hover for all screen models (was X-only)
  - Sunken display effect with inset shadow
### Changed

- `DisplaySpec::from_dimensions` now uses exact requested dimensions instead of snapping to OG/X presets
- `layout.grey_levels` replaced by `layout.colors` and `layout.color_count` in Lua API
- OG display max PNG size increased from 90KB to 200KB to accommodate color indexed PNGs
- Default and graytest screens use luminance-based text contrast for palette swatches
- Small screen (< 400px width) adaptations: doubled title/tagline/footer sizes

### Fixed

- i16 overflow in dither error diffusion (now uses i32 with clamping)
- graytest.svg missing `value` field in bar data

## 0.9.0 - 2026-01-19

### New

- Layout helpers for responsive screen development:
  - Added `layout` global table with pre-computed responsive values (`width`, `height`, `scale`, `center_x`, `center_y`, `grey_levels`, `margin`, `margin_sm`, `margin_lg`)
  - Added `scale_font(value)` helper for scaling font sizes (returns float to preserve precision)
  - Added `scale_pixel(value)` helper for scaling pixel values (returns floored integer for pixel alignment)
  - Added `greys(levels)` helper for generating grey palettes matching device capabilities
- Dev mode: Run `byonk dev` to start server with live reload and device simulator
  - Web-based device simulator at `/dev` showing rendered screens
  - Select any screen from config dropdown
  - Switch between device models (OG 800x480, X 1872x1404) or set custom dimensions
  - Custom parameters JSON editor for testing Lua scripts
  - Live reload: screens automatically re-render when Lua/SVG files change (requires SCREENS_DIR)
  - Error display: Lua and template errors shown in UI
  - MAC address resolution: enter a device MAC to auto-load its configured screen and params
  - Device simulation: battery voltage, RSSI, and timestamp override for testing
  - Grey level selector: test screens with 4-level (OG) or 16-level (X) dithering
  - Pixel inspector lens: hover over rendered image to see magnified view
- TRMNL X 16-grey-level support:
  - Configurable grey levels per device (4 for OG, 16 for X)
  - 4-bit PNG output for 16-level displays
  - Device context includes `grey_levels` field
### Changed

- Updated all example screens (default, hello, transit, floerli, graytest) to use new layout helpers
- Dithering now preserves pixels at exact grey levels (solid colors not dithered), improving UI element quality

### Fixed

## 0.8.2 - 2026-01-16

### New

- Versioned documentation: Documentation now tracks releases with version selector dropdown
- Documentation shows latest stable release by default, with option to switch to dev or older versions
- Dev documentation includes warning banner indicating unreleased content
- Backfill support: Run docs workflow with "backfill" option to rebuild docs for past releases
### Changed

### Fixed

## 0.8.1 - 2026-01-16

## 0.8.0 - 2026-01-16

### New

- Template inheritance: Use `{% extends "layouts/base.svg" %}` to create reusable base layouts with overridable blocks
- Template includes: Use `{% include "components/header.svg" %}` to embed reusable SVG components
- Built-in layout and components: `layouts/base.svg`, `components/header.svg`, `components/footer.svg`, `components/status_bar.svg`
- Lua `url_encode(string)` function: URL-encode strings for safe use in URLs
- Lua `url_decode(string)` function: Decode URL-encoded strings
- HTTP response caching: New `cache_ttl` option for `http_request`/`http_get` to cache responses (LRU cache with max 100 entries)
- Request tracing: Each HTTP request now gets a unique request ID for log correlation
- Header parsing utilities: Internal `HeaderMapExt` trait for cleaner API handler code
### Changed

- Content cache now uses synchronous `std::sync::RwLock` instead of `tokio::sync::RwLock` to avoid nested runtime blocking when called from `spawn_blocking` contexts
- Simplified header parsing in API handlers using new `HeaderMapExt` utilities

### Fixed

- Fixed unbounded cache growth: content cache now uses LRU eviction with max 100 entries to prevent memory leaks in long-running deployments
- Fixed nested runtime blocking in display handler: removed `block_on()` calls inside `spawn_blocking()` which could cause deadlocks under load
- Fixed regex recompilation: image href regex in template service is now compiled once using `OnceLock` instead of on every render call
- Fixed trailing slash compatibility: API endpoints now accept URLs with or without trailing slashes (e.g., `/api/setup/`) for TRMNL firmware 1.6.9+ compatibility
- Removed unused `handle_display_json` function (dead code cleanup)

## 0.7.1 - 2026-01-14

### New

- Integration tests verifying actual TCP connection closure behavior
### Changed

- Refactored server setup into shared `server` module used by both production and tests

### Fixed

- Disabled HTTP keep-alive to prevent connection accumulation from ESP32 clients that request keep-alive but never reuse connections ([firmware PR #274](https://github.com/usetrmnl/trmnl-firmware/pull/274))

## 0.7.0 - 2026-01-13

### New

- New `http_request()` function with full HTTP method control (GET, POST, PUT, DELETE, PATCH, HEAD) and comprehensive options
- New `http_post()` convenience function for POST requests with `body` or `json` options
- `http_get()` and `http_request()` now support: `params` (auto URL-encoded query parameters), `headers`, `body`, `json` (auto-serialized with Content-Type), `basic_auth`, `timeout`, `follow_redirects`, `max_redirects`, and `danger_accept_invalid_certs` (for self-signed/expired certificates)
- TLS certificate options for HTTP functions: `ca_cert` (custom CA for server verification), `client_cert` and `client_key` (mTLS client authentication)
- Comprehensive test suite with 194 tests covering API endpoints, Lua functions, error paths, TLS/HTTPS scenarios, unit tests, and end-to-end flows
- Coverage reporting via `make coverage`, `make coverage-text`, and `make coverage-ci` (lcov format for CI)
- Mock HTTP server infrastructure for testing Lua HTTP functions without external requests
- Mock HTTPS server infrastructure for testing TLS certificate handling (self-signed certs, custom CA, mTLS)
- Unit tests for core modules: config parsing, display specs, error handling, template rendering, asset loading

## 0.6.1 - 2026-01-11

### Changed

- **Breaking:** `qr_svg()` now uses margin-based positioning (`top`, `left`, `right`, `bottom`) instead of absolute `x`, `y` coordinates. Screen dimensions are automatically read from device context.

## 0.6.0 - 2026-01-08

### New

- Lua `qr_svg()` function: Generate pixel-aligned QR codes with anchor-based positioning for embedding in SVG templates. Supports `anchor` option ("top-left", "top-right", "bottom-left", "bottom-right", "center") so you don't need to calculate QR code size for positioning.
### Changed

- Hello screen now includes QR code example demonstrating the `qr_svg()` function with anchor-based positioning
- Documentation: Moved "Understanding the Result" section after QR code instructions in first-screen tutorial
- Documentation: Docker commands now include `--pull always` to ensure users get the latest container image

### Fixed

- Hello tutorial screen now uses bundled Outfit font with explicit weights to render correctly on systems without sans-serif fonts (fixes blank screenshot in docs)

## 0.5.3 - 2026-01-05

### Changed

- Simplified image URLs: removed `?w=...&h=...` query parameters, dimensions are now stored in cache
- Tutorial now includes Step 0 explaining how to set up a workspace with environment variables
### Fixed

- CLI help no longer claims `serve` is the default command (status display is the default)
- Docker container now defaults to `serve` command, so `docker run` starts the server as expected
- Documentation: updated architecture docs to reflect content hash URLs (removed outdated signed URL references)
- Documentation: removed obsolete `w` and `h` query parameters from HTTP API docs

## 0.5.1 - 2026-01-01

### New

- Extended device context: `device.model`, `device.firmware_version`, `device.width`, and `device.height` now available in Lua scripts and SVG templates
- CLI render options: `--battery`, `--rssi`, and `--firmware` flags for testing templates with device data
### Changed

### Fixed

## 0.5.0 - 2026-01-01

### Changed

- Redesigned default screen: cleaner layout with full-bleed background image, large centered BYONK logo with drop shadow, vertical grayscale swatches on left edge, vertical gradient bar on right edge, and compact info bar at bottom
### New

- Embedded assets: All screens, fonts, and config are now embedded in the binary using rust-embed
- Zero-config operation: Binary works standalone without any external files
- `byonk init` command: Extract embedded assets to filesystem for customization (`--screens`, `--fonts`, `--config`, `--all`, `--list`)
- Auto-seeding: When `SCREENS_DIR`, `FONTS_DIR`, or `CONFIG_FILE` env vars are set and the path is empty/missing, embedded assets are automatically extracted there
- Merge behavior: External files override embedded assets, with fallback to embedded for missing files
- New `FONTS_DIR` environment variable for configurable font directory
- Lua `base64_encode(data)` function: Encode binary data to base64 strings
- Lua `read_asset(path)` function: Read files from screen asset directories (`screens/<screen>/`)
- Automatic SVG image resolution: `<image href="logo.png"/>` in templates automatically resolves to `screens/<screen>/logo.png` and embeds as data URI
- Default command shows status: Running `byonk` without arguments displays environment variables and asset sources instead of starting the server
- Simplified image URLs: Use content hash in path (`/api/image/{hash}.png`) instead of signed URLs with expiration

### Changed

- PNG compression improved: uses maximum compression with Paeth filter for smaller file sizes

### Removed

- `URL_SECRET` environment variable and URL signing (replaced by content hash-based URLs)

### Fixed

## 0.4.1 - 2025-12-30

### New

- Content change detection: `/api/display` now returns a content-based hash as the `filename`, allowing TRMNL devices to detect when screen content has actually changed
### Changed

- SVG template rendering now happens during `/api/display` instead of `/api/image`, enabling the content hash to be computed before the device fetches the image
- Content cache now stores pre-rendered SVG instead of raw script data, making `/api/image` faster
- PNG compression improved: uses maximum compression with Paeth filter for smaller file sizes

### Removed

- `URL_SECRET` environment variable and URL signing (replaced by content hash-based URLs)

### Fixed

## 0.4.0 - 2025-12-30

### Changed

- Improved dithering quality: switched to blue-noise-modulated error diffusion with serpentine scanning, reducing visible "worm" artifacts while preserving sharp edges for UI content
### Fixed

## 0.3.2 - 2025-12-30

### Fixed

- Docker image now works: added Cross.toml and RUSTFLAGS for truly static musl binaries
## 0.3.1 - 2025-12-30

### New

- CLI `render` subcommand: `byonk render --mac XX:XX:XX:XX:XX:XX --output file.png` renders screens directly without starting a server
- Hello world tutorial screen (`screens/hello.lua`, `screens/hello.svg`) with screenshot in documentation
### Changed

- `make docs-samples` now uses CLI render command (no server needed)
- PNG output now uses native 2-bit grayscale instead of indexed color for faster firmware decoding
- Standardized documentation diagrams to use appropriate Mermaid types (flowchart, sequenceDiagram)
- Split complex architecture sequence diagram into focused phase-specific diagrams
- Upgraded mdBook to v0.5.2 and mdbook-mermaid to v0.17.0 in CI workflow
- Simplified mermaid setup: removed manual theme/mermaid.js, now managed by mdbook-mermaid

### Fixed

- Release builds now use static musl binaries, fixing glibc/musl mismatch in Docker images

## 0.3.0 - 2025-12-30

### New

- Default screen is now a TV-style test pattern showing device MAC, battery voltage, RSSI, 4 gray levels, gradient for dithering demo, and resolution test bars
- Device MAC address (`device.mac`) now available in both Lua scripts and templates
- Docker image tags for major (`0`), minor (`0.2`), and patch (`0.2.1`) versions
- Documentation migrated to mdBook with mermaid diagrams
- Updated installation docs for Docker and pre-built binaries
- Added CLAUDE.md with project guidelines
### Changed

- Renamed project branding from "TRMNL BOYS" to "Byonk - Bring Your Own Ink"
- Makefile now runs fmt and clippy before build targets
- Updated Makefile for mdBook (removed old rspress/npm targets)
- Faster container builds using pre-built binaries instead of compiling from source
- Switched from OpenSSL to rustls for better cross-platform compatibility
- Refactored content caching: `/api/display` now caches Lua script output (JSON data) instead of rendered PNGs; PNG rendering happens on-demand in `/api/image` when requested
- Removed unused static fallback SVG (`static/svgs/default.svg`) and ContentProvider

### Fixed

## 0.2.2 - 2025-12-29

## 0.2.1 - 2025-12-28

## 0.2.0 - 2025-12-28

## 0.1.0 - 2025-12-28

### New

- Device context (`device.battery_voltage`, `device.rssi`) available in templates and Lua scripts
- Template namespacing: `data.*` (Lua), `device.*` (device info), `params.*` (config)
- `skip_update` support: scripts can return `skip_update = true` to tell device to check back later without rendering new content
- Script-controlled refresh rate: the `refresh_rate` returned by Lua scripts is now properly sent to the device
### Changed

- Content rendering now happens in `/api/display` instead of `/api/image` for better control over refresh timing
- Rendered content is cached between `/api/display` and `/api/image` requests

### Fixed

- Fixed refresh_rate being hardcoded to 900 seconds instead of using script-returned value

## 0.1.0 - 2025-12-27

- Initial release
- Lua scripting support with HTTP, JSON, and HTML parsing
- Tera-based SVG templating
- Variable font support via patched resvg
- Floyd-Steinberg dithering for 4-level grayscale e-ink displays
- Device-specific configuration via config.yaml
- HMAC-SHA256 signed URLs for security
