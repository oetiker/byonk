import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";

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
});
