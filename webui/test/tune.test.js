import { describe, it, expect, vi, beforeEach } from "vitest";
import { tune } from "../src/lib/state/tune.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

const PANEL = { id: "e1004", name: "E1004", colors_actual: "#101010,#E8E8E8,#00FF00" };

beforeEach(() => {
  vi.restoreAllMocks();
  tune.reset();
  simulate.reset();
});

describe("tune store", () => {
  it("loads a panel's configured colors as the working copy and the baseline", () => {
    tune.selectPanel(PANEL.id, PANEL);
    expect(tune.colors).toEqual(["#101010", "#E8E8E8", "#00FF00"]);
    expect(tune.baseline).toEqual(["#101010", "#E8E8E8", "#00FF00"]);
    expect(tune.dirty).toBe(false);
  });

  it("drives the render query without any fetch", () => {
    const spy = vi.spyOn(globalThis, "fetch");
    tune.selectPanel(PANEL.id, PANEL);
    tune.setColor(2, "#00994D");

    expect(simulate.colors_actual_override).toEqual({
      panel: "e1004",
      colors: "#101010,#E8E8E8,#00994D",
    });
    expect(tune.dirty).toBe(true);
    expect(spy).not.toHaveBeenCalled();
  });

  it("starts a tuning session when a device is selected and ends it on deselect", async () => {
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue();

    await tune.selectDevice("kitchen");
    expect(start).toHaveBeenCalledWith("kitchen");

    await tune.selectDevice("");
    expect(end).toHaveBeenCalledWith("kitchen");
  });

  it("push sends the override AND re-touches the session", async () => {
    const put = vi.spyOn(api, "setPanelOverride").mockResolvedValue();
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    tune.selectPanel(PANEL.id, PANEL);
    await tune.selectDevice("kitchen");
    start.mockClear();
    tune.setColor(2, "#00994D");

    await tune.push();

    expect(put).toHaveBeenCalledWith("e1004", "#101010,#E8E8E8,#00994D");
    expect(start).toHaveBeenCalledWith("kitchen");
    expect(tune.pushed).toBe(true);
  });

  it("discard clears the override, ends the session and reverts the working copy", async () => {
    vi.spyOn(api, "setPanelOverride").mockResolvedValue();
    vi.spyOn(api, "startTuningSession").mockResolvedValue();
    const del = vi.spyOn(api, "clearPanelOverride").mockResolvedValue();
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue();

    tune.selectPanel(PANEL.id, PANEL);
    await tune.selectDevice("kitchen");
    tune.setColor(2, "#00994D");
    await tune.push();

    await tune.discard();

    expect(del).toHaveBeenCalledWith("e1004");
    expect(end).toHaveBeenCalledWith("kitchen");
    expect(tune.colors).toEqual(tune.baseline);
    expect(tune.pushed).toBe(false);
    expect(simulate.colors_actual_override).toBeNull();
  });

  it("save writes, refreshes metadata, and stops reading as dirty", async () => {
    const save = vi.spyOn(api, "savePanelColors").mockResolvedValue();
    const refresh = vi.spyOn(screens, "refresh").mockResolvedValue();
    tune.selectPanel(PANEL.id, PANEL);
    tune.setColor(2, "#00994D");

    await tune.save();

    expect(save).toHaveBeenCalledWith("e1004", "#101010,#E8E8E8,#00994D");
    // The description promises a metadata refresh -- assert it actually
    // happened, not just that the fields it would have no bearing on ended
    // up looking right.
    expect(refresh).toHaveBeenCalled();
    expect(tune.baseline).toEqual(["#101010", "#E8E8E8", "#00994D"]);
    expect(tune.dirty).toBe(false);
    expect(tune.pushed).toBe(false);
    expect(tune.error).toBeNull();
  });

  it("records a failure instead of letting it reject unhandled", async () => {
    vi.spyOn(api, "setPanelOverride").mockRejectedValue(new Error("boom"));
    tune.selectPanel(PANEL.id, PANEL);

    await expect(tune.push()).resolves.toBeUndefined();
    expect(tune.error).toMatchObject({ op: "push" });
    expect(tune.pushed).toBe(false);
  });
});
