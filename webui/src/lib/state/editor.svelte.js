import * as api from "../api.js";

/**
 * Open files and their buffers.
 *
 * This lives outside any component on purpose: IconRail can unmount
 * EditorPane at any moment, and an unsaved buffer must survive that.
 *
 * `dirty` and `issues` are deliberately separate. They render similarly --
 * a marker on the tab -- and mean unrelated things: dirty is "differs from
 * disk", issues is "does not parse". Conflating them was worth a test.
 */
function makeTab(ref, name, text, etag, readOnly) {
  return {
    ref,
    name,
    text,
    saved: text,
    etag,
    issues: [],
    readOnly,
    // Bumped ONLY by replaceFromDisk -- see EditorPane.svelte's tab-swap
    // effect. Ordinary typing never touches it, so it does not reopen the
    // Ruling-22 hole (setDoc re-running on every keystroke); an external
    // buffer swap on the ALREADY-active tab does bump it, which is the
    // signal that effect needs but ref/name/readOnly alone can't give it
    // (Ruling 28 / task-7 fix round 1: resolveReload() on a save conflict
    // calls replaceFromDisk on the active tab, and nothing else about that
    // tab's identity changes).
    revision: 0,
    // Derived, not stored -- two sources of truth for "differs from disk"
    // is one too many. A plain getter, not a prototype helper: Svelte's
    // $state deep-proxies plain objects pushed into a $state array, and
    // reading `this.text`/`this.saved` from inside this getter still goes
    // through that proxy, so it tracks like a $derived would -- from a tab
    // object that isn't itself a class field or living inside a component.
    get dirty() {
      return this.text !== this.saved;
    },
  };
}

class Editor {
  tabs = $state([]);
  activeIndex = $state(-1);

  /** `null` or `{ref, name, source: "save" | "watch"}` -- Task 10, spec §8.5. */
  conflict = $state(null);

  /**
   * The last save failure that ISN'T a conflict -- `{op: "save", message}`.
   * `trySave()` now runs from a keybinding and a visible button (Ruling
   * 38), neither of which has its own catch, so a 401 after session
   * expiry, a dropped connection, a disk error, or api.js's
   * PreconditionRequiredError (428, missing/invalid If-Match) must land
   * somewhere visible instead of becoming an unhandled rejection (Ruling
   * 39). `StatusBar` renders it, the same split `screens.error` /
   * `simulate.error` / `tune.error` already use.
   */
  error = $state(null);

  // CodeMirror cursor position, 1-based. Lives here, not as local component
  // state in EditorPane, because StatusBar (Task 10) needs to read it and
  // is a sibling, not a child that could receive it as a prop from the one
  // place a CodeMirror instance actually exists (same reasoning as
  // `watchConnected` below, Ruling 40).
  cursorLine = $state(1);
  cursorCol = $state(1);

  // Whether the SSE connection AppShell opens is currently live. It must be
  // shared state that AppShell WRITES and StatusBar READS, not a local
  // `const es` invisible outside AppShell's onMount -- Ruling 40: a
  // hard-wired "watching" indicator is worse than none, because it asserts
  // a guarantee the page cannot make.
  watchConnected = $state(false);

  get active() {
    return this.tabs[this.activeIndex] ?? null;
  }

  reset() {
    this.tabs = [];
    this.activeIndex = -1;
    this.conflict = null;
    this.error = null;
    this.cursorLine = 1;
    this.cursorCol = 1;
    this.watchConnected = false;
  }

  setCursor(line, col) {
    this.cursorLine = line;
    this.cursorCol = col;
  }

  async open(ref, name, { readOnly = false } = {}) {
    const found = this.tabs.findIndex((t) => t.ref === ref && t.name === name);
    if (found >= 0) {
      this.activeIndex = found;
      return;
    }
    const { text, etag } = await api.readFile(ref, name);
    this.tabs.push(makeTab(ref, name, text, etag, readOnly));
    this.activeIndex = this.tabs.length - 1;
  }

  close(i) {
    if (i < 0 || i >= this.tabs.length) return;
    this.tabs.splice(i, 1);
    // Land on a real neighbour. Leaving activeIndex past the end is how a
    // closed active tab turns into a blank editor with no way back.
    if (this.tabs.length === 0) this.activeIndex = -1;
    else if (this.activeIndex >= this.tabs.length) this.activeIndex = this.tabs.length - 1;
    else if (this.activeIndex > i) this.activeIndex -= 1;
  }

  setText(text) {
    const t = this.active;
    if (!t || t.readOnly) return;
    t.text = text;
  }

  async check() {
    const t = this.active;
    if (!t) return;
    const report = await api.checkFile(t.ref, t.name, t.text);
    t.issues = report.issues ?? [];
  }

  async save() {
    const t = this.active;
    if (!t) return;
    const { etag } = await api.writeFile(t.ref, t.name, t.text, t.etag);
    t.etag = etag;
    t.saved = t.text;
  }

  async replaceFromDisk(ref, name) {
    const t = this.tabs.find((x) => x.ref === ref && x.name === name);
    if (!t) return;
    const { text, etag } = await api.readFile(ref, name);
    t.text = text;
    t.saved = text;
    t.etag = etag;
    t.revision += 1;
  }

