// Whether the Edit pane's Details block is open, remembered per subject KIND
// ("screen", "panel", "device"). Collapsed by default: the block holds what
// names the subject, which is rarely what you came to edit. Auto-opening (a
// new placeholder, unsaved Details edits, a failed Save) is the editors' own
// business and is never written here -- only the user's explicit choice is.

export const STORAGE_KEY = "byonk.details.v1";

class DetailsState {
  #s = $state({});

  isOpen(kind) {
    return this.#s[kind] === true;
  }

  set(kind, open) {
    if (this.isOpen(kind) === open) return;
    this.#s = { ...this.#s, [kind]: open };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.#s));
    } catch {
      // Storage denied: the choice then lasts for the page session only.
    }
  }

  load() {
    try {
      const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
      const next = {};
      for (const k of ["screen", "panel", "device"]) if (parsed?.[k] === true) next[k] = true;
      this.#s = next;
    } catch {
      // Missing, corrupt or unreadable: keep the default (collapsed).
    }
  }

  reset() {
    this.#s = {};
  }
}

export const details = new DetailsState();
