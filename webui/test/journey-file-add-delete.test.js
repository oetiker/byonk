import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * Adding and deleting a file, and the post-save green tick.
 *
 * R5, ruled 2026-09-15. `docs/src/guide/dev-mode.md` has described a `+`
 * next to the file tabs since the hand-written UI, and the SPA never had one:
 * `api.deleteFile` had zero callers and `POST /dev/api/screen/validate` had
 * none either, though spec 2026-09-07-dev-mode-editor-design.md:315 runs it
 * after every successful save and :326 turns its result into one green tick
 * per clean file.
 *
 * Driven through the app root and the real buttons, because the lesson of
 * this branch is that a store and a component can both be green about the two
 * sides of a wire nobody joined.
 */
const FILES = [
  { name: "meta.yaml", size: 10, text: true },
  { name: "script.lua", size: 20, text: true },
  { name: "screen.svg", size: 30, text: true },
];

let listing;

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
  editor.reset();
  editor.files = [];
  screens.selected = null;
  screens.repos = [];
  screens.list = [];
  screens.error = null;
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };

  listing = [...FILES];
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
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockImplementation(async () => [...listing]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });
  vi.spyOn(api, "validateScreen").mockResolvedValue({ ok: true, issues: [] });
});

afterEach(() => {
  cleanup();
  delete window.prompt;
  delete window.confirm;
});

async function mountWithScreen(ref = "local/a") {
  render(AppShell);
  await waitFor(() => expect(screens.repos).toHaveLength(2));
  screens.select(ref);
  await waitFor(() => expect(editor.files).toHaveLength(listing.length));
  await tick();
}

const addButton = () => dom.getByRole("button", { name: "Add file" });
const delButton = () => dom.getByRole("button", { name: "Delete file" });

describe("adding a file", () => {
  it("creates it, lists it and opens it", async () => {
    const write = vi.spyOn(api, "writeFile").mockImplementation(async (ref, name) => {
      listing.push({ name, size: 0, text: true });
      return { etag: '"new"' };
    });
    api.readFile.mockImplementation(async (ref, name) =>
      name === "helper.lua" ? { text: "", etag: '"new"' } : { text: "-- lua", etag: '"1"' },
    );
    window.prompt = () => "helper.lua";

    await mountWithScreen();
    await fireEvent.click(addButton());

    // Created with a null etag, which api.js sends as `If-Match: *` -- byonk's
    // "create, and fail if it already exists".
    await waitFor(() => expect(write).toHaveBeenCalledWith("local/a", "helper.lua", "", null));
    // Listed, from a refreshed server listing rather than a locally invented
    // entry, and opened as the active buffer.
    await waitFor(() => expect(editor.files.map((f) => f.name)).toContain("helper.lua"));
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));
    expect(dom.getAllByRole("tab").map((t) => t.textContent)).toEqual(
      expect.arrayContaining([expect.stringContaining("helper.lua")]),
    );
  });

  it("reports an existing name instead of silently opening the file that is there", async () => {
    // docs/src/guide/dev-mode.md: "Creating a file that already exists is
    // reported rather than silently opening the existing one."
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    window.prompt = () => "script.lua";

    await mountWithScreen();
    const before = editor.active?.name;
    await fireEvent.click(addButton());

    await waitFor(() =>
      expect(editor.error).toEqual({ op: "add-file", message: "script.lua already exists" }),
    );
    expect(dom.getByText(/add-file: script.lua already exists/)).toBeTruthy();
    expect(editor.active?.name).toBe(before);
  });

  it("a cancelled prompt does nothing at all", async () => {
    const write = vi.spyOn(api, "writeFile");
    window.prompt = () => null;

    await mountWithScreen();
    await fireEvent.click(addButton());
    await tick();

    expect(write).not.toHaveBeenCalled();
    expect(editor.error).toBeNull();
  });

  it("is offered only for a writable screen", async () => {
    await mountWithScreen("byonk-builtin/b");
    expect(addButton()).toBeDisabled();

    screens.select("local/a");
    await waitFor(() => expect(addButton()).not.toBeDisabled());
  });
});

