import { describe, it, expect, beforeEach } from "vitest";
import { render, screen as dom, cleanup } from "@testing-library/svelte";
import Ladder from "../src/lib/components/Ladder.svelte";

// No auto-cleanup in this vitest setup (no globals): without it the loop below
// would stack ladders and `getByTestId("effective")` would match several (PF4).
beforeEach(() => cleanup());

const ORDER = ["override", "device_final", "screen", "device_base", "panel", "fallback"];
function report(from, rungs, effective = "v") {
  return { effective, from, order: ORDER, rungs };
}
const status = (rung) => document.querySelector(`[data-rung="${rung}"]`).dataset.status;

describe("Ladder", () => {
  for (const rung of ORDER) {
    it(`shows ${rung} winning`, () => {
      const rungs = { [rung]: { value: rung, status: "winner" } };
      if (rung !== "fallback") rungs.fallback = { value: "fb", status: "outranked" };
      render(Ladder, { setting: "dither", editing: [], state: "ok", report: report(rung, rungs, rung) });
      expect(status(rung)).toBe("winner");
      expect(dom.getByTestId("effective").textContent).toContain(rung === "device_final" ? "device final" : rung.replace("_", " "));
      for (const other of ORDER.filter((r) => r !== rung && r !== "fallback")) expect(status(other)).toBe("absent");
    });
  }

  it("override wins over everything, and an edited panel rung says so", () => {
    render(Ladder, {
      setting: "strength", editing: ["panel"], state: "ok",
      report: report("override", {
        override: { value: 0.5, status: "winner" },
        screen: { value: 0.9, status: "outranked" },
        panel: { value: 0.5, status: "outranked" },
      }, 0.5),
    });
    expect(status("override")).toBe("winner");
    expect(dom.getByRole("status").textContent).toContain("your panel value is not used in this preview");
    expect(document.querySelector('[data-rung="panel"]').textContent).toContain("●");
  });

  it("explains a rejected rung with its reason", () => {
    render(Ladder, {
      setting: "colors_actual", editing: ["panel"], state: "ok",
      report: report("fallback", {
        panel: { value: ["#101010", "#E8E8E8"], status: "rejected", reason: "has 2 usable entries but the resolved palette has 16" },
      }, null),
    });
    expect(status("panel")).toBe("rejected");
    expect(dom.getByRole("status").textContent).toContain(
      "your panel value was rejected: has 2 usable entries but the resolved palette has 16");
  });

  it("says no render yet, and shows no rungs", () => {
    render(Ladder, { setting: "dither", editing: [], state: "none", report: null });
    expect(dom.getByText("No render yet.")).toBeTruthy();
    expect(document.querySelector("[data-rung]")).toBe(null);
  });

  it("says the last render failed instead of showing stale rungs", () => {
    render(Ladder, { setting: "dither", editing: [], state: "failed", failure: "Render error: boom", report: null });
    expect(dom.getByRole("alert").textContent).toContain("Last render failed: Render error: boom");
    expect(document.querySelector("[data-rung]")).toBe(null);
  });

  it("labels a payload shown while Preview is closed", () => {
    render(Ladder, { setting: "dither", editing: [], state: "ok", stale: true,
      report: report("fallback", { fallback: { value: "atkinson", status: "winner" } }, "atkinson") });
    expect(dom.getByText("From the last render — Preview is closed.")).toBeTruthy();
  });
});
