// U11: the Library's "Screen repos" section and the Edit pane for a repo.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
import { tick } from "svelte";
import LibraryPane from "../src/lib/components/LibraryPane.svelte";
import EditPane from "../src/lib/components/EditPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { screenRepos, NEW_REPO } from "../src/lib/state/screenRepos.svelte.js";
import * as api from "../src/lib/api.js";

const repo = (o) => ({
  kind: "git", repo: null, pin: null, has_token: false, status: "ready", pin_kind: null,
  resolved_sha: null, last_fetched: null, error: null, screen_count: 0, writable: false, managed: false, ...o,
});
const REPOS = [
  repo({ handle: "byonk-builtin", kind: "builtin", managed: true, screen_count: 5 }),
  repo({ handle: "local", kind: "local", managed: true, writable: true }),
  repo({ handle: "weather", repo: "https://example.com/weather.git", pin: "main", has_token: true,
         resolved_sha: "0123456789abcdef0123456789abcdef01234567", last_fetched: "2026-10-12T08:00:00Z",
         screen_count: 2 }),
  repo({ handle: "broken", repo: "https://example.com/broken.git", status: "error",
         error: "authentication failed" }),
];

let listSpy;
function serve(repos = REPOS, addon_mode = false) {
  listSpy = vi.spyOn(api, "listScreenRepos").mockResolvedValue({ addon_mode, repos });
}

beforeEach(async () => {
  localStorage.clear();
  cleanup();
  vi.restoreAllMocks();
  subject.reset();
  configDraft.reset();
  screenRepos.reset();
  screens.error = null;
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({ screens: [{ name: "local/a" }], devices: [], panels: [], dither_algorithms: [] });
  serve();
  await screens.refresh();
  await screenRepos.refresh();
});
afterEach(() => cleanup());

const section = () => screen.getByRole("region", { name: "Screen repos" });
const field = (id) => document.querySelector(`#${id}`);
async function type(id, value) {
  const el = field(id);
  el.value = value;
  await fireEvent.input(el);
}

