// One draft store for both config editors (spec §3.2, §7): "edit one rung of
// a config object". Panel schema: the panel rung. Device schema: the base and
// final rungs plus the fields with no rung. Drafts are kept per subject
// (P2-4); the baseline is always today's config from `screens.meta`, so a
// Save followed by `screens.refresh()` leaves the draft clean.

import * as api from "../api.js";
import { screens } from "./screens.svelte.js";

export const NUMBER_FIELDS = [
  "max_error", "noise_scale", "chroma_clamp", "strength",
  "gamut.knee", "gamut.amount", "gamut.max_compression",
];
export const PANEL_FIELDS = ["colors_actual", ...NUMBER_FIELDS];
export const DEVICE_CASCADE_FIELDS = ["colors", "colors_actual", "dither", ...NUMBER_FIELDS, "refresh"];
export const DEVICE_PLAIN_FIELDS = [
  "screen", "name", "params", "temperature_profile", "maximum_compatibility", "min_png_bytes",
];
export const SCHEMAS = {
  panel: { rungs: ["panel"], fields: PANEL_FIELDS, plain: [] },
  device: { rungs: ["device_base", "device_final"], fields: DEVICE_CASCADE_FIELDS, plain: DEVICE_PLAIN_FIELDS },
};
const NUMERIC = new Set([...NUMBER_FIELDS, "refresh", "min_png_bytes"]);
/** A device override cannot carry these (ruling C1: an override takes `dither`, never `colors` or `refresh`; Plan 1 Tasks 2 and 3). */
const NOT_PUSHABLE = new Set(["colors", "refresh"]);

function norm(field, v) {
  if (v === "" || v === undefined || v === null) return null;
  if (NUMERIC.has(field)) {
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  }
  return typeof v === "string" ? v.trim() || null : v;
}

/** `{field: value|null}` over `fields`, reading gamut sub-fields from `src.gamut`. */
function flatten(src, fields) {
  const out = {};
  for (const f of fields) {
    out[f] = f.startsWith("gamut.") ? (src?.gamut?.[f.slice(6)] ?? null) : (src?.[f] ?? null);
  }
  return out;
}

/** The inverse: nulls dropped, gamut nested (omitted when empty). */
function nest(flat) {
  const out = {};
  const gamut = {};
  for (const [f, v] of Object.entries(flat)) {
    if (v === null) continue;
    if (f.startsWith("gamut.")) gamut[f.slice(6)] = v;
    else out[f] = v;
  }
  if (Object.keys(gamut).length) out.gamut = gamut;
  return out;
}

const SCALAR_DITHER_KEYS = new Set(["max_error", "noise_scale", "chroma_clamp", "strength", "gamut"]);
function algorithmsOf(dither) {
  const out = {};
  for (const [k, v] of Object.entries(dither ?? {})) if (!SCALAR_DITHER_KEYS.has(k)) out[k] = v;
  return out;
}

const keyOf = (kind, id) => `${kind}:${id}`;

class ConfigDraft {
  drafts = $state({});
  pushed = $state(null);
  error = $state(null);
  live = $state({ panels: [], devices: [], tuning_sessions: [] });
  usage = $state({ panels: {}, unseen: [] });
  /** Bumped on every draft edit; the Preview re-renders on it (P2-7). */
  revision = $state(0);

