import { describe, it, expect, beforeEach, vi } from "vitest";
// The brief's code block imports `fireEvent` from "@testing-library/dom"
// directly; that package is only a transitive dependency here (pulled in by
// @testing-library/svelte), and pnpm's strict node-linker refuses to
// resolve it as a direct import (phantom-dependency protection). Every
// other test in this suite already imports
// `fireEvent` from @testing-library/svelte, which re-exports it -- follow
// that convention instead of adding a new direct dependency for one name.
import { render, screen, fireEvent, cleanup } from "@testing-library/svelte";
import IconRail from "../src/lib/components/IconRail.svelte";
import { layout, PANELS } from "../src/lib/state/layout.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";

beforeEach(() => {
  // The brief's beforeEach resets localStorage and the layout store but
  // does not unmount the previous test's render (@testing-library/svelte
  // has no project-wide auto-cleanup configured here -- every other
  // suite calls cleanup() itself for the same reason).
  // Without it, each `render(IconRail)` call across this file's three
  // tests leaves the prior mount's DOM in place, so "getByRole" queries
  // in later tests match multiple stale + fresh elements.
  cleanup();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
});

describe("IconRail", () => {
  it("offers one labelled toggle per panel", () => {
    render(IconRail);
    const toggles = screen
      .getAllByRole("button")
      .filter((el) => el.hasAttribute("aria-pressed"));
    expect(toggles.map((el) => el.getAttribute("aria-label"))).toEqual(["Library", "Edit", "Preview"]);
    expect(PANELS).toHaveLength(3);
  });

  it("reports each panel's state through aria-pressed", async () => {
    render(IconRail);
    const lib = screen.getByRole("button", { name: /library/i });
    expect(lib).toHaveAttribute("aria-pressed", "true");

    await fireEvent.click(lib);
    expect(layout.isOpen("library")).toBe(false);
    expect(lib).toHaveAttribute("aria-pressed", "false");
  });

  it("carries no badge or count -- the status bar reports those", () => {
    render(IconRail);
    // Spec §1: "The toggle state is the only thing the icon carries."
    const rail = screen.getByRole("toolbar", { name: /panels/i });
    expect(rail.textContent.replace(/\s/g, "")).toBe("");
  });

  // Ruling 52: the brief creates ThemeSwitch.svelte but never mounts it.
  // It is mounted here, pushed to the bottom with `mt-auto`, specifically
  // because IconRail is the one piece of chrome no panel toggle can
  // unmount -- this pins that it stays reachable even when the editor
  // panel (the one the brief's own text calls out) is closed.
  //
  // Mutation performed to confirm this can fail (then reverted): wrapped
  // the `<div class="mt-auto"><ThemeSwitch /></div>` block in
  // IconRail.svelte inside `{#if layout.isOpen("editor")}...{/if}`.
  // Result: this test failed with
  //   TestingLibraryElementError: Unable to find an accessible element
  //   with the role "button" and name `/theme/i`
  // confirming it actually exercises the gating, not just the mount.
  it("keeps the theme switch reachable with the editor panel closed", () => {
    if (layout.isOpen("editor")) layout.toggle("editor");
    render(IconRail);
    expect(layout.isOpen("editor")).toBe(false);
    expect(screen.getByRole("button", { name: /theme/i })).toBeInTheDocument();
  });

  // Ruling 63: the rewrite dropped Logout outright -- `can_logout` in
  // `whoami`'s response (src/api/dev_auth.rs) exists for exactly one
  // purpose, telling this UI whether to render the control. Both
  // directions are pinned, not just the positive case: under Home
  // Assistant Ingress `can_logout` is false and Task 14 Step 3 item 3
  // requires that NO Logout control renders at all.
  //
  // Mounted alongside the theme switch, inside the same `mt-auto` group --
  // IconRail is the one piece of chrome no panel toggle can unmount
  // (Ruling 49/52's reasoning applies here just as much as it did to the
  // theme switch).
  it("renders no Logout control when can_logout is false (e.g. Ingress)", () => {
    auth.canLogout = false;
    render(IconRail);
    expect(screen.queryByRole("button", { name: /logout/i })).toBeNull();
  });

  it("renders a Logout control when can_logout is true, reachable with every panel closed", () => {
    for (const p of PANELS) {
      if (layout.isOpen(p)) layout.toggle(p);
    }
    auth.canLogout = true;
    render(IconRail);
    expect(screen.getByRole("button", { name: /logout/i })).toBeInTheDocument();
  });

  it("clicking Logout calls auth.logout()", async () => {
    auth.canLogout = true;
    const logoutSpy = vi.spyOn(auth, "logout").mockResolvedValue();
    render(IconRail);
    await fireEvent.click(screen.getByRole("button", { name: /logout/i }));
    expect(logoutSpy).toHaveBeenCalledTimes(1);
  });
});
