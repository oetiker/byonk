import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * Resizing a pane and finding the same layout after a reload.
 *
 * The panes are a PaneForge Resizable group (Library | Edit | Preview),
 * sized in percent and persisted by PaneForge itself under
 * `paneforge:<autoSaveId>`. Which panels are open is persisted separately by
 * `layout`. This file drives both through the real shell -- a pointer drag
 * on a handle, the rail -- and then remounts, which is the only thing a user
 * actually does, and it crosses the AppShell/PaneForge/layout seam.
 */
const PANES_KEY = "paneforge:byonk.panes.v2";

/**
 * A measurable extent for the pane GROUP only.
 *
 * PaneForge turns a drag into percent by dividing the pointer offset by the
 * group element's width, and jsdom reports every element as 0x0. Stubbing
 * `Element.prototype` wholesale cannot be done here: this file mounts the
 * whole shell, and a non-zero rect makes CodeMirror attempt a real text
 * measurement in its rAF callback, which throws out of jsdom as an
 * unhandled exception rather than a test failure. So the stub is scoped to
 * the group element by its data attribute.
 */
function stubGroupRect({ width = 1000, height = 800 } = {}) {
  const real = Element.prototype.getBoundingClientRect;
  return vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (!this.hasAttribute?.("data-pane-group")) return real.call(this);
    return { width, height, top: 0, left: 0, right: width, bottom: height, x: 0, y: 0, toJSON() {} };
  });
}

/** The handle that resizes a pane: left of Edit for "library", right of Edit for "editor". */
function handle(pane) {
  return dom.getAllByRole("separator").find((el) => el.getAttribute("aria-controls") === pane);
}
const sizeOf = (pane) => Number(handle(pane).getAttribute("aria-valuenow"));
const saved = () => JSON.parse(localStorage.getItem(PANES_KEY) ?? "null");
const savedLayout = () => Object.values(saved() ?? {})[0]?.layout;

const paneEl = (id) => document.querySelector(`[data-pane-id="${id}"]`);
/** The pane's flex-grow, i.e. its percent of the row (jsdom computes no widths). */
const flexOf = (id) => Number(/flex:\s*([\d.]+)/.exec(paneEl(id).getAttribute("style"))[1]);
const LABELS = { library: "Library", editor: "Edit", preview: "Preview" };
function railButton(name) {
  return dom.getByRole("button", { name, container: dom.getByRole("toolbar", { name: /panels/i }) });
}
/** For every pane: rail pressed <=> pane rendered with a size above 0. */
function expectRailMatchesPanes() {
  for (const [id, label] of Object.entries(LABELS)) {
    const pressed = railButton(label).getAttribute("aria-pressed") === "true";
    const shown = !!paneEl(id) && flexOf(id) > 0;
    expect(shown, `${id}: aria-pressed=${pressed}, rendered>0=${shown}`).toBe(pressed);
  }
}

/** One complete pointer drag of `dx` px on a 1000px group, as a user performs it. */
async function drag(el, dx) {
  await fireEvent.mouseDown(el, { clientX: 500, clientY: 10 });
  await fireEvent.mouseMove(document.body, { clientX: 500 + dx, clientY: 10 });
  await fireEvent.mouseUp(window, { clientX: 500 + dx, clientY: 10 });
}

let rectSpy;

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
  editor.reset();
  editor.files = [];
  subject.reset();
  simulate.reset();
  screens.repos = [];
  screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  rectSpy = stubGroupRect();
});

afterEach(() => {
  rectSpy?.mockRestore();
  vi.useRealTimers();
  cleanup();
});

