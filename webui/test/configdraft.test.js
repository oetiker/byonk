import { describe, it, expect, vi, beforeEach } from "vitest";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  configDraft.reset();
  subject.reset();
  screens.meta = {
    panels: [{ id: "e1004", name: "reTerminal E1004", match: "reterminal_e1004", width: 1600, height: 1200,
               colors: "#000000,#FFFFFF", colors_actual: "#101010,#E8E8E8",
               dither: { strength: 0.8, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } } }],
    devices: [{ id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: { g: "hi" },
                base: { dither: "floyd-steinberg", gamut: { knee: 0.5, amount: 0.9 } }, final: null,
                temperature_profile: null, maximum_compatibility: null, min_png_bytes: null }],
    dither_algorithms: [],
  };
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(screens, "refresh").mockImplementation(async ({ applied } = {}) => applied?.());
});

describe("configDraft", () => {
  it("starts from config and is clean", () => {
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.8);
    expect(configDraft.value("panel", "e1004", "panel", "gamut.knee")).toBe(0.6);
    expect(configDraft.value("device", "kitchen", "device_base", "dither")).toBe("floyd-steinberg");
    expect(configDraft.plainValue("device", "kitchen", "name")).toBe("Kitchen");
    expect(configDraft.dirtyCount("panel", "e1004")).toBe(0);
  });

  it("a panel wire carries the whole rung, per-algorithm blocks included", () => {
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    expect(configDraft.panelWire("e1004")).toEqual({
      colors: "#000000,#FFFFFF", colors_actual: "#101010,#E8E8E8",
      dither: { strength: 0.5, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } },
    });
    expect(configDraft.dirtyCount("panel", "e1004")).toBe(1);
  });

  it("a draft survives a detour to another subject (P2-4)", () => {
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    configDraft.set("device", "kitchen", "device_final", "dither", "atkinson");
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.5);
  });

  it("push carries dirty set pushable fields only, final over base (P2-1, P2-2)", () => {
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    configDraft.set("device", "kitchen", "device_final", "strength", 0.7);
    configDraft.set("device", "kitchen", "device_base", "refresh", 60);
    configDraft.set("device", "kitchen", "device_base", "dither", null);
    expect(configDraft.pushBody("device", "kitchen")).toEqual({ strength: 0.7 });
    expect(configDraft.unpushable("device", "kitchen").sort()).toEqual([
      "dither (removal)", "refresh", "strength (outranked by final)",
    ]);
  });

  it("push never shows what Save will not: a base value outranked by the effective final is left out (PF5)", () => {
    // kitchen's configured final sets strength; nobody edited it.
    screens.meta.devices[0].final = { strength: 0.4 };
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    configDraft.set("device", "kitchen", "device_base", "max_error", 0.5);
    expect(configDraft.pushBody("device", "kitchen")).toEqual({ max_error: 0.5 });
    expect(configDraft.unpushable("device", "kitchen")).toEqual(["strength (outranked by final)"]);
  });

  it("push sends the override, starts the watched device's session, and lists it as live", async () => {
    const pushP = vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    const sess = vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.push("panel", "e1004", { watchDevice: "kitchen" });
    expect(pushP).toHaveBeenCalledWith("e1004", { strength: 0.5 });
    expect(sess).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toEqual({ kind: "panel", id: "e1004", device: "kitchen" });
  });

  it("panel save sends the whole rung and ends the session", async () => {
    vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue(null);
    const save = vi.spyOn(api, "savePanel").mockResolvedValue(null);
    configDraft.set("panel", "e1004", "panel", "colors_actual", null);
    await configDraft.push("panel", "e1004", { watchDevice: "kitchen" });
    await configDraft.save("panel", "e1004");
    expect(save).toHaveBeenCalledWith("e1004", {
      id: "e1004", name: "reTerminal E1004", match: "reterminal_e1004", width: 1600, height: 1200,
      colors: "#000000,#FFFFFF",
      dither: { strength: 0.8, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } },
    });
    expect(end).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toBe(null);
  });

  it("a removed gamut sub-field is cleared by name in the one request (C3)", () => {
    configDraft.set("device", "kitchen", "device_base", "gamut.knee", null);
    configDraft.set("device", "kitchen", "device_final", "dither", "atkinson");
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({
      clear: ["gamut.knee"], final: { dither: "atkinson" } });
  });

  it("dirty params are cleared and set in one request, so the server replaces them wholesale", () => {
    configDraft.setPlain("device", "kitchen", "params", '{"other": 1}');
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({ clear: ["params"], params: { other: 1 } });
  });

  it("a changed base gamut value is sent as a set on the base rung, with no clear", () => {
    configDraft.set("device", "kitchen", "device_base", "gamut.amount", 0.7);
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({ gamut: { amount: 0.7 } });
  });

  it("device save is ONE PATCH carrying the clears and the sets", async () => {
    const calls = [];
    vi.spyOn(api, "saveDevice").mockImplementation(async (k, b) => (calls.push(b), { key: k }));
    configDraft.set("device", "kitchen", "device_base", "dither", null);
    configDraft.setPlain("device", "kitchen", "name", "Cuisine");
    await configDraft.save("device", "kitchen");
    expect(calls).toEqual([{ clear: ["dither"], name: "Cuisine" }]);
    expect(configDraft.dirtyCount("device", "kitchen")).toBe(0);
  });

  it("discard reverts the draft and clears a live override even after a reload", async () => {
    vi.spyOn(api, "listOverrides").mockResolvedValue({
      panels: [{ key: "e1004", values: { strength: 0.5 }, expires_in_secs: 100 }], devices: [], tuning_sessions: [],
    });
    const rel = vi.spyOn(api, "releasePanelOverride").mockResolvedValue(null);
    await configDraft.refreshLive();
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.discard("panel", "e1004");
    expect(rel).toHaveBeenCalledWith("e1004");
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.8);
  });

  it("release gives up a pushed override but keeps the draft", async () => {
    vi.spyOn(api, "pushDeviceOverride").mockResolvedValue(null);
    const rel = vi.spyOn(api, "releaseDeviceOverride").mockResolvedValue(null);
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    await configDraft.push("device", "kitchen");
    await configDraft.release();
    expect(rel).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toBe(null);
    expect(configDraft.value("device", "kitchen", "device_base", "strength")).toBe(0.3);
  });

  it("a failed push is its own error and leaves nothing marked pushed", async () => {
    vi.spyOn(api, "pushPanelOverride").mockRejectedValue(new Error("404 Not Found"));
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.push("panel", "e1004");
    expect(configDraft.error).toEqual({ op: "push", message: "404 Not Found" });
    expect(configDraft.pushed).toBe(null);
  });

  it("a failing usage load is configDraft's error and never screens.error (R66)", async () => {
    screens.error = null;
    vi.spyOn(api, "panelUsage").mockRejectedValue(new Error("500 boom"));
    await configDraft.loadUsage();
    expect(configDraft.error).toEqual({ op: "panel usage", message: "500 boom" });
    expect(screens.error).toBe(null);
  });

  it("load fetches the live list and the usage", async () => {
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"] }, unseen: ["attic"] });
    await configDraft.load();
    expect(configDraft.usage).toEqual({ panels: { e1004: ["kitchen"] }, unseen: ["attic"] });
    expect(configDraft.live.panels).toEqual([]);
  });

  it("every edit bumps the revision the Preview watches", () => {
    const before = configDraft.revision;
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    expect(configDraft.revision).toBe(before + 1);
  });

  it("a failed release is reported, not swallowed", async () => {
    vi.spyOn(api, "pushDeviceOverride").mockResolvedValue(null);
    vi.spyOn(api, "releaseDeviceOverride").mockRejectedValue(new Error("503 Unavailable"));
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    await configDraft.push("device", "kitchen");
    await configDraft.release();
    expect(configDraft.error).toEqual({ op: "release", message: "503 Unavailable" });
  });
});