  /**
   * save() that turns a 412 into the conflict banner and everything else
   * into `error`, instead of throwing out of a keybinding or a button click
   * with nothing catching it (Ruling 39). This covers BOTH of api.js's
   * typed failures -- `ConflictError` (412) becomes the banner,
   * `PreconditionRequiredError` (428, a missing/invalid If-Match) falls
   * into the generic branch below along with a 401, a dropped connection,
   * or a disk error -- plus anything else `save()` can throw.
   */
  async trySave() {
    const t = this.active;
    if (!t) return;
    this.error = null;
    try {
      await this.save();
    } catch (e) {
      if (e instanceof api.ConflictError) {
        this.conflict = { ref: t.ref, name: t.name, source: "save" };
        return;
      }
      this.error = { op: "save", message: e.message };
    }
  }

  /**
   * The watcher reported a change to one open file (ref+name already
   * resolved -- see `onWatchEvent` below for the real SSE payload, which
   * carries neither).
   *
   * Conflicts ONLY when the buffer is dirty. A clean buffer is silently
   * refreshed from disk instead of interrupting the user with a Reload/
   * Overwrite choice between two identical outcomes -- Ruling 37, restoring
   * the old UI's behaviour (static/dev/dev.js:1626-1631:
   * `if (entry.dirty) showConflict(name); else { ...reload... }`).
   */
  onFileChange({ ref, name }) {
    const t = this.tabs.find((x) => x.ref === ref && x.name === name);
    if (!t) return;
    if (t.dirty) {
      this.conflict = { ref, name, source: "watch" };
      return;
    }
    // Not awaited by the caller (onWatchEvent may be routing several of
    // these from one event) -- caught here so a failed background refresh
    // becomes visible on `error` instead of an unhandled rejection, same
    // as the old UI's showFailure() on this path.
    return this.replaceFromDisk(ref, name).catch((e) => {
      this.error = { op: "watch-reload", message: e.message };
    });
  }

  /**
   * Parse the watcher's REAL SSE payload and route it to `onFileChange`.
   *
   * `handle_events` (src/api/dev.rs) emits a JSON array of bare basenames
   * -- it maps `event.paths` through `.filter_map(|p| p.file_name())`,
   * deliberately discarding the directory. It is NOT `{ref, name}`
   * (Ruling 36). Match by basename against every open tab, exactly as the
   * old UI did (dev.js:1622-1631, `state.open.get(name)`): a tab's `ref`
   * plays no part in the match because the event carries none.
   *
   * Known and accepted: two open tabs that share a filename in different
   * repos BOTH see the event when only one of them actually changed. That
   * is a false banner (or an unwanted silent reload), not a lost edit --
   * the server-side fix (watching per-ref, or reporting the full relative
   * path) is deliberately out of scope for this branch.
   */
  async onWatchEvent(names) {
    const pending = [];
    for (const name of names ?? []) {
      for (const t of this.tabs) {
        if (t.name === name) pending.push(this.onFileChange({ ref: t.ref, name: t.name }));
      }
    }
    await Promise.all(pending);
  }

  async resolveReload() {
    const c = this.conflict;
    if (!c) return;
    try {
      await this.replaceFromDisk(c.ref, c.name);
      // Only clear the conflict THIS call was resolving. A different file's
      // conflict can arrive from the watcher while this await is in
      // flight (onFileChange runs synchronously and overwrites
      // `this.conflict` the moment it's raised) -- nulling unconditionally
      // here would silently clobber that fresh banner with no notification,
      // even though its buffer is still dirty and out of sync with disk
      // (review finding 1, fix round 2).
      if (this.conflict === c) this.conflict = null;
    } catch (e) {
      // Leave `conflict` up so the banner (and its buttons) stay available
      // to retry -- same reasoning as the old UI's reload.onclick catch.
      this.error = { op: "reload", message: e.message };
    }
  }

  /**
   * Keep my text and win.
   *
   * Re-read the file for its ETAG ONLY. Taking its text as well would throw
   * away the edit this button exists to preserve. Never exercised before
   * Task 10's tests -- the old UI's Overwrite path is the direct model
   * (dev.js's `over.onclick`).
   */
  async resolveOverwrite() {
    const c = this.conflict;
    if (!c) return;
    const t = this.tabs.find((x) => x.ref === c.ref && x.name === c.name);
    if (!t) {
      if (this.conflict === c) this.conflict = null;
      return;
    }
    try {
      const { etag } = await api.readFile(c.ref, c.name);
      const res = await api.writeFile(c.ref, c.name, t.text, etag);
      t.etag = res.etag;
      t.saved = t.text;
      // Same guard as resolveReload() above, same reason: a different
      // file's fresh conflict can arrive from the watcher while the
      // readFile/writeFile round trip above is in flight, and must not be
      // clobbered by THIS call's completion (review finding 1, fix round 2).
      if (this.conflict === c) this.conflict = null;
    } catch (e) {
      this.error = { op: "overwrite", message: e.message };
    }
  }
}

export const editor = new Editor();
