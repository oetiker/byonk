import * as api from "../api.js";
import { screens } from "./screens.svelte.js";
import { subject } from "./subject.svelte.js";

/**
 * Screen repos as the Library and the Edit pane see them (U11), over the
 * `dev/api/screen-repos` routes (U10).
 *
 * A git repo needs a URL to exist, so "+" cannot write a placeholder the way
 * panels and devices do (ruling U11-R1): it opens an unsaved draft under the
 * subject id `NEW_REPO`, which Save POSTs. Until then it is not in the list.
 * Drafts of existing repos are kept per handle, so a detour keeps them (P2-4).
 */

/** The subject id of the unsaved new-repo draft. Never a valid handle (the server's rule below). */
export const NEW_REPO = "+new";
/** The server's handle rule (`check_handle`, src/api/dev_repos.rs). */
export const HANDLE_RE = /^[a-z0-9][a-z0-9_-]*$/;
/**
 * How often the list is polled while a fetch runs: every second for the
 * first two minutes, then every ten seconds; a failed GET backs off
 * (doubling, up to ten seconds) instead of ending the poll.
 */
const POLL_MS = 1000;
const POLL_SLOW_AFTER = 120;
const POLL_MAX_MS = 10_000;

class ScreenRepos {
  addonMode = $state(false);
  list = $state([]);
  /** `{[handle | NEW_REPO]: {handle, repo, pin, token, clearToken}}` */
  drafts = $state({});
  /** A refused Save or Update of one repo: `{key, op, message}`; the Edit message row shows it. */
  failure = $state(null);
  /** The list could not be loaded. */
  error = $state(null);
  #pollGen = 0;

  find(handle) {
    return this.list.find((r) => r.handle === handle) ?? null;
  }

  /**
   * A repo byonk itself registers, a folder (kind `path` or `dir`: not git,
   * nothing to fetch and no URL to edit), or every repo in add-on mode:
   * nothing here can change it.
   */
  readOnly(key) {
    if (this.addonMode) return true;
    if (key === NEW_REPO) return false;
    const r = this.find(key);
    return r?.managed !== false || r?.kind !== "git";
  }

  async load() {
    try {
      await this.refresh();
      this.error = null;
    } catch (e) {
      this.error = e?.message ?? String(e);
    }
  }

  async refresh() {
    const out = await api.listScreenRepos();
    this.addonMode = !!out?.addon_mode;
    this.list = out?.repos ?? [];
    this.error = null;
  }

