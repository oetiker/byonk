// What the work area is about: a screen, a panel profile, or a device
// (spec §2). The mode is not a separate variable -- there is nothing a mode
// could say that the subject does not.
//
// The three per-mode preview choices live here too, because they outlive a
// subject switch: mode a's hardware target, mode b's borrowed screen
// (remembered for the page session, R-H 1) and mode b's watched device.

export const DEFAULT_BORROWED_SCREEN = "byonk-builtin/calibration/tone";

class Subject {
  kind = $state(null);
  id = $state(null);
  /** Mode a: render the screen as `{kind: "panel"|"device", id}`, or null for the 800x480 default. */
  target = $state(null);
  /** Mode b: which screen renders on the panel being calibrated. */
  borrowedScreen = $state(DEFAULT_BORROWED_SCREEN);
  /** Mode b: which device of that panel the user watches ("" = none). */
  watchDevice = $state("");

  get key() {
    return this.kind ? `${this.kind}:${this.id}` : null;
  }

  /** The screen being EDITED -- only in mode a. */
  get screenRef() {
    return this.kind === "screen" ? this.id : null;
  }

  is(kind, id) {
    return this.kind === kind && this.id === id;
  }

  select(kind, id) {
    if (!kind || id === null || id === undefined || id === "") {
      this.clear();
      return;
    }
    this.kind = kind;
    this.id = id;
  }

  /**
   * An entry was renamed (a panel Id or device Key saved under a new name,
   * a screen moved): the subject, mode a's target, mode b's watched device
   * and borrowed screen follow it, so the preview keeps rendering it.
   */
  renamed(kind, from, to) {
    if (this.is(kind, from)) this.id = to;
    if (kind === "screen" && this.borrowedScreen === from) this.borrowedScreen = to;
    if (this.target?.kind === kind && this.target.id === from) this.target = { kind, id: to };
    if (kind === "device" && this.watchDevice === from) this.watchDevice = to;
  }

  clear() {
    this.kind = null;
    this.id = null;
  }

  reset() {
    this.clear();
    this.target = null;
    this.borrowedScreen = DEFAULT_BORROWED_SCREEN;
    this.watchDevice = "";
  }
}

export const subject = new Subject();
