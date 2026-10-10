// Which panels are open, persisted.
//
// The pane SIZES are not kept here: AppShell's Resizable group persists them
// itself (PaneForge `autoSaveId`, in percent), including the size a collapsed
// pane returns to when it is reopened -- a panel that reopens at a default
// size instead of the size it had reads as a bug (spec §1). This record only
// says which of the three are open; AppShell collapses and expands the panes
// to match it.

export const PANELS = ["library", "editor", "preview"];
export const STORAGE_KEY = "byonk.layout.v1";

const DEFAULTS = {
  panels: { library: true, editor: true, preview: true },
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
    this.setOpen(name, !this.isOpen(name));
  }

  setOpen(name, open) {
    if (this.isOpen(name) === open) return;
    this.#s.panels[name] = open;
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
      // Older records also carried px splitter sizes (`sizes.x`, `sizes.y`);
      // sizes are percent and PaneForge's now, so those are ignored.
      this.#s = next;
    } catch {
      // Corrupt stored state is not worth a broken page. Keep the defaults.
    }
  }
}

export const layout = new Layout();