describe("deleting a file", () => {
  it("removes it, closes its buffer and drops it from the listing", async () => {
    listing.push({ name: "helper.lua", size: 5, text: true });
    const del = vi.spyOn(api, "deleteFile").mockImplementation(async (ref, name) => {
      listing = listing.filter((f) => f.name !== name);
    });
    window.confirm = () => true;

    await mountWithScreen();
    await editor.open("local/a", "helper.lua");
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));

    await fireEvent.click(delButton());

    await waitFor(() => expect(del).toHaveBeenCalledWith("local/a", "helper.lua"));
    await waitFor(() => expect(editor.files.map((f) => f.name)).not.toContain("helper.lua"));
    expect(editor.tabs.map((t) => t.name)).not.toContain("helper.lua");
    // Closing landed on a real neighbour rather than a blank editor.
    expect(editor.active).toBeTruthy();
  });

  it("a cancelled confirm deletes nothing", async () => {
    listing.push({ name: "helper.lua", size: 5, text: true });
    const del = vi.spyOn(api, "deleteFile");
    window.confirm = () => false;

    await mountWithScreen();
    await editor.open("local/a", "helper.lua");
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));

    await fireEvent.click(delButton());
    await tick();

    expect(del).not.toHaveBeenCalled();
    expect(editor.files.map((f) => f.name)).toContain("helper.lua");
  });

  // The server refuses these three (`CORE_FILES`, src/api/dev_files.rs) and is
  // the authority. The UI simply must not offer an action that cannot work.
  it.each(["meta.yaml", "script.lua", "screen.svg"])(
    "is not offered for the core file %s",
    async (name) => {
      window.confirm = () => true;
      await mountWithScreen();
      await editor.open("local/a", name);
      await waitFor(() => expect(editor.active?.name).toBe(name));

      expect(delButton()).toBeDisabled();
    },
  );

  it("is not offered for a read-only screen", async () => {
    listing.push({ name: "helper.lua", size: 5, text: true });
    await mountWithScreen("byonk-builtin/b");
    await editor.open("byonk-builtin/b", "helper.lua", { readOnly: true });
    await tick();

    expect(delButton()).toBeDisabled();
  });

  it("a refused delete is reported and the file stays", async () => {
    listing.push({ name: "helper.lua", size: 5, text: true });
    vi.spyOn(api, "deleteFile").mockRejectedValue(new Error("422 Unprocessable"));
    window.confirm = () => true;

    await mountWithScreen();
    await editor.open("local/a", "helper.lua");
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));

    await fireEvent.click(delButton());

    await waitFor(() => expect(editor.error).toMatchObject({ op: "delete-file" }));
    expect(dom.getByText(/delete-file: 422 Unprocessable/)).toBeTruthy();
    expect(editor.files.map((f) => f.name)).toContain("helper.lua");
  });

  // Adding or deleting one file must not cost the user their edits to
  // another: `refreshFiles()` exists precisely so this path does not go
  // through `loadScreen()`, whose `reset()` discards every buffer.
  it("keeps unsaved edits in another buffer", async () => {
    listing.push({ name: "helper.lua", size: 5, text: true });
    vi.spyOn(api, "deleteFile").mockImplementation(async (ref, name) => {
      listing = listing.filter((f) => f.name !== name);
    });
    window.confirm = () => true;

    await mountWithScreen();
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("unsaved edit");

    await editor.open("local/a", "helper.lua");
    await fireEvent.click(delButton());
    await waitFor(() => expect(editor.files.map((f) => f.name)).not.toContain("helper.lua"));

    const lua = editor.tabs.find((t) => t.name === "script.lua");
    expect(lua?.text).toBe("unsaved edit");
    expect(lua?.dirty).toBe(true);
  });
});

describe("the post-save green tick", () => {
  it("nothing is ticked before a validate has ever run", async () => {
    await mountWithScreen();
    expect(dom.queryByLabelText(/is valid/)).toBeNull();
  });

  it("a successful save validates the screen and ticks every clean file", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });

    await mountWithScreen();
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("-- edited");
    await tick();

    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(api.validateScreen).toHaveBeenCalledWith("local/a"));
    for (const f of FILES) {
      await waitFor(() => expect(dom.getByLabelText(`${f.name} is valid`)).toBeTruthy());
    }
  });

  it("a file the report flags is not ticked, and its issues reach its buffer", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    api.validateScreen.mockResolvedValue({
      ok: false,
      issues: [
        { severity: "error", location: "script.lua", message: "syntax error", line: 3 },
      ],
    });

    await mountWithScreen();
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(editor.active?.issues).toHaveLength(1));
    expect(dom.queryByLabelText("script.lua is valid")).toBeNull();
    // The other two were examined and were clean.
    expect(dom.getByLabelText("meta.yaml is valid")).toBeTruthy();
    expect(dom.getByLabelText("screen.svg is valid")).toBeTruthy();
    expect(dom.getByLabelText("has errors")).toBeTruthy();
  });

  // Some findings belong to the screen, not to one file
  // (src/services/screen_store.rs raises them against the screen ref). Such
  // an issue must neither be dropped nor mark every file clean by default.
  it("a screen-level issue is kept and ticks nothing it does not own", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    api.validateScreen.mockResolvedValue({
      ok: false,
      issues: [{ severity: "error", location: "local/a", message: "screen is broken" }],
    });

    await mountWithScreen();
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(editor.report?.issues).toHaveLength(1));
    // Every FILE was clean, so every file ticks -- the screen-level finding
    // is retained separately rather than suppressing them.
    for (const f of FILES) {
      expect(dom.getByLabelText(`${f.name} is valid`)).toBeTruthy();
    }
  });

  it("a validate that fails is reported as a validate, not as a failed save", async () => {
    // The save DID land. Reporting it as a save failure would push the user
    // to save again over their own fresh etag.
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    api.validateScreen.mockRejectedValue(new Error("500 Internal Server Error"));

    await mountWithScreen();
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(editor.error).toMatchObject({ op: "validate" }));
    expect(editor.active.dirty).toBe(false); // the save really did succeed
    expect(dom.getByText(/validate: 500/)).toBeTruthy();
  });

  it("a save that conflicts does not validate -- disk was never written", async () => {
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());

    await mountWithScreen();
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(editor.conflict).toBeTruthy());
    expect(api.validateScreen).not.toHaveBeenCalled();
  });

  // The verdict describes the saved text. Once the user types again it no
  // longer describes what is on screen, so the tick must go.
  it("the tick disappears again as soon as the buffer is dirtied", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });

    await mountWithScreen();
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(dom.getByLabelText("script.lua is valid")).toBeTruthy());

    editor.setText("-- edited again");
    await tick();

    expect(dom.queryByLabelText("script.lua is valid")).toBeNull();
    // The other files were not touched, so their verdict still stands.
    expect(dom.getByLabelText("meta.yaml is valid")).toBeTruthy();
  });

  it("switching screens drops the previous screen's verdict", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });

    await mountWithScreen();
    editor.setText("-- edited");
    await tick();
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(dom.getByLabelText("script.lua is valid")).toBeTruthy());

    screens.select("byonk-builtin/b");
    await waitFor(() => expect(editor.report).toBeNull());
    expect(dom.queryByLabelText(/is valid/)).toBeNull();
  });
});