  /**
   * After a write that landed: reload the repo list and the screens. A
   * failure here is not the write's failure -- it lands on `error` (shown in
   * the Library section), never as "Save failed".
   */
  async #reload() {
    try {
      await this.refresh();
      await screens.refresh();
      return true;
    } catch (e) {
      this.error = e?.message ?? String(e);
      return false;
    }
  }

  /** Open the unsaved new-repo draft; returns its subject id. */
  startNew() {
    const taken = new Set(this.list.map((r) => r.handle));
    let handle = "new-repo";
    for (let n = 2; taken.has(handle); n++) handle = `new-repo-${n}`;
    this.drafts = { ...this.drafts, [NEW_REPO]: { handle, repo: "", pin: "", token: "", clearToken: false } };
    this.failure = null;
    return NEW_REPO;
  }

  discardNew() {
    if (!(NEW_REPO in this.drafts)) return;
    this.forget(NEW_REPO);
  }

  forget(key) {
    const { [key]: _, ...rest } = this.drafts;
    this.drafts = rest;
    if (this.failure?.key === key) this.failure = null;
  }

  #baseline(key) {
    const r = this.find(key);
    return r ? { handle: r.handle, repo: r.repo ?? "", pin: r.pin ?? "", token: "", clearToken: false } : null;
  }

  /** What the Edit pane shows: the draft, else the repo as listed; null for neither. */
  draft(key) {
    return this.drafts[key] ?? this.#baseline(key);
  }

  set(key, field, value) {
    const d = this.draft(key);
    if (!d) return;
    const next = { ...d, [field]: value };
    if (field === "token" && value !== "") next.clearToken = false;
    this.drafts = { ...this.drafts, [key]: next };
    if (this.failure?.key === key) this.failure = null;
  }

  /** Remove the token on Save; the token field empties. */
  clearToken(key) {
    this.set(key, "token", "");
    this.drafts = { ...this.drafts, [key]: { ...this.drafts[key], clearToken: true } };
  }

  dirty(key) {
    if (key === NEW_REPO) return NEW_REPO in this.drafts;
    const d = this.drafts[key];
    const b = this.#baseline(key);
    if (!d || !b) return false;
    return d.repo !== b.repo || d.pin !== b.pin || d.token !== "" || d.clearToken;
  }

  /** Why Save cannot go yet, as user-facing sentences. */
  missing(key) {
    const d = this.draft(key);
    if (!d || (key !== NEW_REPO && this.find(key)?.kind !== "git")) return [];
    const out = [];
    if (key === NEW_REPO && !HANDLE_RE.test(d.handle)) {
      out.push("The handle takes lowercase letters, digits, - and _, starting with a letter or digit.");
    }
    if (!d.repo.trim()) out.push("A screen repo needs a repo URL — fill it in to enable Save.");
    return out;
  }

  /** The POST body: empty optional fields are left out. */
  addBody(key) {
    const d = this.draft(key);
    const body = { handle: d.handle, repo: d.repo.trim() };
    if (d.pin.trim()) body.pin = d.pin.trim();
    if (d.token) body.token = d.token;
    return body;
  }

  /** The PATCH body: only what changed. An empty token keeps the stored one; Clear token sends "". */
  patchBody(key) {
    const d = this.draft(key);
    const b = this.#baseline(key);
    const body = {};
    if (d.repo !== b.repo) body.repo = d.repo.trim();
    if (d.pin !== b.pin) body.pin = d.pin.trim();
    if (d.token) body.token = d.token;
    else if (d.clearToken) body.token = "";
    return body;
  }

  /** Save the draft (POST for the new one, PATCH otherwise). Resolves to whether it landed. */
  async save(key) {
    const isNew = key === NEW_REPO;
    let handle = key;
    try {
      if (isNew) {
        const body = this.addBody(key);
        await api.addScreenRepo(body);
        handle = body.handle;
      } else {
        await api.patchScreenRepo(key, this.patchBody(key));
      }
    } catch (e) {
      this.failure = { key, op: "save", message: e?.message ?? String(e) };
      return false;
    }
    // The write landed: from here on nothing is a failed Save.
    this.forget(key);
    if (isNew && subject.is("screen-repo", NEW_REPO)) subject.select("screen-repo", handle);
    await this.#reload();
    this.#watch(handle);
    return true;
  }

  /** Fetch the repo again now, then follow its status. */
  async update(handle) {
    if (this.failure?.key === handle) this.failure = null;
    try {
      await api.updateScreenRepo(handle);
    } catch (e) {
      this.failure = { key: handle, op: "update", message: e?.message ?? String(e) };
      return false;
    }
    await this.#reload();
    this.#watch(handle);
    return true;
  }

  /**
   * Delete a repo. A refused DELETE (409: in use) is thrown to the caller
   * (LibraryPane's `run` puts it on `screens.error`, i.e. the app status
   * bar); a failed reload after it is not.
   */
  async remove(handle) {
    await api.deleteScreenRepo(handle);
    this.forget(handle);
    await this.#reload();
  }

  /** Reload the list quietly (choosing a repo): a status seen earlier may be stale. */
  async sync() {
    try {
      await this.refresh();
    } catch (e) {
      this.error = e?.message ?? String(e);
    }
  }

  /**
   * A fetch runs in the background after add and update: poll the list while
   * the repo says "fetching", then refresh the screens it brought. Not awaited
   * by Save; a later poll or `reset()` supersedes it.
   */
  #watch(handle) {
    const gen = ++this.#pollGen;
    const tick = async (n, failures) => {
      if (gen !== this.#pollGen || this.find(handle)?.status !== "fetching") return;
      const delay = failures > 0
        ? Math.min(POLL_MS * 2 ** failures, POLL_MAX_MS)
        : n >= POLL_SLOW_AFTER ? POLL_MAX_MS : POLL_MS;
      await new Promise((r) => setTimeout(r, delay));
      if (gen !== this.#pollGen) return;
      try {
        await this.refresh();
      } catch (e) {
        // A transient failure: say so in the Library and try again later.
        this.error = e?.message ?? String(e);
        return tick(n + 1, failures + 1);
      }
      if (this.find(handle)?.status !== "fetching") {
        await screens.refresh().catch((e) => (this.error = e?.message ?? String(e)));
        return;
      }
      return tick(n + 1, 0);
    };
    tick(0, 0);
  }

  reset() {
    this.#pollGen++;
    this.addonMode = false;
    this.list = [];
    this.drafts = {};
    this.failure = null;
    this.error = null;
  }
}

export const screenRepos = new ScreenRepos();
