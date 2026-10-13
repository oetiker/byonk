import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";

// Pane sizes are PaneForge's (percent, `autoSaveId`); open/closed state is
// `layout`'s. These pin the shell-level contract of both: the persisted
// layout applies on mount, PaneForge debounces its own writes, and a plain
// page load does not re-write byonk's layout record.
const PANES_KEY = "paneforge:byonk.panes.v2";

function editHandle() {
  return screen.getAllByRole("separator").find((el) => el.getAttribute("aria-controls") === "editor");
}

beforeEach(() => {
  cleanup();
  localStorage.clear();
  layout.reset();
  subject.reset();
  editor.reset();
  editor.files = [];
});

afterEach(() => {
  vi.useRealTimers();
});

describe("AppShell", () => {
  it("keeps status messages out of the flow above the panes (U12)", async () => {
    const { configDraft } = await import("../src/lib/state/configDraft.svelte.js");
    editor.error = { op: "save", message: "syntax error near x" };
    const { container } = render(AppShell);
    const root = container.firstElementChild;
    // The shell's only flow child is the pane row: nothing sits above it.
    expect(root.children.length).toBe(1);
    const row = root.firstElementChild;
    // The strip lives in an absolutely positioned overlay inside that row.
    const msg = screen.getByText(/syntax error near x/);
    const overlay = screen.getByTestId("status-overlay");
    expect(row.contains(overlay)).toBe(true);
    expect(overlay.contains(msg)).toBe(true);
    expect(overlay.className.split(/\s+/)).toEqual(expect.arrayContaining(["absolute", "bottom-0"]));
    expect(row.className.split(/\s+/)).toContain("relative");
    editor.error = null;
    configDraft.reset?.();
  });

  it("puts the conflict banner in the same non-shifting overlay (U12)", async () => {
    editor.conflict = { name: "script.lua", source: "watch" };
    render(AppShell);
    const overlay = screen.getByTestId("status-overlay");
    expect(overlay.contains(screen.getByRole("button", { name: "Reload" }))).toBe(true);
    editor.conflict = null;
  });

  it("always shows the simulated inputs under the preview, with no toggle", () => {
    render(AppShell);
    expect(screen.getByTestId("simulated-inputs")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Simulated inputs/ })).toBeNull();
    // Class contract: natural height in a box capped at half the column that
    // does not shrink; the body under the fixed header is the scroller.
    const box = screen.getByTestId("simulated-inputs").parentElement.className.split(/\s+/);
    expect(box).toEqual(expect.arrayContaining(["shrink-0", "max-h-[50%]"]));
    const body = screen.getByTestId("simulated-inputs-body").className.split(/\s+/);
    expect(body).toEqual(expect.arrayContaining(["min-h-0", "overflow-auto"]));
  });

  it("gives the simulated inputs a section-height header titled 'Simulated inputs'", () => {
    render(AppShell);
    const region = screen.getByRole("region", { name: "Simulated inputs" });
    const heading = screen.getByRole("heading", { name: "Simulated inputs" });
    expect(region.contains(heading)).toBe(true);
    expect(heading.parentElement.className).toMatch(/\bh-\(--section-h\)/);
    expect(heading.parentElement.className).toMatch(/\bshrink-0\b/);
  });

  it("applies a persisted pane layout on mount", async () => {
    render(AppShell);
    await fireEvent.keyDown(editHandle(), { key: "ArrowLeft" }); // 42 -> 32
    await waitFor(() => expect(localStorage.getItem(PANES_KEY)).toContain("[18,32,50]"));
    cleanup();
    layout.reset();

    render(AppShell);
    await tick();
    expect(editHandle()).toHaveAttribute("aria-valuenow", "32");
  });

  it("writes the pane layout once a burst of resizes settles, not once per step", async () => {
    vi.useFakeTimers();
    render(AppShell);
    await vi.advanceTimersByTimeAsync(200); // let the mount-time write settle
    const setItem = vi.spyOn(Storage.prototype, "setItem");

    for (const key of ["ArrowLeft", "ArrowRight", "ArrowLeft"]) {
      await fireEvent.keyDown(editHandle(), { key }); // 42 -> 32 -> 42 -> 32
    }
    const paneWrites = () => setItem.mock.calls.filter(([k]) => k === PANES_KEY);
    expect(paneWrites()).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(200);
    expect(paneWrites()).toHaveLength(1);
    expect(paneWrites()[0][1]).toContain("[18,32,50]"); // the settled layout
  });

  it("does not re-write the layout record on mount when nothing has changed", async () => {
    // Review fix round 1, finding 2, carried over: a plain page load must
    // not re-persist the open/closed record it just loaded.
    vi.useFakeTimers();
    const setItem = vi.spyOn(Storage.prototype, "setItem");

    render(AppShell);
    await vi.advanceTimersByTimeAsync(200);

    expect(setItem.mock.calls.filter(([k]) => k === STORAGE_KEY)).toHaveLength(0);
  });

  it("the Edit panel asks for a subject when there is none", async () => {
    render(AppShell);
    expect(await screen.findByText("Pick a screen, panel, device or screen repo in the Library.")).toBeTruthy();
  });
});