describe("Library: Screen repos section", () => {
  it("is the fourth section and lists every repo with its source and status", () => {
    render(LibraryPane);
    const names = [...document.querySelectorAll("section[aria-label]")].map((s) => s.getAttribute("aria-label"));
    expect(names).toEqual(["Screens", "Panels", "Devices", "Screen repos"]);
    const rows = within(section()).getAllByRole("button").filter((b) => b.dataset.repo);
    expect(rows.map((b) => b.dataset.repo)).toEqual(["byonk-builtin", "local", "weather", "broken"]);
    expect(within(rows[0]).getByText("built in")).toBeTruthy();
    expect(within(rows[1]).getByText("local folder")).toBeTruthy();
    expect(within(rows[2]).getByText("https://example.com/weather.git")).toBeTruthy();
    const mark = within(rows[3]).getByTestId("repo-status");
    expect(mark.getAttribute("title")).toBe("authentication failed");
    expect(mark.getAttribute("class")).toMatch(/text-destructive/);
  });

  it("selects a repo as the subject", async () => {
    render(LibraryPane);
    await fireEvent.click(within(section()).getByRole("button", { name: /^weather/ }));
    expect(subject.key).toBe("screen-repo:weather");
    expect(within(section()).getByRole("button", { name: /^weather/ }).getAttribute("aria-current")).toBe("true");
  });

  it('"+" opens an unsaved new-repo draft, not a list entry', async () => {
    const add = vi.spyOn(api, "addScreenRepo");
    render(LibraryPane);
    await fireEvent.click(within(section()).getByRole("button", { name: "Add screen repo" }));
    expect(subject.key).toBe(`screen-repo:${NEW_REPO}`);
    expect(screenRepos.draft(NEW_REPO)).toMatchObject({ handle: "new-repo", repo: "", pin: "", token: "" });
    expect(add).not.toHaveBeenCalled();
    expect(within(section()).queryByRole("button", { name: /^new-repo/ })).toBeNull();
  });

  it("names Delete for a repo and deletes it after the inline confirmation", async () => {
    const del = vi.spyOn(api, "deleteScreenRepo").mockResolvedValue({ deleted: "weather" });
    const refresh = vi.spyOn(screens, "refresh");
    subject.select("screen-repo", "weather");
    render(LibraryPane);
    await fireEvent.pointerDown(screen.getByRole("button", { name: "More actions" }), { button: 0, pointerType: "mouse" });
    const item = await screen.findByRole("menuitem", { name: "Delete" });
    expect(item.textContent).toContain("Delete screen repo");
    await fireEvent.keyDown(item, { key: "Escape" });
    cleanup();
    render(LibraryPane);
    const button = within(screen.getByTestId("library-actions")).getByRole("button", { name: "Delete" });
    await fireEvent.click(button);
    const form = await screen.findByRole("form", { name: "Confirm screen repo deletion" });
    expect(form.textContent).toContain("weather");
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("weather"));
    await waitFor(() => expect(subject.kind).toBe(null));
    expect(refresh).toHaveBeenCalled();
    expect(listSpy.mock.calls.length).toBeGreaterThan(1);
  });

  it("shows the server's 409 when a repo in use cannot be deleted", async () => {
    vi.spyOn(api, "deleteScreenRepo").mockRejectedValue(
      new Error("409 Conflict: screen repo `weather` is referenced by device(s): kitchen"));
    subject.select("screen-repo", "weather");
    render(LibraryPane);
    await fireEvent.click(within(screen.getByTestId("library-actions")).getByRole("button", { name: "Delete" }));
    const form = await screen.findByRole("form", { name: "Confirm screen repo deletion" });
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screens.error?.message).toMatch(/referenced by device\(s\): kitchen/));
    expect(subject.key).toBe("screen-repo:weather");
  });

  it("cannot delete a repo byonk manages", () => {
    subject.select("screen-repo", "local");
    render(LibraryPane);
    expect(within(screen.getByTestId("library-actions")).getByRole("button", { name: "Delete" })).toBeDisabled();
  });

  it("in add-on mode hides '+' and says where repos are managed", async () => {
    serve(REPOS, true);
    await screenRepos.refresh();
    subject.select("screen-repo", "weather");
    render(LibraryPane);
    expect(within(section()).queryByRole("button", { name: "Add screen repo" })).toBeNull();
    expect(within(section()).getByText("Managed in the byonk add-on configuration in Home Assistant.")).toBeTruthy();
    expect(within(screen.getByTestId("library-actions")).getByRole("button", { name: "Delete" })).toBeDisabled();
  });
});

