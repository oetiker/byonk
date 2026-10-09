import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * Dragging a splitter and finding the same layout after a reload.
 *
 * The two halves are each already covered, separately and by neither's
 * counterpart: `splitpane.test.js` drives a real pointer drag against a
 * standalone harness and asserts `aria-valuenow`, and `appshell.test.js`
 * drives the persistence with ARROW KEYS. Nothing has ever dragged a
 * splitter in the real shell and then reloaded -- which is the only thing a
 * user actually does, and it crosses the SplitPane/AppShell/layout seam
 * where this branch's defects have all lived.
 *
 * Both axes matter and they are separate effects with separate debounce
 * timers, so both are exercised: x separates the editor from the right-hand
 * column, y separates preview from simulate.
 */
/**
 * A measurable extent for the SplitPane hosts ONLY.
 *
 * SplitPane reads `host.getBoundingClientRect()` to derive the dragged size
 * (`rect.right - clientX` on x, `rect.bottom - clientY` on y), and jsdom
 * reports every element as 0x0, so the drag has to be given a container to
 * measure against. Stubbing `Element.prototype` wholesale is what
 * `splitpane.test.js` does, but it can't be done here: this file mounts the
 * whole shell, and a non-zero rect makes CodeMirror attempt a real text
 * measurement in its rAF callback, which throws out of jsdom
 * (`textRange(...).getClientRects is not a function`) as an unhandled
 * exception rather than a test failure.
 *
 * So the stub is scoped to the hosts by shape -- a SplitPane host is the one
 * element with a separator as a direct child -- and everything else keeps
 * jsdom's own answer.
 */
function stubHostRect({ width = 1000, height = 800 } = {}) {
  const real = Element.prototype.getBoundingClientRect;
  return vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function () {
    if (!this.querySelector?.(':scope > [role="separator"]')) return real.call(this);
    return {
      width,
      height,
      top: 0,
      left: 0,
      right: width,
      bottom: height,
      x: 0,
      y: 0,
      toJSON() {},
    };
  });
}

/**
 * SplitPane gives both separators the same accessible name ("Resize panel"),
 * so they are told apart by orientation: the outer axis="x" one is
 * "vertical", the inner axis="y" one "horizontal".
 */
function separator(orientation) {
  return dom
    .getAllByRole("separator")
    .find((el) => el.getAttribute("aria-orientation") === orientation);
}

/** One complete pointer drag, as a real user performs it. */
async function drag(el, { clientX = 0, clientY = 0 }) {
  await fireEvent.pointerDown(el, { pointerId: 1, clientX: 0, clientY: 0 });
  await fireEvent.pointerMove(window, { pointerId: 1, clientX, clientY });
  await fireEvent.pointerUp(window, { pointerId: 1 });
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
  rectSpy = stubHostRect();
});

afterEach(() => {
  rectSpy?.mockRestore();
  vi.useRealTimers();
  cleanup();
});

describe("splitter drag, persisted across a reload", () => {
  it("a dragged x splitter still holds its size after the app is remounted", async () => {
    render(AppShell);

    const sep = separator("vertical");
    expect(sep).toBeTruthy();
    expect(Number(sep.getAttribute("aria-valuenow"))).toBe(420); // the default

    // axis="x" derives the size from `rect.right - clientX`, so 1000 - 700
    // is a 300px right-hand column -- inside the bounds, not clamped, which
    // is what makes the exact value below meaningful.
    await drag(sep, { clientX: 700 });
    await waitFor(() => expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(300));

    // The write is debounced by 200ms (Ruling 12), so it has NOT landed yet.
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null")?.sizes?.x).not.toBe(300);
    await waitFor(() =>
      expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes.x).toBe(300),
    );

    // Reload: drop the DOM and the in-memory store, then let AppShell's own
    // `layout.load()` restore from storage. The size must be right on the
    // FIRST paint -- a value applied only after mount shows the default for
    // a frame and reads as the layout having been forgotten.
    cleanup();
    layout.reset();
    render(AppShell);

    expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(300);
  });

  it("a dragged y splitter still holds its size after the app is remounted", async () => {
    render(AppShell);

    const sep = separator("horizontal");
    expect(sep).toBeTruthy();
    expect(Number(sep.getAttribute("aria-valuenow"))).toBe(260); // the default

    // axis="y" derives from `rect.bottom - clientY`: 800 - 600 = 200.
    await drag(sep, { clientY: 600 });
    await waitFor(() =>
      expect(Number(separator("horizontal").getAttribute("aria-valuenow"))).toBe(200),
    );
    await waitFor(() => expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes.y).toBe(200));

    cleanup();
    layout.reset();
    render(AppShell);

    expect(Number(separator("horizontal").getAttribute("aria-valuenow"))).toBe(200);
  });

  // The two axes have independent debounced effects writing into ONE
  // persisted record. A write that serialised the whole record from a stale
  // snapshot would let the later axis silently undo the earlier one.
  it("dragging both axes persists both, not just the last one", async () => {
    render(AppShell);

    await drag(separator("vertical"), { clientX: 700 });
    await drag(separator("horizontal"), { clientY: 600 });

    await waitFor(() => {
      const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
      expect(saved.sizes).toEqual({ x: 300, y: 200 });
    });

    cleanup();
    layout.reset();
    render(AppShell);

    expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(300);
    expect(Number(separator("horizontal").getAttribute("aria-valuenow"))).toBe(200);
  });

  // Sizes and panel visibility share one record on purpose (layout.svelte.js
  // header: "a panel that reopens at a default size instead of the size it
  // had reads as a bug"). Closing a panel after a drag must not drop the
  // dragged size, and the size must come back with the panel.
  it("closing a panel after a drag keeps the dragged size, and reopening restores it", async () => {
    render(AppShell);

    await drag(separator("vertical"), { clientX: 700 });
    await waitFor(() => expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes.x).toBe(300));

    const rail = dom.getByRole("toolbar", { name: /panels/i });
    const libraryBtn = dom.getByRole("button", { name: /^library$/i, container: rail });
    await fireEvent.click(libraryBtn);

    await waitFor(() => {
      const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
      expect(saved.panels.library).toBe(false);
      expect(saved.sizes.x).toBe(300);
    });

    cleanup();
    layout.reset();
    render(AppShell);

    expect(dom.queryByRole("button", { name: "New" })).toBeNull();
    expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(300);
  });

  // The layout is a convenience; losing it must never break the editor
  // (layout.svelte.js's own catch blocks say so). A denied or full storage
  // has to leave a working, draggable app behind.
  it("survives storage being unavailable -- the drag still works, it just is not remembered", async () => {
    const setItem = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new Error("QuotaExceededError");
      });

    render(AppShell);
    await drag(separator("vertical"), { clientX: 700 });

    await waitFor(() =>
      expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(300),
    );
    await waitFor(() => expect(setItem).toHaveBeenCalled());
    // Still interactive afterwards: the throw was swallowed, not propagated
    // out of the effect.
    expect(dom.getByRole("toolbar", { name: /panels/i })).toBeTruthy();
  });

  // Corrupt stored state must not take the page down with it.
  it("falls back to the defaults when the stored record is corrupt", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");

    render(AppShell);

    expect(Number(separator("vertical").getAttribute("aria-valuenow"))).toBe(420);
    expect(dom.getByRole("button", { name: "New" })).toBeTruthy();
  });
});
