import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { layout, PANELS } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * The icon rail's five toggles, driven through the real app root.
 *
 * `iconrail.test.js` mounts IconRail alone and asserts `aria-pressed` and the
 * `layout` store; `appshell.test.js` mounts AppShell and drives `layout`
 * directly. Both are green, and neither can see the seam BETWEEN them -- that
 * a click on a given rail button mounts or unmounts the pane it names. That
 * is the same shape as every defect this branch has produced (Findings G, I,
 * J): two component suites green about the two sides of a wire nobody tested.
 *
 * So this file never touches `layout` to change what is shown. It clicks the
 * button a user would click and then looks for the pane's own markup.
 *
 * Each pane is identified by something it alone renders, not by a test id
 * added for the occasion:
 *   files    -- the "New" button in its toolbar
 *   editor   -- its `role="tablist"` tab strip
 *   tune     -- `data-testid="tune-pane"` (already present)
 *   preview  -- the `alt="Screen preview"` image
 *   simulate -- the `#sim-device` select
 *
 * Simulate is matched by id, NOT by its "Device" label: TunePane carries a
 * "Device" label of its own (`#tune-device`), so a label query reports the
 * simulate pane as mounted whenever tune is open. That ambiguity showed up
 * here as a failure in the aria-pressed test below and is worth naming --
 * an accessible name that is unique within one pane need not be unique
 * within the shell.
 */
const MARKER = {
  files: () => dom.queryByRole("button", { name: "New" }),
  editor: () => dom.queryByRole("tablist"),
  tune: () => dom.queryByTestId("tune-pane"),
  preview: () => dom.queryByAltText("Screen preview"),
  simulate: () => document.querySelector("#sim-device"),
};

/** The rail button for a panel, by the accessible name IconRail gives it. */
function railButton(panel) {
  const rail = dom.getByRole("toolbar", { name: /panels/i });
  return dom.getByRole("button", {
    name: new RegExp(`^${panel}$`, "i"),
    container: rail,
  });
}

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
  editor.reset();
  editor.files = [];
  screens.selected = null;
  screens.repos = [];
  screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });
});

afterEach(() => {
  cleanup();
});

describe("icon rail toggles, through the app root", () => {
  // The four panels open by default. Clicking each must take its pane away
  // and a second click must bring it back -- a toggle that only worked one
  // way would still flip `aria-pressed` and still pass iconrail.test.js.
  for (const panel of ["files", "editor", "preview", "simulate"]) {
    it(`${panel}: one click unmounts its pane, a second brings it back`, async () => {
      render(AppShell);
      await waitFor(() => expect(MARKER[panel]()).toBeTruthy());

      await fireEvent.click(railButton(panel));
      await waitFor(() => expect(MARKER[panel]()).toBeNull());

      await fireEvent.click(railButton(panel));
      await waitFor(() => expect(MARKER[panel]()).toBeTruthy());
    });
  }

  // Tune is the one panel that starts closed, so it exercises the opposite
  // order -- and it is the newest pane, added by the colour-tuning work after
  // the other four had already shipped.
  it("tune: starts closed, one click mounts it, a second takes it away", async () => {
    render(AppShell);
    expect(MARKER.tune()).toBeNull();

    await fireEvent.click(railButton("tune"));
    await waitFor(() => expect(MARKER.tune()).toBeTruthy());

    await fireEvent.click(railButton("tune"));
    await waitFor(() => expect(MARKER.tune()).toBeNull());
  });

  // Ruling 49: every legal panel combination must stay usable, and the rail
  // is the only chrome no toggle can unmount. Closing all five leaves the
  // user with the rail alone -- and therefore with a way back.
  it("with all five closed, the rail survives and can reopen any of them", async () => {
    render(AppShell);
    await waitFor(() => expect(MARKER.files()).toBeTruthy());

    for (const p of PANELS) {
      if (layout.isOpen(p)) await fireEvent.click(railButton(p));
    }
    await waitFor(() => {
      for (const p of PANELS) expect(MARKER[p]()).toBeNull();
    });

    expect(dom.getByRole("toolbar", { name: /panels/i })).toBeTruthy();
    for (const p of PANELS) expect(railButton(p)).toBeTruthy();

    await fireEvent.click(railButton("editor"));
    await waitFor(() => expect(MARKER.editor()).toBeTruthy());
  });

  // Closing a panel is a view change, not a data change: the open buffer and
  // the selection must survive it. Reaching for the rail to get more room
  // and losing unsaved work would be the worst possible reading of a toggle.
  it("closing and reopening the editor panel keeps the open buffer and its unsaved text", async () => {
    render(AppShell);
    // Wait for the boot load before selecting. `screens.refresh()` sets
    // `repos` and `list` together, so a user can never click a screen before
    // the repo list exists -- but a test calling `select()` by hand can, and
    // `writable()` would then answer false and open the screen READ-ONLY,
    // where `editor.setText()` is a deliberate no-op (editor.svelte.js:201).
    await waitFor(() => expect(screens.repos).toHaveLength(1));

    screens.select("local/a");
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    expect(editor.active.readOnly).toBe(false);

    editor.setText("unsaved edit");

    await fireEvent.click(railButton("editor"));
    await waitFor(() => expect(MARKER.editor()).toBeNull());
    await fireEvent.click(railButton("editor"));
    await waitFor(() => expect(MARKER.editor()).toBeTruthy());

    expect(screens.selected).toBe("local/a");
    expect(editor.active?.name).toBe("script.lua");
    expect(editor.active?.text).toBe("unsaved edit");
    // The tab strip was rebuilt from the listing, not left empty -- and it
    // still carries the unsaved-changes dot, so the user is told the buffer
    // is dirty rather than being shown a clean-looking tab over edited text.
    const tabs = dom.getAllByRole("tab");
    expect(tabs).toHaveLength(1);
    expect(tabs[0].textContent).toContain("script.lua");
    expect(dom.getByLabelText("unsaved changes")).toBeTruthy();
  });

  // The rail's own state must agree with what is on screen. These can
  // disagree: `aria-pressed` reads `layout`, the pane reads `layout`, and a
  // pane guarded on the wrong key would leave the button lit over nothing.
  it("aria-pressed agrees with what is actually mounted, for every panel", async () => {
    render(AppShell);
    await waitFor(() => expect(MARKER.files()).toBeTruthy());

    for (const p of PANELS) {
      const pressed = railButton(p).getAttribute("aria-pressed") === "true";
      expect(!!MARKER[p](), `${p}: aria-pressed=${pressed} but pane mounted=${!!MARKER[p]()}`).toBe(
        pressed,
      );
    }

    // And again after flipping every one of them, so the agreement is not an
    // accident of the default layout.
    for (const p of PANELS) await fireEvent.click(railButton(p));
    await waitFor(() => expect(MARKER.tune()).toBeTruthy());

    for (const p of PANELS) {
      const pressed = railButton(p).getAttribute("aria-pressed") === "true";
      expect(!!MARKER[p](), `${p}: aria-pressed=${pressed} but pane mounted=${!!MARKER[p]()}`).toBe(
        pressed,
      );
    }
  });

  // Panel visibility shares one persisted record with the split sizes
  // (layout.svelte.js's header comment). A toggle must reach that record, or
  // the layout a user arranges is gone on the next load.
  it("a toggle persists, so the arrangement survives a reload", async () => {
    render(AppShell);
    await waitFor(() => expect(MARKER.files()).toBeTruthy());

    await fireEvent.click(railButton("files"));
    await fireEvent.click(railButton("tune"));
    await waitFor(() => expect(MARKER.tune()).toBeTruthy());

    // Re-mount from scratch, as a reload would: drop the DOM, wipe the
    // in-memory store, and let AppShell's own `layout.load()` restore it.
    cleanup();
    layout.reset();
    render(AppShell);

    await waitFor(() => expect(MARKER.tune()).toBeTruthy());
    expect(MARKER.files()).toBeNull();
    expect(railButton("files")).toHaveAttribute("aria-pressed", "false");
    expect(railButton("tune")).toHaveAttribute("aria-pressed", "true");
  });
});

