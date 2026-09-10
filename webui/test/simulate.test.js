import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

const realRenderUrl = api.renderUrl;

beforeEach(() => {
  simulate.reset();
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
});

afterEach(() => vi.restoreAllMocks());

describe("simulate store", () => {
  it("builds a relative render URL and omits unset controls", () => {
    simulate.dither = "atkinson";
    const u = simulate.url("local/hello");
    expect(u.startsWith("/")).toBe(false);
    expect(u).toContain("screen=local%2Fhello");
    expect(u).toContain("dither=atkinson");
    expect(u).not.toContain("panel=");
    expect(u).not.toContain("colors_actual=");
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

  it("(a) sends the device selection as `mac`, not `device`", () => {
    simulate.mac = "AA:BB:CC:DD:EE:FF";
    const u = simulate.url("local/hello");
    expect(u).toContain("mac=AA%3ABB%3ACC%3ADD%3AEE%3AFF");
    expect(u).not.toContain("device=");
  });

  it("(b) sends width/height from the chosen panel's profile, not as raw fields", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "trmnl_x", name: "TRMNL X (16-grey)", width: 1872, height: 1404 }],
    };
    simulate.panel = "trmnl_x";
    const u = simulate.url("local/hello");
    expect(u).toContain("width=1872");
    expect(u).toContain("height=1404");
    // The wire value is the panel's id, never its display name.
    expect(u).toContain("panel=trmnl_x");
    expect(u).not.toContain("TRMNL");
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
    let u = simulate.url("local/hello");
    expect(u).not.toContain("use_actual=");
    expect(u).not.toContain("colors_actual=");

    // Panel chosen and it has colors_actual: both travel.
    simulate.panel = "with_actual";
    u = simulate.url("local/hello");
    expect(u).toContain("use_actual=true");
    expect(u).toContain("colors_actual=%23111%2C%23EEE");

    // Panel chosen but it has no colors_actual: use_actual still travels,
    // colors_actual does not.
    simulate.panel = "no_actual";
    u = simulate.url("local/hello");
    expect(u).toContain("use_actual=true");
    expect(u).not.toContain("colors_actual=");
  });

  it("(d) sends timestamp only while time is locked", () => {
    simulate.timestamp = 1234567890;
    let u = simulate.url("local/hello");
    expect(u).not.toContain("timestamp=");

    simulate.timeLocked = true;
    u = simulate.url("local/hello");
    expect(u).toContain("timestamp=1234567890");
  });

  it("(e) JSON-parses and re-stringifies params, and throws on invalid JSON", () => {
    simulate.params = '{"a": 1}';
    const u = simulate.url("local/hello");
    expect(u).toContain(`params=${encodeURIComponent('{"a":1}')}`);

    simulate.params = "{not json";
    expect(() => simulate.url("local/hello")).toThrow();
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
    let u = simulate.url("local/hello");
    expect(u).not.toContain("max_error=");
    expect(u).not.toContain("noise_scale=");

    // Differs from the default: sent.
    simulate.max_error = "0.5";
    simulate.noise_scale = "2.0";
    u = simulate.url("local/hello");
    expect(u).toContain("max_error=0.5");
    expect(u).toContain("noise_scale=2");

    // chroma_clamp has no per-algorithm default: sent whenever non-empty.
    simulate.chroma_clamp = "0.3";
    u = simulate.url("local/hello");
    expect(u).toContain("chroma_clamp=0.3");

    // strength: 1 is the engine default, omitted; anything else is sent.
    simulate.strength = "1";
    u = simulate.url("local/hello");
    expect(u).not.toContain("strength=");
    simulate.strength = "0.5";
    u = simulate.url("local/hello");
    expect(u).toContain("strength=0.5");
  });

  it("(f) sends every non-empty tunable when the selected algorithm's defaults are unknown", () => {
    // The screens fetch failed, or the dither field names an algorithm not
    // in the list: with no default to compare against, the user's value is
    // the only thing left to trust.
    simulate.dither = "unknown-algo";
    simulate.max_error = "2.0";
    simulate.noise_scale = "3.0";
    const u = simulate.url("local/hello");
    expect(u).toContain("max_error=2");
    expect(u).toContain("noise_scale=3");
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

  it("Ruling 34: guards `strength` against null (what a cleared number input yields), not just against empty string", () => {
    simulate.strength = null;
    const u = simulate.url("local/hello");
    expect(u).not.toContain("strength=");
  });

  it("Ruling 34: never even puts a `strength` key on the query object when it is null -- not relying on qs() to filter it out downstream", () => {
    // `qs()` drops `null` values regardless of this guard, which is exactly
    // the "accidental correctness" the guard closes: a plain
    // `expect(url).not.toContain("strength=")` passes whether or not the
    // guard exists, because qs() silently absorbs the mistake. Spy on
    // `api.renderUrl` (pass-through) to see the object `url()` actually
    // built, before qs() ever touches it.
    const spy = vi.spyOn(api, "renderUrl").mockImplementation((p) => realRenderUrl(p));
    simulate.strength = null;
    simulate.url("local/hello");
    const query = spy.mock.calls[0][0];
    expect(Object.hasOwn(query, "strength")).toBe(false);
  });
});
