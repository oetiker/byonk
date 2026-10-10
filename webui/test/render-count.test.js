// Task U4: one render request per user-visible change, none for a change
// that does not alter the request. Every extra request runs the screen's Lua
// and a full render on the server -- for a screen that fetches remote data
// that is seconds of work, so these count calls, not just the final picture.
//
// Renders here answer after RENDER_MS, longer than PreviewPane's 250 ms busy
// delay: the over-firing this file pins down only shows for a render slow
// enough to raise the "Rendering..." indicator.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import PreviewPane from "../src/lib/components/PreviewPane.svelte";
import SimulatedInputs from "../src/lib/components/SimulatedInputs.svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

const RENDER_MS = 400;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const PANELS = [
  { id: "e1004", name: "E1004", width: 800, height: 480, colors: "#000000,#FFFFFF",
    colors_actual: "#101010,#E8E8E8", dither: null },
];
const DEVICES = [
  { id: "kitchen", name: "Kitchen", screen: "local/a", panel: "e1004", params: {}, base: {}, final: null },
];
// Measured inks resolve, so the "show measured inks" box is enabled.
const HEADER = JSON.stringify({ colors_actual: { effective: "#101010,#E8E8E8", from: "panel", rungs: {} } });

/** A slow render API; returns the spy and a reader for the bodies it was sent. */
function slowRender() {
  const spy = vi.spyOn(api, "renderPreview").mockImplementation(async () => {
    await wait(RENDER_MS);
    return {
      ok: true,
      headers: { get: (h) => (h.toLowerCase() === "x-byonk-resolution" ? HEADER : null) },
      blob: async () => new Blob(["png"], { type: "image/png" }),
    };
  });
  return { spy, bodies: () => spy.mock.calls.map(([b]) => b) };
}

function resetStores() {
  subject.reset();
  resolution.reset();
  configDraft.reset();
  simulate.reset();
  simulate.error = null;
  screens.meta = { devices: structuredClone(DEVICES), panels: structuredClone(PANELS), dither_algorithms: [] };
}

describe("PreviewPane issues one render per change", () => {
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    resetStores();
  });
  afterEach(() => cleanup());

  it("selecting a screen renders once, even when the render is slow enough to show the indicator", async () => {
    const r = slowRender();
    render(PreviewPane);
    subject.select("screen", "local/a");
    await waitFor(() => expect(r.spy).toHaveBeenCalledTimes(1));
    await wait(3 * RENDER_MS);
    expect(r.spy).toHaveBeenCalledTimes(1);
  });

  it("toggling measured inks renders exactly once more, and toggling back once more again", async () => {
    const r = slowRender();
    subject.select("screen", "local/a");
    subject.target = { kind: "panel", id: "e1004" };
    render(PreviewPane);
    await waitFor(() => expect(resolution.payload).not.toBe(null));
    await wait(3 * RENDER_MS);
    const before = r.spy.mock.calls.length;
    const box = dom.getByRole("checkbox", { name: /measured inks/i });

    await fireEvent.click(box);
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(1);
    expect(r.bodies().at(-1).view.use_actual).toBe(false);

    await fireEvent.click(box);
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(2);
    expect(r.bodies().at(-1).view.use_actual).toBe(true);
  });

  it("typing a simulated input renders once after the pause, not once per keystroke", async () => {
    const r = slowRender();
    subject.select("screen", "local/a");
    render(PreviewPane);
    render(SimulatedInputs);
    await waitFor(() => expect(r.spy).toHaveBeenCalledTimes(1));
    await wait(3 * RENDER_MS);
    const before = r.spy.mock.calls.length;

    const battery = document.querySelector("#sim-battery");
    for (const v of ["4", "3.9", "3.8"]) {
      battery.value = v;
      await fireEvent.input(battery);
      await wait(30);
    }
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(1);
    expect(r.bodies().at(-1).simulated.battery_voltage).toBe(3.8);
  });

  it("a write that leaves the request unchanged renders nothing", async () => {
    const r = slowRender();
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(r.spy).toHaveBeenCalledTimes(1));
    await wait(3 * RENDER_MS);

    // What `screens.refresh()` and `configDraft.loadUsage()` do when the
    // server answers the same as before: a new object, the same content.
    screens.meta = JSON.parse(JSON.stringify(screens.meta));
    configDraft.usage = JSON.parse(JSON.stringify(configDraft.usage));
    subject.select("screen", "local/a");
    await wait(3 * RENDER_MS);
    expect(r.spy).toHaveBeenCalledTimes(1);
  });
});

