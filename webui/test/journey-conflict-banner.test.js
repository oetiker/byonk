import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * The conflict banner as the user meets it: on screen, read, and clicked.
 *
 * `conflict.test.js` is thorough and contains not one DOM assertion. It
 * drives `editor.trySave()`, `editor.onFileChange()`, `editor.resolveReload()`
 * and `editor.resolveOverwrite()` directly and asserts on `editor.conflict` --
 * the store. Even its "end to end" SSE test stops at the store. So the wire
 * from `editor.conflict` to a visible `role="alert"`, and from the banner's
 * two buttons back to the resolvers, has never been exercised: a banner that
 * rendered nothing, named the wrong file, or whose buttons called nothing
 * would leave that whole suite green.
 *
 * That is the same shape as Findings G, I and J. This file owns it.
 */

/** Minimal stand-in for the browser's EventSource, as conflict.test.js uses. */
class FakeEventSource {
  static instances = [];
  constructor(url) {
    this.url = url;
    this.listeners = {};
    this.onopen = null;
    this.onerror = null;
    FakeEventSource.instances.push(this);
  }
  addEventListener(type, fn) {
    (this.listeners[type] ??= []).push(fn);
  }
  close() {}
  emit(type, data) {
    for (const fn of this.listeners[type] ?? []) fn({ data });
  }
}

/**
 * The conflict banner.
 *
 * Not `queryByRole("alert")` alone: StatusStrip's five error surfaces carry
 * `role="alert"` too (Ruling 66), so the role is ambiguous the moment any
 * error is showing -- which is exactly the state the retry tests below put
 * the app into. The banner is the alert that offers a way out.
 */
function banner() {
  const found = dom
    .queryAllByRole("alert")
    .filter((el) => [...el.querySelectorAll("button")].some((b) => b.textContent.trim() === "Reload"));
  expect(found.length).toBeLessThan(2);
  return found[0] ?? null;
}

let realEventSource;

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
  editor.reset();
  editor.files = [];
  subject.reset();
  screens.repos = [];
  screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  realEventSource = globalThis.EventSource;
  globalThis.EventSource = FakeEventSource;
  FakeEventSource.instances = [];
});

afterEach(() => {
  cleanup();
  if (realEventSource === undefined) delete globalThis.EventSource;
  else globalThis.EventSource = realEventSource;
});

/**
 * Mount the shell and leave one dirty buffer open.
 *
 * The tab is opened AFTER the mount, for the reason conflict.test.js records:
 * AppShell's Task-1 effect calls `editor.loadScreen()` on mount, whose
 * `reset()` would wipe a tab opened beforehand.
 */
async function mountWithDirtyTab({ text = "disk", etag = '"1"' } = {}) {
  render(AppShell);
  await tick();
  vi.spyOn(api, "readFile").mockResolvedValue({ text, etag });
  await editor.open("local/a", "script.lua");
  editor.setText("mine");
  await tick();
}

describe("the conflict banner, on screen", () => {
  it("a 412 on save puts a banner on screen naming the file, with both choices", async () => {
    await mountWithDirtyTab();
    expect(banner()).toBeNull();

    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await tick();

    const b = banner();
    expect(b).toBeTruthy();
    expect(b.textContent).toContain("script.lua");
    // The save-triggered wording. It tells the user the change landed while
    // they were working, which is the whole reason the 412 came back.
    expect(b.textContent).toContain("while you were editing");
    expect(dom.getByRole("button", { name: "Reload" })).toBeTruthy();
    expect(dom.getByRole("button", { name: "Overwrite" })).toBeTruthy();
  });

  it("the watcher's own payload raises the same banner, without the save wording", async () => {
    await mountWithDirtyTab();
    const es = FakeEventSource.instances.at(-1);
    expect(es).toBeTruthy();

    // The server sends a bare array of basenames (src/api/dev.rs's
    // handle_events), not {ref, name}.
    es.emit("file-change", JSON.stringify(["script.lua"]));
    await waitFor(() => expect(banner()).toBeTruthy());

    expect(banner().textContent).toContain("script.lua");
    expect(banner().textContent).toContain("changed on disk");
    expect(banner().textContent).not.toContain("while you were editing");
  });

  // Ruling 37: a clean buffer is refreshed silently rather than interrupting
  // the user with a choice between two identical outcomes. The visible half
  // of that ruling -- that NO banner appears -- has never been checked.
  it("a clean buffer is refreshed silently, with no banner at all", async () => {
    render(AppShell);
    await tick();
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    await tick();
    expect(editor.active.dirty).toBe(false);

    api.readFile.mockResolvedValue({ text: "newer on disk", etag: '"2"' });
    FakeEventSource.instances.at(-1).emit("file-change", JSON.stringify(["script.lua"]));

    await waitFor(() => expect(editor.active.text).toBe("newer on disk"));
    expect(banner()).toBeNull();
  });
});

