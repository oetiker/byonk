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
import { screenRepos, NEW_REPO } from "../src/lib/state/screenRepos.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * U11 journey, mocked at the API layer (the journey harness has no server
 * to fetch a `file:///` repo with): "+" opens an unsaved draft, Save adds
 * the repo, its screens appear in the Screens section, and selecting a repo
 * never renders.
 */
const repo = (o) => ({
  kind: "git", repo: null, pin: null, has_token: false, status: "ready", pin_kind: null,
  resolved_sha: null, last_fetched: null, error: null, screen_count: 0, writable: false, managed: false, ...o,
});
const LOCAL = repo({ handle: "local", kind: "local", managed: true, writable: true, screen_count: 1 });

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
  screenRepos.reset();
  screens.repos = []; screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  screens.error = null;
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [{ name: "local/a" }], devices: [], panels: [], dither_algorithms: [] });
  vi.spyOn(api, "listScreenRepos").mockResolvedValue({ addon_mode: false, repos: [LOCAL] });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "--", etag: '"1"' });
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
});
afterEach(() => cleanup());

const renders = (calls, from) => calls.slice(from).filter(([m, u]) => m === "POST" && u === "api/render");
const settle = () => new Promise((r) => setTimeout(r, 400));
async function edit(sel, value) {
  const el = await waitFor(() => {
    const e = document.querySelector(sel);
    if (!e) throw new Error(`no ${sel}`);
    return e;
  });
  el.value = value;
  await fireEvent.input(el);
}

describe("Screen repos journey", () => {
  it("+ opens a draft, Save adds the repo and its screens, selecting a repo renders nothing", async () => {
    const calls = stubServer();
    const add = vi.spyOn(api, "addScreenRepo").mockImplementation(async (body) => {
      api.listScreenRepos.mockResolvedValue({ addon_mode: false, repos: [LOCAL,
        repo({ handle: body.handle, repo: body.repo, screen_count: 1, resolved_sha: "abc123" })] });
      api.listRepos.mockResolvedValue([{ handle: "local", writable: true }, { handle: body.handle, writable: false }]);
      api.listScreens.mockResolvedValue({ screens: [{ name: "local/a" }, { name: `${body.handle}/clock` }],
        devices: [], panels: [], dither_algorithms: [] });
      return repo({ handle: body.handle, repo: body.repo, status: "fetching" });
    });

    render(AppShell);
    await waitFor(() => expect(screenRepos.list).toHaveLength(1));
    await tick();
    const library = dom.getByRole("region", { name: "Library" });
    const reposSection = within(library).getByRole("region", { name: "Screen repos" });

    // A screen renders once ...
    await fireEvent.click(within(library).getByRole("button", { name: "a" }));
    await waitFor(() => expect(renders(calls, 0).length).toBe(1));

    // ... a repo does not render at all, and the Preview is empty.
    const before = calls.length;
    await fireEvent.click(within(reposSection).getByRole("button", { name: /^local/ }));
    await settle();
    expect(renders(calls, before)).toHaveLength(0);
    expect(dom.getByTestId("preview-title").textContent).toBe("nothing to preview");

    // "+" opens the unsaved draft in Edit.
    await fireEvent.click(within(reposSection).getByRole("button", { name: "Add screen repo" }));
    expect(subject.key).toBe(`screen-repo:${NEW_REPO}`);
    await edit("#repo-handle", "clocks");
    await edit("#repo-url", "file:///srv/fixtures/clocks");
    await settle();
    expect(renders(calls, before)).toHaveLength(0);

    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(add).toHaveBeenCalledWith({ handle: "clocks", repo: "file:///srv/fixtures/clocks" }));
    await waitFor(() => expect(subject.key).toBe("screen-repo:clocks"));
    const row = await waitFor(() => within(reposSection).getByRole("button", { name: /^clocks/ }));
    expect(row.getAttribute("aria-current")).toBe("true");
    // The Screens section shows the new repo's screens.
    await waitFor(() => expect(within(library).getByRole("button", { name: "clock" })).toBeTruthy());
    expect(dom.getByRole("region", { name: "Status" }).textContent).toContain("abc123");
    await settle();
    expect(renders(calls, before)).toHaveLength(0);
  });

  it("switching subject away from an unsaved draft discards it", async () => {
    stubServer();
    render(AppShell);
    await waitFor(() => expect(screenRepos.list).toHaveLength(1));
    const library = dom.getByRole("region", { name: "Library" });
    await fireEvent.click(within(library).getByRole("button", { name: "Add screen repo" }));
    await edit("#repo-url", "https://example.com/x.git");
    await fireEvent.click(within(library).getByRole("button", { name: "a" }));
    await tick();
    expect(screenRepos.draft(NEW_REPO)).toBe(null);
  });
});
