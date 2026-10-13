import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, screen as dom } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { screenRepos } from "../src/lib/state/screenRepos.svelte.js";
import { details } from "../src/lib/state/details.svelte.js";

// U14: the Details block hides behind a "…" in the Edit header; the active
// file tab is marked from aria-selected.
const cmMock = { setDoc: vi.fn(), setDiagnostics: vi.fn(), setTheme: vi.fn(), destroy: vi.fn(), view: {} };
vi.mock("../src/lib/codemirror.js", () => ({ createEditor: vi.fn(() => cmMock) }));
import EditorPane from "../src/lib/components/EditorPane.svelte";
import ConfigEditor from "../src/lib/components/ConfigEditor.svelte";
import ScreenRepoEditor from "../src/lib/components/ScreenRepoEditor.svelte";

const REPOS = [{ handle: "local", writable: true }, { handle: "examples", writable: false }];
const regionOf = () => dom.queryByRole("region", { name: "Details" });
const toggle = () => dom.queryByRole("button", { name: "Details" });

beforeEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  editor.reset();
  editor.files = [];
  subject.reset();
  configDraft.reset();
  resolution.reset();
  details.reset();
  try { localStorage.clear(); } catch {}
  vi.spyOn(api, "listRepos").mockResolvedValue(REPOS);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }], devices: [], panels: [], dither_algorithms: [] });
  vi.spyOn(api, "listFiles").mockResolvedValue([
    { name: "meta.yaml", size: 1, text: true }, { name: "script.lua", size: 1, text: true },
    { name: "screen.svg", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "--", etag: '"1"' });
  vi.spyOn(api, "validateScreen").mockResolvedValue({ ok: true, issues: [] });
  vi.spyOn(api, "checkFile").mockResolvedValue({ ok: true, issues: [] });
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listPendingDevices").mockResolvedValue([]);
  await screens.refresh();
  screens.meta = {
    panels: [{ id: "e1004", name: "E1004", match: "m", width: 1600, height: 1200, colors: "#000000,#FFFFFF" },
             { id: "new_panel", name: "New panel", match: null, width: 800, height: 480, colors: "#000000,#FFFFFF" }],
    devices: [{ id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: {}, base: {}, final: null },
              { id: "new-device", screen: "local/a", panel: null, name: "New device", params: {}, base: {}, final: null }],
    dither_algorithms: [],
  };
  configDraft.usage = { panels: {}, unseen: [] };
});
afterEach(() => cleanup());

async function openScreen() {
  subject.select("screen", "local/a");
  await editor.loadScreen("local/a", { readOnly: false });
  render(EditorPane);
  flushSync();
}

describe("screen Details", () => {
  it("is collapsed by default and the … button toggles it", async () => {
    await openScreen();
    expect(regionOf()).toBeNull();
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    expect(toggle().getAttribute("aria-controls")).toBe("screen-details");
    await fireEvent.click(toggle());
    expect(regionOf()).toBeTruthy();
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(regionOf().id).toBe("screen-details");
    await fireEvent.click(toggle());
    expect(regionOf()).toBeNull();
  });

  it("opens by itself while a Path edit is pending and cannot be hidden then", async () => {
    await openScreen();
    editor.setTarget("path", "moved");
    flushSync();
    expect(regionOf()).toBeTruthy();
    await fireEvent.click(toggle());
    expect(regionOf()).toBeTruthy();
  });

  it("opens when a move was refused", async () => {
    await openScreen();
    editor.moveFailure = "path taken";
    flushSync();
    expect(regionOf()).toBeTruthy();
  });

  it("remembers the choice per kind", async () => {
    await openScreen();
    await fireEvent.click(toggle());
    cleanup();
    await openScreen();
    expect(regionOf()).toBeTruthy();
  });
});

describe("file tabs", () => {
  it("the active tab carries aria-selected and the active classes; switching moves them", async () => {
    await openScreen();
    const tabs = () => dom.getAllByRole("tab");
    await fireEvent.click(dom.getByRole("tab", { name: "script.lua" }));
    await waitFor(() => expect(dom.getByRole("tab", { name: "script.lua" }).getAttribute("aria-selected")).toBe("true"));
    const marked = (t) => t.parentElement.className.includes("has-aria-selected:border-b-primary")
      && t.parentElement.className.includes("has-aria-selected:bg-background");
    expect(tabs().filter((t) => t.getAttribute("aria-selected") === "true").map((t) => t.textContent.trim())).toEqual(["script.lua"]);
    expect(tabs().every(marked)).toBe(true);
    // Unselected tabs are muted and carry the transparent underline.
    for (const t of tabs()) expect(t.parentElement.className).toMatch(/text-muted-foreground/);
    await fireEvent.click(dom.getByRole("tab", { name: "screen.svg" }));
    await waitFor(() => expect(dom.getByRole("tab", { name: "screen.svg" }).getAttribute("aria-selected")).toBe("true"));
    expect(dom.getByRole("tab", { name: "script.lua" }).getAttribute("aria-selected")).toBe("false");
    expect(dom.getByRole("tab", { name: "screen.svg" }).className).toMatch(/aria-selected:font-semibold/);
  });
});

describe("panel and device Details", () => {
  it("are collapsed for an existing panel; … toggles; aria-expanded follows", async () => {
    subject.select("panel", "e1004");
    render(ConfigEditor);
    expect(regionOf()).toBeNull();
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    await fireEvent.click(toggle());
    expect(regionOf()).toBeTruthy();
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
  });

  it("are collapsed for an existing device", () => {
    subject.select("device", "kitchen");
    render(ConfigEditor);
    expect(regionOf()).toBeNull();
  });

  it("open for a brand-new placeholder panel and device", () => {
    subject.select("panel", "new_panel");
    render(ConfigEditor);
    expect(regionOf()).toBeTruthy();
    cleanup();
    subject.select("device", "new-device");
    render(ConfigEditor);
    expect(regionOf()).toBeTruthy();
  });

  it("open while a Details field is dirty", async () => {
    subject.select("panel", "e1004");
    render(ConfigEditor);
    expect(regionOf()).toBeNull();
    configDraft.setPlain("panel", "e1004", "name", "Renamed");
    flushSync();
    expect(regionOf()).toBeTruthy();
  });

  it("a dirty non-Details setting does not open it", () => {
    subject.select("panel", "e1004");
    render(ConfigEditor);
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    flushSync();
    expect(regionOf()).toBeNull();
  });

  it("open after a Save failure", async () => {
    subject.select("panel", "e1004");
    render(ConfigEditor);
    configDraft.saveFailure = { key: "panel:e1004", message: "409 Conflict" };
    flushSync();
    expect(regionOf()).toBeTruthy();
  });

  it("toggling Details renders nothing", async () => {
    subject.select("panel", "e1004");
    render(ConfigEditor);
    const rev = configDraft.revision;
    await fireEvent.click(toggle());
    await fireEvent.click(toggle());
    expect(configDraft.revision).toBe(rev);
  });
});

describe("screen repo Details", () => {
  it("are always open and there is no … button", () => {
    screenRepos.list = [{ handle: "examples", kind: "builtin", status: "ok", screen_count: 1, managed: true }];
    subject.select("screen-repo", "examples");
    render(ScreenRepoEditor);
    expect(regionOf()).toBeTruthy();
    expect(toggle()).toBeNull();
  });
});
