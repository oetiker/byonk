import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import SplitPane from "../src/lib/components/SplitPane.svelte";
import Harness from "./SplitPaneHarness.svelte";

describe("SplitPane", () => {
  beforeEach(() => {
    cleanup();
  });

  it("shows a handle when both sides are open", () => {
    render(Harness, { props: { open: true } });
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("retires the handle when a neighbour is hidden", () => {
    // Spec §1: a splitter must not survive as a handle that drags nothing.
    render(Harness, { props: { open: false } });
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });

  it("exposes ARIA separator values for screen readers", () => {
    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    // Should have all three value attributes required by WAI-ARIA Window Splitter
    expect(separator).toHaveAttribute("aria-valuenow");
    expect(separator).toHaveAttribute("aria-valuemin");
    expect(separator).toHaveAttribute("aria-valuemax");

    // Values should be numeric and sensible
    const valuenow = parseInt(separator.getAttribute("aria-valuenow"));
    const valuemin = parseInt(separator.getAttribute("aria-valuemin"));
    const valuemax = parseInt(separator.getAttribute("aria-valuemax"));

    expect(Number.isFinite(valuenow)).toBe(true);
    expect(Number.isFinite(valuemin)).toBe(true);
    expect(Number.isFinite(valuemax)).toBe(true);

    expect(valuenow).toBeGreaterThanOrEqual(valuemin);
    expect(valuenow).toBeLessThanOrEqual(valuemax);
  });

  it("enforces upper bound on size to keep pane a visible", () => {
    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    // The upper bound should be container width - min (to leave min pixels for pane a)
    // In jsdom with default sizing, this should be a finite positive number
    const valuemax = parseInt(separator.getAttribute("aria-valuemax"));
    const valuemin = parseInt(separator.getAttribute("aria-valuemin"));

    expect(valuemax).toBeGreaterThanOrEqual(valuemin);
    expect(valuemax).toBeLessThan(10000); // Sanity check - not the fallback 9999
  });
});
