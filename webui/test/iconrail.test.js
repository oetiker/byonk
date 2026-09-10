import { describe, it, expect, beforeEach } from "vitest";
// The brief's code block imports `fireEvent` from "@testing-library/dom"
// directly; that package is only a transitive dependency here (pulled in by
// @testing-library/svelte), and pnpm's strict node-linker refuses to
// resolve it as a direct import (phantom-dependency protection). Every
// other test in this suite (see splitpane.test.js) already imports
// `fireEvent` from @testing-library/svelte, which re-exports it -- follow
// that convention instead of adding a new direct dependency for one name.
import { render, screen, fireEvent, cleanup } from "@testing-library/svelte";
import IconRail from "../src/lib/components/IconRail.svelte";
import { layout, PANELS } from "../src/lib/state/layout.svelte.js";

beforeEach(() => {
  // The brief's beforeEach resets localStorage and the layout store but
  // does not unmount the previous test's render (@testing-library/svelte
  // has no project-wide auto-cleanup configured here -- see
  // splitpane.test.js, which calls cleanup() itself for the same reason).
  // Without it, each `render(IconRail)` call across this file's three
  // tests leaves the prior mount's DOM in place, so "getByRole" queries
  // in later tests match multiple stale + fresh elements.
  cleanup();
  localStorage.clear();
  layout.reset();
});

describe("IconRail", () => {
  it("offers one labelled toggle per panel", () => {
    render(IconRail);
    for (const p of PANELS) {
      expect(screen.getByRole("button", { name: new RegExp(p, "i") })).toBeInTheDocument();
    }
  });

  it("reports each panel's state through aria-pressed", async () => {
    render(IconRail);
    const files = screen.getByRole("button", { name: /files/i });
    expect(files).toHaveAttribute("aria-pressed", "true");

    await fireEvent.click(files);
    expect(layout.isOpen("files")).toBe(false);
    expect(files).toHaveAttribute("aria-pressed", "false");
  });

  it("carries no badge or count -- the status bar reports those", () => {
    render(IconRail);
    // Spec §1: "The toggle state is the only thing the icon carries."
    const rail = screen.getByRole("toolbar", { name: /panels/i });
    expect(rail.textContent.replace(/\s/g, "")).toBe("");
  });
});
