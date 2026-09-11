import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/svelte";
import { tick } from "svelte";
import TunePane from "../src/lib/components/TunePane.svelte";
import { tune } from "../src/lib/state/tune.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import * as api from "../src/lib/api.js";
import { hexToHsl, hslToHex } from "../src/lib/utils/color.js";

beforeEach(() => {
  // @testing-library/svelte has no project-wide auto-cleanup configured in
  // this repo (see iconrail.test.js, splitpane.test.js, simulatecontrols.
  // test.js, and others, which all call cleanup() for the same reason). The
  // brief's beforeEach omitted it; without it each render(TunePane) leaves
  // the prior test's DOM in place, so getByRole/getByLabelText/getByText
  // queries in later tests match multiple stale + fresh elements -- this is
  // what actually made "says so when a device's panel is not known" and
  // "Push, Save and Discard call the store" fail (both threw
  // "Found multiple elements", not "unable to find"): stray nodes from
  // earlier renders in the same file, confirmed by running each in
  // isolation, where both pass.
  cleanup();
  vi.restoreAllMocks();
  // `cleanup()` above also unmounts the previous test's TunePane
  // synchronously, firing its onDestroy, which calls `tune.release()` -- an
  // async method nothing here awaits. Left unmocked, that call reaches
  // `api.endTuningSession`/`api.clearPanelOverride`, which `fetch()` a
  // relative URL; jsdom 30 under vitest carries Node's real global `fetch`
  // (see CLAUDE.md's jsdom-is-not-raw-jsdom note), not a stub, so this is a
  // real (if fast-failing, on a relative-URL parse error) call escaping the
  // test that triggered it. It did not reproduce a second failure here, but
  // nothing pins its timing, so defaulting both to a resolved no-op keeps
  // every such call inert rather than relying on how quickly it happens to
  // fail; individual tests below still override them with their own spies
  // where the call itself is under test.
  vi.spyOn(api, "endTuningSession").mockResolvedValue();
  vi.spyOn(api, "clearPanelOverride").mockResolvedValue();
  tune.reset();
  simulate.reset();
  screens.meta.panels = [
    { id: "e1004", name: "E1004", colors: "#000000,#FFFFFF,#00FF00",
      colors_actual: "#101010,#E8E8E8,#00994D" },
    { id: "og", name: "OG", colors: "#000000,#FFFFFF" },
  ];
  screens.meta.devices = [
    { id: "kitchen", screen: "local/a", panel: "e1004" },
    { id: "attic", screen: "local/a", panel: null },
  ];
});

describe("TunePane", () => {
  it("shows one swatch per colour once a panel is chosen", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    expect(screen.getAllByRole("radio")).toHaveLength(3);
  });

  it("selecting a device sets the panel and starts a session", async () => {
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Device"), { target: { value: "kitchen" } });
    await tick();
    expect(start).toHaveBeenCalledWith("kitchen");
    expect(tune.panel).toBe("e1004");
  });

  it("says so when a device's panel is not known from config", async () => {
    vi.spyOn(api, "startTuningSession").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Device"), { target: { value: "attic" } });
    await tick();
    expect(screen.getByText(/pick the panel/i)).toBeTruthy();
  });

  it("a slider drag changes the selected swatch and the working copy", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    await fireEvent.click(screen.getAllByRole("radio")[2]);
    await fireEvent.input(screen.getByLabelText("Lightness"), { target: { value: "20" } });
    await tick();
    expect(tune.colors[2]).not.toBe("#00994D");
    expect(tune.dirty).toBe(true);
  });

  it("a hex entry that is not a colour changes nothing", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    await fireEvent.click(screen.getAllByRole("radio")[0]);
    await fireEvent.input(screen.getByLabelText("Hex"), { target: { value: "nope" } });
    await tick();
    expect(tune.colors[0]).toBe("#101010");
  });

  it("Push, Save and Discard call the store", async () => {
    const push = vi.spyOn(tune, "push").mockResolvedValue();
    const save = vi.spyOn(tune, "save").mockResolvedValue();
    const discard = vi.spyOn(tune, "discard").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });

    await fireEvent.click(screen.getByRole("button", { name: /push to panel/i }));
    await fireEvent.click(screen.getByRole("button", { name: /save to config/i }));
    await fireEvent.click(screen.getByRole("button", { name: /discard/i }));

    expect(push).toHaveBeenCalled();
    expect(save).toHaveBeenCalled();
    expect(discard).toHaveBeenCalled();
  });

  it("releases the server-side override when it unmounts", async () => {
    const release = vi.spyOn(tune, "release").mockResolvedValue();
    const { unmount } = render(TunePane);
    unmount();
    expect(release).toHaveBeenCalled();
  });

  it("round-trips a colour through hex and HSL without drift", () => {
    // Guards the ported maths: the old UI's sliders write HSL and the hex box
    // writes hex, so a value that changes when it merely passes through both
    // makes every swatch creep as the user works.
    //
    // The brief's list included "#3040A0". Verified directly against
    // `static/dev/dev.js:1168-1238` (the port's source, run standalone, not
    // this repo's copy): that hex does NOT round-trip losslessly even
    // through the *original* code -- hexToHsl(...) rounds h/s/l to integers
    // (Math.round(h*360) etc.), which is inherently lossy for roughly 90% of
    // random RGB values (measured), and #3040A0 -> {h:231,s:54,l:41} ->
    // #3041A1 is one of them. That is a property of the reference
    // implementation's own rounding, not a defect a faithful port could or
    // should fix (the task explicitly forbids "improving" the rounding), so
    // it is not something this test can hold a correct port to. Swapped for
    // "#0AB4D6", a saturated blue confirmed lossless against both this port
    // and the extracted original, so the test still spans black, white, and
    // three saturated hues while actually discriminating a drifting port
    // rather than failing unconditionally.
    for (const hex of ["#000000", "#FFFFFF", "#00994D", "#B03030", "#0AB4D6"]) {
      expect(hslToHex(...Object.values(hexToHsl(hex))).toUpperCase()).toBe(hex.toUpperCase());
    }
  });
});
