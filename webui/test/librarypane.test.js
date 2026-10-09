import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
import { tick } from "svelte";
import { editor } from "../src/lib/state/editor.svelte.js";
import LibraryPane from "../src/lib/components/LibraryPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import * as api from "../src/lib/api.js";

async function answer(label, text) {
  const form = await screen.findByRole("form", { name: label });
  const input = within(form).getByRole("textbox");
  await fireEvent.input(input, { target: { value: text } });
  await fireEvent.keyDown(input, { key: "Enter" });
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

beforeEach(async () => {
  localStorage.clear();
  // No project-wide auto-cleanup is configured (see iconrail.test.js,
  // splitpane.test.js) -- without this, the second test's render leaves the
  // first test's DOM in place and getByRole matches more than one element.
  cleanup();
  vi.restoreAllMocks();
  // `screens` is a module singleton: reset the fields tests below mutate so
  // one test's selection or captured error can't leak into the next.
  subject.reset();
  configDraft.reset();
  screens.error = null;
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listRepos").mockResolvedValue([
    { handle: "local", writable: true },
    { handle: "byonk-builtin", writable: false },
  ]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }, { name: "byonk-builtin/b" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  await screens.refresh();
});

describe("LibraryPane", () => {
  it("disables Rename and Delete on a read-only screen, keeps Copy and New", async () => {
    // Spec §8.6.
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /copy/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /new/i })).toBeEnabled();
  });

  it("enables Rename and Delete on a writable screen", () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeEnabled();
  });

  it("selects the new screen after a successful create", async () => {
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(subject.screenRef).toBe("local/fresh"));
  });

  it("selects the copy after a successful copy", async () => {
    vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "local/dup" });
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await answer("Copy to name", "dup");
    await waitFor(() => expect(subject.screenRef).toBe("local/dup"));
  });

  it("selects the renamed screen after a successful rename", async () => {
    // This is the behaviour the rename-reselect deviation (task-6-report.md)
    // exists for: without it, `subject.screenRef` would stay pinned to the
    // pre-rename ref, which no longer names anything after the rename.
    vi.spyOn(api, "renameScreen").mockResolvedValue({ ref: "local/renamed" });
    subject.select("screen", "local/a");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
    await answer("Rename to", "renamed");
    await waitFor(() => expect(subject.screenRef).toBe("local/renamed"));
  });

  it("captures a rejected create on screens.error instead of an unhandled rejection", async () => {
    // Finding 1: none of the four handlers used to catch a rejected api.*
    // call, so a 409/dropped-connection/401 became an unhandled promise
    // rejection with no user-visible trace -- the dialog just closed. This
    // is the regression guard; see task-6-report.md's fix-round-1 section
    // for the break/restore transcript proving it fails without the catch.
    vi.spyOn(api, "createScreen").mockRejectedValue(new Error("409 Conflict: name taken"));
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "boom");
    await waitFor(() => {
      expect(screens.error).toEqual({ op: "create", message: "409 Conflict: name taken" });
    });
  });

  it("clears a stale error when the next operation is submitted", async () => {
    screens.error = { op: "create", message: "a previous failure" };
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(screens.error).toBeNull());
  });

  it("New opens a name form with the focus in it; Escape closes it and creates nothing", async () => {
    const create = vi.spyOn(api, "createScreen");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    const form = await screen.findByRole("form", { name: "New screen name" });
    await tick();
    expect(document.activeElement).toBe(within(form).getByRole("textbox"));
    await fireEvent.keyDown(within(form).getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("form", { name: "New screen name" })).toBeNull();
    expect(create).not.toHaveBeenCalled();
  });

  it("Rename starts from the screen's current name", async () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
    const form = await screen.findByRole("form", { name: "Rename to" });
    expect(within(form).getByRole("textbox").value).toBe("a");
  });

  it("Delete asks inline, names the screen, and deletes only on Delete", async () => {
    const del = vi.spyOn(api, "deleteScreen").mockResolvedValue(undefined);
    subject.select("screen", "local/a");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /delete/i }));
    const form = await screen.findByRole("form", { name: "Confirm screen deletion" });
    expect(form.textContent).toContain("Delete local/a? This cannot be undone.");
    expect(del).not.toHaveBeenCalled();
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("local/a"));
  });

  it("Cancel on the delete form deletes nothing", async () => {
    const del = vi.spyOn(api, "deleteScreen");
    subject.select("screen", "local/a");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /delete/i }));
    const form = await screen.findByRole("form", { name: "Confirm screen deletion" });
    await fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("form", { name: "Confirm screen deletion" })).toBeNull();
    expect(del).not.toHaveBeenCalled();
  });

  it("a pending delete is dropped when the subject changes", async () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /delete/i }));
    await screen.findByRole("form", { name: "Confirm screen deletion" });
    subject.select("screen", "byonk-builtin/b");
    await waitFor(() => expect(screen.queryByRole("form", { name: "Confirm screen deletion" })).toBeNull());
  });

  it("a failed create keeps the form open with the name", async () => {
    vi.spyOn(api, "createScreen").mockRejectedValue(new Error("409 Conflict: name taken"));
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "boom");
    await waitFor(() => expect(screens.error).toEqual({ op: "create", message: "409 Conflict: name taken" }));
    const form = screen.getByRole("form", { name: "New screen name" });
    expect(within(form).getByRole("textbox").value).toBe("boom");
  });

  it("New and Copy with no writable repo say so on screens.error and call nothing", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "byonk-builtin", writable: false }]);
    await screens.refresh();
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "x/y" });
    const copy = vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "x/y" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(screens.error?.op).toBe("create"));
    expect(screens.error.message).toMatch(/no writable screen repo/i);
    expect(create).not.toHaveBeenCalled();

    cleanup();
    screens.error = null;
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await answer("Copy to name", "dup");
    await waitFor(() => expect(screens.error?.op).toBe("copy"));
    expect(screens.error.message).toMatch(/no writable screen repo/i);
    expect(copy).not.toHaveBeenCalled();
  });

  it("a successful create closes the form", async () => {
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(screen.queryByRole("form", { name: "New screen name" })).toBeNull());
  });

  describe("renaming a screen with unsaved edits", () => {
    async function dirtyScreen() {
      vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
      vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });
      subject.select("screen", "local/a");
      editor.reset();
      await editor.loadScreen("local/a");
      await editor.open("local/a", "script.lua");
      editor.setText("-- edited");
      expect(editor.active.dirty).toBe(true);
    }

    it("asks before renaming, and renames only when confirmed", async () => {
      await dirtyScreen();
      const rename = vi.spyOn(api, "renameScreen").mockResolvedValue({ ref: "local/renamed" });
      render(LibraryPane);
      await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
      await answer("Rename to", "renamed");
      const form = await screen.findByRole("form", { name: "Confirm rename with unsaved edits" });
      expect(form.textContent).toContain("local/a has unsaved edits");
      expect(rename).not.toHaveBeenCalled();
      await fireEvent.click(within(form).getByRole("button", { name: "Rename anyway" }));
      await waitFor(() => expect(rename).toHaveBeenCalledWith("local/a", "renamed"));
      await waitFor(() => expect(subject.screenRef).toBe("local/renamed"));
    });

    it("Cancel renames nothing and keeps the edits", async () => {
      await dirtyScreen();
      const rename = vi.spyOn(api, "renameScreen");
      render(LibraryPane);
      await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
      await answer("Rename to", "renamed");
      const form = await screen.findByRole("form", { name: "Confirm rename with unsaved edits" });
      await fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("form", { name: "Confirm rename with unsaved edits" })).toBeNull();
      expect(rename).not.toHaveBeenCalled();
      expect(editor.active.dirty).toBe(true);
    });

    it("renames straight away when nothing is unsaved", async () => {
      editor.reset();
      const rename = vi.spyOn(api, "renameScreen").mockResolvedValue({ ref: "local/renamed" });
      subject.select("screen", "local/a");
      render(LibraryPane);
      await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
      await answer("Rename to", "renamed");
      await waitFor(() => expect(rename).toHaveBeenCalledWith("local/a", "renamed"));
      expect(screen.queryByRole("form", { name: "Confirm rename with unsaved edits" })).toBeNull();
    });
  });

  it("never opens a native dialog", async () => {
    const prompt = vi.spyOn(window, "prompt");
    const confirm = vi.spyOn(window, "confirm");
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    expect(prompt).not.toHaveBeenCalled();
    expect(confirm).not.toHaveBeenCalled();
  });
});