describe("resolving the conflict by clicking the banner", () => {
  it("Reload takes their text, drops mine, and clears the banner", async () => {
    await mountWithDirtyTab();
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await waitFor(() => expect(banner()).toBeTruthy());

    api.readFile.mockResolvedValue({ text: "theirs", etag: '"2"' });
    await fireEvent.click(dom.getByRole("button", { name: "Reload" }));

    await waitFor(() => expect(banner()).toBeNull());
    expect(editor.active.text).toBe("theirs");
    expect(editor.active.etag).toBe('"2"');
    expect(editor.active.dirty).toBe(false);
  });

  it("Overwrite keeps my text, wins with a fresh etag, and clears the banner", async () => {
    await mountWithDirtyTab();
    const write = vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await waitFor(() => expect(banner()).toBeTruthy());

    // The re-read must supply the ETAG only -- taking its text as well would
    // discard the very edit this button exists to preserve.
    api.readFile.mockResolvedValue({ text: "theirs", etag: '"2"' });
    write.mockResolvedValue({ etag: '"3"' });

    await fireEvent.click(dom.getByRole("button", { name: "Overwrite" }));

    await waitFor(() => expect(banner()).toBeNull());
    expect(write).toHaveBeenLastCalledWith("local/a", "script.lua", "mine", '"2"');
    expect(editor.active.text).toBe("mine");
    expect(editor.active.etag).toBe('"3"');
    expect(editor.active.dirty).toBe(false);
  });

  // A resolution that fails must leave the banner up, or the user is left
  // with a dirty buffer, no warning, and no way back to the choice.
  it("a failing Reload leaves the banner up so the choice can be retried", async () => {
    await mountWithDirtyTab();
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await waitFor(() => expect(banner()).toBeTruthy());

    api.readFile.mockRejectedValue(new Error("network down"));
    await fireEvent.click(dom.getByRole("button", { name: "Reload" }));

    await waitFor(() => expect(editor.error).toMatchObject({ op: "reload" }));
    expect(banner()).toBeTruthy();
    expect(editor.active.text).toBe("mine");

    // The failure must be reported as the operation it WAS. StatusStrip
    // labelled every `editor.error` "save:" regardless of its `op`, so a
    // failed Reload told the user a save had failed -- and no save had been
    // attempted. Invisible to statusstrip.test.js, whose fixtures only ever
    // set `op: "save"`; the same blind-fixture shape as Finding G.
    // (Behind the "+N": the conflict outranks the error in the one-line bar.)
    await fireEvent.click(dom.getByRole("button", { name: /^\+\d+$/ }));
    expect(dom.getByText(/reload: network down/)).toBeTruthy();
    expect(dom.queryByText(/save: network down/)).toBeNull();

    // And the retry works once the failure clears -- the buttons are still live.
    api.readFile.mockResolvedValue({ text: "theirs", etag: '"2"' });
    await fireEvent.click(dom.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(banner()).toBeNull());
    expect(editor.active.text).toBe("theirs");
  });

  it("a failing Overwrite leaves the banner up and keeps my text", async () => {
    await mountWithDirtyTab();
    const write = vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await waitFor(() => expect(banner()).toBeTruthy());

    api.readFile.mockResolvedValue({ text: "theirs", etag: '"2"' });
    write.mockRejectedValue(new Error("still conflicting"));

    await fireEvent.click(dom.getByRole("button", { name: "Overwrite" }));

    await waitFor(() => expect(editor.error).toMatchObject({ op: "overwrite" }));
    expect(banner()).toBeTruthy();
    expect(editor.active.text).toBe("mine");
  });

  // The banner is not one of the five toggleable panels; AppShell mounts it
  // above the panel row on purpose. Closing every panel must not hide it --
  // a warning the user cannot see is worse than no warning.
  it("stays visible with every panel closed", async () => {
    await mountWithDirtyTab();
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    await waitFor(() => expect(banner()).toBeTruthy());

    const rail = dom.getByRole("toolbar", { name: /panels/i });
    for (const p of ["Library", "Edit", "Preview"]) {
      await fireEvent.click(dom.getByRole("button", { name: p, container: rail }));
    }
    await tick();

    expect(banner()).toBeTruthy();
    expect(dom.getByRole("button", { name: "Reload" })).toBeTruthy();
  });
});
