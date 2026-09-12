import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

// The brief's own AppShell code block (task-5-brief.md Step 5) restores
// persisted layout from `onMount(() => layout.load())` and then wires
// `bind:size` straight into an `$effect` that calls `layout.setSize()` on
// every change. Both are load-bearing for the shell and neither is
// exercised by any test the brief specifies (only iconrail.test.js is
// required). These tests pin the two deviations this implementation makes
// from that code block:
//   1. layout.load() must run before xSize/ySize are read, not in onMount,
//      or a persisted split size is silently dropped on first paint.
//   2. the persisted write (layout.setSize -> save -> localStorage) must be
//      debounced, not fired synchronously on every bound-size change
//      (Ruling 12).
//   3. review fix round 1, finding 2: mounting with no interaction at all
//      must not schedule a write either -- xSize/ySize start out equal to
//      what `layout` already holds (they were just read from it), so
//      persisting them again 200ms later would re-save the exact value
//      just loaded, for no visible effect, on every single page load.

function findVerticalSeparator() {
  // The outer (axis="x") SplitPane's separator is aria-orientation=
  // "vertical" per SplitPane.svelte; the inner axis="y" one is
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
});

afterEach(() => {
  vi.useRealTimers();
});

describe("AppShell", () => {
  it("applies a persisted split size on first paint, not only after mount", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        panels: { files: true, editor: true, preview: true, simulate: true },
        sizes: { x: 555, y: 260 },
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

  it("mounts TunePane only while the tune panel is open", async () => {
    layout.reset();
    render(AppShell);
    expect(screen.queryByTestId("tune-pane")).toBeNull();

    layout.toggle("tune");
    await tick();
    expect(screen.getByTestId("tune-pane")).toBeTruthy();
  });

  it("keeps the horizontal splitter alive when only tune is open on the right", async () => {
    layout.reset();
    for (const p of ["preview", "simulate"]) if (layout.isOpen(p)) layout.toggle(p);
    layout.toggle("tune");
    // The brief's code block for this test never calls render(AppShell),
    // which means screen has nothing mounted and the assertion below would
    // fail unconditionally -- true even with a correct implementation. Add
    // the render call, consistent with every other test in this file.
    render(AppShell);
    await tick();
    // The splitter separates the editor from everything to its right; tune
    // being the only thing there is still something there.
    expect(screen.getByRole("separator", { name: /vertical|horizontal|resize/i })).toBeTruthy();
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
