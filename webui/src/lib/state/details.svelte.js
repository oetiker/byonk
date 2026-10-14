// Whether a screen's Details block is open in the Edit pane. Collapsed by
// default: the block says where the screen lives, which is rarely what you
// came to edit. Panels, devices and screen repos always show their Details, so
// only screens need this. Auto-opening (a pending Repo/Path edit, a refused
// move) is the editor's own business and is never written here -- only the
// user's explicit choice is.

export const STORAGE_KEY = "byonk.details.v2";

class DetailsState {
  #open = $state(false);

  isOpen() {
    return this.#open;
  }

  set(open) {
    if (this.#open === open) return;
    this.#open = open;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(open));
    } catch {
      // Storage denied: the choice then lasts for the page session only.
    }
  }

  load() {
    try {
      this.#open = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") === true;
    } catch {
      // Missing, corrupt or unreadable: keep the default (collapsed).
    }
  }

  reset() {
    this.#open = false;
  }
}

export const details = new DetailsState();
