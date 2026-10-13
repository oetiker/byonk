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
  // No project-wide auto-cleanup is configured (see iconrail.test.js)
  // -- without this, the second test's render leaves the
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
  it("disables Delete on a read-only screen, keeps Copy and New", async () => {
    // Spec §8.6.
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    expect(screen.getByRole("button", { name: /delete/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /copy/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: "New" })).toBeEnabled();
  });

  it("enables Delete on a writable screen", () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    expect(screen.getByRole("button", { name: /delete/i })).toBeEnabled();
  });

  // U9: a screen's Repo and Path are edited in the Edit pane's Details; the
  // Library keeps New / Copy / Delete only.
  it("has no Rename, neither in the header nor in the folded menu", async () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    const actions = screen.getByTestId("library-actions");
    expect(within(actions).getAllByRole("button").map((b) => b.getAttribute("aria-label")))
      .toEqual(["New", "Copy", "Delete"]);
    expect(screen.queryByRole("button", { name: /rename/i })).toBeNull();
    await fireEvent.pointerDown(screen.getByRole("button", { name: "More actions" }), { button: 0, pointerType: "mouse" });
    await screen.findByRole("menuitem", { name: "New" });
    expect(screen.queryByRole("menuitem", { name: /rename/i })).toBeNull();
  });

  it("selects the new screen after a successful create", async () => {
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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

  it("captures a rejected create on screens.error instead of an unhandled rejection", async () => {
    // Finding 1: none of the four handlers used to catch a rejected api.*
    // call, so a 409/dropped-connection/401 became an unhandled promise
    // rejection with no user-visible trace -- the dialog just closed. This
    // is the regression guard; see task-6-report.md's fix-round-1 section
    // for the break/restore transcript proving it fails without the catch.
    vi.spyOn(api, "createScreen").mockRejectedValue(new Error("409 Conflict: name taken"));
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
    await answer("New screen name", "boom");
    await waitFor(() => {
      expect(screens.error).toEqual({ op: "create", message: "409 Conflict: name taken" });
    });
  });

  it("clears a stale error when the next operation is submitted", async () => {
    screens.error = { op: "create", message: "a previous failure" };
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(screens.error).toBeNull());
  });

  it("New opens a name form with the focus in it; Escape closes it and creates nothing", async () => {
    const create = vi.spyOn(api, "createScreen");
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
    const form = await screen.findByRole("form", { name: "New screen name" });
    await tick();
    expect(document.activeElement).toBe(within(form).getByRole("textbox"));
    await fireEvent.keyDown(within(form).getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("form", { name: "New screen name" })).toBeNull();
    expect(create).not.toHaveBeenCalled();
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
    await answer("New screen name", "fresh");
    await waitFor(() => expect(screen.queryByRole("form", { name: "New screen name" })).toBeNull());
  });

  it("never opens a native dialog", async () => {
    const prompt = vi.spyOn(window, "prompt");
    const confirm = vi.spyOn(window, "confirm");
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(LibraryPane);
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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
    await fireEvent.click(screen.getByRole("button", { name: "New" }));
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

  it("keeps the New/Copy/Delete toolbar and inline forms while a section is collapsed", async () => {
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
    expect(sec.className).toMatch(/min-h-\(--section-h\)/);
  });
});

describe("LibraryPane headings and chevrons", () => {
  const header = (name) => screen.getByRole("button", { name: new RegExp(name, "i") });

  it("draws a 16px chevron that rotates when the section is open", async () => {
    render(LibraryPane);
    const chev = () => header("Screens").querySelector("svg");
    expect(chev()).toBeTruthy();
    expect(chev().getAttribute("aria-hidden")).toBe("true");
    expect(chev().getAttribute("class")).toMatch(/\bsize-4\b/);
    expect(chev().getAttribute("class")).toMatch(/\brotate-90\b/);
    await fireEvent.click(header("Screens"));
    expect(chev().getAttribute("class")).not.toMatch(/\brotate-90\b/);
  });

  it("keeps the section minimum equal to the header height (one token)", () => {
    render(LibraryPane);
    expect(header("Screens").className).toMatch(/\bh-\(--section-h\)/);
    expect(header("Screens").closest("section").className).toMatch(/min-h-\(--section-h\)/);
  });

  it("rows are row-token high; the selected one is the quiet sidebar accent", () => {
    subject.select("screen", "local/a");
    render(LibraryPane);
    const rows = screen.getAllByRole("button").filter((b) => b.className.includes("h-(--row-h)"));
    expect(rows.length).toBeGreaterThan(0);
    const current = rows.find((b) => b.getAttribute("aria-current") === "true");
    expect(current.className).toMatch(/\bbg-sidebar-accent\b/);
    expect(current.className).toMatch(/\btext-sidebar-accent-foreground\b/);
    expect(current.className).not.toMatch(/\bbg-primary\b/);
  });

  it("sub-group labels are headings: spaced, ruled below, not selectable, not interactive", () => {
    render(LibraryPane);
    const label = screen.getByText("local");
    const cls = label.className.split(/\s+/);
    expect(cls).toContain("select-none");
    expect(cls).toContain("cursor-default");
    expect(cls).toContain("border-b");
    expect(cls.some((c) => /^pt-[2-9]/.test(c) || /^mt-/.test(c))).toBe(true);
    expect(label.tagName).not.toBe("BUTTON");
    expect(label.getAttribute("tabindex")).toBe(null);
    expect(cls.join(" ")).not.toMatch(/hover:/);
  });
});

describe("LibraryPane narrow header (fix round 1, finding 2)", () => {
  it("folds the three actions into a More-actions menu below 14rem, by container query", () => {
    render(LibraryPane);
    const header = screen.getByRole("region", { name: "Library" }).querySelector('[data-slot="pane-header"]');
    expect(header.className).toMatch(/@container/);
    expect(screen.getByTestId("library-actions").className).toMatch(/@max-\[14rem\]:hidden/);
    const menu = screen.getByTestId("library-actions-menu").className.split(/\s+/);
    expect(menu).toEqual(expect.arrayContaining(["hidden", "@max-[14rem]:flex"]));
  });

  it("the menu offers the same three actions, under the same names and gates", async () => {
    render(LibraryPane); // no subject: Copy, Delete disabled
    const trigger = screen.getByRole("button", { name: "More actions" });
    // bits-ui opens a DropdownMenu on pointerdown (a click would open and shut it).
    await fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
    const item = async (name) => screen.findByRole("menuitem", { name });
    expect(await item("New")).not.toHaveAttribute("data-disabled");
    for (const n of ["Copy", "Delete"]) expect(await item(n)).toHaveAttribute("data-disabled");

    await fireEvent.click(await item("New"));
    const form = await screen.findByRole("form", { name: "New screen name" });
    // Fix round 2, finding 4: the menu closing must not pull focus back to
    // its trigger -- the name field keeps it.
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    await new Promise((r) => setTimeout(r, 50));
    expect(document.activeElement).toBe(within(form).getByRole("textbox"));
  });
});

describe("LibraryPane: + adds a placeholder panel or device (U7)", () => {
  const dom = screen;
  const PANELS = [
    { id: "e1004", name: "reTerminal E1004", colors: "#000000,#FFFFFF,#FF0000", width: 1600, height: 1200 },
  ];
  const toggle = (name) => dom.getByRole("button", { name: new RegExp(`^${name}$`, "i") });
  let refresh;

  beforeEach(() => {
    cleanup();
    screens.meta = { panels: PANELS, devices: [{ id: "new-device", name: "New device", panel: null }], dither_algorithms: [] };
    refresh = vi.spyOn(screens, "refresh").mockResolvedValue();
  });

  it("puts a labelled + button beside the Panels and Devices toggles, never inside them", () => {
    render(LibraryPane);
    for (const [label, section] of [["Add panel", "Panels"], ["Add device", "Devices"]]) {
      const plus = dom.getByRole("button", { name: label });
      expect(plus.closest("button[aria-expanded]")).toBeNull();
      const t = toggle(section);
      expect(t).toHaveAttribute("aria-expanded", "true");
      expect(plus.parentElement.contains(t) || t.parentElement.contains(plus)).toBe(true);
      expect(t.parentElement.className).toMatch(/\bh-\(--section-h\)/);
    }
  });

  it("+ on Panels creates new_panel at once with defaults, selects it, and opens no form", async () => {
    const create = vi.spyOn(api, "createPanel").mockResolvedValue({ id: "new_panel" });
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Add panel" }));
    await waitFor(() => expect(subject.key).toBe("panel:new_panel"));
    expect(create).toHaveBeenCalledWith({ id: "new_panel", name: "New panel", colors: "#000000,#FFFFFF" });
    expect(refresh).toHaveBeenCalled();
    expect(dom.queryByRole("form")).toBeNull();
  });

  it("+ on Panels takes the next free placeholder id and copies colors and size from the panel subject", async () => {
    screens.meta = { ...screens.meta, panels: [...PANELS, { id: "new_panel", name: "New panel", colors: "#000000,#FFFFFF" }] };
    const create = vi.spyOn(api, "createPanel").mockResolvedValue({ id: "new_panel_2" });
    subject.select("panel", "e1004");
    render(LibraryPane);
    // The "+" ("Add panel") no longer shares its name with the "New panel" row.
    expect(dom.getAllByRole("button", { name: "Add panel" })).toHaveLength(1);
    expect(dom.getByRole("button", { name: "New panel" })).not.toHaveAttribute("aria-expanded");
    await fireEvent.click(dom.getByRole("button", { name: "Add panel" }));
    await waitFor(() => expect(subject.key).toBe("panel:new_panel_2"));
    expect(create).toHaveBeenCalledWith({
      id: "new_panel_2", name: "New panel", colors: "#000000,#FFFFFF,#FF0000", width: 1600, height: 1200 });
  });

  it("+ on Devices creates the next free new-device key on the first screen and selects it", async () => {
    const create = vi.spyOn(api, "createDevice").mockResolvedValue({ key: "new-device-2", screen: "local/a" });
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Add device" }));
    await waitFor(() => expect(subject.key).toBe("device:new-device-2"));
    expect(create).toHaveBeenCalledWith({ key: "new-device-2", name: "New device", screen: "local/a" });
    expect(dom.queryByRole("form")).toBeNull();
  });

  it("+ on Devices uses the screen that is the subject", async () => {
    screens.meta = { ...screens.meta, devices: [] };
    const create = vi.spyOn(api, "createDevice").mockResolvedValue({ key: "new-device", screen: "byonk-builtin/b" });
    subject.select("screen", "byonk-builtin/b");
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Add device" }));
    await waitFor(() => expect(subject.key).toBe("device:new-device"));
    expect(create).toHaveBeenCalledWith({ key: "new-device", name: "New device", screen: "byonk-builtin/b" });
  });

  it("+ on a collapsed section expands it so the new entry shows", async () => {
    vi.spyOn(api, "createPanel").mockResolvedValue({ id: "new_panel" });
    render(LibraryPane);
    await fireEvent.click(toggle("Panels"));
    await fireEvent.click(dom.getByRole("button", { name: "Add panel" }));
    await waitFor(() => expect(toggle("Panels")).toHaveAttribute("aria-expanded", "true"));
  });

  it("a failed create reports on screens.error and leaves the subject alone", async () => {
    vi.spyOn(api, "createPanel").mockRejectedValue(new Error("409 Conflict: panel `new_panel` already exists"));
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Add panel" }));
    await waitFor(() => expect(screens.error?.message).toMatch(/409/));
    expect(subject.kind).toBe(null);
  });

  it("the folded menu items create at once too", async () => {
    const panel = vi.spyOn(api, "createPanel").mockResolvedValue({ id: "new_panel" });
    const device = vi.spyOn(api, "createDevice").mockResolvedValue({ key: "new-device-2", screen: "local/a" });
    render(LibraryPane);
    await fireEvent.pointerDown(dom.getByRole("button", { name: "More actions" }), { button: 0, pointerType: "mouse" });
    await fireEvent.click(await dom.findByRole("menuitem", { name: "Add panel" }));
    await waitFor(() => expect(subject.key).toBe("panel:new_panel"));
    await fireEvent.pointerDown(dom.getByRole("button", { name: "More actions" }), { button: 0, pointerType: "mouse" });
    await fireEvent.click(await dom.findByRole("menuitem", { name: "Add device" }));
    await waitFor(() => expect(subject.key).toBe("device:new-device-2"));
    expect(panel).toHaveBeenCalledTimes(1);
    expect(device).toHaveBeenCalledTimes(1);
    expect(dom.queryByRole("form")).toBeNull();
  });
});

describe("LibraryPane: delete a panel or a device (U7)", () => {
  const dom = screen;
  beforeEach(() => {
    cleanup();
    screens.meta = {
      panels: [{ id: "e1004", name: "reTerminal E1004", colors: "#000000,#FFFFFF" }],
      devices: [{ id: "kitchen", name: "Kitchen", panel: "e1004" }],
      dither_algorithms: [],
    };
    vi.spyOn(screens, "refresh").mockResolvedValue();
  });

  const tipOf = async (btn) => {
    await fireEvent.pointerMove(btn, { pointerType: "mouse" });
    await fireEvent.focus(btn);
    return btn;
  };

  it.each([
    ["screen", "local/a", "Delete screen"],
    ["panel", "e1004", "Delete panel"],
    ["device", "kitchen", "Delete device"],
  ])("the Delete tooltip and menu item follow a %s subject", async (kind, id, text) => {
    subject.select(kind, id);
    render(LibraryPane);
    const btn = dom.getByRole("button", { name: "Delete" });
    expect(btn.disabled).toBe(false);
    await tipOf(btn);
    expect(await dom.findByText(text, {}, { timeout: 2000 })).toBeTruthy();
    cleanup();
    render(LibraryPane);
    await fireEvent.pointerDown(dom.getByRole("button", { name: "More actions" }), { button: 0, pointerType: "mouse" });
    const item = await dom.findByRole("menuitem", { name: "Delete" });
    expect(item.textContent).toContain(text);
    expect(item).not.toHaveAttribute("data-disabled");
  });

  it.each([["panel", "e1004"], ["device", "kitchen"]])("Copy stays screen-only: disabled for a %s", (kind, id) => {
    subject.select(kind, id);
    render(LibraryPane);
    expect(dom.getByRole("button", { name: "Copy" }).disabled).toBe(true);
  });

  it("deletes a panel after the inline confirmation, then clears the subject", async () => {
    const del = vi.spyOn(api, "deletePanel").mockResolvedValue({ deleted: "e1004" });
    subject.select("panel", "e1004");
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Delete" }));
    const form = await dom.findByRole("form", { name: "Confirm panel deletion" });
    expect(form.textContent).toContain("reTerminal E1004");
    expect(del).not.toHaveBeenCalled();
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("e1004"));
    await waitFor(() => expect(subject.kind).toBe(null));
    expect(screens.refresh).toHaveBeenCalled();
    expect(dom.queryByRole("form")).toBeNull();
    // A later entry under the same id must not inherit the deleted one's draft.
    expect(configDraft.drafts).toEqual({});
  });

  it("deletes a device after the inline confirmation, then clears the subject", async () => {
    const del = vi.spyOn(api, "deleteDevice").mockResolvedValue({ deleted: "kitchen" });
    subject.select("device", "kitchen");
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Delete" }));
    const form = await dom.findByRole("form", { name: "Confirm device deletion" });
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("kitchen"));
    await waitFor(() => expect(subject.kind).toBe(null));
  });

  it("a panel in use (409) keeps the subject and shows the server message on screens.error", async () => {
    vi.spyOn(api, "deletePanel").mockRejectedValue(new Error("409 Conflict: panel `e1004` is used by: kitchen"));
    subject.select("panel", "e1004");
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Delete" }));
    const form = await dom.findByRole("form", { name: "Confirm panel deletion" });
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screens.error?.message).toMatch(/is used by: kitchen/));
    expect(subject.key).toBe("panel:e1004");
  });

  it("a pending panel delete is dropped when the subject changes", async () => {
    subject.select("panel", "e1004");
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "Delete" }));
    await dom.findByRole("form", { name: "Confirm panel deletion" });
    subject.select("device", "kitchen");
    await tick();
    expect(dom.queryByRole("form", { name: "Confirm panel deletion" })).toBeNull();
  });
});
