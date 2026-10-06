import { describe, it, expect, beforeEach } from "vitest";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";

beforeEach(() => {
  simulate.reset();
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
});

describe("simulate store", () => {
  it("builds a render request and omits unset controls", () => {
    simulate.dither = "atkinson";
    const b = simulate.request("local/hello");
    expect(b.screen).toBe("local/hello");
    expect(b.width).toBe(800);
    expect(b.height).toBe(480);
    expect(b.override.dither).toBe("atkinson");
    expect(b.simulated.params).toEqual({});
    expect(b.panel).toBeUndefined();
    expect(b.override.colors_actual).toBeUndefined();
    expect(b.view).toBeUndefined();
    // Blank fields are left out, not sent as "".
    expect(Object.hasOwn(b.simulated, "battery_voltage")).toBe(false);
    expect(Object.hasOwn(b.override, "chroma_clamp")).toBe(false);
  });

  it("takes width and height from the chosen panel profile", () => {
    // Fixture corrected per Controller Ruling 32: the field a panel is
    // matched on is `id` (the wire value), not `name` -- the brief's
    // original fixture put an id-shaped string in the `name` field, which
    // tested the wrong contract. Same panel data, right field.
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "trmnl_x", width: 1872, height: 1404 }],
    };
    simulate.panel = "trmnl_x";
    expect(simulate.size).toEqual({ width: 1872, height: 1404 });
  });

  it("falls back to the default size when no panel is chosen", () => {
    expect(simulate.size).toEqual({ width: 800, height: 480 });
  });

  // --- Controller Ruling 26/27: one test per conditional rule -------------

  it("(a) sends the device selection as `device`, and then no `screen`", () => {
    simulate.mac = "AA:BB:CC:DD:EE:FF";
    const b = simulate.request("local/hello");
    expect(b.device).toBe("AA:BB:CC:DD:EE:FF");
    expect(Object.hasOwn(b, "mac")).toBe(false);
    // With a device selected the preview shows that device's own screen, as
    // it did before the POST: the server would otherwise render `screen` AS
    // the device, which is the dev-UI work area's mode a, not this drawer's.
    expect(Object.hasOwn(b, "screen")).toBe(false);
  });

  it("sends numeric controls as JSON numbers", () => {
    simulate.battery_voltage = "4.1";
    simulate.rssi = "-67";
    simulate.chroma_clamp = "0.3";
    const b = simulate.request("local/hello");
    expect(b.simulated.battery_voltage).toBe(4.1);
    expect(b.simulated.rssi).toBe(-67);
    expect(b.override.chroma_clamp).toBe(0.3);
  });

  it("(b) sends width/height from the chosen panel's profile, not as raw fields", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "trmnl_x", name: "TRMNL X (16-grey)", width: 1872, height: 1404 }],
    };
    simulate.panel = "trmnl_x";
    const b = simulate.request("local/hello");
    expect(b.width).toBe(1872);
    expect(b.height).toBe(1404);
    // The wire value is the panel's id, never its display name.
    expect(b.panel).toBe("trmnl_x");
    expect(JSON.stringify(b)).not.toContain("TRMNL");
  });

  it("(c) sends use_actual and colors_actual only when a panel is chosen, and colors_actual only when the panel has one", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [
        { id: "with_actual", name: "With actual", width: 800, height: 480, colors: "#000,#FFF", colors_actual: "#111,#EEE" },
        { id: "no_actual", name: "No actual", width: 800, height: 480, colors: "#000,#FFF" },
      ],
    };

    // No panel chosen: neither field appears (already covered by the floor
    // test above, restated here for locality with its siblings).
    let b = simulate.request("local/hello");
    expect(b.view).toBeUndefined();
    expect(b.override.colors_actual).toBeUndefined();

    // Panel chosen and it has colors_actual: both travel.
    simulate.panel = "with_actual";
    b = simulate.request("local/hello");
    expect(b.view.use_actual).toBe(true);
    expect(b.override.colors_actual).toBe("#111,#EEE");

    // Panel chosen but it has no colors_actual: use_actual still travels,
    // colors_actual does not.
    simulate.panel = "no_actual";
    simulate.use_actual = false;
    b = simulate.request("local/hello");
    expect(b.view.use_actual).toBe(false);
    expect(b.override.colors_actual).toBeUndefined();
  });

  it("(d) sends timestamp only while time is locked", () => {
    simulate.timestamp = 1234567890;
    let b = simulate.request("local/hello");
    expect(b.simulated.timestamp).toBeUndefined();

    simulate.timeLocked = true;
    b = simulate.request("local/hello");
    expect(b.simulated.timestamp).toBe(1234567890);
  });

  it("(e) JSON-parses params into an object, and throws on invalid JSON", () => {
    simulate.params = '{"a": 1}';
    const b = simulate.request("local/hello");
    expect(b.simulated.params).toEqual({ a: 1 });

    simulate.params = "{not json";
    expect(() => simulate.request("local/hello")).toThrow();
  });

  it("(f) sends max_error/noise_scale only when they differ from the selected algorithm's default; chroma_clamp always; strength only when not 1", () => {
    screens.meta = {
      devices: [],
      panels: [],
      dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1.0, noise_scale: 8.0 } }],
    };
    simulate.dither = "atkinson";

    // Matches the default for the selected algorithm: omitted.
    simulate.max_error = "1.0";
    simulate.noise_scale = "8.0";
    let b = simulate.request("local/hello");
    expect(b.override.max_error).toBeUndefined();
    expect(b.override.noise_scale).toBeUndefined();

    // Differs from the default: sent.
    simulate.max_error = "0.5";
    simulate.noise_scale = "2.0";
    b = simulate.request("local/hello");
    expect(b.override.max_error).toBe(0.5);
    expect(b.override.noise_scale).toBe(2);

    // chroma_clamp has no per-algorithm default: sent whenever non-empty.
    simulate.chroma_clamp = "0.3";
    b = simulate.request("local/hello");
    expect(b.override.chroma_clamp).toBe(0.3);

    // strength: 1 is the engine default, omitted; anything else is sent.
    simulate.strength = "1";
    b = simulate.request("local/hello");
    expect(b.override.strength).toBeUndefined();
    simulate.strength = "0.5";
    b = simulate.request("local/hello");
    expect(b.override.strength).toBe(0.5);
  });

  it("(f) sends every non-empty tunable when the selected algorithm's defaults are unknown", () => {
    // The screens fetch failed, or the dither field names an algorithm not
    // in the list: with no default to compare against, the user's value is
    // the only thing left to trust.
    simulate.dither = "unknown-algo";
    simulate.max_error = "2.0";
    simulate.noise_scale = "3.0";
    const b = simulate.request("local/hello");
    expect(b.override.max_error).toBe(2);
    expect(b.override.noise_scale).toBe(3);
  });

  // --- Fix round 1: Controller Rulings 32 and 34 ---------------------------

  it("Ruling 32: matches a panel by id only, never by a name that collides with another panel's id", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [
        // A display name that collides with a DIFFERENT panel's id, and
        // sorted first -- an `id`-OR-`name` match would hit this one
        // before ever reaching the panel whose id actually is "trmnl_x".
        { id: "og", name: "trmnl_x", width: 1, height: 1 },
        { id: "trmnl_x", name: "decoy", width: 1872, height: 1404 },
      ],
    };
    simulate.panel = "trmnl_x";
    // Must resolve the panel whose ID is "trmnl_x", not the one whose NAME
    // happens to read "trmnl_x".
    expect(simulate.size).toEqual({ width: 1872, height: 1404 });
  });

  it("Ruling 34: never puts a `strength` key on the request when it is null (what a cleared number input yields)", () => {
    simulate.strength = null;
    const b = simulate.request("local/hello");
    expect(Object.hasOwn(b.override, "strength")).toBe(false);
  });

  // --- colors_actual_override: the tune store's working copy --------------

  it("prefers a matching colors_actual_override in the render query", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "e1004", name: "E1004", width: 800, height: 480, colors: "#000,#FFF", colors_actual: "#101010,#E8E8E8" }],
    };
    simulate.panel = "e1004";
    simulate.use_actual = true;
    simulate.colors_actual_override = { panel: "e1004", colors: "#111111,#EEEEEE" };
    const b = simulate.request("local/a");
    expect(b.override.colors_actual).toBe("#111111,#EEEEEE");
  });

  it("ignores an override that names a different panel", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "e1004", name: "E1004", width: 800, height: 480, colors: "#000,#FFF", colors_actual: "#101010,#E8E8E8" }],
    };
    simulate.panel = "e1004";
    simulate.use_actual = true;
    simulate.colors_actual_override = { panel: "og", colors: "#111111,#EEEEEE" };
    const b = simulate.request("local/a");
    // The panel actually selected still gets its own configured colours.
    expect(b.override.colors_actual).toBe("#101010,#E8E8E8");
  });

  it("falls back to the configured colors once the override is cleared", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "e1004", name: "E1004", width: 800, height: 480, colors: "#000,#FFF", colors_actual: "#101010,#E8E8E8" }],
    };
    simulate.panel = "e1004";
    simulate.use_actual = true;
    simulate.colors_actual_override = { panel: "e1004", colors: "#111111,#EEEEEE" };
    simulate.colors_actual_override = null;
    const b = simulate.request("local/a");
    expect(b.override.colors_actual).toBe("#101010,#E8E8E8");
  });
});
