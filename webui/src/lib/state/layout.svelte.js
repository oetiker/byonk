// Panel visibility and splitter sizes, persisted together.
//
// "Together" is the point: a panel that reopens at a default size instead of
// the size it had reads as a bug, so one record carries both (spec §1).

export const PANELS = ["library", "editor", "preview"];
export const STORAGE_KEY = "byonk.layout.v1";

const DEFAULTS = {
  panels: { library: true, editor: true, preview: true },
  sizes: { x: 420 },
};

function fresh() {
  return structuredClone(DEFAULTS);
}

class Layout {
  #s = $state(fresh());

  get panels() {
    return this.#s.panels;
  }

  isOpen(name) {
    return this.#s.panels[name] === true;
  }

  toggle(name) {
    this.#s.panels[name] = !this.#s.panels[name];
    this.save();
  }

  size(axis) {
    return this.#s.sizes[axis];
  }

  setSize(axis, px) {
    this.#s.sizes[axis] = px;
    this.save();
  }

  reset() {
    this.#s = fresh();
  }

  save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.#s));
    } catch {
      // A private window, or storage denied. The layout is a convenience;
      // losing it must never break the editor.
    }
  }

  load() {
    let raw;
    try {
      raw = localStorage.getItem(STORAGE_KEY);
    } catch {
      return;
    }
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw);
      const next = fresh();
      for (const p of PANELS) {
        if (typeof parsed?.panels?.[p] === "boolean") next.panels[p] = parsed.panels[p];
      }
      // Only "x" exists; an old stored "y" (the retired preview/inputs divider) is ignored.
      for (const a of ["x"]) {
        if (Number.isFinite(parsed?.sizes?.[a])) next.sizes[a] = parsed.sizes[a];
      }
      this.#s = next;
    } catch {
      // Corrupt stored state is not worth a broken page. Keep the defaults.
    }
  }
}

export const layout = new Layout();