describe("LibraryPane with more than one writable repo", () => {
  // Finding G. The suite above seeds exactly ONE writable repo, so which one
  // gets picked is never observable there. A real install has at least two --
  // `local` and the seeded `examples` -- and /api/repos sorts them
  // writable-first then handle-ascending (src/api/dev_files.rs:149), so
  // `examples` always arrives first. This fixture reproduces that order.
  beforeEach(async () => {
    cleanup();
    vi.restoreAllMocks();
    subject.reset();
    configDraft.reset();
    screens.error = null;
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "examples", writable: true },
      { handle: "local", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "examples/sample" }, { name: "byonk-builtin/b" }],
      devices: [],
      panels: [],
      dither_algorithms: [],
    });
    await screens.refresh();
  });

  it("creates a new screen in local, not in the alphabetically-first examples", async () => {
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("local");
  });

  it("copies into local, not into the alphabetically-first examples", async () => {
    const copy = vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "local/dup" });
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await answer("Copy to name", "dup");
    await waitFor(() => expect(copy).toHaveBeenCalled());
    expect(copy.mock.calls[0][1]).toBe("local");
  });

  it("falls back to the first writable repo when local is absent", async () => {
    // `local` is registered from `screens_dir`, but a deployment that points
    // every repo elsewhere can leave it out. The default must degrade, not
    // break New entirely.
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "examples", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    await screens.refresh();
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "examples/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("examples");
  });

  it("falls back to the first writable repo when local is read-only", async () => {
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "examples", writable: true },
      { handle: "local", writable: false },
    ]);
    await screens.refresh();
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "examples/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("examples");
  });
});

