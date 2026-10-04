import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";

// EditorPane is tested against a mocked codemirror.js: component tests
// cannot drive CodeMirror's real key handling in jsdom (that mechanism is
// covered directly against the real module in editor.test.js, and a real
// keystroke is deferred to Task 14's browser verification -- Ruling 24).
// What lives here is EditorPane's OWN reactivity: whether its `$effect`s
// call into CodeMirror the right number of times for a given store change.
const cmMock = {
  setDoc: vi.fn(),
  setDiagnostics: vi.fn(),
  setTheme: vi.fn(),
  destroy: vi.fn(),
  view: {},
};
let lastOpts = null;

vi.mock("../src/lib/codemirror.js", () => ({
  createEditor: vi.fn((host, opts) => {
    lastOpts = opts;
    return cmMock;
  }),
}));

// Imported after the mock so EditorPane's own `../codemirror.js` import
// resolves to the mock above (vi.mock is hoisted, but importing the
// component after declaring it keeps the intent readable top-to-bottom).
import EditorPane from "../src/lib/components/EditorPane.svelte";

beforeEach(() => {
  cleanup();
  vi.clearAllMocks();
  lastOpts = null;
  editor.reset();
  editor.files = [];
  screens.selected = null;
  screens.repos = [];
});

afterEach(() => {
  vi.useRealTimers();
});

