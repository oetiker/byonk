import { describe, it, expect, vi, beforeEach } from "vitest";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  editor.reset();
  editor.files = [];
  screens.error = null;
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

  // Ruling 64, fix-round guard: the old hand-written UI's `scheduleCheck`
  // (static/dev/dev.js, pre-rewrite) fired the check request and wrote its
  // response straight into the editor with no staleness check at all. An
  // async round trip started for an older buffer can resolve AFTER the
  // user has typed further -- writing that stale response over the
  // meanwhile-updated `issues` would show diagnostics for text the buffer
  // no longer contains. `check()` must compare the buffer's CURRENT text
  // against what it actually sent before applying a response.
  it("discards a check() response if the buffer changed while the request was in flight", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "a", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    let resolveCheck;
    vi.spyOn(api, "checkFile").mockImplementation(
      () => new Promise((resolve) => (resolveCheck = resolve)),
    );

    const pending = editor.check(); // in flight, checking text "a"
    editor.setText("b"); // the buffer moves on before the response arrives

    resolveCheck({
      ok: false,
      issues: [{ severity: "error", location: "script.lua", message: "boom", line: 1 }],
    });
    await pending;

    // The stale response (for "a") must not land now that the buffer reads
    // "b" -- applying it would show a diagnostic for text that is no longer
    // in the editor.
    expect(editor.active.issues).toEqual([]);
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

// --- Task 1: loadScreen() ---------------------------------------------
//
// Before this fix, nothing ever called `api.listFiles()`, so selecting a
// screen never populated `editor.files` and the tab strip had nothing to
// render.
describe("editor.loadScreen()", () => {
  it("populates files from api.listFiles", async () => {
    const entries = [
      { name: "meta.yaml", size: 1, text: true },
      { name: "script.lua", size: 2, text: true },
      { name: "screen.svg", size: 3, text: true },
    ];
    vi.spyOn(api, "listFiles").mockResolvedValue(entries);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });

    await editor.loadScreen("local/a");

    expect(editor.files).toEqual(entries);
  });

  it("auto-opens script.lua when present and text", async () => {
    vi.spyOn(api, "listFiles").mockResolvedValue([
      { name: "script.lua", size: 2, text: true },
    ]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });

    await editor.loadScreen("local/a");

    expect(editor.active?.name).toBe("script.lua");
    expect(editor.active?.text).toBe("-- lua");
  });

  it("does not auto-open when script.lua is absent", async () => {
    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "meta.yaml", size: 1, text: true }]);
    const readFileSpy = vi.spyOn(api, "readFile");

    await editor.loadScreen("local/a");

    expect(editor.tabs).toHaveLength(0);
    expect(readFileSpy).not.toHaveBeenCalled();
  });

  it("does not auto-open a script.lua entry that is text: false", async () => {
    vi.spyOn(api, "listFiles").mockResolvedValue([
      { name: "script.lua", size: 2, text: false },
    ]);
    const readFileSpy = vi.spyOn(api, "readFile");

    await editor.loadScreen("local/a");

    expect(editor.tabs).toHaveLength(0);
    expect(readFileSpy).not.toHaveBeenCalled();
  });

  it("loadScreen(null) empties files", async () => {
    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "meta.yaml", size: 1, text: true }]);
    await editor.loadScreen("local/a");
    expect(editor.files).toHaveLength(1);

    await editor.loadScreen(null);
    expect(editor.files).toEqual([]);
  });

  it("switching screens clears the previous screen's buffers", async () => {
    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "one.lua", size: 1, text: true }]);
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    expect(editor.tabs).toHaveLength(1);

    vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "two.lua", size: 1, text: true }]);
    await editor.loadScreen("local/b");

    expect(editor.tabs).toHaveLength(0);
    expect(editor.active).toBeNull();
  });

  it("a rejecting listFiles sets screens.error, leaves files empty, and does not reject", async () => {
    vi.spyOn(api, "listFiles").mockRejectedValue(new Error("disk gone"));

    await expect(editor.loadScreen("local/a")).resolves.toBeUndefined();

    expect(editor.files).toEqual([]);
    expect(screens.error).toEqual({ op: "files", message: "disk gone" });
    // Not editor.error -- StatusStrip hard-codes a "save:" prefix on that
    // field (Ruling R6), which would misreport a listing failure as one.
    expect(editor.error).toBeNull();
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

// StatusBar (Task 10, spec §1) needs the cursor's line/column. `opts.onCursor`
// is the only way out of CodeMirror's own EditorView -- this proves it fires
// with the real 1-based line/col on both a selection move and a doc change,
// against the REAL module (not EditorPane's mock in editorpane.test.js,
// which only proves wiring, not that codemirror.js actually reports the
// right numbers).
describe("createEditor onCursor", () => {
  it("reports 1-based line/col on a caret move and on a doc change", async () => {
    const { createEditor } = await import("../src/lib/codemirror.js");
    const host = document.createElement("div");
    const calls = [];
    const { view, destroy } = createEditor(host, {
      doc: "one\ntwo\nthree",
      onCursor: (line, col) => calls.push([line, col]),
    });
    try {
      const line2From = view.state.doc.line(2).from;

      calls.length = 0;
      view.dispatch({ selection: { anchor: line2From } });
      expect(calls.at(-1)).toEqual([2, 1]);

      view.dispatch({ selection: { anchor: line2From + 2 } });
      expect(calls.at(-1)).toEqual([2, 3]);

      // A doc change moves the caret too (setDoc's full-document replace,
      // used on a tab swap, is exactly this): the tab-swap case must not
      // require a second, separate call to report the new position. A
      // full-document replace maps the old selection (inside the replaced
      // range) to the start of the new content, not its end -- confirmed
      // directly against CodeMirror's own mapping, not assumed.
      calls.length = 0;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "x" } });
      expect(calls).toHaveLength(1);
      expect(calls[0]).toEqual([1, 1]);
    } finally {
      destroy();
    }
  });

  it("does not require onCursor -- it stays optional like onChange", async () => {
    const { createEditor } = await import("../src/lib/codemirror.js");
    const host = document.createElement("div");
    const { view, destroy } = createEditor(host, { doc: "abc" });
    try {
      expect(() => view.dispatch({ selection: { anchor: 1 } })).not.toThrow();
    } finally {
      destroy();
    }
  });
});

// Ruling 64: `editor.svelte.js`'s `check()` and `codemirror.js`'s
// `setDiagnostics()` were each individually fine -- the defect was that
// NOTHING called them together. This proves the two real modules (not
// EditorPane's mock, see editorpane.test.js for the wiring-level proof)
// actually connect: a syntax error reported by `check()` must show up as a
// real CodeMirror diagnostic, not just as an entry in `editor.active.issues`.
describe("editor.check() + real CodeMirror (Ruling 64)", () => {
  it("a checked syntax error becomes a real CodeMirror diagnostic", async () => {
    const { createEditor } = await import("../src/lib/codemirror.js");
    const { diagnosticCount } = await import("@codemirror/lint");

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "bad lua", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    const host = document.createElement("div");
    const cm = createEditor(host, { doc: editor.active.text, language: "lua" });
    try {
      expect(diagnosticCount(cm.view.state)).toBe(0);

      vi.spyOn(api, "checkFile").mockResolvedValue({
        ok: false,
        issues: [{ severity: "error", location: "script.lua", message: "boom", line: 1 }],
      });
      await editor.check();
      cm.setDiagnostics(editor.active.issues); // EditorPane's own $effect does exactly this

      expect(diagnosticCount(cm.view.state)).toBe(1);
    } finally {
      cm.destroy();
    }
  });
});
