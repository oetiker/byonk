import * as api from "../api.js";
import { screens } from "./screens.svelte.js";

/**
 * The simulate drawer's controls, and the render URL they build.
 *
 * This is a straight port of `drawerParams()` in the old `static/dev/dev.js`
 * (see it for the semantics this mirrors) onto `RenderQuery`
 * (`src/api/dev.rs:29`) -- NOT the plan brief's `DEFAULTS`/`url()` sketch,
 * which contradicts the old UI in six ways (Controller Ruling 26):
 *
 *   (a) the Device control's wire name is `mac`, not `device` -- with a
 *       device selected the server resolves the screen from its config and
 *       ignores `screen` entirely.
 *   (b) `width`/`height` are never user fields: they come from the chosen
 *       panel's profile (`size`, below), falling back to 800x480.
 *   (c) `use_actual` and `colors_actual` travel only when a panel is chosen
 *       -- `colors_actual` further requires that panel to actually have
 *       one, so the preview does not depend on the server's override map
 *       having been written first.
 *   (d) `timestamp` is Unix seconds and travels only while `timeLocked` --
 *       a live clock needs no override, and sending one would pin every
 *       render to the second the field last ticked.
 *   (e) `params` is JSON-parsed and re-stringified, and THROWS on invalid
 *       JSON so the error surfaces instead of silently rendering with
 *       whatever the box last held. The caller (Task 9's `renderNow()`)
 *       catches it.
 *   (f) `max_error`/`noise_scale` travel only when they differ from the
 *       SELECTED ALGORITHM's own default (from `screens.meta.dither_
 *       algorithms[].defaults`) -- comparing against one fixed number would
 *       make every render carry an override the user never asked for. With
 *       no defaults known (algorithm not found, e.g. the screens fetch
 *       failed) every non-empty value is sent, since the user's value is
 *       the only thing left to trust. `chroma_clamp` has no per-algorithm
 *       default and is sent whenever non-empty; `strength` only when it is
 *       not 1.
 *
 * `colors` and `model` are deliberately absent from the field set: the old
 * UI's render query omits `colors` too and lets the server apply its own
 * fallback (dev.js:237-241).
 *
 * Fix round 1 (Rulings 32-34) corrected three things found in review:
 *   - Ruling 32: `panel` is matched by `id` only (see `#selectedPanel()`),
 *     not `id` OR `name` -- the wire value is always the config id.
 *   - Ruling 33: selecting a device populates `panel`/`dither` from that
 *     device's own config, same caution as the old UI's
 *     `selectIfPresent()` (only when the value is actually present in the
 *     list) -- see `SimulateControls.svelte`'s `onDeviceChange()`.
 *   - Ruling 34: `strength` is guarded against `null` explicitly (see
 *     `isBlank()`), not left to `qs()` to drop downstream.
 */

const DEFAULT_SIZE = { width: 800, height: 480 };

const DEFAULTS = {
  mac: "",
  panel: "",
  use_actual: true,
  dither: "",
  max_error: "",
  noise_scale: "",
  chroma_clamp: "",
  strength: "",
  battery_voltage: "",
  rssi: "",
  timestamp: "",
  timeLocked: false,
  params: "{}",
};

/**
 * True for a field left untouched: "" (the default text-field value) or
 * `null` -- what a Svelte `bind:value` on a cleared `<input type="number">`
 * yields, never a string (verified directly: typing produces a `number`,
 * clearing produces `null`). `undefined` is covered defensively for the
 * same reason.
 */
function isBlank(v) {
  return v === "" || v === undefined || v === null;
}

/** `v` differs from `dflt` for the "send only on override" fields (f). */
function differsFromDefault(v, dflt) {
  if (isBlank(v)) return false;
  if (dflt === undefined || dflt === null) return true;
  return parseFloat(v) !== parseFloat(dflt);
}

class Simulate {
  /**
   * The last failed render, or null. Shape: `{ op, message }`, matching
   * `screens.error` (screens.svelte.js) -- `op` is always "render" (there is
   * only one kind of render here); `message` is either `url()`'s own thrown
   * message (invalid params JSON, rule (e) above) or the server's
   * `RenderErrorResponse.error`/`.details` (src/api/dev.rs:157-161).
   * Captured by `PreviewPane`'s `renderNow()`, which is the one place that
   * calls `fetch`/`url()` and can see either failure; rendering it is Task
   * 10's job (Controller Ruling 41), the same split `screens.error` uses
   * with `FilesPane`. Not touched by `reset()`: a render failure isn't a
   * drawer field, and `renderNow()` clears it itself at the start of the
   * next attempt.
   */
  error = $state(null);

