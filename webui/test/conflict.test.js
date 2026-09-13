import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import { tick } from "svelte";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import AppShell from "../src/lib/components/AppShell.svelte";

beforeEach(() => {
  vi.restoreAllMocks();
  editor.reset();
});

async function openDirty() {
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
  await editor.open("local/a", "script.lua");
  editor.setText("mine");
}

describe("conflict handling", () => {
  it("raises the banner when a save returns 412", async () => {
    await openDirty();
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    expect(editor.conflict).toMatchObject({
      ref: "local/a",
      name: "script.lua",
      source: "save",
    });
  });

  it("raises the banner when the watcher reports a file-change", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });
    expect(editor.conflict).toMatchObject({ source: "watch" });
  });

  it("ignores a file-change for a file that is not open", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/other", name: "script.lua" });
    expect(editor.conflict).toBeNull();
  });

  it("Reload replaces the buffer and clears dirty", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    await editor.resolveReload();

    expect(editor.active.text).toBe("theirs");
    expect(editor.active.dirty).toBe(false);
    expect(editor.conflict).toBeNull();
  });

  it("Overwrite keeps my text and wins with a fresh etag", async () => {
    // Never exercised before this test. Overwrite must re-read only the etag,
    // not the text -- re-reading the text would discard the edit it exists to
    // preserve.
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    const write = vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"3"' });
    await editor.resolveOverwrite();

    expect(write.mock.calls[0][2]).toBe("mine");
    expect(write.mock.calls[0][3]).toBe('"2"');
    expect(editor.active.text).toBe("mine");
    expect(editor.active.dirty).toBe(false);
    expect(editor.conflict).toBeNull();
  });

  // --- Review finding 1: conflict clobber race ------------------------------
  //
  // resolveReload()/resolveOverwrite() capture `const c = this.conflict` on
  // entry but null `this.conflict` unconditionally on success. If a fresh
  // conflict for a DIFFERENT file arrives from the watcher while the first
  // resolve is still awaiting its own network call, the fresh conflict gets
  // silently clobbered by the first resolve's completion -- the second
  // file's banner vanishes with no notification, even though that buffer is
  // still dirty and out of sync with disk.
  describe("resolving one conflict must not clobber a different one raised in the meantime", () => {
    it("resolveReload: a fresh conflict raised mid-await survives the in-flight reload's completion", async () => {
      // Tab A: open, dirty, conflicted via the watcher.
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk-a", etag: '"1"' });
      await editor.open("local/a", "a.lua");
      editor.setText("mine-a");
      editor.onFileChange({ ref: "local/a", name: "a.lua" });
      expect(editor.conflict).toMatchObject({ ref: "local/a", name: "a.lua" });

      // Tab B: open and dirty, not yet conflicted.
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk-b", etag: '"1"' });
      await editor.open("local/b", "b.lua");
      editor.setText("mine-b");

      // A's reload is now in flight, held open by a deferred readFile so a
      // second conflict can be interleaved mid-await.
      let releaseReadFile;
      const deferred = new Promise((resolve) => {
        releaseReadFile = () => resolve({ text: "theirs-a", etag: '"2"' });
      });
      vi.spyOn(api, "readFile").mockReturnValue(deferred);

      const reloadPromise = editor.resolveReload();

      // While A's reload is still awaiting readFile, the watcher reports B.
      editor.onFileChange({ ref: "local/b", name: "b.lua" });
      expect(editor.conflict).toMatchObject({ ref: "local/b", name: "b.lua" });

      // Let A's reload complete.
      releaseReadFile();
      await reloadPromise;

      // A's own reload must have gone through...
      const tabA = editor.tabs.find((t) => t.ref === "local/a");
      expect(tabA.text).toBe("theirs-a");
      expect(tabA.dirty).toBe(false);
      // ...but B's banner, which A's completion did not raise and is not
      // resolving, must survive.
      expect(editor.conflict).toMatchObject({ ref: "local/b", name: "b.lua" });
    });

    it("resolveOverwrite: a fresh conflict raised mid-await survives the in-flight overwrite's completion", async () => {
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk-a", etag: '"1"' });
      await editor.open("local/a", "a.lua");
      editor.setText("mine-a");
      editor.onFileChange({ ref: "local/a", name: "a.lua" });
      expect(editor.conflict).toMatchObject({ ref: "local/a", name: "a.lua" });

      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk-b", etag: '"1"' });
      await editor.open("local/b", "b.lua");
      editor.setText("mine-b");

      let releaseReadFile;
      const deferred = new Promise((resolve) => {
        releaseReadFile = () => resolve({ text: "ignored-a", etag: '"2"' });
      });
      vi.spyOn(api, "readFile").mockReturnValue(deferred);
      vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"3"' });

      const overwritePromise = editor.resolveOverwrite();

      editor.onFileChange({ ref: "local/b", name: "b.lua" });
      expect(editor.conflict).toMatchObject({ ref: "local/b", name: "b.lua" });

      releaseReadFile();
      await overwritePromise;

      // A's overwrite kept its own text and won, per Overwrite's contract...
      const tabA = editor.tabs.find((t) => t.ref === "local/a");
      expect(tabA.text).toBe("mine-a");
      expect(tabA.dirty).toBe(false);
      // ...but B's banner must survive A's completion.
      expect(editor.conflict).toMatchObject({ ref: "local/b", name: "b.lua" });
    });
  });

  // --- Ruling 37: onFileChange must respect dirty ---------------------------
  //
  // The brief's own test only ever calls onFileChange on a DIRTY buffer
  // (openDirty() always edits it first). A brief-literal implementation that
  // raises the banner for every open file, dirty or not, would still pass
  // every test above -- this is the one that would fail.
  it("silently reloads a CLEAN buffer instead of raising the banner (Ruling 37)", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    // Deliberately never edited -- editor.active.dirty is false here.

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "updated on disk", etag: '"2"' });
    await editor.onFileChange({ ref: "local/a", name: "script.lua" });

    expect(editor.conflict).toBeNull();
    expect(editor.active.text).toBe("updated on disk");
    expect(editor.active.etag).toBe('"2"');
    expect(editor.active.dirty).toBe(false);
  });

  // --- Ruling 36: the real SSE payload is an array of bare basenames -------
  //
  // The brief's test drives {ref, name} straight into onFileChange, which
  // bypasses the parsing entirely and cannot catch a broken parser. These
  // drive the REAL payload shape (a bare array of filenames, exactly what
  // `handle_events` in src/api/dev.rs sends) through the actual parsing
  // path (`onWatchEvent`).
  describe("onWatchEvent -- the real array-of-basenames payload", () => {
    it("matches an open, dirty tab by basename and raises the banner", async () => {
      await openDirty();
      await editor.onWatchEvent(["script.lua", "unrelated.lua"]);
      expect(editor.conflict).toMatchObject({
        ref: "local/a",
        name: "script.lua",
        source: "watch",
      });
    });

    it("matches an open, clean tab by basename and silently reloads it", async () => {
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
      await editor.open("local/a", "script.lua");

      vi.spyOn(api, "readFile").mockResolvedValue({ text: "fresh", etag: '"2"' });
      await editor.onWatchEvent(["script.lua"]);

      expect(editor.conflict).toBeNull();
      expect(editor.active.text).toBe("fresh");
    });

    it("ignores a basename that matches no open tab", async () => {
      await openDirty();
      await editor.onWatchEvent(["nothing-open-by-this-name.lua"]);
      expect(editor.conflict).toBeNull();
    });

    it("over-triggers when two open tabs share a basename in different repos -- a false banner, not a lost edit", async () => {
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
      await editor.open("local/a", "script.lua");
      await editor.open("local/b", "script.lua");
      // Dirty both independently -- setText only touches the active tab.
      editor.tabs[0].text = "mine-a";
      editor.tabs[1].text = "mine-b";

      await editor.onWatchEvent(["script.lua"]);

      // Both tabs matched; the store has one `conflict` field, so the
      // second match wins -- what matters here is that a lookup keyed by
      // basename alone (no ref in the payload) actually reached BOTH tabs,
      // not that either result is somehow lost.
      expect(editor.conflict).toMatchObject({ name: "script.lua", source: "watch" });
      expect(["local/a", "local/b"]).toContain(editor.conflict.ref);
    });
  });

  // --- Ruling 39: trySave must never throw ----------------------------------
  //
  // The brief's trySave rethrows anything that isn't a ConflictError. With
  // save now reachable from a keybinding and a button (neither has its own
  // catch), a rethrow here is an unhandled promise rejection with no
  // feedback. `.resolves` fails outright if trySave still throws.
  describe("trySave routes non-conflict failures to editor.error (Ruling 39)", () => {
    it("a 428 PreconditionRequiredError does not throw and is recorded", async () => {
      await openDirty();
      vi.spyOn(api, "writeFile").mockRejectedValue(new api.PreconditionRequiredError());
      await expect(editor.trySave()).resolves.toBeUndefined();
      expect(editor.conflict).toBeNull();
      expect(editor.error).toMatchObject({ op: "save" });
    });

    it("a generic failure (e.g. a 401 after session expiry) does not throw and is recorded", async () => {
      await openDirty();
      vi.spyOn(api, "writeFile").mockRejectedValue(new Error("401 Unauthorized"));
      await expect(editor.trySave()).resolves.toBeUndefined();
      expect(editor.error).toMatchObject({ op: "save" });
      expect(editor.error.message).toMatch(/401/);
    });

    it("a successful save clears a previous error", async () => {
      await openDirty();
      vi.spyOn(api, "writeFile").mockRejectedValueOnce(new Error("boom"));
      await editor.trySave();
      expect(editor.error).not.toBeNull();

      vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"9"' });
      await editor.trySave();
      expect(editor.error).toBeNull();
    });
  });
});

