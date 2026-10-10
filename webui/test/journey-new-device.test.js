import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
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

/**
 * U5 journey: from the Devices "+" to the new device selected in the
 * Library, across the real AppShell -- the Library's form, the API, the
 * store refresh, the subject, and the Preview that follows it.
 */
const PANELS = [{ id: "og", name: "TRMNL OG", width: 800, height: 480, colors: "#000000,#FFFFFF", colors_actual: null, dither: null }];
const base = { dither: null, colors: null, colors_actual: null, max_error: null, noise_scale: null,
  chroma_clamp: null, strength: null, refresh: null, gamut: {} };
const HALL = { id: "11:22:33:44:55:66", screen: "local/b", panel: null, name: "Hall", params: {}, base,
  final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null };

function payload(devices) {
  return { screens: [{ name: "local/a" }, { name: "local/b" }], devices, panels: PANELS,
    dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1, noise_scale: 0 } }] };
}

function stubServer() {
  const calls = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init = {}) => {
    calls.push([init.method ?? "GET", url, init.body ? JSON.parse(init.body) : undefined]);
    return {
      ok: true, status: 200,
      headers: { get: () => null },
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
  screens.error = null;
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue(payload([]));
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "--", etag: '"1"' });
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { og: [] }, unseen: [] });
});
afterEach(() => cleanup());

describe("New device", () => {
  it("goes from + through a pending pick to the new device selected and previewed", async () => {
    const calls = stubServer();
    vi.spyOn(api, "listPendingDevices").mockResolvedValue([
      { mac: "11:22:33:44:55:66", registration_code: "ABCDE12345", model: "og", firmware_version: "1", last_seen: "x" },
    ]);
    const create = vi.spyOn(api, "createDevice").mockImplementation(async (body) => {
      api.listScreens.mockResolvedValue(payload([HALL]));
      api.panelUsage.mockResolvedValue({ panels: { og: [] }, unseen: [HALL.id] });
      return { key: body.key, screen: body.screen };
    });

    render(AppShell);
    await waitFor(() => expect(screens.meta.panels).toHaveLength(1));
    await tick();
    const library = dom.getByRole("region", { name: "Library" });

    await fireEvent.click(within(library).getByRole("button", { name: "New device" }));
    const form = await within(library).findByRole("form", { name: "New device" });
    await fireEvent.click(await within(form).findByRole("button", { name: "11:22:33:44:55:66 · og" }));
    const sel = within(form).getByRole("combobox", { name: "Screen" });
    sel.value = "local/b";
    await fireEvent.change(sel);
    await fireEvent.input(within(form).getByRole("textbox", { name: "Name" }), { target: { value: "Hall" } });

    const before = calls.length;
    await fireEvent.click(within(form).getByRole("button", { name: "OK" }));

    await waitFor(() => expect(subject.key).toBe("device:11:22:33:44:55:66"));
    expect(create).toHaveBeenCalledWith({ key: "11:22:33:44:55:66", screen: "local/b", name: "Hall" });
    const row = await within(library).findByRole("button", { name: /^Hall · / });
    expect(row).toHaveAttribute("aria-current", "true");
    await waitFor(() => expect(within(library).queryByRole("form", { name: "New device" })).toBeNull());

    // One render request per change: creating renders only the new subject.
    const renders = () =>
      calls.slice(before).filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
    await waitFor(() => expect(renders().length).toBeGreaterThan(0));
    await new Promise((r) => setTimeout(r, 50));
    expect(renders()).toHaveLength(1);
    expect(renders()[0]).toMatchObject({ device: "11:22:33:44:55:66", screen: "local/b" });
  });
});