  #info(kind, id) {
    const list = kind === "panel" ? screens.meta.panels : screens.meta.devices;
    return (list ?? []).find((x) => x.id === id) ?? null;
  }

  baseline(kind, id) {
    const info = this.#info(kind, id);
    if (kind === "panel") {
      return {
        values: { panel: { colors_actual: info?.colors_actual ?? null, ...flatten(info?.dither, NUMBER_FIELDS) } },
        plain: {},
      };
    }
    return {
      values: {
        device_base: flatten(info?.base, DEVICE_CASCADE_FIELDS),
        device_final: flatten(info?.final, DEVICE_CASCADE_FIELDS),
      },
      plain: {
        screen: info?.screen ?? null,
        name: info?.name ?? null,
        params: JSON.stringify(info?.params ?? {}, null, 2),
        temperature_profile: info?.temperature_profile ?? null,
        maximum_compatibility: info?.maximum_compatibility ?? null,
        min_png_bytes: info?.min_png_bytes ?? null,
      },
    };
  }

  #current(kind, id) {
    return this.drafts[keyOf(kind, id)] ?? this.baseline(kind, id);
  }

  #edit(kind, id, fn) {
    const k = keyOf(kind, id);
    const entry = structuredClone($state.snapshot(this.drafts[k] ?? this.baseline(kind, id)));
    fn(entry);
    this.drafts = { ...this.drafts, [k]: entry };
    this.revision += 1;
  }

  value(kind, id, rung, field) {
    return this.#current(kind, id).values[rung]?.[field] ?? null;
  }
  set(kind, id, rung, field, v) {
    this.#edit(kind, id, (e) => (e.values[rung][field] = norm(field, v)));
  }
  plainValue(kind, id, field) {
    return this.#current(kind, id).plain[field] ?? null;
  }
  setPlain(kind, id, field, v) {
    this.#edit(kind, id, (e) => (e.plain[field] = field === "params" ? v : norm(field, v)));
  }

  dirty(kind, id) {
    const cur = this.#current(kind, id);
    const base = this.baseline(kind, id);
    const out = [];
    for (const rung of SCHEMAS[kind].rungs) {
      for (const f of SCHEMAS[kind].fields) {
        if ((cur.values[rung][f] ?? null) !== (base.values[rung][f] ?? null)) out.push({ rung, field: f });
      }
    }
    for (const f of SCHEMAS[kind].plain) {
      if ((cur.plain[f] ?? null) !== (base.plain[f] ?? null)) out.push({ rung: null, field: f });
    }
    return out;
  }
  dirtyCount(kind, id) {
    return this.dirty(kind, id).length;
  }
  isDirty(kind, id, rung, field) {
    return this.dirty(kind, id).some((d) => d.rung === rung && d.field === field);
  }

  /** `draft.panel` of the render request: the whole rung. */
  panelWire(id) {
    const info = this.#info("panel", id);
    const v = this.#current("panel", id).values.panel;
    const { colors_actual, ...numbers } = v;
    const dither = { ...nest(numbers), ...algorithmsOf(info?.dither) };
    const out = { colors: info?.colors };
    if (colors_actual !== null) out.colors_actual = colors_actual;
    if (Object.keys(dither).length) out.dither = dither;
    return out;
  }

  /** `screen`, `draft.base|final|params` of a mode c render request. */
  deviceWire(id) {
    const cur = this.#current("device", id);
    let params;
    try {
      params = JSON.parse(cur.plain.params || "{}");
    } catch (e) {
      throw new Error("Invalid JSON in the device's params: " + e.message);
    }
    return {
      screen: cur.plain.screen,
      params,
      base: nest(cur.values.device_base),
      final: nest(cur.values.device_final),
    };
  }

  /**
   * P2-1/P2-2: dirty, set, pushable values; a device's final over its base.
   * Ruling PF5: Push never shows what Save will not. A dirty base value is
   * skipped whenever the field's EFFECTIVE final value (draft or config) is
   * non-null, because Save leaves that final winning; `unpushable()` lists it
   * as "outranked by final".
   */
  pushBody(kind, id) {
    const cur = this.#current(kind, id);
    const flat = {};
    const rungs = kind === "panel" ? ["panel"] : ["device_base", "device_final"];
    for (const { rung, field } of this.dirty(kind, id)) {
      if (!rungs.includes(rung) || NOT_PUSHABLE.has(field)) continue;
      const v = cur.values[rung][field];
      if (v === null) continue;
      if (rung === "device_base" && cur.values.device_final[field] !== null) continue;
      flat[field] = v;
    }
    return nest(flat);
  }

  /** What a Push leaves out, for the footer's "applies on Save only". */
  unpushable(kind, id) {
    const cur = this.#current(kind, id);
    const out = new Set();
    for (const { rung, field } of this.dirty(kind, id)) {
      if (rung === null) out.add(field);
      else if (NOT_PUSHABLE.has(field)) out.add(field);
      else if (cur.values[rung][field] === null) out.add(`${field} (removal)`);
      else if (rung === "device_base" && cur.values.device_final[field] !== null) out.add(`${field} (outranked by final)`);
    }
    return [...out];
  }

  savePanelBody(id) {
    const { colors_actual, dither } = this.panelWire(id);
    const out = {};
    if (colors_actual !== undefined) out.colors_actual = colors_actual;
    if (dither !== undefined) out.dither = dither;
    return out;
  }

  /**
   * The device Save, as ONE `DeviceWrite` (controller ruling C3): the server
   * clears first and sets second, so a removed value goes in `clear` and a
   * changed one is set, with no ordering for the client to get wrong. Only
   * dirty fields travel; `gamut` sets carry just the dirty knobs (the server
   * merges them over what is there).
   */
  saveDeviceBody(id) {
    const cur = this.#current("device", id);
    const clear = [];
    const body = {};
    const final = {};
    const gamut = { device_base: {}, device_final: {} };
    for (const { rung, field } of this.dirty("device", id)) {
      if (rung === null) {
        const v = cur.plain[field];
        if (field === "params") {
          clear.push("params");
          body.params = JSON.parse(v || "{}");
        } else if (v === null) clear.push(field);
        else body[field] = v;
        continue;
      }
      const v = cur.values[rung][field];
      const prefix = rung === "device_final" ? "final." : "";
      if (v === null) {
        clear.push(prefix + field);
      } else if (field.startsWith("gamut.")) {
        gamut[rung][field.slice(6)] = v;
      } else if (rung === "device_final") {
        final[field] = v;
      } else {
        body[field] = v;
      }
    }
    if (Object.keys(gamut.device_base).length) body.gamut = gamut.device_base;
    if (Object.keys(gamut.device_final).length) final.gamut = gamut.device_final;
    if (Object.keys(final).length) body.final = final;
    if (clear.length) body.clear = clear;
    return Object.keys(body).length ? body : null;
  }

  liveFor(kind, id) {
    const list = kind === "panel" ? this.live.panels : this.live.devices;
    return (list ?? []).find((o) => o.key === id) ?? null;
  }

  async refreshLive() {
    try {
      this.live = await api.listOverrides();
    } catch (e) {
      this.error = { op: "overrides", message: e.message };
    }
  }

  async loadUsage() {
    try {
      this.usage = await api.panelUsage();
    } catch (e) {
      this.error = { op: "panel usage", message: e.message };
    }
  }

  async load() {
    await Promise.all([this.refreshLive(), this.loadUsage()]);
  }

  /** Stage 1: put the dirty values on real hardware. A button, never a debounce. */
  async push(kind, id, { watchDevice = "" } = {}) {
    const body = this.pushBody(kind, id);
    try {
      if (kind === "panel") await api.pushPanelOverride(id, body);
      else await api.pushDeviceOverride(id, body);
    } catch (e) {
      this.error = { op: "push", message: e.message };
      return;
    }
    this.pushed = { kind, id, device: kind === "panel" ? watchDevice : "" };
    this.error = null;
    // P2-3: the session starts here, never from the Preview.
    if (kind === "panel" && watchDevice) {
      try {
        await api.startTuningSession(watchDevice);
      } catch (e) {
        this.error = { op: "session", message: e.message };
      }
    }
    await this.refreshLive();
  }

  /** Stage 2: write config.yaml. The server drops the override itself. */
  async save(kind, id) {
    try {
      if (kind === "panel") {
        await api.savePanel(id, this.savePanelBody(id));
      } else {
        const body = this.saveDeviceBody(id);
        if (body) await api.saveDevice(id, body);
      }
    } catch (e) {
      this.error = { op: "save", message: e.message };
      return;
    }
    this.error = null;
    const { [keyOf(kind, id)]: _gone, ...rest } = this.drafts;
    this.drafts = rest;
    this.revision += 1;
    await this.#endSessionFor(kind, id);
    await screens.refresh();
    await this.load();
  }

  async discard(kind, id) {
    const { [keyOf(kind, id)]: _gone, ...rest } = this.drafts;
    this.drafts = rest;
    this.revision += 1;
    if (this.liveFor(kind, id) || (this.pushed?.kind === kind && this.pushed?.id === id)) {
      try {
        if (kind === "panel") await api.releasePanelOverride(id);
        else await api.releaseDeviceOverride(id);
        this.error = null;
      } catch (e) {
        this.error = { op: "discard", message: e.message };
      }
    }
    await this.#endSessionFor(kind, id);
    await this.refreshLive();
  }

  /** Subject switch or Edit closed: give up server state, keep the draft. */
  async release() {
    const p = this.pushed;
    if (!p) return;
    try {
      if (p.kind === "panel") await api.releasePanelOverride(p.id);
      else await api.releaseDeviceOverride(p.id);
    } catch {
      // Best effort; the strip still lists what is live, and the TTL ends it.
    }
    await this.#endSessionFor(p.kind, p.id);
    await this.refreshLive();
  }

  /** The strip's Discard for an override this page did not push (a reload). */
  async discardLive(kind, key) {
    try {
      if (kind === "panel") await api.releasePanelOverride(key);
      else await api.releaseDeviceOverride(key);
    } catch (e) {
      this.error = { op: "discard", message: e.message };
    }
    if (this.pushed?.kind === kind && this.pushed?.id === key) await this.#endSessionFor(kind, key);
    await this.refreshLive();
  }

  async #endSessionFor(kind, id) {
    const p = this.pushed;
    if (!p || p.kind !== kind || p.id !== id) return;
    this.pushed = null;
    if (!p.device) return;
    try {
      await api.endTuningSession(p.device);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  reset() {
    this.drafts = {};
    this.pushed = null;
    this.error = null;
    this.live = { panels: [], devices: [], tuning_sessions: [] };
    this.usage = { panels: {}, unseen: [] };
    this.revision = 0;
  }
}

export const configDraft = new ConfigDraft();
