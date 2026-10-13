import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, within, screen as dom } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";

// U9: a screen's Repo and Path are its identity, edited in the Edit pane's
// Details block; Save moves the screen. CodeMirror is mocked as in
// editorpane.test.js -- only the pane's own wiring is under test here.
const cmMock = { setDoc: vi.fn(), setDiagnostics: vi.fn(), setTheme: vi.fn(), destroy: vi.fn(), view: {} };
let lastOpts = null;
vi.mock("../src/lib/codemirror.js", () => ({
  createEditor: vi.fn((host, opts) => {
    lastOpts = opts;
    return cmMock;
  }),
}));
import EditorPane from "../src/lib/components/EditorPane.svelte";

const REPOS = [
  { handle: "local", writable: true },
  { handle: "shared", writable: true },
  { handle: "byonk-builtin", writable: false },
  { handle: "examples", writable: false },
];
const listing = (refs) => ({ screens: refs.map((name) => ({ name })), devices: [], panels: [], dither_algorithms: [] });

async function open(ref) {
  subject.select("screen", ref);
  await editor.loadScreen(ref, { readOnly: !screens.writable(ref) });
  render(EditorPane);
  flushSync();
  return dom.getByRole("region", { name: "Details" });
}

async function type(el, value) {
  el.value = value;
  await fireEvent.input(el);
}

beforeEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  lastOpts = null;
  editor.reset();
  editor.files = [];
  subject.reset();
  screens.error = null;
  vi.spyOn(api, "listRepos").mockResolvedValue(REPOS);
  vi.spyOn(api, "listScreens").mockResolvedValue(
    listing(["local/a", "local/home/clock", "byonk-builtin/default", "examples/hello"]),
  );
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "--", etag: '"1"' });
  vi.spyOn(api, "validateScreen").mockResolvedValue({ ok: true, issues: [] });
  vi.spyOn(api, "checkFile").mockResolvedValue({ ok: true, issues: [] });
  await screens.refresh();
});
afterEach(() => cleanup());

