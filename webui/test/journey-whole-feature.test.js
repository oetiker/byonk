import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import * as api from "../src/lib/api.js";

const PANELS = [
  { id: "e1004", name: "reTerminal E1004", width: 800, height: 480,
    colors: "#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00",
    colors_actual: "#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D",
    dither: { strength: 0.8 } },
  { id: "x", name: "TRMNL X", width: 1600, height: 1200, colors: "#000000,#FFFFFF", colors_actual: null, dither: null },
];
const DEVICES = [
  { id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: { greeting: "hi" },
    base: { dither: "floyd-steinberg", colors: null, colors_actual: null, max_error: null, noise_scale: null,
            chroma_clamp: null, strength: null, refresh: null, gamut: {} },
    final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null },
];
const RESOLUTION = {
  dither: { effective: "atkinson", from: "screen",
    order: ["override", "device_final", "screen", "device_base", "panel", "fallback"],
    rungs: { screen: { value: "atkinson", status: "winner" }, fallback: { value: "atkinson", status: "outranked" } } },
};

/** Every fetch the app makes, as [method, url, body]. Renders answer a PNG with the header. */
function stubServer() {
  const calls = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init = {}) => {
    calls.push([init.method ?? "GET", url, init.body ? JSON.parse(init.body) : undefined]);
    return {
      ok: true, status: 200,
      headers: { get: (h) => (h.toLowerCase() === "x-byonk-resolution" ? JSON.stringify(RESOLUTION) : null) },
      blob: async () => new Blob(["png"], { type: "image/png" }),
      text: async () => "", json: async () => ({}),
    };
  });
  return calls;
}

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset(); theme.reset(); auth.reset(); editor.reset(); editor.files = [];
  simulate.reset(); simulate.error = null;
  subject.reset();
  resolution.reset();
  configDraft.reset();
  screens.repos = []; screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }, { name: "local/b" }, { name: "byonk-builtin/calibration/tone" }],
    devices: DEVICES, panels: PANELS,
    dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1, noise_scale: 0 } },
                        { name: "floyd-steinberg", defaults: { max_error: 1, noise_scale: 0 } }],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([
    { name: "script.lua", size: 1, text: true }, { name: "screen.svg", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockImplementation(async (_ref, name) => ({ text: `-- ${name}`, etag: '"1"' }));
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"], x: [] }, unseen: ["attic"] });
});
afterEach(() => cleanup());

async function mount() {
  render(AppShell);
  await waitFor(() => expect(screens.meta.panels).toHaveLength(2));
  await tick();
}

describe("the work area end to end, through AppShell", () => {
  it("screen → panel push → device save → back to the screen, with every surface agreeing", async () => {
    const calls = stubServer();
    vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    vi.spyOn(api, "endTuningSession").mockResolvedValue(null);
    const release = vi.spyOn(api, "releasePanelOverride").mockResolvedValue(null);
    const saveDevice = vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "kitchen", screen: "local/a" });
    await mount();

    // a) a screen, with a dirty buffer
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("-- mine");

    // b) a panel: push, then the strip shows it live
    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    const s = await waitFor(() => document.querySelector("#cfg-panel-strength"));
    s.value = "0.4"; await fireEvent.input(s);
    vi.spyOn(api, "listOverrides").mockResolvedValue({
      panels: [{ key: "e1004", values: { strength: 0.4 }, expires_in_secs: 1799 }], devices: [], tuning_sessions: [] });
    await fireEvent.click(dom.getByRole("button", { name: "Push to panel" }));
    await waitFor(() => expect(dom.getAllByText(/◉ Live on/).length).toBeGreaterThan(0));
    expect(dom.getByText("It reverts on its own within 30 minutes, or on Discard.")).toBeTruthy();

    // c) a device: leaving the panel released the push; the ladder restarts
    vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
    await fireEvent.click(dom.getByRole("button", { name: "Kitchen · reTerminal E1004" }));
    await waitFor(() => expect(release).toHaveBeenCalledWith("e1004"));
    const fd = await waitFor(() => document.querySelector("#cfg-device_final-dither"));
    fd.value = "atkinson"; await fireEvent.change(fd);
    await waitFor(() => expect(calls.filter(([, u]) => u === "api/render").at(-1)[2].draft.final).toEqual({ dither: "atkinson" }));
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(saveDevice).toHaveBeenCalledWith("kitchen", { final: { dither: "atkinson" } }));

    // d) back to the screen: buffer intact, Edit shows the file editor again
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/a"));
    expect(editor.active.text).toBe("-- mine");
    expect(document.querySelector('[data-testid="config-editor"]')).toBe(null);
    await waitFor(() => expect(calls.filter(([, u]) => u === "api/render").at(-1)[2]).toMatchObject({ screen: "local/a" }));
  });
});
