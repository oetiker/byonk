import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";

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
});