describe("configDraft identity (U7)", () => {
  it("a panel's identity starts from meta, match included, and edits mark it dirty", () => {
    expect(configDraft.plainValue("panel", "e1004", "id")).toBe("e1004");
    expect(configDraft.plainValue("panel", "e1004", "name")).toBe("reTerminal E1004");
    expect(configDraft.plainValue("panel", "e1004", "match")).toBe("reterminal_e1004");
    expect(configDraft.plainValue("panel", "e1004", "width")).toBe(1600);
    expect(configDraft.plainValue("panel", "e1004", "colors")).toBe("#000000,#FFFFFF");
    configDraft.setPlain("panel", "e1004", "width", "800");
    expect(configDraft.plainValue("panel", "e1004", "width")).toBe(800);
    expect(configDraft.dirty("panel", "e1004")).toEqual([{ rung: null, field: "width" }]);
  });

  it("an identity edit does not touch the render revision, a tuning edit does", () => {
    const rev = configDraft.revision;
    const ver = configDraft.version;
    configDraft.setPlain("panel", "e1004", "name", "Renamed");
    configDraft.setPlain("device", "kitchen", "key", "pantry");
    configDraft.setPlain("device", "kitchen", "panel", "og");
    configDraft.setPlain("device", "kitchen", "name", "Pantry");
    expect(configDraft.revision).toBe(rev);
    expect(configDraft.version).toBe(ver + 4);
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    expect(configDraft.revision).toBe(rev + 1);
  });

  it("identity is not part of the render draft until saved", () => {
    configDraft.setPlain("panel", "e1004", "colors", "#000000,#FFFFFF,#FF0000");
    expect(configDraft.panelWire("e1004").colors).toBe("#000000,#FFFFFF");
  });

  it("identity edits apply on Save only and never travel in a Push", () => {
    configDraft.setPlain("panel", "e1004", "name", "Renamed");
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    expect(configDraft.pushBody("panel", "e1004")).toEqual({ strength: 0.5 });
    expect(configDraft.unpushable("panel", "e1004")).toEqual(["name"]);
  });

  it("panel Save sends the whole panel; a cleared optional field is left out (= removed)", async () => {
    const save = vi.spyOn(api, "savePanel").mockResolvedValue({ id: "e1004" });
    configDraft.setPlain("panel", "e1004", "match", "");
    configDraft.setPlain("panel", "e1004", "height", "");
    configDraft.setPlain("panel", "e1004", "name", " Big ");
    await configDraft.save("panel", "e1004");
    expect(save).toHaveBeenCalledWith("e1004", {
      id: "e1004", name: "Big", width: 1600, colors: "#000000,#FFFFFF", colors_actual: "#101010,#E8E8E8",
      dither: { strength: 0.8, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } },
    });
  });

  it("a panel Save with a changed Id re-selects the subject under the new id and leaves no draft", async () => {
    subject.select("panel", "e1004");
    subject.target = { kind: "panel", id: "e1004" };
    vi.spyOn(screens, "refresh").mockImplementation(async ({ applied } = {}) => {
      screens.meta = { ...screens.meta, panels: [{ ...screens.meta.panels[0], id: "big" }] };
      applied?.();
    });
    const save = vi.spyOn(api, "savePanel").mockResolvedValue({ id: "big" });
    configDraft.setPlain("panel", "e1004", "id", "big");
    await configDraft.save("panel", "e1004");
    expect(save.mock.calls[0][0]).toBe("e1004");
    expect(save.mock.calls[0][1].id).toBe("big");
    expect(subject.key).toBe("panel:big");
    // The preview's target follows the rename, so it keeps rendering the entry.
    expect(subject.target).toEqual({ kind: "panel", id: "big" });
    expect(configDraft.dirtyCount("panel", "big")).toBe(0);
    expect(configDraft.drafts).toEqual({});
  });

  it("a 409 keeps the draft and records the server message for that subject", async () => {
    subject.select("panel", "e1004");
    vi.spyOn(api, "savePanel").mockRejectedValue(new Error("409 Conflict: panel `og` already exists"));
    configDraft.setPlain("panel", "e1004", "id", "og");
    await configDraft.save("panel", "e1004");
    expect(configDraft.plainValue("panel", "e1004", "id")).toBe("og");
    expect(configDraft.dirtyCount("panel", "e1004")).toBe(1);
    expect(configDraft.saveError("panel", "e1004")).toBe("409 Conflict: panel `og` already exists");
    expect(configDraft.saveError("device", "kitchen")).toBe(null);
    expect(subject.key).toBe("panel:e1004");
  });

  it("a device's key and panel start from meta and Save sends them in the one PATCH", async () => {
    expect(configDraft.plainValue("device", "kitchen", "key")).toBe("kitchen");
    expect(configDraft.plainValue("device", "kitchen", "panel")).toBe("e1004");
    subject.select("device", "kitchen");
    subject.target = { kind: "device", id: "kitchen" };
    subject.watchDevice = "kitchen";
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "pantry", screen: "local/a" });
    configDraft.setPlain("device", "kitchen", "key", "pantry");
    configDraft.setPlain("device", "kitchen", "panel", "");
    await configDraft.save("device", "kitchen");
    expect(save).toHaveBeenCalledWith("kitchen", { key: "pantry", clear: ["panel"] });
    expect(subject.key).toBe("device:pantry");
    expect(subject.target).toEqual({ kind: "device", id: "pantry" });
    expect(subject.watchDevice).toBe("pantry");
  });

  it("a rename leaves a target or watched device that names another entry alone", async () => {
    subject.select("device", "kitchen");
    subject.target = { kind: "panel", id: "kitchen" };
    subject.watchDevice = "hall";
    vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "pantry", screen: "local/a" });
    configDraft.setPlain("device", "kitchen", "key", "pantry");
    await configDraft.save("device", "kitchen");
    expect(subject.target).toEqual({ kind: "panel", id: "kitchen" });
    expect(subject.watchDevice).toBe("hall");
  });

  it("a save failure is shown in one place (the editor, not configDraft.error) and clears on the next edit", async () => {
    vi.spyOn(api, "savePanel").mockRejectedValue(new Error("409 Conflict: panel `og` already exists"));
    configDraft.setPlain("panel", "e1004", "id", "og");
    await configDraft.save("panel", "e1004");
    expect(configDraft.error).toBe(null);
    expect(configDraft.saveError("panel", "e1004")).toMatch(/already exists/);
    configDraft.setPlain("panel", "e1004", "id", "og2");
    expect(configDraft.saveError("panel", "e1004")).toBe(null);
  });

  it("a successful save clears an earlier save failure", async () => {
    const save = vi.spyOn(api, "savePanel").mockRejectedValueOnce(new Error("400 Bad Request: bad colors"));
    configDraft.setPlain("panel", "e1004", "colors", "#000000");
    await configDraft.save("panel", "e1004");
    expect(configDraft.saveError("panel", "e1004")).toMatch(/bad colors/);
    save.mockResolvedValueOnce({ id: "e1004" });
    await configDraft.save("panel", "e1004");
    expect(configDraft.saveError("panel", "e1004")).toBe(null);
  });

  it("a device panel choice is set by id", () => {
    configDraft.setPlain("device", "kitchen", "panel", "og");
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({ panel: "og" });
  });

  it("names the identity fields Save needs and does not have", () => {
    expect(configDraft.missing("panel", "e1004")).toEqual([]);
    configDraft.setPlain("panel", "e1004", "name", "");
    configDraft.setPlain("panel", "e1004", "id", " ");
    expect(configDraft.missing("panel", "e1004")).toEqual(["id", "name"]);
    configDraft.setPlain("device", "kitchen", "key", "");
    expect(configDraft.missing("device", "kitchen")).toEqual(["key"]);
  });
});