describe("EditorPane", () => {
  it("does not re-run setDoc for a keystroke in the active tab (Ruling 22)", async () => {
    // If the tab-swap effect tracked `t.text`, this onChange -- exactly what
    // a real keystroke triggers via createEditor's updateListener -- would
    // re-run it and call setDoc again, replacing the document under the
    // caret. It must not: the effect keys on ref/name/readOnly only.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "hello", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    render(EditorPane);
    flushSync();
    expect(lastOpts).not.toBeNull();

    const callsAfterMount = cmMock.setDoc.mock.calls.length;

    lastOpts.onChange("hello!");
    flushSync();

    expect(editor.active.text).toBe("hello!");
    expect(cmMock.setDoc).toHaveBeenCalledTimes(callsAfterMount);
  });

  it("does re-run setDoc when the active tab's identity changes", async () => {
    // The companion case to the above: the effect must still fire on an
    // actual tab swap, so the "don't track text" fix isn't just "never call
    // setDoc again".
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    await editor.open("local/a", "two.lua");

    render(EditorPane);
    flushSync();
    const callsAfterMount = cmMock.setDoc.mock.calls.length;

    editor.activeIndex = 0; // switch back to one.lua
    flushSync();

    expect(cmMock.setDoc.mock.calls.length).toBeGreaterThan(callsAfterMount);
    expect(cmMock.setDoc).toHaveBeenLastCalledWith("x", "lua", false);
  });

  it("shows the unsaved-changes marker once the buffer diverges from disk (Ruling 25)", async () => {
    // The store test only reads `editor.active.dirty` directly, which a
    // plain non-reactive getter would also satisfy while `{#if t.dirty}` in
    // the rendered tab strip never updates. This proves the marker actually
    // re-renders.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "hello", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    // The tab strip now renders from `editor.files` (Task 1), not `tabs` --
    // give it a listing entry for the file this test already opened above.
    editor.files = [{ name: "script.lua", size: 5, text: true }];

    const { queryByLabelText } = render(EditorPane);
    flushSync();
    expect(queryByLabelText("unsaved changes")).toBeNull();

    lastOpts.onChange("hello!");
    flushSync();

    expect(queryByLabelText("unsaved changes")).not.toBeNull();
  });

  it("re-runs setDoc when replaceFromDisk swaps the ALREADY-active tab's buffer (Ruling 28, fix round 1)", async () => {
    // Task-7 fix round 1, Important finding: resolveReload() (Task 10) calls
    // replaceFromDisk(c.ref, c.name) on what is typically the currently-open
    // tab -- ref/name/readOnly don't change, so without a dedicated signal
    // (the tab's `revision` counter) the tab-swap effect would never re-run
    // and CodeMirror would keep showing the stale buffer while the store's
    // text/etag/dirty were already correct underneath.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "mine", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    render(EditorPane);
    flushSync();
    const callsAfterMount = cmMock.setDoc.mock.calls.length;

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    await editor.replaceFromDisk("local/a", "script.lua");
    flushSync();

    expect(cmMock.setDoc.mock.calls.length).toBeGreaterThan(callsAfterMount);
    expect(cmMock.setDoc).toHaveBeenLastCalledWith("theirs", "lua", false);
  });

  // --- Ruling 38: save must actually be wired -------------------------------
  //
  // No brief in this plan wired a save action at all: codemirror.js's Mod-s
  // keymap called `opts.onSave`, and this component never passed one --
  // the keybinding was inert. These pin both halves of the fix: the
  // keybinding routes to `editor.trySave()`, AND there is a control a user
  // can find without knowing the keybinding exists.
  it("passes onSave to createEditor, wired to editor.trySave() (Ruling 38)", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "hello", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    const trySaveSpy = vi.spyOn(editor, "trySave").mockResolvedValue();

    render(EditorPane);
    flushSync();

    expect(lastOpts.onSave).toBeTypeOf("function");
    lastOpts.onSave();
    expect(trySaveSpy).toHaveBeenCalledTimes(1);
  });

  it("shows a visible Save control, enabled only while the active tab is dirty, that calls trySave (Ruling 38)", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "hello", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    const trySaveSpy = vi.spyOn(editor, "trySave").mockResolvedValue();

    const { getByRole } = render(EditorPane);
    flushSync();

    const save = getByRole("button", { name: /save/i });
    expect(save).toBeDisabled();

    lastOpts.onChange("hello!"); // dirties the active tab, same as a keystroke
    flushSync();
    expect(save).not.toBeDisabled();

    await fireEvent.click(save);
    expect(trySaveSpy).toHaveBeenCalledTimes(1);
  });

  // --- Ruling 64: syntax checking must actually reach the gutter ----------
  //
  // `editor.svelte.js`'s `check()` existed and worked; NOTHING ever called
  // it. These pin the fix at the level where the actual defect lived: this
  // component's `onChange` wiring, not the store or codemirror.js (both of
  // which were already individually correct -- see editor.test.js's own
  // Ruling-64 coverage for that half).
  describe("syntax checking (Ruling 64)", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    it("debounces a check() call after typing, which reaches CodeMirror's setDiagnostics", async () => {
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "ok", etag: '"1"' });
      await editor.open("local/a", "script.lua");
      vi.spyOn(api, "checkFile").mockResolvedValue({
        ok: false,
        issues: [{ severity: "error", location: "script.lua", message: "boom", line: 1 }],
      });

      render(EditorPane);
      flushSync();
      cmMock.setDiagnostics.mockClear();

      lastOpts.onChange("bad lua");
      flushSync();
      // Not yet -- still inside the debounce window.
      expect(cmMock.setDiagnostics).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1000);
      flushSync();

      expect(editor.active.issues).toHaveLength(1);
      expect(cmMock.setDiagnostics).toHaveBeenLastCalledWith(editor.active.issues);
    });

    it("keys the debounce per tab -- switching tabs must not cancel a pending check for the tab that was actually edited", async () => {
      // The old hand-written UI (static/dev/dev.js's `scheduleCheck`) kept
      // ONE shared timer for every open file: editing file A then switching
      // to file B within the debounce window cancelled A's pending check
      // outright (`clearTimeout(checkTimer)` has no idea which file it was
      // scheduled for). That is a real defect, not a style choice -- worth
      // fixing here rather than porting faithfully.
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "ok", etag: '"1"' });
      await editor.open("local/a", "one.lua");
      await editor.open("local/a", "two.lua");
      const checkFileSpy = vi
        .spyOn(api, "checkFile")
        .mockResolvedValue({ ok: false, issues: [{ severity: "error", message: "boom", line: 1 }] });

      const oneLua = editor.tabs.find((t) => t.name === "one.lua");
      const twoLua = editor.tabs.find((t) => t.name === "two.lua");
      editor.activeIndex = editor.tabs.indexOf(oneLua);

      render(EditorPane);
      flushSync();

      lastOpts.onChange("bad one"); // edits one.lua, schedules its check

      editor.activeIndex = editor.tabs.indexOf(twoLua); // switch away within the window
      flushSync();
      lastOpts.onChange("bad two"); // edits two.lua, schedules ITS OWN check

      await vi.advanceTimersByTimeAsync(1000);
      flushSync();

      // Both files' checks must have gone through -- neither cancelled the
      // other's.
      expect(checkFileSpy).toHaveBeenCalledWith(oneLua.ref, "one.lua", "bad one");
      expect(checkFileSpy).toHaveBeenCalledWith(twoLua.ref, "two.lua", "bad two");
      expect(oneLua.issues).toHaveLength(1);
      expect(twoLua.issues).toHaveLength(1);
    });
  });

  it("passes onCursor to createEditor, wired to editor.setCursor() (spec §1 status bar)", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "hello", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    const setCursorSpy = vi.spyOn(editor, "setCursor");

    render(EditorPane);
    flushSync();

    expect(lastOpts.onCursor).toBeTypeOf("function");
    lastOpts.onCursor(3, 22);
    expect(setCursorSpy).toHaveBeenCalledWith(3, 22);
  });
});