describe("Edit pane: a screen repo", () => {
  it("a new draft: Save POSTs it and selects the new handle", async () => {
    const add = vi.spyOn(api, "addScreenRepo").mockImplementation(async (body) => {
      serve([...REPOS, repo({ handle: body.handle, repo: body.repo, status: "ready" })]);
      return repo({ handle: body.handle, repo: body.repo, status: "fetching" });
    });
    const refresh = vi.spyOn(screens, "refresh");
    subject.select("screen-repo", screenRepos.startNew());
    render(EditPane);
    expect(field("repo-handle").value).toBe("new-repo");
    await type("repo-handle", "clocks");
    await type("repo-url", "https://example.com/clocks.git");
    await type("repo-pin", "v1");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(add).toHaveBeenCalledWith({ handle: "clocks", repo: "https://example.com/clocks.git", pin: "v1" }));
    await waitFor(() => expect(subject.key).toBe("screen-repo:clocks"));
    expect(screenRepos.draft(NEW_REPO)).toBe(null);
    expect(refresh).toHaveBeenCalled();
    expect(screenRepos.find("clocks")).toBeTruthy();
  });

  it("a new draft with an invalid handle or no URL cannot be saved", async () => {
    subject.select("screen-repo", screenRepos.startNew());
    render(EditPane);
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByText(/needs a repo URL/)).toBeTruthy();
    await type("repo-url", "https://example.com/x.git");
    await type("repo-handle", "Bad Handle");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByText(/lowercase letters, digits/)).toBeTruthy();
    await type("repo-handle", "ok");
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  });

  it("shows the server's refusal in the message row and keeps the draft", async () => {
    vi.spyOn(api, "addScreenRepo").mockRejectedValue(new Error("400 Bad Request: repo URL needs a scheme"));
    subject.select("screen-repo", screenRepos.startNew());
    render(EditPane);
    await type("repo-url", "github.com/x/y");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    const row = await screen.findByTestId("repo-messages");
    expect(row.textContent).toContain("Save failed: 400 Bad Request: repo URL needs a scheme");
    expect(subject.key).toBe(`screen-repo:${NEW_REPO}`);
    expect(screenRepos.draft(NEW_REPO).repo).toBe("github.com/x/y");
  });

  it("an existing repo: the handle is read-only text and the token is never prefilled", () => {
    subject.select("screen-repo", "weather");
    render(EditPane);
    expect(field("repo-handle")).toBeNull();
    expect(within(screen.getByRole("region", { name: "Details" })).getByText("weather")).toBeTruthy();
    expect(field("repo-url").value).toBe("https://example.com/weather.git");
    expect(field("repo-pin").value).toBe("main");
    const token = field("repo-token");
    expect(token.type).toBe("password");
    expect(token.value).toBe("");
    expect(token.placeholder).toBe("set");
  });

  it("Save PATCHes only what changed; an empty token is left out", async () => {
    const patch = vi.spyOn(api, "patchScreenRepo").mockResolvedValue(repo({ handle: "weather" }));
    subject.select("screen-repo", "weather");
    render(EditPane);
    await type("repo-pin", "v2");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(patch).toHaveBeenCalledWith("weather", { pin: "v2" }));
  });

  it("a typed token is sent; Clear token sends the clear", async () => {
    const patch = vi.spyOn(api, "patchScreenRepo").mockResolvedValue(repo({ handle: "weather" }));
    subject.select("screen-repo", "weather");
    render(EditPane);
    await type("repo-token", "s3cret");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(patch).toHaveBeenLastCalledWith("weather", { token: "s3cret" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear token" }));
    expect(field("repo-token").placeholder).toBe("");
    // Enabled once the first Save has finished.
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(patch).toHaveBeenLastCalledWith("weather", { token: "" }));
  });

  it("shows the status block and Update now refreshes the repo and the screens", async () => {
    const upd = vi.spyOn(api, "updateScreenRepo").mockResolvedValue(repo({ handle: "weather", status: "fetching" }));
    const refresh = vi.spyOn(screens, "refresh");
    subject.select("screen-repo", "weather");
    render(EditPane);
    const status = screen.getByRole("region", { name: "Status" });
    expect(status.textContent).toContain("0123456789abcdef0123456789abcdef01234567");
    expect(status.textContent).toMatch(/2026/);
    const calls = listSpy.mock.calls.length;
    await fireEvent.click(screen.getByRole("button", { name: "Update now" }));
    await waitFor(() => expect(upd).toHaveBeenCalledWith("weather"));
    await waitFor(() => expect(listSpy.mock.calls.length).toBeGreaterThan(calls));
    await waitFor(() => expect(refresh).toHaveBeenCalled());
  });

  it("shows the last error in the status block", () => {
    subject.select("screen-repo", "broken");
    render(EditPane);
    expect(screen.getByRole("region", { name: "Status" }).textContent).toContain("authentication failed");
  });

  it("a repo byonk manages is read-only: no inputs, no Save, no Update now", () => {
    subject.select("screen-repo", "local");
    render(EditPane);
    expect(screen.getByRole("region", { name: "Details" }).querySelector("input")).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Update now" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Clear token" })).toBeNull();
  });

  it("in add-on mode a git repo is read-only too", async () => {
    serve(REPOS, true);
    await screenRepos.refresh();
    subject.select("screen-repo", "weather");
    render(EditPane);
    expect(screen.getByRole("region", { name: "Details" }).querySelector("input")).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  });

  it("Discard on a new draft drops it and clears the subject", async () => {
    subject.select("screen-repo", screenRepos.startNew());
    render(EditPane);
    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await tick();
    expect(subject.kind).toBe(null);
    expect(screenRepos.draft(NEW_REPO)).toBe(null);
  });
});
