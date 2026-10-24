import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
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

describe("SimulatedInputs current time", () => {
  async function setTime(value) {
    const el = document.getElementById("sim-time");
    el.value = value;
    await fireEvent.input(el);
    await fireEvent.change(el);
    await tick();
  }

  it("clearing the field releases the clock instead of locking it on NaN", async () => {
    render(SimulatedInputs);
    await setTime("2026-01-02T03:04");
    expect(simulate.timeLocked).toBe(true);
    expect(Number.isFinite(simulate.simulated({ withParams: false }).timestamp)).toBe(true);

    await setTime("");
    expect(simulate.timeLocked).toBe(false);
    expect(simulate.simulated({ withParams: false })).not.toHaveProperty("timestamp");
    expect(Number.isNaN(simulate.timestamp)).toBe(false);
  });
});