describe("AppShell initial data load", () => {
  // Task 14's browser walk found the SPA unusable on a fresh load: the
  // screen list, the Device and Panel Profile menus and the Dither menu
  // were all empty, while `/dev/api/repos` and `/dev/api/screens` both
  // answered 200 with full payloads. `screens.refresh()` was reachable
  // only from FilesPane's four buttons and from `tune`, and three of those
  // four buttons need a screen already selected -- so nothing populated the
  // store at boot and the only unblocked action was "New".
  //
  // Every FilesPane/simulate test calls `screens.refresh()` in its own
  // beforeEach, which is why the whole suite passed over this: the store
  // was hand-fed in the tests and fed by nobody in the app.
  it("populates repos, screens and meta on mount, with no user action", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "local/a" }],
      devices: [{ id: "d1" }],
      panels: [{ name: "p1" }],
      dither_algorithms: ["floyd-steinberg"],
    });
    screens.repos = [];
    screens.list = [];
    screens.meta = { devices: [], panels: [], dither_algorithms: [] };

    render(AppShell);

    await waitFor(() => expect(screens.repos).toHaveLength(1));
    expect(screens.list.map((s) => s.ref)).toEqual(["local/a"]);
    expect(screens.meta.dither_algorithms).toEqual(["floyd-steinberg"]);
    // `grouped` is what FilesPane renders; it derives from `repos`, so an
    // empty repo list hides every screen even when `list` is full.
    expect(screens.grouped).toHaveLength(1);
  });
});

describe("AppShell wires the subject to editor.loadScreen (Task 1, the journey test)", () => {
  // This is the point of the whole plan. The rewrite's own component tests
  // all passed while the app itself loaded nothing (see the boot-load test
  // above, and 32106892's report of the identical class of defect): every
  // one of them hand-fed the store it was testing in its own beforeEach.
  // This test does NOT call `screens.refresh()`, `editor.loadScreen()` or
  // `editor.open()` itself -- it mounts the real app root and drives it
  // only through `subject.select()`, the same call FilesPane's click
  // handler makes, then asserts on data the app itself must have put there.
  it("lists a selected screen's files and opens script.lua, with no direct store call", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
    vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [{ name: "local/a" }] });
    vi.spyOn(api, "listFiles").mockResolvedValue([
      { name: "meta.yaml", size: 10, text: true },
      { name: "script.lua", size: 20, text: true },
      { name: "screen.svg", size: 30, text: true },
    ]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });

    render(AppShell);

    subject.select("screen", "local/a");

    await waitFor(() =>
      expect(editor.files.map((f) => f.name)).toEqual(["meta.yaml", "script.lua", "screen.svg"]),
    );
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    expect(editor.active?.text).toBe("-- lua");

    // The tab strip (EditorPane) rendered real tabs from that listing --
    // not just the store fields in isolation.
    await waitFor(() => expect(screen.getAllByRole("tab").length).toBe(3));
  });

  // Fix round 1: the brief's Step 2 named two properties the effect "must
  // have, both testable" -- neither had a test. Inspection alone is what
  // let the original unwired-seam defect ship (see the journey test's own
  // comment above), so these drive the real effect through the real app
  // root, the same as the journey test.
  it("does not re-run when screens.refresh() replaces repos/list for an unrelated reason, so an open buffer survives", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
    vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [{ name: "local/a" }] });
    const listFilesSpy = vi
      .spyOn(api, "listFiles")
      .mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });

    render(AppShell);
    subject.select("screen", "local/a");
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));

    editor.setText("unsaved edit"); // dirty the open buffer
    expect(listFilesSpy).toHaveBeenCalledTimes(1);

    // Something unrelated to the subject replaces repos/list -- e.g. a
    // rename or delete elsewhere causing FilesPane to call screens.refresh().
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "local", writable: true },
      { handle: "other", writable: true },
    ]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "local/a" }, { name: "other/b" }],
    });
    await screens.refresh();
    await tick();

    // If the effect had re-run, loadScreen()'s reset() would have discarded
    // the unsaved edit and listFiles would have been called again.
    expect(listFilesSpy).toHaveBeenCalledTimes(1);
    expect(editor.active?.text).toBe("unsaved edit");
  });

  it("watchConnected survives a screen switch -- StatusBar's 'watch: connected' must not flip for a screen change", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "local/a" }, { name: "local/b" }],
    });
    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });

    render(AppShell);
    subject.select("screen", "local/a");
    await waitFor(() => expect(editor.active?.ref).toBe("local/a"));

    // Stand-in for `es.onopen` having already fired -- jsdom has no
    // EventSource (see AppShell's own onMount guard), so this simulates the
    // live-connection state StatusBar's "watch: connected" reflects.
    editor.watchConnected = true;

    subject.select("screen", "local/b");
    await waitFor(() => expect(editor.active?.ref).toBe("local/b"));

    expect(editor.watchConnected).toBe(true);
  });

  it("selecting a panel does not reload the editor, and coming back to the same screen does not either", async () => {
    const listFiles = vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
    render(AppShell);
    subject.select("screen", "local/a");
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("dirty");
    subject.select("panel", "e1004");
    await tick();
    subject.select("screen", "local/a");
    await tick();
    expect(listFiles).toHaveBeenCalledTimes(1);
    expect(editor.active.text).toBe("dirty");
    expect(editor.active.dirty).toBe(true);
  });

  it("a subject switch away from screens leaves screens.error alone (R7)", async () => {
    render(AppShell);
    screens.error = { op: "files", message: "boom" };
    subject.select("device", "kitchen");
    await tick();
    expect(screens.error).toEqual({ op: "files", message: "boom" });
  });
});
