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
import { details } from "../src/lib/state/details.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * U7 journeys: the Library "+" writes a placeholder entry at once and
 * selects it; its identity is then edited in the Edit pane's Details block
 * and saved -- across the real AppShell (Library, Edit, Preview, stores).
 */
const PANELS = [{ id: "og", name: "TRMNL OG", match: null, width: 800, height: 480, colors: "#000000,#FFFFFF", colors_actual: null, dither: null }];
const NEW_PANEL = { id: "new_panel", name: "New panel", match: null, width: null, height: null, colors: "#000000,#FFFFFF", colors_actual: null, dither: null };
const base = { dither: null, colors: null, colors_actual: null, max_error: null, noise_scale: null,
  chroma_clamp: null, strength: null, refresh: null, gamut: {} };
const device = (id, name) => ({ id, screen: "local/a", panel: null, name, params: {}, base,
  final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null });

function payload(devices, panels = PANELS) {
  return { screens: [{ name: "local/a" }, { name: "local/b" }], devices, panels,
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
  // Details are collapsed by default (U14); these tests are about their content.
  details.reset();
  details.set(true);
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

const renderBodies = (calls, from) =>
  calls.slice(from).filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
const settle = () => new Promise((r) => setTimeout(r, 400)); // past the 300 ms draft debounce

async function edit(sel, value) {
  const el = await waitFor(() => {
    const e = document.querySelector(sel);
    if (!e) throw new Error(`no ${sel}`);
    return e;
  });
  el.value = value;
  await fireEvent.input(el);
}

describe("New panel", () => {
  it("+ adds new_panel, its Id and size are edited in Details, Save shows it renamed and selected", async () => {
    const calls = stubServer();
    const create = vi.spyOn(api, "createPanel").mockImplementation(async (body) => {
      api.listScreens.mockResolvedValue(payload([], [...PANELS, NEW_PANEL]));
      return { id: body.id };
    });
    const save = vi.spyOn(api, "savePanel").mockImplementation(async (_id, body) => {
      api.listScreens.mockResolvedValue(payload([], [...PANELS, { ...NEW_PANEL, id: body.id, width: body.width, height: body.height }]));
      return { id: body.id };
    });

    render(AppShell);
    await waitFor(() => expect(screens.meta.panels).toHaveLength(1));
    await tick();
    const library = dom.getByRole("region", { name: "Library" });

    const before = calls.length;
    await fireEvent.click(within(library).getByRole("button", { name: "Add panel" }));
    await waitFor(() => expect(subject.key).toBe("panel:new_panel"));
    expect(create).toHaveBeenCalledWith({ id: "new_panel", name: "New panel", colors: "#000000,#FFFFFF" });
    expect(within(library).queryByRole("form")).toBeNull();
    // Creating selects the new subject: at most its one render.
    await waitFor(() => expect(renderBodies(calls, before).length).toBeGreaterThan(0));
    await settle();
    expect(renderBodies(calls, before)).toHaveLength(1);

    // Typing identity never renders.
    const typed = calls.length;
    await edit("#cfg-plain-id", "hall_panel");
    await edit("#cfg-plain-width", "1600");
    await edit("#cfg-plain-height", "1200");
    await settle();
    expect(renderBodies(calls, typed)).toHaveLength(0);
    expect(dom.getByText("3 unsaved")).toBeTruthy();

    const saved = calls.length;
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(subject.key).toBe("panel:hall_panel"));
    // A saved change re-renders once: the renamed panel, at its new size.
    await waitFor(() => expect(renderBodies(calls, saved).length).toBeGreaterThan(0));
    await settle();
    expect(renderBodies(calls, saved)).toHaveLength(1);
    expect(renderBodies(calls, saved)[0]).toMatchObject({ panel: "hall_panel" });
    expect(save).toHaveBeenCalledWith("new_panel", {
      id: "hall_panel", name: "New panel", width: 1600, height: 1200, colors: "#000000,#FFFFFF" });
    // The row keeps its name ("New panel"; the "+" is "Add panel") and is the selected one.
    const row = await within(library).findByRole("button", { name: "New panel" });
    expect(row).toHaveAttribute("aria-current", "true");
    expect(subject.is("panel", "hall_panel")).toBe(true);
    expect(document.querySelector("#cfg-plain-id").value).toBe("hall_panel");
    expect(dom.getByText("0 unsaved")).toBeTruthy();
  });
});

describe("New device", () => {
  it("+ adds new-device on the first screen; a pending MAC becomes its key on Save", async () => {
    const calls = stubServer();
    vi.spyOn(api, "listPendingDevices").mockResolvedValue([
      { mac: "11:22:33:44:55:66", registration_code: "ABCDE12345", model: "og", firmware_version: "1", last_seen: "x" },
    ]);
    const create = vi.spyOn(api, "createDevice").mockImplementation(async (body) => {
      api.listScreens.mockResolvedValue(payload([device(body.key, body.name)]));
      return { key: body.key, screen: body.screen };
    });
    const save = vi.spyOn(api, "saveDevice").mockImplementation(async (_key, body) => {
      api.listScreens.mockResolvedValue(payload([device(body.key, body.name)]));
      return { key: body.key, screen: "local/a" };
    });

    render(AppShell);
    await waitFor(() => expect(screens.meta.panels).toHaveLength(1));
    await tick();
    const library = dom.getByRole("region", { name: "Library" });

    const before = calls.length;
    await fireEvent.click(within(library).getByRole("button", { name: "Add device" }));
    await waitFor(() => expect(subject.key).toBe("device:new-device"));
    expect(create).toHaveBeenCalledWith({ key: "new-device", name: "New device", screen: "local/a" });
    await waitFor(() => expect(renderBodies(calls, before).length).toBeGreaterThan(0));
    await settle();
    expect(renderBodies(calls, before)).toHaveLength(1);
    expect(renderBodies(calls, before)[0]).toMatchObject({ device: "new-device", screen: "local/a" });

    const key = await waitFor(() => document.querySelector("#cfg-plain-key"));
    const list = document.getElementById(key.getAttribute("list"));
    await waitFor(() => expect(list.querySelector("option")?.value).toBe("11:22:33:44:55:66"));
    await edit("#cfg-plain-key", "11:22:33:44:55:66");
    await edit("#cfg-plain-name", "Hall");
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(subject.key).toBe("device:11:22:33:44:55:66"));
    expect(save).toHaveBeenCalledWith("new-device", { key: "11:22:33:44:55:66", name: "Hall" });
    const row = await within(library).findByRole("button", { name: /^Hall · / });
    expect(row).toHaveAttribute("aria-current", "true");
  });
});
