import * as api from "../api.js";
import { screens } from "./screens.svelte.js";
import { subject } from "./subject.svelte.js";

/** How long after its own write the page ignores the watcher reporting it. */
const OWN_WRITE_ECHO_MS = 2000;

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
  /** The screen `loadScreen()` last loaded -- what the tab strip, validate, add and delete act on (P2-8). */
  ref = $state(null);
  tabs = $state([]);
  activeIndex = $state(-1);

  /**
   * The currently selected screen's file LISTING -- `{name, size, text}`
   * exactly as `api.listFiles(ref)` returns it, one entry per file that
   * exists on disk. This is distinct from `tabs`: `files` is what EXISTS,
   * `tabs` is what has actually been LOADED into a buffer (opened, possibly
   * edited, possibly never saved). A file can be listed here with no tab
   * open for it -- e.g. before it's ever clicked, or after its tab was
   * closed (closing a buffer does not delete the file).
   */
  files = $state([]);

  /**
   * The selected binary entry, `{ref, name}`, or null. A binary file has no
   * text buffer, so it is not in `tabs`; while this is set the pane shows the
   * asset viewer instead of CodeMirror and no text tab counts as selected.
   * `assetRev` busts the browser's image cache after the bytes change.
   */
  asset = $state(null);
  assetRev = $state(0);

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
   * `simulate.error` already use.
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

  /**
   * Bumped whenever a screen file changed on disk: our own save, or the
   * watcher reporting an edit made elsewhere. The preview re-renders on it,
   * because a file change leaves the render request byte-identical and
   * nothing else would tell it (Task U4).
   */
  diskRevision = $state(0);

  /**
   * Where Save would move the screen (U9): `{handle, path}`, or null while
   * the Details block is untouched. Save-only -- nothing renders it.
   */
  moveDraft = $state(null);

  /**
   * Why the last move was refused (a 409 for a taken path, a read-only
   * repo), or null. Shown once, in the Edit pane's message row, and gone on
   * the next Details edit or a successful move.
   */
  moveFailure = $state(null);

  /** basename -> when this page last wrote it; see `onWatchEvent`. */
  #ownWrites = new Map();

  /** `quiet`: record the own write but leave the render to the caller (a move). */
  #wrote(name, { quiet = false } = {}) {
    this.#ownWrites.set(name, Date.now());
    if (!quiet) this.diskRevision += 1;
  }

  /**
   * The last whole-screen `validate` result, or `null` when none has run for
   * the current screen.
   *
   * `{clean: [name, ...], issues: [...]}`. Spec
   * 2026-09-07-dev-mode-editor-design.md:315 runs `validate` after every
   * successful save and :326 shows "one green tick per clean file" -- so what
   * has to be remembered is not just the issues but WHICH files were examined
   * and found clean. A file with no issue is not automatically clean: before
   * the first validate nothing has been examined at all, and a tick then
   * would assert a check that never happened (Ruling 40's reasoning, applied
   * to validity instead of the watcher).
   *
   * Cleared by `reset()`, so it never outlives the screen it describes.
   */
  report = $state(null);

  /**
   * Files the last validate examined and found clean, as a Set for the tab
   * strip to test against. Empty when no validate has run.
   */
  get clean() {
    return new Set(this.report?.clean ?? []);
  }

  /** The name of whatever the strip shows as selected: an asset, else the active tab. */
  get selectedName() {
    return this.asset?.name ?? this.active?.name ?? null;
  }

  get active() {
    return this.tabs[this.activeIndex] ?? null;
  }

  /** The loaded screen's own place, `{handle, path}`, or null. */
  get place() {
    if (!this.ref) return null;
    const i = this.ref.indexOf("/");
    return { handle: this.ref.slice(0, i), path: this.ref.slice(i + 1) };
  }

  /** Where the screen is going: the Details draft, else where it is. */
  get target() {
    return this.moveDraft ?? this.place;
  }

  /** The Details name a place other than the screen's own. */
  get moveDirty() {
    const d = this.moveDraft;
    const p = this.place;
    return !!d && !!p && (d.handle !== p.handle || d.path.trim() !== p.path);
  }

  /** The Details hold an emptied Path: nothing a move could go to. */
  get moveMissing() {
    return !!this.moveDraft && this.moveDraft.path.trim() === "";
  }

  /** Edit one Details field (`"handle"` or `"path"`). */
  setTarget(field, value) {
    if (!this.place) return;
    this.moveDraft = { ...this.target, [field]: value };
    this.moveFailure = null;
  }

  reset() {
    this.ref = null;
    this.tabs = [];
    this.activeIndex = -1;
    this.asset = null;
    this.conflict = null;
    this.error = null;
    this.moveDraft = null;
    this.moveFailure = null;
    this.cursorLine = 1;
    this.cursorCol = 1;
    // A validate result describes ONE screen. Carrying it across a switch
    // would tick files in the new screen because files of the same name
    // validated in the old one.
    this.report = null;
    // Deliberately NOT touched here (it was, until Task 1): `loadScreen()`
    // below calls `reset()` on every screen switch, and `watchConnected`
    // reflects whether the SSE connection AppShell opened is live -- a fact
    // about the transport, not about which screen is open. Clearing it here
    // would flip StatusBar's "watch: connected" to "disconnected" for a
    // frame on every screen switch even though the connection never
    // dropped. AppShell's `onMount` (the only writer of `true`) still sets
    // it back to `false` on its own teardown/onerror paths.
  }

  setCursor(line, col) {
    this.cursorLine = line;
    this.cursorCol = col;
  }

  async open(ref, name, { readOnly = false } = {}) {
    const found = this.tabs.findIndex((t) => t.ref === ref && t.name === name);
    if (found >= 0) {
      this.activeIndex = found;
      this.asset = null;
      return;
    }
    const { text, etag } = await api.readFile(ref, name);
    this.tabs.push(makeTab(ref, name, text, etag, readOnly));
    this.activeIndex = this.tabs.length - 1;
    this.asset = null;
  }

  /** Select a binary entry of the current screen: no buffer, just the viewer. */
  selectAsset(name) {
    if (this.ref) this.asset = { ref: this.ref, name };
  }

  /**
   * Switch to a different screen: replace `files` with its listing, and
   * auto-open `script.lua` when it's present as a text file -- mirrors the
   * pre-rewrite UI's static/dev/dev.js:436-438 ("if there's a script.lua
   * with text, open it").
   *
   * `reset()` first: switching screens must not carry another screen's
   * buffers, conflict banner, or error across (`watchConnected` is the one
   * exception -- see `reset()`'s own comment).
   *
   * `files` is left in the order the server sends it -- `meta.yaml`,
   * `script.lua`, `screen.svg`, then alphabetical
   * (src/services/screen_store.rs:1074) -- and is NOT re-sorted here. The
   * pre-rewrite UI sorted it again client-side; that duplication is not
   * worth porting.
   */
  async loadScreen(ref, { readOnly = false } = {}) {
    this.reset();
    this.ref = ref ?? null;
    if (!ref) {
      this.files = [];
      return;
    }
    // Clears `screens.error` only once there is an actual load to attempt --
    // NOT above the `!ref` return. Same shape as `FilesPane.svelte`'s
    // `run()` ("a fresh operation clears the previous error so a stale
    // failure can't linger after a later success"): without this, giving up
    // on a screen whose listing failed and picking a working one -- or just
    // deselecting -- left that banner on screen forever, since only the
    // catch branch below ever touched it. Placed here rather than at the
    // top of the function (fix round 1's original placement) because a
    // `loadScreen(null)` -- which is what AppShell's mount-time effect
    // calls before anything is ever selected -- is not a "fresh operation"
    // with a previous failure of its own to drop; it is the editor doing
    // nothing, and doing nothing should not reach across into another
    // store and clear ITS state. `reset()` above only ever touches the
    // editor's own fields for exactly that reason (Ruling 66's invariant:
    // five error surfaces that must never clobber one another). This is
    // deliberately NOT the same rule `screens.load()` follows: that is the
    // boot-time load, which has no previous failure of its own to drop and
    // would otherwise risk wiping an error set moments earlier by something
    // else. A `loadScreen(ref)` with an actual ref IS user-initiated (a
    // click, or the effect that follows one) every time it runs, so past
    // this point it always has "previous" to clear, exactly like `run()`.
    screens.error = null;
    try {
      this.files = await api.listFiles(ref);
    } catch (e) {
      // Caught, not re-thrown: a failed listing must not take the page
      // down. Recorded on `screens.error`, NOT `this.error` -- listing a
      // screen's files is the screens store's business, and it renders as
      // "files: <message>", which is what actually happened (Ruling R6).
      // (StatusStrip used to hard-code a "save:" prefix on `editor.error`,
      // which was a second reason to route it here; that mislabel is fixed
      // -- Finding L -- and the routing above stands on its own.)
      screens.error = { op: "files", message: e?.message ?? String(e) };
      this.files = [];
      return;
    }
    const scriptLua = this.files.find((f) => f.name === "script.lua");
    if (scriptLua?.text) {
      await this.open(ref, "script.lua", { readOnly });
    }
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

  /**
   * `t` defaults to the active tab but can name a specific one -- Task
   * 13a/Ruling 64: EditorPane debounces a call per EDITED tab, not per
   * "whatever is active when the timer fires", so a tab switch during the
   * debounce window doesn't route the check at the wrong buffer.
   *
   * Staleness guard: `text` is snapshotted before the request goes out, and
   * the response is discarded if the tab's text no longer matches it. The
   * old hand-written UI's equivalent (static/dev/dev.js's `scheduleCheck`)
   * wrote a response straight in with no such check -- harmless there only
   * because it re-read `e.editor.getValue()` fresh at write time in a
   * single-threaded callback with no snapshot to compare against; porting
   * that here (writing whatever comes back, whenever it comes back) would
   * let a slow response for an EARLIER keystroke overwrite a newer one that
   * already arrived, showing diagnostics for text no longer in the buffer.
   */
  async check(t = this.active) {
    if (!t) return;
    const text = t.text;
    const report = await api.checkFile(t.ref, t.name, text);
    if (t.text !== text) return; // stale: the buffer moved on while this was in flight
    t.issues = report.issues ?? [];
  }

  async save(t = this.active, { quiet = false } = {}) {
    if (!t) return;
    const { etag } = await api.writeFile(t.ref, t.name, t.text, t.etag);
    t.etag = etag;
    t.saved = t.text;
    this.#wrote(t.name, { quiet });
  }

  /**
   * Validate the whole screen on disk and fan the result out.
   *
   * Spec :315 ("after a successful save the page runs `validate` on the
   * screen") and :326 ("the `validate` result ... shows one green tick per
   * clean file"). `check` is the typing-time counterpart and looks at ONE
   * draft buffer; this looks at every file as it now exists on disk, which is
   * why it belongs after a save and not during one.
   *
   * Issues are matched to files by `location`. An issue whose location is not
   * a file in the listing -- the screen ref itself, for a finding no single
   * file owns -- is kept in `report.issues` and simply ticks nothing; it must
   * not silently mark every file clean, and it must not be dropped.
   *
   * A loaded tab's `issues` are replaced ONLY for a clean-or-flagged file
   * this report actually covers, and only while the buffer matches disk:
   * overwriting a dirty buffer's typing-time diagnostics with a verdict about
   * the saved text would show the user marks for text they can no longer see.
   */
  async validate(ref = this.ref) {
    if (!ref) return;
    const report = await api.validateScreen(ref);
    const issues = report?.issues ?? [];

    const byFile = new Map();
    for (const i of issues) {
      if (!i?.location) continue;
      if (!byFile.has(i.location)) byFile.set(i.location, []);
      byFile.get(i.location).push(i);
    }

    // Only files that exist in the listing can be ticked. `files` is the
    // screen's own listing, so this cannot tick a name the report invented.
    const clean = this.files.filter((f) => !byFile.has(f.name)).map((f) => f.name);

    this.report = { clean, issues };

    for (const t of this.tabs) {
      if (t.ref !== ref || t.dirty) continue;
      t.issues = byFile.get(t.name) ?? [];
    }
  }

  /**
   * Create an empty file in the current screen and open it.
   *
   * `writeFile` with a null etag sends `If-Match: *`, which byonk reads as
   * "create, and fail with 412 if it already exists" (api.js) -- so an
   * existing name comes back as `ConflictError` and is REPORTED rather than
   * silently opening the file that is already there
   * (docs/src/guide/dev-mode.md).
   *
   * The listing is refreshed from the server rather than appended to locally:
   * the server decides the order (`meta.yaml`, `script.lua`, `screen.svg`,
   * then alphabetical) and inventing a position here would put a new file in
   * a place a reload then moves it out of.
   */
  async addFile(name) {
    const ref = this.ref;
    if (!ref || !name) return;
    this.error = null;
    try {
      await api.writeFile(ref, name, "", null);
    } catch (e) {
      const message =
        e instanceof api.ConflictError ? `${name} already exists` : e.message;
      this.error = { op: "add-file", message };
      return;
    }
    await this.refreshFiles(ref);
    await this.open(ref, name, { readOnly: false });
  }

  /**
   * Delete one file from the current screen and close its buffer.
   *
   * `meta.yaml`, `script.lua` and `screen.svg` are refused by the server
   * (`CORE_FILES`, src/api/dev_files.rs) with a 422. The UI does not offer
   * the control for them, but the check is not duplicated here -- one
   * authority, and the server's answer is reported if something asks anyway.
   */
  async deleteFile(name) {
    const ref = this.ref;
    if (!ref || !name) return;
    this.error = null;
    try {
      await api.deleteFile(ref, name);
    } catch (e) {
      this.error = { op: "delete-file", message: e.message };
      return;
    }
    const i = this.tabs.findIndex((t) => t.ref === ref && t.name === name);
    if (i >= 0) this.close(i);
    if (this.asset?.name === name) this.asset = null;
    await this.refreshFiles(ref);
  }

  /**
   * Rename one file. The server owns the rules (core files 422, existing
   * target 412). A clean open text tab follows the new name; the UI refuses
   * a dirty one (a rename must not lose, or silently re-home, an unsaved
   * buffer), and so does this, as a second guard. One render per rename.
   */
  async renameFile(from, to) {
    const ref = this.ref;
    if (!ref || !from || !to || from === to) return;
    const tab = this.tabs.find((t) => t.ref === ref && t.name === from);
    this.error = null;
    if (tab?.dirty) {
      this.error = { op: "rename-file", message: `save ${from} before renaming it` };
      return;
    }
    try {
      await api.renameFile(ref, from, to);
    } catch (e) {
      const message = e instanceof api.ConflictError ? `${to} already exists` : e.message;
      this.error = { op: "rename-file", message };
      return;
    }
    if (tab) {
      tab.name = to;
      tab.revision += 1;
    }
    if (this.asset?.name === from) this.asset = { ref, name: to };
    this.assetRev += 1;
    this.#wrote(from, { quiet: true });
    this.#wrote(to);
    await this.refreshFiles(ref);
  }

  /**
   * Upload one file's bytes. `replace` first reads the current etag and
   * overwrites; without it the PUT is create-only and an existing name comes
   * back as "exists" so the caller can ask. Renders nothing itself: the
   * caller ends a batch with `finishUpload`, which renders once.
   * @returns {Promise<"ok" | "exists" | "failed">}
   */
  async uploadFile(ref, file, { replace = false } = {}) {
    if (!ref) return "failed";
    this.error = null;
    try {
      const etag = replace ? await api.rawEtag(ref, file.name) : null;
      await api.putRaw(ref, file.name, file, etag);
    } catch (e) {
      if (!replace && e instanceof api.ConflictError) return "exists";
      const message = e instanceof api.ConflictError ? `${file.name} changed meanwhile` : e.message;
      this.error = { op: "upload", message };
      return "failed";
    }
    this.#wrote(file.name, { quiet: true });
    return "ok";
  }

  /** End of an upload batch: refresh, render once, show the last file. */
  async finishUpload(ref, names) {
    // The batch's screen, captured at its start: if another screen is open
    // now, the files are on disk but nothing here belongs to them.
    if (!ref || !names.length || this.ref !== ref) return;
    await this.refreshFiles(ref);
    this.diskRevision += 1;
    this.assetRev += 1;
    // An open buffer of a replaced file: reload it, or raise the conflict
    // banner when it has unsaved edits (same as the watcher does).
    await Promise.all(names.map((name) => this.onFileChange({ ref, name })));
    const last = names[names.length - 1];
    const entry = this.files.find((f) => f.name === last);
    if (!entry) return;
    if (entry.text) await this.open(ref, last, { readOnly: false });
    else this.selectAsset(last);
  }

  /**
   * Re-read the screen's listing without disturbing open buffers.
   *
   * Deliberately NOT `loadScreen()`, which calls `reset()` and would discard
   * every unsaved buffer -- adding or deleting one file must not cost the
   * user their edits to another.
   */
  async refreshFiles(ref) {
    try {
      this.files = await api.listFiles(ref);
      if (this.asset && !this.files.some((f) => f.name === this.asset.name)) this.asset = null;
    } catch (e) {
      screens.error = { op: "files", message: e?.message ?? String(e) };
    }
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
  async trySave(t = this.active, { quiet = false } = {}) {
    if (!t) return false;
    this.error = null;
    try {
      await this.save(t, { quiet });
    } catch (e) {
      if (e instanceof api.ConflictError) {
        this.conflict = { ref: t.ref, name: t.name, source: "save" };
        return false;
      }
      this.error = { op: "save", message: e.message };
      return false;
    }

    // Spec :315 -- "after a successful save the page runs `validate` on the
    // screen". Only after a SUCCESSFUL save: a save that conflicted or failed
    // left disk untouched, so there is nothing new to validate and a verdict
    // there would describe the other side's file, not the user's.
    //
    // Its own catch, and its own `op`: a validate that fails must not be
    // reported as a failed save. The save DID land, and telling the user
    // otherwise would push them to save again over their own fresh etag.
    try {
      await this.validate(t.ref);
    } catch (e) {
      this.error = { op: "validate", message: e.message };
    }
    return true;
  }

  /**
   * The Edit pane's Save. Without a Details change it saves the active file,
   * as before. With one, it writes every unsaved file of the screen first --
   * moving reloads the editor under the new ref, which would drop them --
   * and moves only when all of them landed.
   *
   * One render for the whole Save, of the new ref: the file saves are quiet
   * (no `diskRevision` bump, which would render the old ref first) and the
   * move's subject change renders the new place with the saved files. Only
   * when nothing moves -- a save or the move refused -- does a landed file
   * save render after all, so the preview never hides a saved change.
   */
  async commit() {
    if (!this.moveDirty) return this.trySave();
    if (this.moveMissing) return false;
    let saved = false;
    for (const t of this.tabs.filter((x) => x.ref === this.ref && x.dirty && !x.readOnly)) {
      if (!(await this.trySave(t, { quiet: true }))) {
        if (saved) this.diskRevision += 1;
        return false;
      }
      saved = true;
    }
    const moved = await this.move();
    if (!moved && saved) this.diskRevision += 1;
    return moved;
  }

  /**
   * Move the screen to the Details' place. The server rewrites the devices
   * on it; the subject (and mode b's borrowed screen) follow the new ref in
   * the same step that installs the refreshed lists, so the preview renders
   * once, for the new ref. A refusal keeps the draft and lands on
   * `moveFailure`.
   */
  async move() {
    const from = this.ref;
    const { handle, path } = this.target;
    this.moveFailure = null;
    let moved;
    try {
      moved = await api.moveScreen(from, handle, path.trim());
    } catch (e) {
      this.moveFailure = e?.message ?? String(e);
      return false;
    }
    // The watcher reports the moved files a moment later; that is this
    // page's own write, not an outside edit to render again.
    const now = Date.now();
    for (const f of this.files) this.#ownWrites.set(f.name, now);
    this.moveDraft = null;
    const follow = () => subject.renamed("screen", from, moved.ref);
    try {
      await screens.refresh({ applied: follow });
    } catch (e) {
      follow();
      screens.error = { op: "refresh", message: e?.message ?? String(e) };
    }
    return true;
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
    // The server's watcher also reports this page's own writes, a few
    // hundred ms later (it batches every 200 ms). The save already bumped
    // `diskRevision`; counting its echo again would render twice. A name
    // written here within OWN_WRITE_ECHO_MS is taken to be that echo -- an
    // outside edit of the same file inside that window is missed, and the
    // next change renders it.
    const now = Date.now();
    const outside = (names ?? []).filter((n) => !(now - (this.#ownWrites.get(n) ?? -Infinity) < OWN_WRITE_ECHO_MS));
    if (outside.length) this.diskRevision += 1;
    if (this.asset && outside.includes(this.asset.name)) this.assetRev += 1;
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
      this.#wrote(c.name);
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
