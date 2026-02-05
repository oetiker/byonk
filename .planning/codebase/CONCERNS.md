# Codebase Concerns

**Analysis Date:** 2026-02-05

## Tech Debt

### Excessive `.unwrap()` and `.expect()` Calls (265 instances)
- **Issue:** Widespread use of panic-inducing `.unwrap()` and `.expect()` calls throughout codebase, especially in test code and config validation paths. Production paths may panic on unexpected conditions.
- **Files:**
  - `src/models/config.rs`: 8 unwrap calls (lines 244, 278, 345, 397, 403, 406, 428, 448) — all in tests but could indicate weak error propagation
  - `src/services/content_cache.rs`: 6 unwrap calls on RwLock operations (lines 98, 131, 140, 147, 252, 302)
  - `src/error.rs`: Panic in test (line 159)
  - `src/models/device.rs`: Panic in test (line 228: "Cross-library verification failed")
  - `src/models/display_spec.rs`: Multiple unwrap calls in tests
  - `src/api/display.rs`: Several `.unwrap_or()` defaults (lines 136, 159, 160, 240)
  - `src/main.rs`: Multiple `.unwrap_or_else()` for logging and config (lines 180, 202, 245, 262, 417, 421, 425, 429, 533, 542, 552-554, 606, 615, 625-627)
  - `src/rendering/svg_to_png.rs`: Line 147 `.unwrap_or()` fallback for oxipng optimization
  - `src/services/template_service.rs`: Line 180 regex capture `.unwrap()`
- **Impact:** Panic on production if config is malformed, network calls fail unexpectedly, or cache lock becomes poisoned. Tests are brittle.
- **Fix approach:**
  - Replace test unwraps with proper error assertions using `expect()` with context messages
  - Add logging before `.unwrap_or()` defaults
  - Use `.map_err()` to add context before propagating errors
  - Consider defensive programming around lock operations (e.g., `RwLock::read().unwrap_or_else(|e| panic!("lock poisoned: {e}"))`)

### RwLock Poisoning Risk
- **Issue:** Six `.unwrap()` calls on RwLock operations in `src/services/content_cache.rs` (lines 98, 131, 140, 147, 252, 302). If any thread panics while holding the lock, subsequent readers/writers will panic.
- **Files:** `src/services/content_cache.rs`
- **Impact:** Cache unavailability for entire application if a single render operation panics. Content becomes stale.
- **Fix approach:**
  - Use `.unwrap_or_else(|e| { ... handle recovery ... })` to gracefully recover from poisoned locks
  - Consider wrapping lock operations in a helper that logs poison events
  - Add metrics/logging for lock poisoning detection

### Panic in Test Code
- **Issue:** Three panic calls in test code that will hard-crash tests instead of using proper assertion failures:
  - `src/error.rs` line 159: `panic!("Expected Render variant")`
  - `src/models/device.rs` line 228: `panic!("Cross-library verification failed: {e}")`
  - `src/models/display_spec.rs` line 120: `panic!("Expected ImageTooLarge error")`
- **Files:** `src/error.rs`, `src/models/device.rs`, `src/models/display_spec.rs`
- **Impact:** Tests fail hard instead of reporting assertion errors, making CI harder to debug.
- **Fix approach:** Replace panics with `assert!()` or `assert_eq!()` with descriptive messages

## Known Bugs

### Content Cache Insertion Order Tracking
- **Problem:** Cache implementation uses Vec to track insertion order for LRU eviction. Assumes cache hits don't affect insertion order (line 102 in `src/services/content_cache.rs`), but Vec::retain() is O(n) per access.
- **Files:** `src/services/content_cache.rs` (lines 59-76, 98-120)
- **Workaround:** Cache is small (max 100 entries), so O(n) operations are tolerable but inefficient
- **Better approach:** Use indexmap crate or other ordered map structure for O(1) LRU updates

### HTTP Cache Key Collision Not Addressed
- **Problem:** HTTP cache in `src/services/http_cache.rs` uses SHA256(URL) as key. No documented collision handling if two URLs hash to same key (unlikely but possible).
- **Files:** `src/services/http_cache.rs`
- **Symptom:** Lua script gets wrong cached response if collision occurs
- **Mitigation:** Small cache (100 entries default) reduces collision probability

## Security Considerations

### Ed25519 Timestamp Validation Incomplete
- **Risk:** Timestamp skew check allows ±60 seconds drift (60,000ms), which could permit replay attacks if clock is significantly skewed or devices are time-synchronized adversarially.
- **Files:** `src/models/device.rs` (line 126: `const MAX_TIMESTAMP_SKEW_MS: i64 = 60_000;`)
- **Current mitigation:** Nanosecond-precision server timestamp comparison; devices must sync within 60 seconds
- **Recommendations:**
  - Document the 60-second window in security notes
  - Consider adding device-specific nonce tracking for strict replay prevention
  - Monitor for suspicious timestamp patterns in logs

