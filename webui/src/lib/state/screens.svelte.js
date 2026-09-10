import * as api from "../api.js";

/**
 * Repos, screens, and which one is selected.
 *
 * Grouping is done over the REPO list, not over the screen list. A writable
 * repo holding no screens is somewhere the user can create one, and it must
 * appear -- `local` on a fresh install is exactly that. Grouping the screen
 * list instead silently drops it, which was the ccc4f47 defect (spec §8.8).
 */
class Screens {
  repos = $state([]);
  list = $state([]);
  selected = $state(null);
  meta = $state({ devices: [], panels: [], dither_algorithms: [] });

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

  select(ref) {
    this.selected = ref;
  }

  async refresh() {
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
  }
}

export const screens = new Screens();
