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
import * as api from "../src/lib/api.js";

const PANELS = [
  { id: "e1004", name: "reTerminal E1004", width: 800, height: 480,
    colors: "#000000,#FFFFFF", colors_actual: null, dither: { strength: 0.8 } },
];
const DEVICES = [
  { id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: {},
    base: { dither: null, colors: null, colors_actual: null, max_error: null, noise_scale: null,
            chroma_clamp: null, strength: null, refresh: null, gamut: {} },
    final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null },
];
const RESOLUTION = {
  dither: { effective: "atkinson", from: "screen",
    order: ["override", "device_final", "screen", "device_base", "panel", "fallback"],
    rungs: { screen: { value: "atkinson", status: "winner" } } },
};

function stubServer() {
  vi.spyOn(globalThis, "fetch").mockImplementation(async () => ({
    ok: true, status: 200,
    headers: { get: (h) => (h.toLowerCase() === "x-byonk-resolution" ? JSON.stringify(RESOLUTION) : null) },
    blob: async () => new Blob(["png"], { type: "image/png" }),
    text: async () => "", json: async () => ({}),
  }));
}

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset(); theme.reset(); auth.reset(); editor.reset(); editor.files = [];
  simulate.reset(); simulate.error = null;
  subject.reset();
  resolution.reset(); // [Task 3]
  screens.repos = []; screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }, { name: "local/b" }],
    devices: DEVICES, panels: PANELS,
    dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1, noise_scale: 0 } }],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([
    { name: "script.lua", size: 1, text: true }, { name: "screen.svg", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockImplementation(async (_ref, name) => ({ text: `-- ${name}`, etag: '"1"' }));
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"] }, unseen: [] });
});
afterEach(() => cleanup());

async function mount() {
  render(AppShell);
  await waitFor(() => expect(screens.meta.panels).toHaveLength(1));
  await tick();
}

describe("spec §9: the buffer-survival journey", () => {
  it("two open files, one dirty, survive a detour through a panel and back", async () => {
    stubServer();
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    await fireEvent.click(dom.getByRole("button", { name: "screen.svg" }));
    await waitFor(() => expect(editor.tabs).toHaveLength(2));
    editor.setText("-- edited");
    expect(editor.active.dirty).toBe(true);

    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    await waitFor(() => expect(subject.kind).toBe("panel"));
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/a"));

    expect(editor.tabs.map((t) => t.name)).toEqual(["script.lua", "screen.svg"]);
    const svg = editor.tabs.find((t) => t.name === "screen.svg");
    expect(svg.text).toBe("-- edited");
    expect(svg.dirty).toBe(true);
    expect(api.listFiles).toHaveBeenCalledTimes(1);
  });
});
