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

  get active() {
    return this.tabs[this.activeIndex] ?? null;
  }

  reset() {
    this.tabs = [];
    this.activeIndex = -1;
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
}

export const editor = new Editor();