### Lua Script Execution Sandbox Incomplete
- **Risk:** Lua scripts have access to HTTP requests, filesystem reads (via AssetLoader), and can influence rendering output. No explicit sandboxing for resource consumption or time limits.
- **Files:** `src/services/lua_runtime.rs` (1140 lines)
- **Specific concerns:**
  - No timeout on Lua script execution (could hang indefinitely on infinite loops)
  - No memory limit enforcement for Lua allocations
  - Scripts can make arbitrary HTTP requests (via `http_get` / `http_request`)
  - Scripts can fetch and parse HTML (via `scraper` integration) and extract data
- **Impact:** Malicious or buggy script could cause DoS (infinite loop), memory exhaustion, or exfiltrate data via HTTP requests
- **Recommendations:**
  - Add script execution timeout (e.g., 30 seconds per script)
  - Implement memory limits for Lua VM
  - Document trusted script sources (embedded vs. external)
  - Consider sandboxing HTTP requests (allowlist domains, rate limiting)

### API Key Registration Code Derivation
- **Risk:** Registration codes derived from API key via SHA256 hash. If API key is compromised, registration code becomes predictable.
- **Files:** `src/models/device.rs` (ApiKey implementation)
- **Current mitigation:** Registration codes are only used for initial device approval; actual auth still requires valid API key
- **Recommendations:** Consider adding per-device salt or key rotation mechanism

### Missing Input Validation on Lua Script Return Values
- **Risk:** Lua scripts return colors and dither mode strings. If script returns malformed data, downstream code may fail or render incorrectly.
- **Files:** `src/services/lua_runtime.rs`, `src/api/display.rs`
- **Example:** If script returns `colors = { "not-a-hex" }`, color parsing silently filters it out rather than rejecting the render
- **Impact:** Silent data corruption or unexpected visual output
- **Recommendations:** Add strict validation of script return values with clear error messages

## Performance Bottlenecks

### Large Lua Runtime File (1140 lines)
- **Problem:** `src/services/lua_runtime.rs` is the largest service file. Single-file definition of all Lua globals, helper functions, and FFI bindings makes it hard to navigate and maintain.
- **Files:** `src/services/lua_runtime.rs`
- **Cause:** All Lua API setup in one function (`setup_globals`)
- **Improvement path:**
  - Extract Lua API modules (layout, device, http, scraper, fonts, etc.) into separate functions
  - Consider module-based structure for Lua binding definitions
  - Current performance acceptable but maintainability suffers

### Content Cache LRU Inefficiency
- **Problem:** Cache uses Vec::retain() to remove entries and reorder, which is O(n) for each cache hit. Also uses HashMap + Vec instead of ordered structure.
- **Files:** `src/services/content_cache.rs`
- **Current impact:** Negligible (max 100 entries, rare cache hits at device render rates)
- **Scaling concern:** If content cache grows to 10,000+ entries, O(n) updates become problematic
- **Fix:** Use indexmap or std::collections::BTreeMap for automatic ordering

### SVG-to-PNG Rendering with External oxipng
- **Problem:** Rendering pipeline does double PNG encoding: first fast in resvg, then optimization pass with oxipng (lines 139-147 in `src/rendering/svg_to_png.rs`). oxipng::optimize_from_memory silently falls back to original if optimization fails.
- **Files:** `src/rendering/svg_to_png.rs`
- **Impact:** ~27% PNG size reduction (documented in CHANGES.md) but adds latency
- **Concern:** Fallback to unoptimized PNG if oxipng fails is silent (only `.unwrap_or()` fallback)
- **Recommendation:** Log optimization failures for monitoring

### Display Dimension Bounds Check
- **Problem:** API accepts display dimensions up to 2000x2000 (MAX_DISPLAY_WIDTH, MAX_DISPLAY_HEIGHT), but max_size_bytes is only 750KB. Very large dimensions could cause OOM during pixmap allocation.
- **Files:** `src/api/display.rs` (lines 21-22, 135-139), `src/rendering/svg_to_png.rs` (line 172)
- **Impact:** A request for 2000x2000 pixmap would allocate 16MB (4 bytes per RGBA pixel), then trigger size validation on PNG output
- **Fix:** Add pre-allocation size check before pixmap creation in `rasterize_svg`

## Fragile Areas

### Config Loading with Fallback to Defaults
- **Files:** `src/models/config.rs` (lines 109-131)
- **Why fragile:** When config parse fails, logs warning but silently falls back to `AppConfig::default()`. Operator doesn't know if config was partially loaded or completely ignored.
- **Safe modification:**
  - Add explicit validation phase after loading
  - Log both parsing error AND list of screens/devices that were loaded
  - Consider strict mode flag to fail on malformed config
- **Test coverage:** Config tests all use valid YAML; no tests for malformed input recovery

### Template Service Regex Matching
- **Files:** `src/services/template_service.rs` (line 180: `cap.get(0).unwrap()`)
- **Why fragile:** Regex capture is assumed to succeed but `.unwrap()` will panic if regex match fails (should not happen, but regex is complex)
- **Safe modification:** Use `.expect("regex always matches")` with context or refactor to avoid assumption
- **Test coverage:** Needs tests for edge-case template variable names (underscores, numbers, special chars)

