import * as api from "../api.js";

/**
 * Repos, screens and their metadata. Which one is selected lives in `subject`.
 *
 * Grouping is done over the REPO list, not over the screen list. A writable
 * repo holding no screens is somewhere the user can create one, and it must
 * appear -- `local` on a fresh install is exactly that. Grouping the screen
 * list instead silently drops it, which was the ccc4f47 defect (spec §8.8).
 */
class Screens {
  repos = $state([]);
  list = $state([]);
  meta = $state({ devices: [], panels: [], dither_algorithms: [] });

  /**
   * The last failed screen operation, or null. Shape: `{ op, message }`,
   * where `op` names which action failed ("create" | "copy" | "rename" |
   * "delete") and `message` is whatever the thrown error carries --
   * `api.js`'s typed errors (ConflictError, PreconditionRequiredError) and
   * plain fetch failures all expose `.message`, so nothing new is invented
   * here. Capturing lives in `FilesPane` (it owns the try/catch around each
   * `api.*` call); this field only makes the result observable elsewhere.
   * Rendering it is Task 10's job (StatusBar).
   */
  error = $state(null);

  grouped = $derived(
    this.repos.map((r) => ({
      handle: r.handle,
      writable: r.writable,
      screens: this.list.filter((s) => s.handle === r.handle),
    })),
  );

  writable(ref) {
    if (!ref) return false;
    const handle = ref.split("/", 1)[0];
    return this.repos.find((r) => r.handle === handle)?.writable === true;
  }

  /**
   * The boot-time load, mirroring `auth.load()`: same contract, same
   * swallow-and-record handling, so a failed first fetch shows up in
   * `StatusStrip` as "load: ..." instead of as an unhandled rejection and a
   * silently empty UI.
   *
   * This exists because `refresh()` was reachable only from FilesPane's four
   * buttons and from `tune`, and three of those four need a screen already
   * selected -- so on a fresh page load nothing populated the store and the
   * screen list, the Device and Panel Profile menus and the Dither menu were
   * all empty. Found by Task 14's browser walk.
   */
  async load() {
    // No `this.error = null` here, unlike FilesPane's `run()`. That clear
    // exists so a fresh user-initiated operation drops the previous
    // failure; a boot load has no previous failure to drop, and clearing
    // anyway lets mounting the app wipe an error set moments earlier.
    // `auth.load()` does not clear either.
    try {
      await this.refresh();
    } catch (e) {
      this.error = { op: "load", message: e?.message ?? String(e) };
    }
  }

  /**
   * `applied`, when given, runs in the same synchronous step that installs
   * the new lists, so state that must follow them (a renamed subject) never
   * lets an effect see the new meta with the old id, or the reverse.
   */
  async refresh({ applied } = {}) {
    const [repos, payload] = await Promise.all([api.listRepos(), api.listScreens()]);
    this.repos = repos;
    this.meta = {
      devices: payload.devices ?? [],
      panels: payload.panels ?? [],
      dither_algorithms: payload.dither_algorithms ?? [],
    };
    this.list = (payload.screens ?? []).map((s) => {
      const handle = s.name.split("/", 1)[0];
      return { ...s, ref: s.name, handle, label: s.name.slice(handle.length + 1) };
    });
    applied?.();
  }
}

export const screens = new Screens();
