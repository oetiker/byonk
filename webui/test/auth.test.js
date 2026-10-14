import { describe, it, expect, vi, beforeEach } from "vitest";
import { auth } from "../src/lib/state/auth.svelte.js";
import * as api from "../src/lib/api.js";

// Ruling 63: `GET /dev/api/whoami` (src/api/dev_auth.rs:355's `whoami()`)
// answers `{user, via, can_logout}` -- `can_logout` exists for exactly one
// purpose, telling this UI whether to render a Logout control (true only
// for a cookie session; false for Ingress and the admin bearer token,
// neither of which is a session that can be ended). This store is the one
// place that calls `api.whoami()` and holds the answer for IconRail to
// read.
beforeEach(() => {
  vi.restoreAllMocks();
  auth.reset();
});

describe("auth store", () => {
  it("loads user/via/canLogout from whoami", async () => {
    vi.spyOn(api, "whoami").mockResolvedValue({
      user: "alice",
      via: "cookie",
      can_logout: true,
    });

    await auth.load();

    expect(auth.user).toBe("alice");
    expect(auth.via).toBe("cookie");
    expect(auth.canLogout).toBe(true);
  });

  it("reports canLogout false under Ingress, where whoami says so", async () => {
    vi.spyOn(api, "whoami").mockResolvedValue({
      user: "ha-user",
      via: "ingress",
      can_logout: false,
    });

    await auth.load();

    expect(auth.canLogout).toBe(false);
  });

  it("captures a failed whoami instead of throwing out of load()", async () => {
    vi.spyOn(api, "whoami").mockRejectedValue(new Error("network down"));

    await expect(auth.load()).resolves.toBeUndefined();
    expect(auth.error).toEqual({ op: "whoami", message: "network down" });
    // A failed lookup must not leave a stale/guessed canLogout=true lying
    // around from a previous session -- reset() already covers the default,
    // this pins that a failure doesn't invent a true value either.
    expect(auth.canLogout).toBe(false);
  });

  it("logout() calls the API and then navigates to the relative login page", async () => {
    const logoutSpy = vi.spyOn(api, "logout").mockResolvedValue();
    const navigateSpy = vi.spyOn(auth, "navigate").mockImplementation(() => {});

    await auth.logout();

    expect(logoutSpy).toHaveBeenCalledTimes(1);
    expect(navigateSpy).toHaveBeenCalledWith("login");
  });

  it("captures a failed logout instead of navigating away", async () => {
    vi.spyOn(api, "logout").mockRejectedValue(new Error("boom"));
    const navigateSpy = vi.spyOn(auth, "navigate").mockImplementation(() => {});

    await auth.logout();

    expect(auth.error).toEqual({ op: "logout", message: "boom" });
    expect(navigateSpy).not.toHaveBeenCalled();
  });
});
