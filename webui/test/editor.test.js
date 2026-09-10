import { describe, it, expect, vi, beforeEach } from "vitest";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  editor.reset();
});

describe("editor store", () => {
  it("keeps dirty and error as separate signals", async () => {
    // Spec §7: they look alike in the UI and mean different things. A buffer
    // can be dirty and valid, or clean and broken.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "ok", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    expect(editor.active.dirty).toBe(false);
    editor.setText("bad");
    expect(editor.active.dirty).toBe(true);
    expect(editor.active.issues).toEqual([]);

    vi.spyOn(api, "checkFile").mockResolvedValue({
      ok: false,
      issues: [{ severity: "error", location: "script.lua", message: "boom", line: 1 }],
    });
    await editor.check();
    expect(editor.active.issues).toHaveLength(1);
    expect(editor.active.dirty).toBe(true);

    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    await editor.save();
    expect(editor.active.dirty).toBe(false);
    expect(editor.active.issues).toHaveLength(1); // still broken, just saved
  });

  it("handles the tab lifecycle, including closing the active one", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    await editor.open("local/a", "two.lua");
    await editor.open("local/a", "three.lua");
    expect(editor.tabs).toHaveLength(3);

    editor.activeIndex = 1;
    editor.close(1);
    expect(editor.tabs.map((t) => t.name)).toEqual(["one.lua", "three.lua"]);
    // Closing the active tab must land on a real neighbour, not off the end.
    expect(editor.activeIndex).toBeLessThan(editor.tabs.length);
    expect(editor.active).not.toBeNull();

    editor.close(0);
    editor.close(0);
    expect(editor.tabs).toHaveLength(0);
    expect(editor.active).toBeNull();
  });

  it("opens an already-open file by focusing its tab, not duplicating it", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    await editor.open("local/a", "one.lua");
    expect(editor.tabs).toHaveLength(1);
  });

  it("marks a tab read-only when its repo is not writable", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("byonk-builtin/b", "script.lua", { readOnly: true });
    expect(editor.active.readOnly).toBe(true);
  });

  it("sends the etag on save and surfaces a conflict", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.setText("y");

    const write = vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await expect(editor.save()).rejects.toBeInstanceOf(api.ConflictError);
    expect(write.mock.calls[0][3]).toBe('"1"');
    expect(editor.active.dirty).toBe(true); // a failed save does not clean it
  });

  it("replaceFromDisk swaps the buffer and clears dirty", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.setText("mine");
    expect(editor.active.dirty).toBe(true);

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    await editor.replaceFromDisk("local/a", "script.lua");
    expect(editor.active.text).toBe("theirs");
    expect(editor.active.etag).toBe('"2"');
    expect(editor.active.dirty).toBe(false);
  });
});

describe("createEditor read-only mechanism (Ruling 24)", () => {
  // Deferred to Task 14 (browser verification is the controller's job on
  // this branch): a real keystroke through CodeMirror's key handling is not
  // exercised here. This checks the two mechanisms spec §8.6 needs instead:
  // `EditorState.readOnly` alone still allows a caret and some commands, so
  // `EditorView.editable` (which controls the DOM's contenteditable
  // attribute) must ALSO be false.
  //
  // Deviation from the ruling's literal assertion: `view.contentDOM
  // .contentEditable` (the `contentEditable` IDL property) is `undefined`
  // under jsdom 30 -- jsdom does not implement that accessor, confirmed by
  // probing it directly: `getAttribute("contenteditable")` returned the
  // string `"false"` while `.contentEditable` and `.isContentEditable` were
  // both absent from the result entirely (`JSON.stringify` drops
  // `undefined`). `EditorView.editable` is reflected onto the DOM as the
  // `contenteditable` content attribute, so asserting on `getAttribute`
  // checks the same mechanism a real browser's IDL property would expose.
  it("sets both state.readOnly and the contenteditable attribute to false", async () => {
    const { createEditor } = await import("../src/lib/codemirror.js");
    const host = document.createElement("div");
    const { view, destroy } = createEditor(host, { doc: "abc", readOnly: true });
    try {
      expect(view.state.readOnly).toBe(true);
      expect(view.contentDOM.getAttribute("contenteditable")).toBe("false");
    } finally {
      destroy();
    }
  });

  it("a writable editor has state.readOnly=false and contenteditable=true", async () => {
    const { createEditor } = await import("../src/lib/codemirror.js");
    const host = document.createElement("div");
    const { view, destroy } = createEditor(host, { doc: "abc", readOnly: false });
    try {
      expect(view.state.readOnly).toBe(false);
      expect(view.contentDOM.getAttribute("contenteditable")).toBe("true");
    } finally {
      destroy();
    }
  });
});