describe("Details block of a screen", () => {
  it("offers Repo (the writable repos) and Path for a screen in a writable repo", async () => {
    const details = await open("local/home/clock");
    const repo = within(details).getByLabelText("Repo");
    expect(repo.tagName).toBe("SELECT");
    expect([...repo.options].map((o) => o.value)).toEqual(["local", "shared"]);
    expect(repo.value).toBe("local");
    expect(within(details).getByLabelText("Path").value).toBe("home/clock");
  });

  it.each(["byonk-builtin/default", "examples/hello"])("shows Repo and Path of %s as plain text", async (ref) => {
    const details = await open(ref);
    const [handle, ...rest] = ref.split("/");
    expect(within(details).queryByRole("combobox")).toBeNull();
    expect(within(details).queryByRole("textbox")).toBeNull();
    expect(within(details).getByText(handle)).toBeTruthy();
    expect(within(details).getByText(rest.join("/"))).toBeTruthy();
    expect(dom.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("editing Path marks the screen unsaved; Save moves it and selects it under its new ref", async () => {
    const move = vi.spyOn(api, "moveScreen").mockImplementation(async () => {
      api.listScreens.mockResolvedValue(listing(["local/b", "local/home/clock"]));
      return { ref: "local/b", rewritten: ["kitchen"] };
    });
    const details = await open("local/a");
    const save = dom.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    expect(within(details).queryByRole("img", { name: "unsaved changes" })).toBeNull();

    await type(within(details).getByLabelText("Path"), "b");
    expect(within(details).getByRole("img", { name: "unsaved changes" })).toBeTruthy();
    expect(save).toBeEnabled();

    await fireEvent.click(save);
    await waitFor(() => expect(subject.key).toBe("screen:local/b"));
    expect(move).toHaveBeenCalledWith("local/a", "local", "b");
  });

  it("changing Repo moves the screen into that repo", async () => {
    const move = vi.spyOn(api, "moveScreen").mockResolvedValue({ ref: "shared/a", rewritten: [] });
    const details = await open("local/a");
    const repo = within(details).getByLabelText("Repo");
    repo.value = "shared";
    await fireEvent.change(repo);
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(subject.key).toBe("screen:shared/a"));
    expect(move).toHaveBeenCalledWith("local/a", "shared", "a");
  });

  it("a refused move keeps the draft and says why, until the next edit", async () => {
    vi.spyOn(api, "moveScreen").mockRejectedValue(new Error("409 Conflict: screen `local/b` already exists"));
    const details = await open("local/a");
    const path = within(details).getByLabelText("Path");
    await type(path, "b");
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));

    const alert = await dom.findByRole("alert");
    expect(alert.textContent).toBe("Save failed: 409 Conflict: screen `local/b` already exists");
    expect(dom.getAllByText(/already exists/)).toHaveLength(1);
    expect(subject.key).toBe("screen:local/a");
    expect(path.value).toBe("b");
    expect(within(details).getByRole("img", { name: "unsaved changes" })).toBeTruthy();
    expect(screens.error).toBeNull();
    expect(editor.error).toBeNull();

    await type(path, "c");
    expect(dom.queryByRole("alert")).toBeNull();
  });

  it("an emptied Path cannot be saved", async () => {
    const move = vi.spyOn(api, "moveScreen");
    const details = await open("local/a");
    await type(within(details).getByLabelText("Path"), "  ");
    expect(dom.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(within(details).getByText(/needs a path/)).toBeTruthy();
    expect(move).not.toHaveBeenCalled();
  });

  it("with file edits too, Save writes the files first and then moves", async () => {
    const order = [];
    vi.spyOn(api, "writeFile").mockImplementation(async (ref, name) => {
      order.push(`write ${ref} ${name}`);
      return { etag: '"2"' };
    });
    vi.spyOn(api, "moveScreen").mockImplementation(async (ref) => {
      order.push(`move ${ref}`);
      return { ref: "local/b", rewritten: [] };
    });
    const details = await open("local/a");
    lastOpts.onChange("-- edited");
    flushSync();
    await type(within(details).getByLabelText("Path"), "b");
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/b"));
    expect(order).toEqual(["write local/a script.lua", "move local/a"]);
  });

  it("a file save that fails stops the move", async () => {
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    const move = vi.spyOn(api, "moveScreen");
    const details = await open("local/a");
    lastOpts.onChange("-- edited");
    flushSync();
    await type(within(details).getByLabelText("Path"), "b");
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(editor.conflict).not.toBeNull());
    expect(move).not.toHaveBeenCalled();
    expect(subject.key).toBe("screen:local/a");
  });

  it("the watcher's echo of a move is this page's own write: no extra render", async () => {
    vi.spyOn(api, "moveScreen").mockResolvedValue({ ref: "local/b", rewritten: [] });
    const details = await open("local/a");
    await type(within(details).getByLabelText("Path"), "b");
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/b"));
    const rev = editor.diskRevision;
    await editor.onWatchEvent(["script.lua"]);
    expect(editor.diskRevision).toBe(rev);
  });

  it("Cmd/Ctrl-S does what Save does: it moves too", async () => {
    const move = vi.spyOn(api, "moveScreen").mockResolvedValue({ ref: "local/b", rewritten: [] });
    const details = await open("local/a");
    await type(within(details).getByLabelText("Path"), "b");
    await lastOpts.onSave();
    await waitFor(() => expect(subject.key).toBe("screen:local/b"));
    expect(move).toHaveBeenCalledWith("local/a", "local", "b");
  });

  it("a refused move after a quiet file save renders the saved file after all", async () => {
    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    vi.spyOn(api, "moveScreen").mockRejectedValue(new Error("409 Conflict: taken"));
    const details = await open("local/a");
    lastOpts.onChange("-- edited");
    flushSync();
    await type(within(details).getByLabelText("Path"), "b");
    const rev = editor.diskRevision;
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await dom.findByRole("alert");
    expect(editor.diskRevision).toBe(rev + 1);
  });
});
