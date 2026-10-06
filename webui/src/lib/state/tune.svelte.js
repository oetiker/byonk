// Panel colour tuning: a working copy, a temporary override, and a durable save.
//
// Two stages, because e-ink is reflective: a calibration is only valid under
// the light the panel hangs in, so it must be able to reach real hardware
// (stage 1, `push`) before it becomes the configuration (stage 2, `save`).
//
// State lives here and not in `TunePane` so that closing the panel -- which
// unmounts the component and clears the server-side override -- does not
// throw away a half-finished calibration.

import * as api from "../api.js";
import { simulate } from "./simulate.svelte.js";
import { screens } from "./screens.svelte.js";

const split = (s) =>
  s
    ? s
        .split(",")
        .map((c) => c.trim())
        .filter(Boolean)
    : [];

class Tune {
  panel = $state("");
  device = $state("");
  colors = $state([]);
  baseline = $state([]);
  selected = $state(0);
  pushed = $state(false);
  /** `null` or `{op: "push" | "save" | "discard" | "session", message}`. */
  error = $state(null);

  get dirty() {
    return this.colors.join(",") !== this.baseline.join(",");
  }

  /** The comma-separated wire form of the working copy. */
  get wire() {
    return this.colors.join(",");
  }

  /**
   * Load a panel's configured colours as the working copy and the baseline.
   *
   * `panelInfo` lets a caller that already has the `PanelInfo` (e.g. a
   * `<select>` handler with the option's data) skip a second lookup; when
   * omitted it is found in `screens.meta.panels` by id, the same match rule
   * `simulate` uses (Ruling 32).
   *
   * Falls back to the panel's nominal `colors` when it has never been
   * measured -- tuning from the nominal palette is the normal first
   * calibration, not an error.
   */
  selectPanel(id, panelInfo = null) {
    const info = panelInfo ?? (screens.meta.panels ?? []).find((p) => p.id === id) ?? null;
    this.panel = id;
    const from = info?.colors_actual ? split(info.colors_actual) : split(info?.colors);
    this.colors = [...from];
    this.baseline = [...from];
    this.selected = 0;
    this.pushed = false;
    this.#mirror();
  }

  /**
   * Start (or move) a tuning session. A previously selected device's session
   * ends first; selecting "" just ends the current one.
   */
  async selectDevice(key) {
    const previous = this.device;
    if (previous && previous !== key) await this.#endSession(previous);
    this.device = key;
    if (!key) return;
    try {
      await api.startTuningSession(key);
      this.error = null;
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  setColor(i, hex) {
    if (i < 0 || i >= this.colors.length) return;
    const next = [...this.colors];
    next[i] = hex;
    this.colors = next;
    this.#mirror();
  }

  /**
   * Stage 1: push the working copy to the panel as a temporary override.
   *
   * The override write and the session re-touch are independent server
   * calls, deliberately not under one `try`/`catch`. `pushed` must be true
   * whenever a server-side override is actually live -- `release()` (below)
   * gates its close-time `clearPanelOverride` on it, so if a successful
   * override write were followed by a failed session re-touch and both fell
   * into one catch that reset `pushed` to `false`, `release()` would skip
   * clearing an override that is, in fact, still sitting on the server: the
   * "an override nobody is watching" trap the spec calls out, self-inflicted
   * by this store's own error handling. A session failure is real and must
   * be surfaced, but as its own error, not by understating the override.
   */
  async push() {
    try {
      await api.setPanelOverride(this.panel, this.wire);
    } catch (e) {
      this.pushed = false;
      this.error = { op: "push", message: e.message };
      return;
    }
    this.pushed = true;
    this.error = null;
    if (!this.device) return;
    // The override is keyed by panel and the session by device, so the
    // server cannot refresh the session from the push. The client does it.
    try {
      await api.startTuningSession(this.device);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  /** Stage 2: write the working copy into `config.yaml` and end the session. */
  async save() {
    try {
      await api.savePanelColors(this.panel, this.wire);
      this.baseline = [...this.colors];
      this.pushed = false;
      this.error = null;
      // The panel list the client holds now understates the truth: the
      // server already dropped the override in favour of the write.
      await screens.refresh();
      await this.#endSession(this.device);
      this.device = "";
    } catch (e) {
      this.error = { op: "save", message: e.message };
    }
  }

  /**
   * Revert the working copy, drop the override, and end the session.
   *
   * Reverts the working copy even if `clearPanelOverride` throws: the same
   * best-effort tradeoff `release()` makes below. A failed clear can leave
   * the browser showing the reverted (pre-push) colours while the server
   * may still be serving the override until the request is retried or the
   * session's 30-minute backstop expires; `tune.error` records the failure
   * so it is visible, but discarding is user-initiated and idempotent to
   * retry, so it is not blocked on the DELETE succeeding.
   *
   * Unlike `release()`, this clears unconditionally rather than gating on
   * `this.pushed` -- discard is the user explicitly asking "make sure
   * nothing of mine is live," so it is allowed to issue a DELETE against a
   * panel that was never actually pushed (a no-op server-side); `release()`
   * gates on `pushed` only to avoid an unnecessary call on the common path
   * where the panel closes without ever having been pushed.
   */
  async discard() {
    try {
      await api.clearPanelOverride(this.panel);
      this.error = null;
    } catch (e) {
      this.error = { op: "discard", message: e.message };
    }
    this.colors = [...this.baseline];
    this.pushed = false;
    // Not `#mirror()`: the override is gone server-side, so the preview must
    // fall back to the panel's configured `colors_actual`, not keep echoing
    // the (now-reverted) working copy as if it were still overriding.
    simulate.colors_actual_override = null;
    await this.#endSession(this.device);
    this.device = "";
  }

  /**
   * Give up the server-side state without touching the working copy.
   *
   * Called when the Tune panel closes. An override that nobody is watching
   * is a trap: it silently changes what every device using that panel
   * renders, with nothing on screen to say so.
   */
  async release() {
    if (this.pushed) {
      try {
        await api.clearPanelOverride(this.panel);
      } catch {
        // Best effort. A failure here leaves an override live, which the
        // status bar still reports; it must not throw out of an unmount.
      }
      this.pushed = false;
    }
    await this.#endSession(this.device);
    this.device = "";
    simulate.colors_actual_override = null;
  }

  reset() {
    this.panel = "";
    this.device = "";
    this.colors = [];
    this.baseline = [];
    this.selected = 0;
    this.pushed = false;
    this.error = null;
    simulate.colors_actual_override = null;
  }

  async #endSession(key) {
    if (!key) return;
    try {
      await api.endTuningSession(key);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  /** Feed the working copy to `simulate.request()`, or clear it if empty. */
  #mirror() {
    simulate.colors_actual_override = this.colors.length
      ? { panel: this.panel, colors: this.wire }
      : null;
  }
}

export const tune = new Tune();