describe("LibraryPane subjects", () => {
  const dom = screen;
  beforeEach(() => {
    cleanup();
    screens.meta = {
      panels: [{ id: "e1004", name: "reTerminal E1004" }],
      devices: [{ id: "kitchen", name: "Kitchen", panel: "e1004" }, { id: "attic", name: null, panel: null }],
      dither_algorithms: [],
    };
    configDraft.usage = { panels: { e1004: ["kitchen"] }, unseen: ["attic"] };
  });

  it("lists panels and devices and makes each the subject", async () => {
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    expect(subject.key).toBe("panel:e1004");
    await fireEvent.click(await dom.findByRole("button", { name: "Kitchen \u00b7 reTerminal E1004" }));
    expect(subject.key).toBe("device:kitchen");
    expect(dom.getByRole("button", { name: "Kitchen \u00b7 reTerminal E1004" }).getAttribute("aria-current")).toBe("true");
  });

  it("names a device's panel as the server resolves it, and says when it is unknown", async () => {
    render(LibraryPane);
    expect(await dom.findByRole("button", { name: "attic \u00b7 panel unknown" })).toBeTruthy();
  });

  it("resolves a device without an explicit panel through the server's usage", async () => {
    screens.meta = { ...screens.meta, devices: [{ id: "attic", name: "Attic", panel: null }] };
    configDraft.usage = { panels: { e1004: ["attic"] }, unseen: [] };
    render(LibraryPane);
    expect(await dom.findByRole("button", { name: "Attic \u00b7 reTerminal E1004" })).toBeTruthy();
  });

  it("does not fetch usage itself; configDraft owns it and its failure never lands on screens.error", async () => {
    const usage = vi.spyOn(api, "panelUsage").mockRejectedValue(new Error("500"));
    render(LibraryPane);
    await Promise.resolve();
    expect(usage).not.toHaveBeenCalled();
    await configDraft.loadUsage();
    expect(screens.error).toBe(null);
    expect(configDraft.error.op).toBe("panel usage");
  });

  it("offers no create or delete for panels and devices", () => {
    render(LibraryPane);
    subject.select("panel", "e1004");
    expect(dom.getByRole("button", { name: "Delete" }).disabled).toBe(true);
  });
});

