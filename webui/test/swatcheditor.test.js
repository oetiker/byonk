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