// --- Ruling 36 (full path) / Ruling 40: AppShell's real EventSource wiring --
//
// A minimal EventSource stand-in. jsdom+vitest does not define EventSource
// at all (confirmed directly -- unlike BroadcastChannel/URL.createObjectURL,
// which survive from Node's own globals). AppShell guards `new EventSource`
// behind `typeof EventSource !== "undefined"` so its other tests (which do
// not install this) keep working; these tests install it deliberately to
// drive the real SSE transport end to end: JSON.parse(e.data) -> the real
// array-of-basenames payload -> editor.onWatchEvent -> editor.conflict, and
// onopen/onerror -> editor.watchConnected.
class FakeEventSource {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.onopen = null;
    this.onerror = null;
    this.listeners = {};
    FakeEventSource.instances.push(this);
  }
  addEventListener(type, cb) {
    (this.listeners[type] ??= []).push(cb);
  }
  close() {}
  emit(type, data) {
    for (const cb of this.listeners[type] ?? []) cb({ data });
  }
}

describe("AppShell's SSE wiring (Ruling 36 full path, Ruling 40)", () => {
  let realEventSource;

  beforeEach(() => {
    cleanup();
    editor.reset();
    layout.reset();
    localStorage.clear();
    realEventSource = globalThis.EventSource;
    globalThis.EventSource = FakeEventSource;
    FakeEventSource.instances = [];
  });

  afterEach(() => {
    cleanup();
    if (realEventSource === undefined) delete globalThis.EventSource;
    else globalThis.EventSource = realEventSource;
  });

  it("parses the server's real array-of-basenames payload and raises the conflict banner end to end", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });

    render(AppShell);
    const es = FakeEventSource.instances.at(-1);
    expect(es).toBeTruthy();
    // Opened AFTER mount, not before: AppShell's own Task-1 effect calls
    // `editor.loadScreen(screens.selected, ...)` on mount (screens.selected
    // is null here, so it's a no-op past `reset()`) -- opening the tab
    // first and mounting second would have this mount's own `reset()` wipe
    // it right back out, which is not what this test is about.
    await tick();
    await editor.open("local/a", "script.lua");
    editor.setText("mine");

    es.emit("file-change", JSON.stringify(["script.lua"]));
    await tick();

    expect(editor.conflict).toMatchObject({
      ref: "local/a",
      name: "script.lua",
      source: "watch",
    });
  });

  it("a malformed event does not break the page", async () => {
    render(AppShell);
    const es = FakeEventSource.instances.at(-1);
    expect(() => es.emit("file-change", "not json")).not.toThrow();
  });

  it("reflects the connection's open/error state on editor.watchConnected, for StatusBar to read (Ruling 40)", async () => {
    render(AppShell);
    const es = FakeEventSource.instances.at(-1);

    expect(editor.watchConnected).toBe(false);
    es.onopen();
    expect(editor.watchConnected).toBe(true);
    es.onerror();
    expect(editor.watchConnected).toBe(false);
  });
});