// --- Task 1: the tab strip renders editor.files, not editor.tabs ---------
//
// Before this fix, selecting a screen never populated `editor.files` (or
// `tabs`), so the tab strip -- iterating `tabs` -- was permanently empty:
// there was no way to open a second, third, ... file from the UI at all,
// only whatever `script.lua` auto-open produced. These pin the three
// listing-driven behaviours from task-1-brief.md Step 3/4.
describe("EditorPane renders editor.files (Task 1)", () => {
  beforeEach(() => {
    screens.selected = "local/a";
    screens.repos = [{ handle: "local", writable: true }];
  });

  it("renders a binary entry disabled, not clickable, and it does not open on click", async () => {
    editor.files = [{ name: "logo.png", size: 1234, text: false }];
    const readFileSpy = vi.spyOn(api, "readFile");

    const { getByText, queryByRole } = render(EditorPane);
    flushSync();

    const entry = getByText("logo.png (1234)");
    expect(entry).toHaveAttribute("aria-disabled", "true");
    // Not a tab, and not a button -- nothing to focus or activate.
    expect(queryByRole("tab")).toBeNull();
    expect(queryByRole("button", { name: /logo\.png/ })).toBeNull();

    await fireEvent.click(entry);
    expect(readFileSpy).not.toHaveBeenCalled();
  });

  it("opens a text entry on click", async () => {
    editor.files = [{ name: "script.lua", size: 10, text: true }];
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- hi", etag: '"1"' });

    const { getByRole } = render(EditorPane);
    flushSync();

    await fireEvent.click(getByRole("button", { name: "script.lua" }));
    flushSync();

    expect(editor.active?.name).toBe("script.lua");
    expect(editor.active?.text).toBe("-- hi");
  });

  it("shows the dirty marker only for a loaded, edited buffer -- not merely because the file is listed", async () => {
    editor.files = [{ name: "script.lua", size: 10, text: true }];
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "ok", etag: '"1"' });

    const { queryByLabelText, getByRole } = render(EditorPane);
    flushSync();
    expect(queryByLabelText("unsaved changes")).toBeNull(); // listed, never opened

    await fireEvent.click(getByRole("button", { name: "script.lua" }));
    flushSync();
    expect(queryByLabelText("unsaved changes")).toBeNull(); // opened, clean

    editor.setText("changed");
    flushSync();
    expect(queryByLabelText("unsaved changes")).not.toBeNull(); // opened, dirty
  });

  it("empties CodeMirror when no tab is left active (Finding H)", async () => {
    // Deleting the selected screen resets the store to no tabs, and the
    // status line says "no file open" -- but the tab-swap effect returned
    // early on a null tab, so CodeMirror kept painting the deleted file.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "old text", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    render(EditorPane);
    flushSync();
    cmMock.setDoc.mockClear();
    cmMock.setDiagnostics.mockClear();

    editor.reset();
    flushSync();

    expect(cmMock.setDoc).toHaveBeenCalledTimes(1);
    const [text, , readOnly] = cmMock.setDoc.mock.calls[0];
    expect(text).toBe("");
    expect(readOnly).toBe(true);
    expect(cmMock.setDiagnostics).toHaveBeenCalledWith([]);
  });
});