describe("pane resize, persisted across a reload", () => {
  it("starts at the percent defaults: Library 18, Edit 42, Preview 40", () => {
    render(AppShell);
    expect(sizeOf("library")).toBe(18);
    expect(sizeOf("editor")).toBe(42);
  });

  it("a dragged handle still holds its size after the app is remounted", async () => {
    render(AppShell);

    // -100px of a 1000px group: Edit loses 10 points to Preview.
    await drag(handle("editor"), -100);
    await waitFor(() => expect(sizeOf("editor")).toBe(32));
    await waitFor(() => expect(savedLayout()).toEqual([18, 32, 50]));

    // Reload: drop the DOM and the in-memory store, then remount.
    cleanup();
    layout.reset();
    render(AppShell);

    await waitFor(() => expect(sizeOf("editor")).toBe(32));
    expect(sizeOf("library")).toBe(18);
  });

  it("Library is resizable too, and never below its 12% minimum", async () => {
    render(AppShell);
    await drag(handle("library"), 50);
    await waitFor(() => expect(sizeOf("library")).toBe(23));
    // Back past the minimum but not far enough to collapse: clamps at 12.
    await drag(handle("library"), -150);
    await waitFor(() => expect(sizeOf("library")).toBe(12));
  });

  it("the handles are keyboard-resizable and carry a visible grip", async () => {
    render(AppShell);
    const h = handle("editor");
    expect(h).toHaveAttribute("tabindex", "0");
    expect(h.querySelector("div")).toBeTruthy(); // withHandle's grip
    await fireEvent.keyDown(h, { key: "ArrowRight" });
    await waitFor(() => expect(sizeOf("editor")).toBe(52));
  });

  it("has no horizontal splitter: the inputs sit under the preview at natural height", () => {
    render(AppShell);
    for (const sep of dom.getAllByRole("separator")) {
      expect(sep.getAttribute("data-direction")).toBe("horizontal");
    }
    expect(dom.getByTestId("simulated-inputs")).toBeTruthy();
  });

  it("ignores the px sizes an older layout record carried and never writes them back", async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      panels: { library: true, editor: true, preview: true },
      sizes: { x: 300, y: 200 },
    }));
    layout.reset();
    render(AppShell);
    expect(sizeOf("editor")).toBe(42);
    await fireEvent.click(dom.getByRole("button", { name: /^library$/i, container: dom.getByRole("toolbar", { name: /panels/i }) }));
    await waitFor(() => expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).panels.library).toBe(false));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes).toBeUndefined();
  });

  // A closed panel reopens at the size it had, in this session and after a
  // reload (layout.svelte.js header: "a panel that reopens at a default size
  // instead of the size it had reads as a bug").
  it("closing a panel after a resize keeps its size, and reopening restores it", async () => {
    render(AppShell);

    await drag(handle("library"), 70); // Library 18 -> 25
    await waitFor(() => expect(sizeOf("library")).toBe(25));
    await new Promise((r) => setTimeout(r, 150)); // PaneForge's debounced save

    await fireEvent.click(railButton("Library"));
    await waitFor(() => expect(paneEl("library")).toBeNull());
    expectRailMatchesPanes();

    cleanup();
    layout.reset();
    render(AppShell);

    expect(dom.queryByRole("button", { name: "New" })).toBeNull();
    expect(paneEl("library")).toBeNull();

    await fireEvent.click(railButton("Library"));
    await waitFor(() => expect(sizeOf("library")).toBe(25));
    expect(dom.getByRole("button", { name: "New" })).toBeTruthy();
    expectRailMatchesPanes();
  });

  // Panes are not collapsible: only the rail opens and closes them, and a
  // drag stops at the pane's minimum -- a pane never sits at 0 under a
  // pressed rail button.
  it("a drag past the minimum stops there; the pane stays open", async () => {
    render(AppShell);
    await drag(handle("library"), -170);
    await waitFor(() => expect(sizeOf("library")).toBe(12));
    expect(layout.isOpen("library")).toBe(true);
    expectRailMatchesPanes();
  });

  // Fix round 1, finding 1 (kept). With only Library open there is nothing
  // to resize against: no handle, Library fills the row, nothing reopens.
  it("with Edit and Preview closed, Library fills the row and nothing reopens", async () => {
    render(AppShell);
    await fireEvent.click(railButton("Edit"));
    await fireEvent.click(railButton("Preview"));
    await waitFor(() => expect(paneEl("preview")).toBeNull());
    // The only pane in the group: it fills the row (PaneForge styles a lone
    // pane flex 1, not 100).
    expect(document.querySelectorAll("[data-pane]")).toHaveLength(1);
    expect(flexOf("library")).toBeGreaterThan(0);
    expect(dom.queryAllByRole("separator")).toHaveLength(0);
    await new Promise((r) => setTimeout(r, 50));
    expect(railButton("Edit")).toHaveAttribute("aria-pressed", "false");
    expect(railButton("Preview")).toHaveAttribute("aria-pressed", "false");
    expect(dom.queryByRole("tablist")).toBeNull();
    expectRailMatchesPanes();
  });

  it("reopening Library after all three were closed gives it the whole row", async () => {
    render(AppShell);
    for (const n of ["Library", "Edit", "Preview"]) await fireEvent.click(railButton(n));
    expect(document.querySelectorAll("[data-pane]")).toHaveLength(0);
    await fireEvent.click(railButton("Library"));
    await waitFor(() => expect(document.querySelectorAll("[data-pane]")).toHaveLength(1));
    expect(flexOf("library")).toBeGreaterThan(0);
    expectRailMatchesPanes();
  });

  // Fix round 2, finding 2: Library stays resizable with Edit closed.
  it("Library is resizable with Edit closed, and Edit stays closed", async () => {
    render(AppShell);
    await fireEvent.click(railButton("Edit"));
    await waitFor(() => expect(paneEl("editor")).toBeNull());
    const before = sizeOf("library");
    await drag(handle("library"), 100); // +10 points
    await waitFor(() => expect(sizeOf("library")).toBeCloseTo(before + 10, 0));
    expect(railButton("Edit")).toHaveAttribute("aria-pressed", "false");
    expectRailMatchesPanes();
  });

  // Fix round 2, finding 1, Repro A: arrow Edit down hard, then toggle.
  it("Repro A: arrowing Edit to its limit and toggling never leaves a pressed pane at 0", async () => {
    render(AppShell);
    for (let i = 0; i < 5; i++) await fireEvent.keyDown(handle("editor"), { key: "ArrowLeft" });
    await waitFor(() => expect(sizeOf("editor")).toBe(25));
    expectRailMatchesPanes();
    for (const n of ["Edit", "Edit", "Library", "Library"]) {
      await fireEvent.click(railButton(n));
      await tick();
      expectRailMatchesPanes();
    }
  });

  // Fix round 2, finding 1, Repro B: the browser's step log, then a reload.
  it("Repro B: any toggle order keeps rail and panes in step, across a reload", async () => {
    render(AppShell);
    await fireEvent.click(railButton("Edit"));
    await fireEvent.click(railButton("Preview")); // Library only
    for (const n of ["Edit", "Edit", "Preview", "Edit", "Preview", "Edit", "Preview", "Edit"]) {
      await fireEvent.click(railButton(n));
      await tick();
      expectRailMatchesPanes();
    }
    await new Promise((r) => setTimeout(r, 150));
    cleanup();
    layout.reset();
    render(AppShell);
    await tick();
    expectRailMatchesPanes();
  });

  // The layout is a convenience; losing it must never break the editor.
  it("survives storage being unavailable -- the resize still works, it just is not remembered", async () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("QuotaExceededError");
    });

    render(AppShell);
    await drag(handle("editor"), -100);
    await waitFor(() => expect(sizeOf("editor")).toBe(32));
    await waitFor(() => expect(setItem).toHaveBeenCalled());
    expect(dom.getByRole("toolbar", { name: /panels/i })).toBeTruthy();
  });

  // Corrupt stored state must not take the page down with it.
  it("falls back to the defaults when the stored records are corrupt", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    localStorage.setItem(PANES_KEY, "{not json");

    render(AppShell);

    expect(sizeOf("editor")).toBe(42);
    expect(dom.getByRole("button", { name: "New" })).toBeTruthy();
  });
});