describe("Logout, through the app root", () => {
  // iconrail.test.js drives Logout with `auth.logout` itself mocked, so the
  // wire from the button to `POST /dev/api/logout` and on to the navigation
  // has never been exercised end to end. Ruling 63 makes this control
  // conditional, which is exactly the kind of gate that can render a button
  // that does nothing.
  it("clicking Logout calls the API and navigates to the relative login path", async () => {
    const logoutApi = vi.spyOn(api, "logout").mockResolvedValue({});
    const navigate = vi.spyOn(auth, "navigate").mockImplementation(() => {});

    render(AppShell);
    await waitFor(() => expect(auth.canLogout).toBe(true));

    await fireEvent.click(dom.getByRole("button", { name: /logout/i }));

    await waitFor(() => expect(logoutApi).toHaveBeenCalledTimes(1));
    // Relative, never "/login": under Ingress the Supervisor strips a path
    // prefix the server never sees.
    expect(navigate).toHaveBeenCalledWith("login");
  });

  // A logout that fails must not pretend to have worked. `auth.error` is the
  // fifth independent surface in StatusStrip (Ruling 66).
  it("a failing logout records the error and does not navigate", async () => {
    vi.spyOn(api, "logout").mockRejectedValue(new Error("boom"));
    const navigate = vi.spyOn(auth, "navigate").mockImplementation(() => {});

    render(AppShell);
    await waitFor(() => expect(auth.canLogout).toBe(true));

    await fireEvent.click(dom.getByRole("button", { name: /logout/i }));

    await waitFor(() => expect(auth.error).toEqual({ op: "logout", message: "boom" }));
    expect(navigate).not.toHaveBeenCalled();
  });

  // Ruling 63's negative half, at the app root rather than in isolation:
  // under Ingress `can_logout` is false and NO control may render.
  it("renders no Logout control at all when whoami says can_logout is false", async () => {
    api.whoami.mockResolvedValue({ user: "ingress", via: "ingress", can_logout: false });

    render(AppShell);
    await waitFor(() => expect(auth.user).toBe("ingress"));

    expect(dom.queryByRole("button", { name: /logout/i })).toBeNull();
  });
});
