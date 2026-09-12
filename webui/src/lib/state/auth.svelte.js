import * as api from "../api.js";

/**
 * Who the dev UI is showing this session as.
 *
 * `GET /dev/api/whoami` (src/api/dev_auth.rs's `whoami()`, spec
 * docs/superpowers/specs/2026-09-07-dev-mode-editor-design.md:169) answers
 * `{user, via, can_logout}`. `can_logout` exists for exactly one purpose:
 * telling this UI whether to render a Logout control at all. It is true
 * only for a cookie session (`DevPrincipal::can_logout()` in dev_auth.rs) --
 * Home Assistant Ingress and the admin bearer token aren't sessions that can
 * be ended, and `IconRail` must render no Logout control at all for either
 * (Ruling 63; Task 14 Step 3 item 3 checks exactly this under Ingress).
 */
class Auth {
  user = $state(null);
  via = $state(null);
  canLogout = $state(false);

  /**
   * The last failed whoami/logout call, or null. Shape matches every other
   * store's captured error (`screens.error`, `simulate.error`, `tune.error`
   * -- `{op, message}`), kept observable here even though nothing renders it
   * yet: a silently-failing Logout button would be strictly worse than the
   * missing control this task fixes (Ruling 39's reasoning).
   */
  error = $state(null);

  async load() {
    try {
      const { user, via, can_logout } = await api.whoami();
      this.user = user;
      this.via = via;
      this.canLogout = !!can_logout;
    } catch (e) {
      this.error = { op: "whoami", message: e.message };
    }
  }

  async logout() {
    this.error = null;
    try {
      await api.logout();
    } catch (e) {
      this.error = { op: "logout", message: e.message };
      return;
    }
    this.navigate("login");
  }

  /**
   * The one browser side effect in this module, split out so a test can
   * replace it -- jsdom's `window.location` cannot be reassigned or safely
   * spied on directly. A relative target, not `/login`: under Home
   * Assistant Ingress the Supervisor strips a path prefix the server never
   * sees (the same rule `api.js` itself follows for every URL it builds).
   */
  navigate(path) {
    window.location.assign(path);
  }

  reset() {
    this.user = null;
    this.via = null;
    this.canLogout = false;
    this.error = null;
  }
}

export const auth = new Auth();
