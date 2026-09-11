import { describe, it, expect, beforeEach } from "vitest";
import { layout, PANELS, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";

beforeEach(() => {
  localStorage.clear();
  layout.reset();
});

describe("layout store", () => {
  it("starts with the default four panels open, tune closed", () => {
    for (const p of PANELS) {
      expect(layout.isOpen(p)).toBe(p !== "tune");
    }
  });

  it("allows every combination, including none open", () => {
    // `tune` defaults closed, unlike the other four, so a single toggle
    // per panel does not land every panel on the same state -- close
    // whichever are open instead of assuming a uniform starting point.
    for (const p of PANELS) if (layout.isOpen(p)) layout.toggle(p);
    // Spec §1: "Do not add a 'you must keep one open' rule -- it turns a
    // toggle into an argument."
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(false);
  });

  it("gives a reopened panel the size it had", () => {
    layout.setSize("x", 640);
    layout.toggle("preview");
    layout.toggle("preview");
    expect(layout.size("x")).toBe(640);
  });

  it("persists open state and sizes together", () => {
    layout.toggle("simulate");
    layout.setSize("y", 220);
    layout.save();

    layout.reset();
    expect(layout.isOpen("simulate")).toBe(true);

    layout.load();
    expect(layout.isOpen("simulate")).toBe(false);
    expect(layout.size("y")).toBe(220);
  });

  it("survives corrupt stored state rather than throwing", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    expect(() => layout.load()).not.toThrow();
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(p !== "tune");
  });

  it("carries tune as a fifth panel, closed by default", () => {
    expect(PANELS).toEqual(["files", "editor", "tune", "preview", "simulate"]);
    layout.reset();
    expect(layout.isOpen("tune")).toBe(false);
    expect(layout.isOpen("files")).toBe(true);
  });

  it("persists and restores tune alongside the other four", () => {
    layout.reset();
    layout.toggle("tune");
    expect(layout.isOpen("tune")).toBe(true);

    const other = new (Object.getPrototypeOf(layout).constructor)();
    other.load();
    expect(other.isOpen("tune")).toBe(true);
  });
});
