import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import { tick } from "svelte";
import SimulatedInputs from "../src/lib/components/SimulatedInputs.svelte";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";

beforeEach(() => {
  cleanup();
  simulate.reset();
  subject.reset();
});
afterEach(() => cleanup());

describe("SimulatedInputs not-saved badge", () => {
  it("shows nothing while no input is set", () => {
    render(SimulatedInputs);
    expect(screen.queryByTestId("sim-badge")).toBeNull();
  });

  it("shows the badge in the always-visible block once an input is set", async () => {
    render(SimulatedInputs);
    simulate.battery_voltage = "3.7";
    await tick();
    const badge = screen.getByTestId("sim-badge");
    expect(badge.textContent).toMatch(/not saved/);
    expect(screen.getByTestId("simulated-inputs").contains(badge)).toBe(true);
  });
});
