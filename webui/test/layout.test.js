import { describe, it, expect, beforeEach } from "vitest";
import { layout, PANELS, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";

beforeEach(() => {
  localStorage.clear();
  layout.reset();
});

describe("layout store", () => {
  it("has three panels, all open by default", () => {
    expect(PANELS).toEqual(["library", "editor", "preview"]);
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(true);
  });

  it("drops stale keys from the five-panel layout without a storage bump", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      panels: { files: false, editor: false, tune: true, preview: true, simulate: false },
      sizes: { x: 500, y: 200 },
    }));
    layout.load();
    expect(layout.isOpen("editor")).toBe(false);
    expect(layout.isOpen("library")).toBe(true); // renamed from files: default-open once
    expect(layout.panels.tune).toBeUndefined();
  });

  it("allows every combination, including none open", () => {
    for (const p of PANELS) if (layout.isOpen(p)) layout.toggle(p);
    // Spec §1: "Do not add a 'you must keep one open' rule -- it turns a
    // toggle into an argument."
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(false);
  });

  it("persists open state", () => {
    layout.toggle("preview");

    layout.reset();
    expect(layout.isOpen("preview")).toBe(true);

    layout.load();
    expect(layout.isOpen("preview")).toBe(false);
  });

  it("setOpen writes only on a change", () => {
    layout.setOpen("library", true); // already open
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
    layout.setOpen("library", false);
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).panels.library).toBe(false);
  });

  it("ignores the px sizes older records carried and never writes them back", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ panels: {}, sizes: { x: 500, y: 200 } }));
    layout.load();
    layout.toggle("editor");
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY)).sizes).toBeUndefined();
  });

  it("survives corrupt stored state rather than throwing", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    expect(() => layout.load()).not.toThrow();
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(true);
  });
});
