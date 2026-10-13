import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen as dom, fireEvent, waitFor, cleanup, within } from "@testing-library/svelte";
import ConfigEditor from "../src/lib/components/ConfigEditor.svelte";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { details } from "../src/lib/state/details.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  // Details are collapsed by default (U14); these tests are about their content.
  for (const k of ["screen", "panel", "device"]) details.set(k, true);
  cleanup(); vi.restoreAllMocks();
  configDraft.reset(); subject.reset(); resolution.reset();
  screens.list = [{ ref: "local/a" }, { ref: "local/b" }];
  screens.meta = {
    panels: [{ id: "e1004", name: "reTerminal E1004", match: "reterminal_e1004", width: 1600, height: 1200,
               colors: "#000000,#FFFFFF",
               colors_actual: "#101010,#E8E8E8", dither: null }],
    devices: [{ id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: {},
                base: {}, final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null }],
    dither_algorithms: [{ name: "atkinson", defaults: {} }, { name: "stucki", defaults: {} }],
  };
  configDraft.usage = { panels: { e1004: ["kitchen", "hall", "office"] }, unseen: ["attic", "porch"] };
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue(configDraft.usage);
});

describe("ConfigEditor, panel schema", () => {
  beforeEach(() => subject.select("panel", "e1004"));

  it("names its subject and warns how many devices it reaches", () => {
    render(ConfigEditor);
    expect(dom.getByText("panel reTerminal E1004")).toBeTruthy();
    expect(dom.getByText("⚠ 3 devices use this panel")).toBeTruthy();
    expect(dom.getByText("2 more devices have not polled since byonk started, so their panel is not known yet.")).toBeTruthy();
  });

  it("puts the action bar above the form, outside the scrolling body", () => {
    render(ConfigEditor);
    const bar = dom.getByTestId("config-actions");
    const body = dom.getByTestId("config-body");
    const save = dom.getByRole("button", { name: "Save" });
    expect(bar.contains(save)).toBe(true);
    expect(bar.contains(dom.getByText("0 unsaved"))).toBe(true);
    expect(body.contains(save)).toBe(false);
    expect(bar.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(dom.queryByRole("contentinfo")).toBeNull();
  });

  it("clips the header subject so secondary text never reaches the actions", () => {
    render(ConfigEditor);
    const subjectSlot = document.querySelector('[data-slot="pane-header-subject"]');
    expect(subjectSlot.className).toMatch(/\boverflow-hidden\b/);
    expect(subjectSlot.className).toMatch(/\bmin-w-0\b/);
    expect(dom.getByTestId("edit-subject").className).toMatch(/\btruncate\b/);
    expect(dom.queryByTestId("config-messages")).toBeNull();
  });

  it("uses the singular for one device", () => {
    configDraft.usage = { panels: { e1004: ["kitchen"] }, unseen: [] };
    render(ConfigEditor);
    expect(dom.getByText("⚠ 1 device uses this panel")).toBeTruthy();
    expect(dom.queryByText(/not polled since byonk started|not known yet/)).toBeNull();
  });

  it("uses the singular for one device with an unknown panel", () => {
    configDraft.usage = { panels: { e1004: ["kitchen"] }, unseen: ["attic"] };
    render(ConfigEditor);
    expect(dom.getByText("1 more device has not polled since byonk started, so its panel is not known yet.")).toBeTruthy();
  });

  // Fix round 1, finding 3: below 22rem (the header is a size container)
  // Discard and Save go icon-only and Push shortens, so the subject keeps
  // room. jsdom has no container queries, so this pins the class contract;
  // the accessible names stay the full labels in both forms.
  it("compacts the header actions by container query, keeping their full names", () => {
    render(ConfigEditor);
    const header = dom.getByTestId("config-actions");
    expect(header.className).toMatch(/@container/);
    for (const name of ["Discard", "Save"]) {
      const btn = dom.getByRole("button", { name });
      expect(btn.querySelector("svg").getAttribute("class")).toMatch(/hidden @max-\[22rem\]:block/);
      expect(btn.querySelector("span").className).toMatch(/@max-\[22rem\]:hidden/);
    }
    const push = dom.getByRole("button", { name: "Push to panel" });
    const [long, short] = push.querySelectorAll("span");
    expect(long.textContent).toBe("Push to panel");
    expect(long.className).toMatch(/@max-\[22rem\]:hidden/);
    expect(short.textContent).toBe("Push");
    expect(short.className).toMatch(/hidden @max-\[22rem\]:inline/);
  });

  it("an edit marks one unsaved change and enables Push and Save", async () => {
    render(ConfigEditor);
    expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(true);
    const strength = document.querySelector("#cfg-panel-strength");
    strength.value = "0.5";
    await fireEvent.input(strength);
    expect(dom.getByText("1 unsaved")).toBeTruthy();
    expect(dom.getByRole("button", { name: "Push to panel" }).disabled).toBe(false);
    expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(false);
  });

  it("Push is disabled while nothing pushable is set (an empty push would store {} as a live override)", async () => {
    const push = vi.spyOn(api, "pushPanelOverride").mockResolvedValue({});
    render(ConfigEditor);
    expect(dom.getByRole("button", { name: "Push to panel" }).disabled).toBe(true);
    // A dirty REMOVAL leaves nothing to push either.
    const ca = document.querySelector("#cfg-panel-colors_actual");
    ca.value = "";
    await fireEvent.input(ca);
    expect(dom.getByText("1 unsaved")).toBeTruthy();
    expect(dom.getByRole("button", { name: "Push to panel" }).disabled).toBe(true);
    await fireEvent.click(dom.getByRole("button", { name: "Push to panel" }));
    expect(push).not.toHaveBeenCalled();
  });

  it("shows a ladder per setting from the shared store", () => {
    resolution.record({ strength: { effective: 1, from: "fallback", order: ["fallback"], rungs: { fallback: { value: 1, status: "winner" } } } }, "panel:e1004");
    render(ConfigEditor);
    expect(document.querySelector('[data-testid="ladder-strength"] [data-rung="fallback"]').dataset.status).toBe("winner");
  });

  it("a ladder ignores a payload that belongs to another subject", () => {
    resolution.record({ strength: { effective: 1, from: "fallback", order: [], rungs: {} } }, "screen:local/a");
    render(ConfigEditor);
    expect(document.querySelector('[data-testid="ladder-strength"]').textContent).toContain("No render yet.");
  });

  it("select another subject after a recorded payload: the old values are gone", async () => {
    resolution.record({ strength: { effective: 0.77, from: "panel", order: ["panel"],
      rungs: { panel: { value: 0.77, status: "winner" } } } }, "panel:e1004");
    screens.meta.panels.push({ id: "x", name: "TRMNL X", colors: "#000000", colors_actual: null, dither: null });
    render(ConfigEditor);
    expect(document.querySelector('[data-testid="ladder-strength"]').textContent).toContain("0.77");
    subject.select("panel", "x");
    await waitFor(() => {
      const t = document.querySelector('[data-testid="ladder-strength"]').textContent;
      expect(t).not.toContain("0.77");
      expect(t).toContain("No render yet.");
    });
  });

  it("states a live push plainly, with the TTL copy", async () => {
    configDraft.live = { panels: [{ key: "e1004", values: { strength: 0.5 }, expires_in_secs: 1700 }], devices: [], tuning_sessions: [] };
    render(ConfigEditor);
    expect(dom.getByText("◉ Live on reTerminal E1004 — the real panel is showing this draft.")).toBeTruthy();
    expect(dom.getByText("It reverts on its own within 30 minutes, or on Discard.")).toBeTruthy();
  });

  it("a usage-load failure lands on configDraft.error, not screens.error", async () => {
    screens.error = null;
    vi.spyOn(api, "panelUsage").mockRejectedValue(new Error("boom"));
    await configDraft.loadUsage();
    expect(configDraft.error).toEqual({ op: "panel usage", message: "boom" });
    expect(screens.error).toBeNull();
  });
});

describe("ConfigEditor, device schema", () => {
  beforeEach(() => subject.select("device", "kitchen"));

  it("carries the plain fields above the cascade, and both rungs per setting", () => {
    render(ConfigEditor);
    for (const f of ["screen", "name", "params", "temperature_profile", "maximum_compatibility", "min_png_bytes"]) {
      expect(document.querySelector(`#cfg-plain-${f}`), f).toBeTruthy();
    }
    expect(document.querySelector("#cfg-device_base-dither")).toBeTruthy();
    expect(document.querySelector("#cfg-device_final-dither")).toBeTruthy();
    expect(document.querySelector("#cfg-device_final-refresh")).toBeTruthy();
  });

  it("says which dirty fields a Push cannot carry", async () => {
    render(ConfigEditor);
    const r = document.querySelector("#cfg-device_base-refresh");
    r.value = "60";
    await fireEvent.input(r);
    const note = dom.getByText(/applies on Save only: refresh/);
    // A fixed row under the header, never inside the scrolling body.
    expect(dom.getByTestId("config-messages").contains(note)).toBe(true);
    expect(dom.getByTestId("config-body").contains(note)).toBe(false);
    expect(dom.getByRole("button", { name: "Push to device" }).disabled).toBe(true);
  });

  it("refuses an emptied screen: Save is disabled and says why, and nothing is sent", async () => {
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({});
    render(ConfigEditor);
    expect(dom.queryByText(/needs a screen/)).toBeNull();
    configDraft.setPlain("device", "kitchen", "screen", "");
    const name = document.querySelector("#cfg-plain-name");
    name.value = "Kitchen 2";
    await fireEvent.input(name);
    expect(dom.getByText(/needs a screen/)).toBeTruthy();
    const btn = dom.getByRole("button", { name: "Save" });
    expect(btn.disabled).toBe(true);
    await fireEvent.click(btn);
    expect(save).not.toHaveBeenCalled();
  });

  it("sends maximum_compatibility as a boolean, never a string", async () => {
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({});
    vi.spyOn(api, "listRepos").mockResolvedValue([]);
    vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [], devices: [], panels: [], dither_algorithms: [] });
    render(ConfigEditor);
    const sel = document.querySelector("#cfg-plain-maximum_compatibility");
    sel.value = "true";
    await fireEvent.change(sel);
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][1].maximum_compatibility).toBe(true);
  });

  it("sends false as false", async () => {
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({});
    vi.spyOn(api, "listRepos").mockResolvedValue([]);
    vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [], devices: [], panels: [], dither_algorithms: [] });
    render(ConfigEditor);
    const sel = document.querySelector("#cfg-plain-maximum_compatibility");
    sel.value = "false";
    await fireEvent.change(sel);
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][1].maximum_compatibility).toBe(false);
  });

  // A half-typed number ("-" on the way to "-0.5", "1e") is no number: the
  // browser reports it as `validity.badInput` with an empty `value`, which
  // must not read as "remove this setting".
  async function typeBad(sel, raw) {
    const el = document.querySelector(sel);
    Object.defineProperty(el, "validity", { configurable: true, value: { badInput: true } });
    el.value = raw;
    await fireEvent.input(el);
    return el;
  }

  it("an unparsable number disables Save and is never sent as a removal", async () => {
    screens.meta.devices[0].base = { strength: 0.5 };
    screens.meta.devices[0].min_png_bytes = 200000;
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({});
    render(ConfigEditor);
    const name = document.querySelector("#cfg-plain-name");
    name.value = "Kitchen 2";
    await fireEvent.input(name);
    const btn = dom.getByRole("button", { name: "Save" });
    expect(btn.disabled).toBe(false);

    for (const sel of ["#cfg-device_base-strength", "#cfg-plain-min_png_bytes"]) {
      const el = await typeBad(sel, "-");
      expect(dom.getByRole("alert").textContent).toMatch(/not a number/);
      expect(dom.getByTestId("config-messages").contains(dom.getByRole("alert"))).toBe(true);
      expect(dom.getByTestId("config-body").contains(dom.getByRole("alert"))).toBe(false);
      expect(btn.disabled).toBe(true);
      await fireEvent.click(btn);
      expect(save).not.toHaveBeenCalled();
      // Finishing the number makes it valid again.
      Object.defineProperty(el, "validity", { configurable: true, value: { badInput: false } });
      el.value = sel.includes("strength") ? "0.25" : "300000";
      await fireEvent.input(el);
      expect(dom.queryByRole("alert")).toBeNull();
      expect(btn.disabled).toBe(false);
    }
    // The raw text reaches the store too (the jsdom path): still no clear.
    configDraft.set("device", "kitchen", "device_final", "strength", "-");
    expect(configDraft.saveDeviceBody("kitchen").clear).toBeUndefined();
  });
});