describe("LibraryPane accordion sections", () => {
  const header = (name) => screen.getByRole("button", { name: new RegExp(name, "i") });

  it("renders Screens, Panels and Devices as expanded disclosure buttons", () => {
    render(LibraryPane);
    for (const n of ["Screens", "Panels", "Devices"]) {
      expect(header(n)).toHaveAttribute("aria-expanded", "true");
    }
    expect(screen.getByRole("button", { name: "a" })).toBeTruthy();
  });

  it("toggling a header hides and shows only its list, and keeps the others visible", async () => {
    render(LibraryPane);
    await fireEvent.click(header("Screens"));
    expect(header("Screens")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: "a" })).toBeNull();
    expect(header("Panels")).toBeTruthy();
    expect(header("Devices")).toBeTruthy();
    await fireEvent.click(header("Screens"));
    expect(header("Screens")).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "a" })).toBeTruthy();
  });

  it("keeps the New/Copy/Rename/Delete toolbar and inline forms while a section is collapsed", async () => {
    render(LibraryPane);
    await fireEvent.click(header("Screens"));
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(await screen.findByRole("form", { name: "New screen name" })).toBeTruthy();
  });

  it("remembers collapsed sections per viewer", async () => {
    render(LibraryPane);
    await fireEvent.click(header("Panels"));
    cleanup();
    render(LibraryPane);
    expect(header("Panels")).toHaveAttribute("aria-expanded", "false");
    expect(header("Screens")).toHaveAttribute("aria-expanded", "true");
  });

  it("still works when localStorage throws", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
    render(LibraryPane);
    expect(header("Devices")).toHaveAttribute("aria-expanded", "true");
    await fireEvent.click(header("Devices"));
    expect(header("Devices")).toHaveAttribute("aria-expanded", "false");
  });

  it("expands the section holding a newly selected subject", async () => {
    render(LibraryPane);
    await fireEvent.click(header("Screens"));
    expect(header("Screens")).toHaveAttribute("aria-expanded", "false");
    subject.select("screen", "local/a");
    await tick();
    expect(header("Screens")).toHaveAttribute("aria-expanded", "true");
  });

  // jsdom has no layout, so the squashed-rows defect (rows shrinking inside a
  // height-limited flex column) is guarded by its class contract: the scrolling
  // body must not be a flex container, and rows must keep their natural height.
  it("keeps rows at natural height: scrolling body is not a flex column, rows are blocks", () => {
    render(LibraryPane);
    const row = screen.getByRole("button", { name: "a" });
    const body = row.closest("div.overflow-auto");
    expect(body).toBeTruthy();
    expect(body.className.split(/\s+/)).not.toContain("flex");
    expect(row.className).toMatch(/\bblock\b/);
    expect(row.className).toMatch(/\bshrink-0\b/);
  });

  it("clips an expanded section and gives it a header-height minimum", () => {
    render(LibraryPane);
    const sec = header("Screens").closest("section");
    expect(sec.className).toMatch(/overflow-hidden/);
    expect(sec.className).toMatch(/min-h-6/);
  });
});
