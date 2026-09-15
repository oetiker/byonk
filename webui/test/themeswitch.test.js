import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import ThemeSwitch from "../src/lib/components/ThemeSwitch.svelte";
import { theme } from "../src/lib/state/theme.svelte.js";

/**
 * The theme control as a user meets it, not as the store sees it.
 *
 * `theme.test.js` covers the store; nothing drove the actual control until
 * this file. That gap is how the walk nearly filed "the theme toggle does
 * nothing" (Finding K's postscript): the control is a three-item bits-ui
 * Select whose trigger opens on `pointerdown`, NOT on `click`, so a plain
 * `fireEvent.click` opens and immediately shuts it and the label never moves.
 *
 * Driving a bits-ui Select under jsdom needs the pointer-capture and
 * scrollIntoView polyfills in `test/setup.js` -- without them the trigger's
 * own handler throws `hasPointerCapture is not a function` and the menu never
 * opens at all.
 */
beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  theme.reset();
  document.body.removeAttribute("style");
});

describe("ThemeSwitch", () => {
  it("opens a three-way menu and applies the chosen mode", async () => {
    render(ThemeSwitch);
    const trigger = screen.getByLabelText(/^Theme:/);
    expect(trigger.getAttribute("aria-label")).toBe("Theme: System");

    await fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent.trim())).toEqual(["System", "Light", "Dark"]);

    // Items select on `pointerup` (select.svelte.js:951), not on `click` --
    // the same asymmetry as the trigger, one level down.
    await fireEvent.pointerUp(options.find((o) => /Dark/.test(o.textContent)), {
      button: 0,
      pointerType: "mouse",
    });
    await waitFor(() => expect(theme.mode).toBe("dark"));
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(trigger.getAttribute("aria-label")).toBe("Theme: Dark");
  });

  it("leaves the body clickable once the menu has closed", async () => {
    // bits-ui's `Select.Content` holds a body scroll lock while it is open
    // (`preventScroll` is true by default in our vendored select-content),
    // which puts `pointer-events: none` on `document.body` and only restores
    // the body style when the content UNMOUNTS. Anything that keeps the
    // content mounted after a close -- `forceMount`, an exit animation that
    // never completes -- leaves the whole app silently unclickable with no
    // visible cause. That was investigated as Finding K and ruled an artifact
    // of an unpainted browser tab, NOT a defect; this pins the contract so a
    // future change to the content wrapper cannot make it a real one.
    render(ThemeSwitch);
    const trigger = screen.getByLabelText(/^Theme:/);

    await fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    const options = await screen.findAllByRole("option");
    expect(document.body.style.pointerEvents).toBe("none"); // correct WHILE open

    await fireEvent.pointerUp(options.find((o) => /Light/.test(o.textContent)), {
      button: 0,
      pointerType: "mouse",
    });
    await waitFor(() => expect(theme.mode).toBe("light"));
    await waitFor(() => expect(document.body.style.pointerEvents).toBe(""));
  });
});
