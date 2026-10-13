import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * U9 journey: a local screen's Path is edited in the Edit pane's Details
 * block and saved -- across the real AppShell (Library, Edit, Preview).
 */
const payload = (refs) => ({ screens: refs.map((name) => ({ name })), devices: [], panels: [],
  dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1, noise_scale: 0 } }] });

function stubServer() {
  const calls = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init = {}) => {
    calls.push([init.method ?? "GET", url, init.body ? JSON.parse(init.body) : undefined]);
    return {
      ok: true, status: 200,
      headers: { get: () => null },
      blob: async () => new Blob(["png"], { type: "image/png" }),
      text: async () => "", json: async () => ({}),
    };
  });
  return calls;
}

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset(); theme.reset(); auth.reset(); editor.reset(); editor.files = [];
  simulate.reset(); simulate.error = null;
  subject.reset();
  resolution.reset();
  configDraft.reset();
  screens.repos = []; screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  screens.error = null;
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue(payload(["local/a", "local/other"]));
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "--", etag: '"1"' });
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  // CodeMirror's setDoc counts as a doc change, so loading a screen schedules a lint check.
  vi.spyOn(api, "checkFile").mockResolvedValue({ ok: true, issues: [] });
});
afterEach(() => cleanup());

const renderBodies = (calls, from) =>
  calls.slice(from).filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
const settle = () => new Promise((r) => setTimeout(r, 400)); // past the 300 ms draft debounce

describe("Moving a screen", () => {
  it("a local screen's Path is edited in Details; Save shows it selected under its new ref with one render", async () => {
    const calls = stubServer();
    const move = vi.spyOn(api, "moveScreen").mockImplementation(async (_ref, handle, name) => {
      api.listScreens.mockResolvedValue(payload([`${handle}/${name}`, "local/other"]));
      return { ref: `${handle}/${name}`, rewritten: [] };
    });

    render(AppShell);
    await waitFor(() => expect(screens.list).toHaveLength(2));
    await tick();
    const library = dom.getByRole("region", { name: "Library" });

    const before = calls.length;
    await fireEvent.click(within(library).getByRole("button", { name: "a" }));
    await waitFor(() => expect(renderBodies(calls, before).length).toBeGreaterThan(0));
    await settle();
    expect(renderBodies(calls, before)).toHaveLength(1);

    // Typing the path never renders.
    const typed = calls.length;
    const path = await waitFor(() => {
      const el = within(dom.getByRole("region", { name: "Details" })).getByLabelText("Path");
      return el;
    });
    path.value = "clock/wall";
    await fireEvent.input(path);
    await settle();
    expect(renderBodies(calls, typed)).toHaveLength(0);

    const saved = calls.length;
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/clock/wall"));
    expect(move).toHaveBeenCalledWith("local/a", "local", "clock/wall");
    await waitFor(() => expect(renderBodies(calls, saved).length).toBeGreaterThan(0));
    await settle();
    expect(renderBodies(calls, saved)).toHaveLength(1);
    expect(renderBodies(calls, saved)[0]).toMatchObject({ screen: "local/clock/wall" });

    const row = await within(library).findByRole("button", { name: "clock/wall" });
    expect(row).toHaveAttribute("aria-current", "true");
    expect(within(library).queryByRole("button", { name: "a" })).toBeNull();
    expect(dom.getByTestId("edit-subject").textContent).toBe("local/clock/wall");
    // The draft is clean: the Details show the new place, nothing unsaved.
    const details = dom.getByRole("region", { name: "Details" });
    await waitFor(() => expect(within(details).getByLabelText("Path").value).toBe("clock/wall"));
    expect(within(details).queryByRole("img", { name: "unsaved changes" })).toBeNull();
    expect(dom.getByRole("button", { name: "Save" })).toBeDisabled();
  });
});
