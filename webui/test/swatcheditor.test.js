import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import SwatchEditor from "../src/lib/components/SwatchEditor.svelte";

beforeEach(() => cleanup());
afterEach(() => cleanup());

const WELL_FORMED = /^#[0-9A-F]{6}(,#[0-9A-F]{6})*$/i;
const FOUR = "#000000,#FFFFFF,#FF0000,#0000FF";
const TWO = "#000000,#FFFFFF";

describe("SwatchEditor after the palette shrinks under the selection", () => {
  async function shrunk() {
    const emitted = [];
    const props = { value: FOUR, baseline: FOUR, onchange: (v) => emitted.push(v) };
    const { rerender } = render(SwatchEditor, props);
    await fireEvent.click(screen.getByRole("radio", { name: "Swatch 4" }));
    await rerender({ ...props, value: TWO });
    await tick();
    return emitted;
  }

  it("a slider edits a swatch that exists, never one past the end", async () => {
    const emitted = await shrunk();
    const hue = screen.getAllByRole("slider")[0];
    await fireEvent.keyDown(hue, { key: "ArrowRight" });
    expect(emitted.length).toBeGreaterThan(0);
    for (const v of emitted) {
      expect(v).toMatch(WELL_FORMED);
      expect(v.split(",")).toHaveLength(2);
    }
  });

  it("the hex box edits a swatch that exists, never one past the end", async () => {
    const emitted = await shrunk();
    const hex = document.getElementById("swatch-hex");
    hex.value = "#123456";
    await fireEvent.input(hex);
    expect(emitted).toEqual(["#000000,#123456"]);
  });
});

describe("SwatchEditor big swatches and side-by-side comparison", () => {
  const LIVE = "#1B1C1F,#D9D6CC,#A8312B";
  const ORIG = "#000000,#FFFFFF,#FF0000";

  it("swatch buttons are 72 px and carry their hex as a caption, keeping the accessible name", () => {
    render(SwatchEditor, { value: LIVE, baseline: ORIG, onchange: () => {} });
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(3);
    radios.forEach((r, i) => {
      expect(r.className).toContain("size-18");
      expect(r.getAttribute("aria-label")).toBe(`Swatch ${i + 1}`);
      expect(r.querySelector("[data-swatch-caption]").textContent).toBe(LIVE.split(",")[i]);
    });
  });

  it("shows original and live as two adjacent 80 px squares with a legend, and follows the selection", async () => {
    render(SwatchEditor, { value: LIVE, baseline: ORIG, onchange: () => {} });
    const sq = () => [
      document.querySelector('[data-testid="cmp-original"]'),
      document.querySelector('[data-testid="cmp-live"]'),
    ];
    expect(sq()[0].className).toContain("size-20");
    expect(sq()[1].className).toContain("size-20");
    expect(sq()[0].nextElementSibling).toBe(sq()[1]);
    expect(sq()[0].style.backgroundColor).toBe("rgb(0, 0, 0)");
    expect(sq()[1].style.backgroundColor).toBe("rgb(27, 28, 31)");
    const legend = document.querySelector('[data-testid="cmp-legend"]');
    expect(legend.textContent).toContain("Original");
    expect(legend.textContent).toContain("#000000");
    expect(legend.textContent).toContain("#1B1C1F");

    await fireEvent.click(screen.getByRole("radio", { name: "Swatch 3" }));
    expect(sq()[0].style.backgroundColor).toBe("rgb(255, 0, 0)");
    expect(sq()[1].style.backgroundColor).toBe("rgb(168, 49, 43)");
    expect(legend.textContent).toContain("#FF0000");
    expect(legend.textContent).toContain("#A8312B");
  });

  it("an entry without a baseline shows an empty dashed original and no hex", () => {
    render(SwatchEditor, { value: LIVE, baseline: "#000000", onchange: () => {} });
    fireEvent.click(screen.getByRole("radio", { name: "Swatch 2" }));
    return tick().then(() => {
      const o = document.querySelector('[data-testid="cmp-original"]');
      expect(o.style.backgroundColor).toBe("");
      expect(o.className).toContain("border-dashed");
      expect(document.querySelector('[data-testid="cmp-original-hex"]').textContent).toBe("");
    });
  });
});
