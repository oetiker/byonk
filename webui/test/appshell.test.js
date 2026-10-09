import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";

// The brief's own AppShell code block (task-5-brief.md Step 5) restores
// persisted layout from `onMount(() => layout.load())` and then wires
// `bind:size` straight into an `$effect` that calls `layout.setSize()` on
// every change. Both are load-bearing for the shell and neither is
// exercised by any test the brief specifies (only iconrail.test.js is
// required). These tests pin the two deviations this implementation makes
// from that code block:
//   1. layout.load() must run before xSize is read, not in onMount,
//      or a persisted split size is silently dropped on first paint.
//   2. the persisted write (layout.setSize -> save -> localStorage) must be
//      debounced, not fired synchronously on every bound-size change
//      (Ruling 12).
//   3. review fix round 1, finding 2: mounting with no interaction at all
//      must not schedule a write either -- xSize starts out equal to
//      what `layout` already holds (they were just read from it), so
//      persisting them again 200ms later would re-save the exact value
//      just loaded, for no visible effect, on every single page load.

function findVerticalSeparator() {
  // The outer (axis="x") SplitPane's separator is aria-orientation=
  // "vertical" per SplitPane.svelte; the (retired) inner axis="y" one was
  // "horizontal". Both share the same aria-label ("Resize panel"), so
  // disambiguate by orientation rather than by accessible name.
  return screen
    .getAllByRole("separator")
    .find((el) => el.getAttribute("aria-orientation") === "vertical");
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
  it("always shows the simulated inputs under the preview, with no toggle", () => {
    render(AppShell);
    expect(screen.getByTestId("simulated-inputs")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Simulated inputs/ })).toBeNull();
    // Class contract: natural height in a capped, scrolling, non-shrinking box.
    const box = screen.getByTestId("simulated-inputs").parentElement.className.split(/\s+/);
    expect(box).toEqual(expect.arrayContaining(["shrink-0", "overflow-auto", "max-h-[50%]"]));
  });

  it("applies a persisted split size on first paint, not only after mount", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        panels: { library: true, editor: true, preview: true },
        sizes: { x: 555 },
      }),
    );

    render(AppShell);

    const separator = findVerticalSeparator();
    expect(separator).toHaveAttribute("aria-valuenow", "555");
  });

  it("debounces the persisted write instead of saving on every bound-size change", async () => {
    vi.useFakeTimers();
    // Spy on layout.setSize() rather than Storage.prototype.setItem: the y
    // axis has its own, independent debounced effect that also settles at
    // the same 200ms mark from mount, so a raw localStorage.setItem count
    // would conflate the two axes. setSize("x", ...) call count isolates
    // the property this test is actually about.
    const setSizeSpy = vi.spyOn(layout, "setSize");

    render(AppShell);
    const separator = findVerticalSeparator();

    // Fire several rapid changes, as a pointermove-driven drag would.
    for (let i = 0; i < 5; i++) {
      await fireEvent.keyDown(separator, { key: "ArrowLeft" }); // -12 each
    }

    // No persisted write yet -- not even the mount-time effect's own first
    // scheduled write has fired, because no fake timer has been advanced.
    // Each subsequent change cancels the previous pending write, so only
    // one write is ever queued at a time.
    expect(setSizeSpy).not.toHaveBeenCalled();
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();

    await vi.advanceTimersByTimeAsync(200);

    // Exactly one write reaches the store for the x axis, carrying the
    // settled value -- not one per keystroke.
    const xCalls = setSizeSpy.mock.calls.filter(([axis]) => axis === "x");
    expect(xCalls).toEqual([["x", 360]]);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes.x).toBe(360);
  });

  it("does not persist on mount when nothing has changed since load", async () => {
    // Review fix round 1, finding 2. No drag, no keypress -- just mount and
    // let the debounce window fully elapse for both axes.
    vi.useFakeTimers();
    const setSizeSpy = vi.spyOn(layout, "setSize");

    render(AppShell);
    await vi.advanceTimersByTimeAsync(200);

    expect(setSizeSpy).not.toHaveBeenCalled();
    // save() is layout's own localStorage.setItem; confirm no write at the
    // storage level too, not just that our spy on setSize saw nothing.
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });

  it("the Edit panel asks for a subject when there is none", async () => {
    render(AppShell);
    expect(await screen.findByText("Pick a screen, panel or device in the Library.")).toBeTruthy();
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
