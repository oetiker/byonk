import { describe, it, expect } from "vitest";
import { buildRenderRequest, previewSize, devicePanel, DEFAULT_SIZE } from "../src/lib/render-request.js";

const meta = {
  panels: [{ id: "e1004", width: 800, height: 480 }, { id: "x", width: 1600, height: 1200 }],
  devices: [{ id: "kitchen", panel: "e1004", screen: "local/k" }, { id: "office", panel: null, screen: "local/o" }],
};
const usage = { panels: { x: ["office"], e1004: ["kitchen"] }, unseen: [] };
const sim = (over = {}) => ({ use_actual: true, simulated: () => ({ params: { a: 1 } }), ...over });
const draft = {
  panelWire: (id) => ({ colors: "#000000,#FFFFFF", colors_actual: `actual-${id}` }),
  deviceWire: (id) => ({ screen: `draft-screen-${id}`, params: { p: 1 }, base: { dither: "stucki" }, final: {} }),
};

describe("buildRenderRequest", () => {
  it("returns null with no subject", () => {
    expect(buildRenderRequest({ subject: { kind: null }, draft, simulate: sim(), meta, usage })).toBe(null);
  });

  it("mode a, no target: the screen at the default size, with simulated params", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: null }, draft, simulate: sim(), meta, usage });
    expect(r.body).toEqual({ screen: "local/a", ...DEFAULT_SIZE, simulated: { params: { a: 1 } }, view: { use_actual: true } });
    expect(r.as).toBe(null);
  });

  it("mode a as a panel: that panel and its size, no draft", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: { kind: "panel", id: "x" } }, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({ screen: "local/a", panel: "x", width: 1600, height: 1200 });
    expect(r.body.draft).toBeUndefined();
  });

  it("mode a as a device: screen AND device together, the device's panel size", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: { kind: "device", id: "office" } }, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({ screen: "local/a", device: "office", width: 1600, height: 1200 });
  });

  it("mode b: the borrowed screen on the subject panel with the panel draft; no device unless watched", () => {
    const s = { kind: "panel", id: "e1004", borrowedScreen: "byonk-builtin/calibration/tone", watchDevice: "" };
    const r = buildRenderRequest({ subject: s, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({
      screen: "byonk-builtin/calibration/tone", panel: "e1004", width: 800, height: 480,
      draft: { panel: { colors: "#000000,#FFFFFF", colors_actual: "actual-e1004" } },
    });
    expect(r.body.device).toBeUndefined();
    const w = buildRenderRequest({ subject: { ...s, watchDevice: "kitchen" }, draft, simulate: sim(), meta, usage });
    expect(w.body.device).toBe("kitchen");
  });

  it("mode c: the draft's screen assignment, both device rungs and params, and no simulated params", () => {
    let asked;
    const simulate = sim({ simulated: ({ withParams }) => ((asked = withParams), {}) });
    const r = buildRenderRequest({ subject: { kind: "device", id: "kitchen" }, draft, simulate, meta, usage });
    expect(asked).toBe(false);
    expect(r.body).toMatchObject({
      device: "kitchen", screen: "draft-screen-kitchen", width: 800, height: 480,
      draft: { base: { dither: "stucki" }, final: {}, params: { p: 1 } },
    });
  });

  it("never sends override: the preview writes and pushes nothing", () => {
    for (const s of [{ kind: "screen", id: "local/a" }, { kind: "panel", id: "e1004", borrowedScreen: "b" }, { kind: "device", id: "kitchen" }]) {
      expect(buildRenderRequest({ subject: s, draft, simulate: sim(), meta, usage }).body.override).toBeUndefined();
    }
  });
});

describe("devicePanel", () => {
  it("prefers the explicit panel, falls back to the server's usage, else null", () => {
    expect(devicePanel({ meta, usage }, "kitchen")).toBe("e1004");
    expect(devicePanel({ meta, usage }, "office")).toBe("x");
    expect(devicePanel({ meta, usage: { panels: {}, unseen: [] } }, "office")).toBe(null);
  });
});

describe("previewSize", () => {
  it("follows the subject without simulated inputs", () => {
    expect(previewSize({ subject: { kind: "panel", id: "x" }, meta, usage })).toEqual({ width: 1600, height: 1200 });
    expect(previewSize({ subject: { kind: "device", id: "office" }, meta, usage })).toEqual({ width: 1600, height: 1200 });
    expect(previewSize({ subject: { kind: null }, meta, usage })).toEqual(DEFAULT_SIZE);
  });
});

describe("previewSize of a device without panel geometry", () => {
  // The server's `resolve_preview_dimensions`: no panel geometry means the
  // model's own size, the model being what the device last reported.
  const m = {
    panels: [{ id: "bare", width: null, height: null }],
    devices: [
      { id: "tx", panel: null, screen: "local/o", model: "x" },
      { id: "txbare", panel: "bare", screen: "local/o", model: "x" },
      { id: "og", panel: null, screen: "local/o", model: "og" },
    ],
  };
  const none = { panels: {}, unseen: [] };
  it("takes the model's size, as the server and the admin preview do", () => {
    const X = { width: 1872, height: 1404 };
    expect(previewSize({ subject: { kind: "device", id: "tx" }, meta: m, usage: none })).toEqual(X);
    expect(previewSize({ subject: { kind: "device", id: "txbare" }, meta: m, usage: none })).toEqual(X);
    expect(previewSize({ subject: { kind: "screen", id: "local/a", target: { kind: "device", id: "tx" } }, meta: m, usage: none })).toEqual(X);
    expect(previewSize({ subject: { kind: "device", id: "og" }, meta: m, usage: none })).toEqual(DEFAULT_SIZE);
    const r = buildRenderRequest({ subject: { kind: "device", id: "tx" }, draft, simulate: sim(), meta: m, usage: none });
    expect(r.body).toMatchObject(X);
  });
});