describe("PreviewPane fix round 1", () => {
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    resetStores();
  });
  afterEach(() => cleanup());

  it("mode c with invalid device params: toggling inks reports the error instead of throwing", async () => {
    const r = slowRender();
    const errors = [];
    const onError = (e) => errors.push(e);
    window.addEventListener("error", onError);
    try {
      subject.select("device", "kitchen");
      render(PreviewPane);
      await waitFor(() => expect(r.spy).toHaveBeenCalledTimes(1));
      configDraft.setPlain("device", "kitchen", "params", "{");
      await waitFor(() => expect(simulate.error?.message).toMatch(/Invalid JSON in the device's params/));
      simulate.error = null;

      await fireEvent.click(dom.getByRole("checkbox", { name: /measured inks/i }));
      await waitFor(() => expect(simulate.error?.message).toMatch(/Invalid JSON in the device's params/));
      expect(r.spy).toHaveBeenCalledTimes(1);
      expect(errors).toEqual([]);
    } finally {
      window.removeEventListener("error", onError);
    }
  });

  it("an unchanged request is retried after the previous render failed", async () => {
    let fail = true;
    const spy = vi.spyOn(api, "renderPreview").mockImplementation(async () =>
      fail
        ? { ok: false, status: 500, statusText: "x", json: async () => ({ error: "boom" }) }
        : { ok: true, headers: { get: () => null }, blob: async () => new Blob(["png"]) },
    );
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(simulate.error?.message).toBe("boom"));
    fail = false;
    // Type a value and take it back within the pause: the request that
    // follows is byte-identical to the one that failed. It must go out.
    simulate.battery_voltage = "4";
    await tick();
    simulate.battery_voltage = "";
    await waitFor(() => expect(simulate.error).toBe(null));
    expect(spy).toHaveBeenCalledTimes(2);
  });
});

// --- through AppShell: saving and the file watcher --------------------------

class FakeEventSource {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.listeners = {};
    FakeEventSource.instances.push(this);
  }
  addEventListener(type, cb) {
    (this.listeners[type] ??= []).push(cb);
  }
  close() {}
  emit(type, data) {
    for (const cb of this.listeners[type] ?? []) cb({ data });
  }
}

describe("saving and the file watcher re-render the preview once", () => {
  let realEventSource;
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    localStorage.clear();
    layout.reset(); theme.reset(); auth.reset(); editor.reset(); editor.files = [];
    resetStores();
    screens.repos = []; screens.list = [];
    realEventSource = globalThis.EventSource;
    globalThis.EventSource = FakeEventSource;
    FakeEventSource.instances = [];
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "local/a" }], devices: DEVICES, panels: PANELS, dither_algorithms: [] });
    vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- disk", etag: '"1"' });
    vi.spyOn(api, "checkFile").mockResolvedValue({ issues: [] });
    vi.spyOn(api, "validateScreen").mockResolvedValue({ clean: ["script.lua"], issues: [] });
    vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"] }, unseen: [] });
  });
  afterEach(() => {
    cleanup();
    if (realEventSource === undefined) delete globalThis.EventSource;
    else globalThis.EventSource = realEventSource;
  });

  async function mountOnScreen() {
    const r = slowRender();
    render(AppShell);
    await waitFor(() => expect(screens.meta.panels).toHaveLength(1));
    await tick();
    await fireEvent.click(await dom.findByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    await waitFor(() => expect(r.spy).toHaveBeenCalled());
    await wait(3 * RENDER_MS);
    return r;
  }

  it("a file-change event from the watcher renders the current screen once", async () => {
    const r = await mountOnScreen();
    const before = r.spy.mock.calls.length;
    FakeEventSource.instances[0].emit("file-change", JSON.stringify(["script.lua"]));
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(1);
    expect(r.bodies().at(-1).screen).toBe("local/a");
  });

  it("a save renders once, and the watcher's echo of that save adds nothing", async () => {
    const r = await mountOnScreen();
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    const before = r.spy.mock.calls.length;
    editor.setText("-- mine");
    await editor.trySave();
    // The server's watcher reports our own write a moment later.
    await wait(250);
    FakeEventSource.instances[0].emit("file-change", JSON.stringify(["script.lua"]));
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(1);
  });

  // L4: the panes are a Resizable group now. Resizing the preview's column,
  // or collapsing and expanding a neighbour, changes no request field.
  it("resizing panes and toggling the Library pane render nothing", async () => {
    const r = await mountOnScreen();
    const before = r.spy.mock.calls.length;
    const editHandle = dom.getAllByRole("separator").find((el) => el.getAttribute("aria-controls") === "editor");
    await fireEvent.keyDown(editHandle, { key: "ArrowLeft" });
    await fireEvent.keyDown(editHandle, { key: "ArrowRight" });
    const rail = dom.getByRole("toolbar", { name: /panels/i });
    await fireEvent.click(dom.getByRole("button", { name: "Library", container: rail }));
    await fireEvent.click(dom.getByRole("button", { name: "Library", container: rail }));
    await wait(3 * RENDER_MS);
    expect(r.spy.mock.calls.length - before).toBe(0);
  });
});