describe("ConfigEditor Details block (U7)", () => {
  const details = () => dom.getByRole("region", { name: "Details" });

  describe("panel", () => {
    beforeEach(() => subject.select("panel", "e1004"));

    it("is the first block of the body and shows the panel's identity, match included", () => {
      render(ConfigEditor);
      const body = dom.getByTestId("config-body");
      const first = body.querySelector("section");
      expect(first).toBe(details());
      const v = (f) => document.querySelector(`#cfg-plain-${f}`).value;
      expect(v("id")).toBe("e1004");
      expect(v("name")).toBe("reTerminal E1004");
      expect(v("match")).toBe("reterminal_e1004");
      expect(v("width")).toBe("1600");
      expect(v("height")).toBe("1200");
      expect(v("colors")).toBe("#000000,#FFFFFF");
      for (const label of ["Id", "Name", "Match", "Width", "Height", "Colors"]) {
        expect(within(details()).getByLabelText(label)).toBeTruthy();
      }
    });

    it("an identity edit counts as unsaved and does not render", async () => {
      render(ConfigEditor);
      const rev = configDraft.revision;
      const w = document.querySelector("#cfg-plain-width");
      w.value = "800";
      await fireEvent.input(w);
      expect(dom.getByText("1 unsaved")).toBeTruthy();
      expect(configDraft.revision).toBe(rev);
      expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(false);
      // Push carries no identity: with nothing else dirty there is nothing to push.
      expect(dom.getByRole("button", { name: "Push to panel" }).disabled).toBe(true);
    });

    it("Save sends the identity with the rung", async () => {
      const save = vi.spyOn(api, "savePanel").mockResolvedValue({ id: "e1004" });
      vi.spyOn(screens, "refresh").mockResolvedValue();
      render(ConfigEditor);
      const m = document.querySelector("#cfg-plain-match");
      m.value = "other_board";
      await fireEvent.input(m);
      await fireEvent.click(dom.getByRole("button", { name: "Save" }));
      await waitFor(() => expect(save).toHaveBeenCalled());
      expect(save.mock.calls[0][1]).toMatchObject({
        id: "e1004", name: "reTerminal E1004", match: "other_board", width: 1600, height: 1200, colors: "#000000,#FFFFFF" });
    });

    it("a 409 keeps the draft and shows the server message in the message row", async () => {
      vi.spyOn(api, "savePanel").mockRejectedValue(new Error("409 Conflict: panel `og` already exists"));
      render(ConfigEditor);
      const idf = document.querySelector("#cfg-plain-id");
      idf.value = "og";
      await fireEvent.input(idf);
      await fireEvent.click(dom.getByRole("button", { name: "Save" }));
      const msg = await dom.findByText(/panel `og` already exists/);
      expect(dom.getByTestId("config-messages").contains(msg)).toBe(true);
      expect(document.querySelector("#cfg-plain-id").value).toBe("og");
      expect(dom.getByText("1 unsaved")).toBeTruthy();
      expect(dom.getAllByText(/already exists/)).toHaveLength(1);
      // The next edit clears the stale message.
      idf.value = "og2";
      await fireEvent.input(idf);
      expect(dom.queryByText(/already exists/)).toBeNull();
    });

    it("an emptied Name disables Save and says why", async () => {
      render(ConfigEditor);
      const n = document.querySelector("#cfg-plain-name");
      n.value = "";
      await fireEvent.input(n);
      expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(true);
      expect(dom.getByText(/needs a name/i)).toBeTruthy();
    });
  });

  describe("device", () => {
    beforeEach(() => subject.select("device", "kitchen"));

    it("holds Key, Name, Screen and Panel; Screen appears once", async () => {
      vi.spyOn(api, "listPendingDevices").mockResolvedValue([]);
      render(ConfigEditor);
      const d = details();
      expect(dom.getByTestId("config-body").querySelector("section")).toBe(d);
      for (const label of ["Key", "Name", "Screen", "Panel"]) expect(within(d).getByLabelText(label)).toBeTruthy();
      expect(dom.getAllByLabelText("Screen")).toHaveLength(1);
      expect(document.querySelector("#cfg-plain-key").value).toBe("kitchen");
      const panel = within(d).getByLabelText("Panel");
      expect([...panel.options].map((o) => [o.value, o.textContent])).toEqual([
        ["", "auto (from the device's Board header)"], ["e1004", "reTerminal E1004"]]);
      expect(panel.value).toBe("e1004");
    });

    it("offers the pending devices' MACs as Key suggestions", async () => {
      const pending = vi.spyOn(api, "listPendingDevices").mockResolvedValue([
        { mac: "11:22:33:44:55:66", registration_code: "ABC", model: "og", firmware_version: "1", last_seen: "x" }]);
      render(ConfigEditor);
      const key = document.querySelector("#cfg-plain-key");
      const list = document.getElementById(key.getAttribute("list"));
      await waitFor(() => expect([...list.querySelectorAll("option")].map((o) => o.value)).toEqual(["11:22:33:44:55:66"]));
      expect(pending).toHaveBeenCalledTimes(1);
    });

    it("a failed pending fetch just gives no suggestions", async () => {
      vi.spyOn(api, "listPendingDevices").mockRejectedValue(new Error("500"));
      render(ConfigEditor);
      await new Promise((r) => setTimeout(r, 0));
      const key = document.querySelector("#cfg-plain-key");
      expect(document.getElementById(key.getAttribute("list")).querySelectorAll("option")).toHaveLength(0);
      expect(configDraft.error).toBe(null);
    });

    it("Save sends key and panel in the PATCH and re-selects the renamed device", async () => {
      vi.spyOn(api, "listPendingDevices").mockResolvedValue([]);
      const save = vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "pantry", screen: "local/a" });
      vi.spyOn(screens, "refresh").mockImplementation(async ({ applied } = {}) => {
        screens.meta = { ...screens.meta, devices: [{ ...screens.meta.devices[0], id: "pantry", panel: null }] };
        applied?.();
      });
      render(ConfigEditor);
      const key = document.querySelector("#cfg-plain-key");
      key.value = "pantry";
      await fireEvent.input(key);
      const panel = document.querySelector("#cfg-plain-panel");
      panel.value = "";
      await fireEvent.change(panel);
      await fireEvent.click(dom.getByRole("button", { name: "Save" }));
      await waitFor(() => expect(save).toHaveBeenCalledWith("kitchen", { key: "pantry", clear: ["panel"] }));
      await waitFor(() => expect(subject.key).toBe("device:pantry"));
      expect(dom.getByText("0 unsaved")).toBeTruthy();
    });
  });
});