  mac = $state(DEFAULTS.mac);
  panel = $state(DEFAULTS.panel);
  use_actual = $state(DEFAULTS.use_actual);
  dither = $state(DEFAULTS.dither);
  max_error = $state(DEFAULTS.max_error);
  noise_scale = $state(DEFAULTS.noise_scale);
  chroma_clamp = $state(DEFAULTS.chroma_clamp);
  strength = $state(DEFAULTS.strength);
  battery_voltage = $state(DEFAULTS.battery_voltage);
  rssi = $state(DEFAULTS.rssi);
  timestamp = $state(DEFAULTS.timestamp);
  timeLocked = $state(DEFAULTS.timeLocked);
  params = $state(DEFAULTS.params);

  reset() {
    this.mac = DEFAULTS.mac;
    this.panel = DEFAULTS.panel;
    this.use_actual = DEFAULTS.use_actual;
    this.dither = DEFAULTS.dither;
    this.max_error = DEFAULTS.max_error;
    this.noise_scale = DEFAULTS.noise_scale;
    this.chroma_clamp = DEFAULTS.chroma_clamp;
    this.strength = DEFAULTS.strength;
    this.battery_voltage = DEFAULTS.battery_voltage;
    this.rssi = DEFAULTS.rssi;
    this.timestamp = DEFAULTS.timestamp;
    this.timeLocked = DEFAULTS.timeLocked;
    this.params = DEFAULTS.params;
  }

  /** A snapshot of every field, keyed by its `RenderQuery` wire name. */
  get fields() {
    return {
      mac: this.mac,
      panel: this.panel,
      use_actual: this.use_actual,
      dither: this.dither,
      max_error: this.max_error,
      noise_scale: this.noise_scale,
      chroma_clamp: this.chroma_clamp,
      strength: this.strength,
      battery_voltage: this.battery_voltage,
      rssi: this.rssi,
      timestamp: this.timestamp,
      params: this.params,
    };
  }

  /**
   * The panel profile the `panel` field currently names, matched against
   * `screens.meta.panels` by `id` only -- the wire value (`RenderQuery.
   * panel`), confirmed from both ends: the old UI's select uses `p.id` as
   * its option value (dev.js:971), and the server resolves it via
   * `Config::get_panel`, a lookup by config id (src/models/
   * config.rs:667-668), not by the human-readable `name` field. `id` and
   * `name` are genuinely distinct in a real config (e.g. `config.yaml`'s
   * `trmnl_x` / `"TRMNL X (16-grey)"`), so matching on `name` as well
   * (Ruling 32) could silently resolve the wrong panel if a display name
   * ever collided with another panel's id. `null` when nothing matches
   * (including when nothing is chosen).
   */
  #selectedPanel() {
    const list = screens.meta.panels ?? [];
    return list.find((p) => p.id === this.panel) ?? null;
  }

  /** Width/height from the chosen panel profile, else the 800x480 default. */
  get size() {
    const p = this.#selectedPanel();
    return p && p.width && p.height ? { width: p.width, height: p.height } : DEFAULT_SIZE;
  }

  url(ref) {
    const size = this.size;
    const query = {
      screen: ref,
      mac: this.mac,
      width: size.width,
      height: size.height,
      battery_voltage: this.battery_voltage,
      rssi: this.rssi,
      dither: this.dither,
      chroma_clamp: this.chroma_clamp,
    };

    // (d) Only while frozen. A live clock needs no override, and sending
    // one would pin every render to the second the field last ticked.
    if (this.timeLocked) query.timestamp = this.timestamp;

    // (e) Throws on invalid JSON -- the caller (Task 9) catches it and
    // surfaces the message instead of rendering with parameters the user
    // did not write.
    try {
      query.params = JSON.stringify(JSON.parse(this.params || "{}"));
    } catch (e) {
      throw new Error("Invalid JSON in parameters: " + e.message);
    }

    // (c) Only when a panel is chosen; colors_actual further requires that
    // panel to actually carry one.
    if (this.panel) {
      query.panel = this.panel;
      query.use_actual = this.use_actual ? "true" : "false";
      const p = this.#selectedPanel();
      if (p && p.colors_actual) query.colors_actual = p.colors_actual;
    }

    // (f) A tunable travels only when it differs from the SELECTED
    // ALGORITHM's own default; with none known every non-empty value is
    // sent.
    const algo = screens.meta.dither_algorithms?.find((a) => a.name === this.dither);
    const defaults = algo ? algo.defaults : null;
    if (differsFromDefault(this.max_error, defaults?.max_error)) {
      query.max_error = this.max_error;
    }
    if (differsFromDefault(this.noise_scale, defaults?.noise_scale)) {
      query.noise_scale = this.noise_scale;
    }

    // No per-algorithm default: the engine leaves strength at 1.0 whatever
    // the kernel, so only a departure from that is sent. Guard against
    // `null` explicitly (Ruling 34) rather than relying on `qs()` to drop
    // it downstream -- `parseFloat(null)` is `NaN`, which is `!== 1` and
    // would otherwise send a bogus `strength=null`.
    if (!isBlank(this.strength) && parseFloat(this.strength) !== 1) {
      query.strength = this.strength;
    }

    return api.renderUrl(query);
  }
}

export const simulate = new Simulate();
