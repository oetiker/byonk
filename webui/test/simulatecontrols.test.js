import { describe, it, expect, beforeEach } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/svelte";
import SimulateControls from "../src/lib/components/SimulateControls.svelte";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";

beforeEach(() => {
  cleanup();
  simulate.reset();
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
});

describe("SimulateControls", () => {
  it("mounts and reflects a Panel Profile selection into the simulate store", async () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [
        {
          id: "trmnl_x",
          name: "TRMNL X (16-grey)",
          width: 1872,
          height: 1404,
          colors: "#000000,#FFFFFF",
          colors_actual: "#111111,#EEEEEE",
        },
      ],
    };

    const { getByLabelText, container } = render(SimulateControls);

    const panelSelect = getByLabelText("Panel Profile");
    await fireEvent.change(panelSelect, { target: { value: "trmnl_x" } });

    expect(simulate.panel).toBe("trmnl_x");
    expect(simulate.size).toEqual({ width: 1872, height: 1404 });
    // "Show actual panel colors" only appears once a panel with
    // colors_actual is chosen.
    expect(container.querySelector('input[type="checkbox"]')).toBeTruthy();
  });

  // --- Ruling 33: choosing a device populates Panel and Dither from that
  // device's own config, the same caution as the old UI's selectIfPresent()
  // -- only when the device actually names a value, and only when that
  // value is still present in the current list. ---------------------------

  it("Ruling 33: selecting a device populates Panel and Dither from its own config", async () => {
    screens.meta = {
      devices: [{ id: "AA:BB", screen: "local/hello", panel: "trmnl_x", dither: "floyd-steinberg" }],
      panels: [{ id: "trmnl_x", name: "TRMNL X", width: 1872, height: 1404 }],
      dither_algorithms: [
        { name: "atkinson", defaults: { max_error: 1.0, noise_scale: 8.0 } },
        { name: "floyd-steinberg", defaults: { max_error: 1.0, noise_scale: 0 } },
      ],
    };

    const { getByLabelText } = render(SimulateControls);

    const deviceSelect = getByLabelText("Device");
    await fireEvent.change(deviceSelect, { target: { value: "AA:BB" } });

    expect(simulate.panel).toBe("trmnl_x");
    expect(simulate.dither).toBe("floyd-steinberg");
  });

  it("Ruling 33: leaves Panel/Dither alone when the device names a value not in the current list", async () => {
    screens.meta = {
      devices: [{ id: "AA:BB", screen: "local/hello", panel: "no-such-panel", dither: "no-such-algo" }],
      panels: [{ id: "trmnl_x", name: "TRMNL X", width: 1872, height: 1404 }],
      dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1.0, noise_scale: 8.0 } }],
    };
    simulate.panel = "trmnl_x";
    simulate.dither = "atkinson";

    const { getByLabelText } = render(SimulateControls);

    const deviceSelect = getByLabelText("Device");
    await fireEvent.change(deviceSelect, { target: { value: "AA:BB" } });

    // Unrecognised references are left alone, not blanked or assigned to
    // something not selectable in their own <select>.
    expect(simulate.panel).toBe("trmnl_x");
    expect(simulate.dither).toBe("atkinson");
  });

  // --- Ruling 34: a cleared <input type="number"> yields `null` through
  // Svelte's bind:value (verified directly), never "" -- the store must
  // handle that path when driven through the real component, not just
  // through a test that assigns "" by hand. ---------------------------------

  it("Ruling 34: clearing the Strength field drops `strength` from the URL instead of sending a bogus value", async () => {
    simulate.strength = "0.5";

    const { getByLabelText } = render(SimulateControls);
    const strengthInput = getByLabelText("Strength");

    await fireEvent.input(strengthInput, { target: { value: "" } });

    expect(simulate.strength).toBeNull();
    const u = simulate.url("local/hello");
    expect(u).not.toContain("strength=");
    expect(u).not.toContain("null");
  });
});