### HTTP Cache TTL Expiration Without Cleanup
- **Files:** `src/services/http_cache.rs` (lines 59-76)
- **Why fragile:** Expired entries are only removed on cache hit. If an entry expires and is never re-requested, it stays in memory until evicted by LRU.
- **Safe modification:** Add background cleanup task to evict expired entries periodically
- **Impact:** Low (100 entry default limit), but for large caches could leak memory

### File Watcher Debounce Timing (200ms hardcoded)
- **Files:** `src/services/file_watcher.rs` (line 80: `Duration::from_millis(200)`)
- **Why fragile:** 200ms debounce is arbitrary and not configurable. Rapid file saves might still trigger multiple reloads.
- **Safe modification:** Make debounce duration configurable, log debounce events
- **Impact:** Dev mode only, so low risk to production

## Scaling Limits

### Content Cache (100 entries, 200KB-750KB per entry)
- **Current capacity:** 100 cached rendered SVG documents
- **Limit:** Hits at high-traffic sites with >100 unique screen configurations per server
- **Scaling path:** Make cache size configurable via env var or config.yaml

### HTTP Cache for Lua (100 entries, variable size per response)
- **Current capacity:** 100 cached HTTP responses from Lua scripts
- **Limit:** Hits if scripts fetch >100 unique URLs per hour
- **Scaling path:** Add configurable size limit and TTL tuning

### File Watcher (Recursive directory scan)
- **Current capacity:** Watches screens/ directory recursively, debounces at 200ms
- **Limit:** Very large screen directories (10,000+ files) may lag on file write detection
- **Scaling path:** Replace notify crate watcher with tokio-stream based polling if needed

## Dependencies at Risk

### Patched resvg Dependency (Variable Font Fork)
- **Risk:** Project uses patched version of resvg from `https://github.com/oetiker/resvg.git` branch `skrifa` (Cargo.toml line 104)
- **Impact:** Custom fork may diverge from upstream. Variable font support not yet in resvg mainline.
- **Migration plan:** Monitor upstream resvg for variable font support; switch to mainline when available
- **Files:** `Cargo.toml` (lines 102-106)

### mlua with Vendored Lua 5.4
- **Risk:** Uses vendored Lua (Cargo.toml line 26: `mlua = { version = "0.10", features = ["lua54", "vendored"] }`)
- **Impact:** Rust compiler must compile C Lua runtime; build times longer on CI
- **Mitigation:** Acceptable trade-off for self-contained binaries. Could switch to system Lua for faster builds.

### eink-dither Internal Crate
- **Risk:** Custom color dithering crate in `crates/eink-dither/` is not published to crates.io
- **Impact:** Dependency locked to workspace; no version stability guarantees
- **Maintenance:** Part of this repo, but needs active maintenance if algorithm changes

## Missing Critical Features

### No Device Heartbeat / Health Check
- **Problem:** Server has no way to know if a device is still alive. Devices only poll on their schedule (typically 15 minutes).
- **Blocks:** Proactive device status monitoring, fleet health dashboards
- **Recommendation:** Consider optional /api/heartbeat endpoint for devices to report health

### No Script Execution Timeout
- **Problem:** Lua scripts can hang indefinitely (infinite loop, blocking network call)
- **Blocks:** DoS protection, resource management
- **Recommendation:** Implement Lua script timeout (30s default) with interruptible execution

### No Audit Logging
- **Problem:** No logging of device registrations, API key usage, or content delivery
- **Blocks:** Security incident investigation, usage analytics
- **Recommendation:** Add optional audit log to track sensitive operations

## Test Coverage Gaps

### Config Validation Edge Cases
- **What's not tested:** Malformed config.yaml recovery, missing default screen, circular screen references (if possible)
- **Files:** `src/models/config.rs` (tests only use valid configs)
- **Risk:** Silent config loading failures
- **Priority:** High — config is critical to operations

### Lua Script Error Handling
- **What's not tested:** Script timeout, memory exhaustion, network failures during http_get
- **Files:** `src/services/lua_runtime.rs` (no timeout tests)
- **Risk:** Unhandled script exceptions could crash request
- **Priority:** High — scripts are user-controllable

### Color Palette Rendering Edge Cases
- **What's not tested:** Single-color palette, duplicate colors in palette, unsupported color formats
- **Files:** `src/rendering/svg_to_png.rs` (palette deduplication in build_eink_palette but tests limited)
- **Risk:** Incorrect dithering output
- **Priority:** Medium — colors are device-specific

### Concurrent Cache Access Under Lock Contention
- **What's not tested:** RwLock behavior under high concurrent load, cache eviction race conditions
- **Files:** `src/services/content_cache.rs`, `src/services/http_cache.rs`
- **Risk:** Data corruption or deadlock under peak load
- **Priority:** Medium — caches are high-traffic paths

### File Watcher Reliability
- **What's not tested:** Rapid file changes, permission denied errors, symlink handling
- **Files:** `src/services/file_watcher.rs` (no integration tests)
- **Risk:** Dev mode may fail to reload screen changes
- **Priority:** Low — dev mode only

---

*Concerns audit: 2026-02-05*
