import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";

vi.mock("../src/lib/codemirror.js", () => ({
  createEditor: vi.fn(() => ({ setDoc: vi.fn(), setDiagnostics: vi.fn(), setTheme: vi.fn(), destroy: vi.fn(), view: {} })),
}));
import EditorPane from "../src/lib/components/EditorPane.svelte";

const FILES = [
  { name: "meta.yaml", size: 10, text: true },
  { name: "script.lua", size: 20, text: true },
  { name: "screen.svg", size: 30, text: true },
  { name: "logo.png", size: 1234, text: false },
  { name: "font.ttf", size: 99, text: false },
];
let listing;

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  editor.reset();
  subject.reset();
  listing = FILES.map((f) => ({ ...f }));
  editor.ref = "local/a";
  editor.files = listing;
  screens.repos = [
    { handle: "local", writable: true },
    { handle: "examples", writable: false },
  ];
  vi.spyOn(api, "listFiles").mockImplementation(async () => listing.map((f) => ({ ...f })));
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
});

const tab = (n) => dom.getByRole("tab", { name: new RegExp(`^${n.replace(".", "\\.")}`) });
async function select(n) {
  await fireEvent.click(tab(n));
  flushSync();
}

describe("asset viewer", () => {
  it("shows an image tab as an <img> from the raw URL with its byte size", async () => {
    render(EditorPane);
    await select("logo.png");
    const img = dom.getByRole("img", { name: "logo.png" });
    expect(img.getAttribute("src")).toMatch(/^api\/file\/raw\?ref=local%2Fa&name=logo\.png/);
    expect(dom.getByText(/1234 bytes/)).toBeTruthy();
  });

  it("shows a non-image as byte size plus a download link", async () => {
    render(EditorPane);
    await select("font.ttf");
    expect(dom.queryByRole("img", { name: "font.ttf" })).toBeNull();
    expect(dom.getByText(/binary file, 99 bytes/)).toBeTruthy();
    const link = dom.getByRole("link", { name: /Download/ });
    expect(link.getAttribute("href")).toMatch(/^api\/file\/raw\?ref=local%2Fa&name=font\.ttf/);
  });

  it("deletes the selected binary through the header minus", async () => {
    const del = vi.spyOn(api, "deleteFile").mockImplementation(async () => {
      listing = listing.filter((f) => f.name !== "logo.png");
    });
    render(EditorPane);
    await select("logo.png");
    await fireEvent.click(dom.getByRole("button", { name: "Delete file" }));
    const form = await dom.findByRole("form", { name: "Confirm file deletion" });
    await fireEvent.click(within(form).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("local/a", "logo.png"));
    await waitFor(() => expect(dom.queryByRole("tab", { name: /logo/ })).toBeNull());
  });
});

// Task U24 (U19 review minor): the toolbar and the status bar describe the
// tab the user is LOOKING AT. An asset shown over a dirty text buffer offers
// no text Save, and the status bar names the asset, not the hidden buffer.
describe("an asset shown over a dirty text buffer", () => {
  it("disables Save and names the asset in the status bar", async () => {
    render(EditorPane);
    await select("script.lua");
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("changed");
    editor.setCursor(3, 7);
    flushSync();
    expect(dom.getByRole("button", { name: "Save" })).toBeEnabled();
    expect(dom.getByText("lua 3:7")).toBeTruthy();

    await select("logo.png");
    expect(dom.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(dom.queryByText("lua 3:7")).toBeNull();
    expect(dom.queryByText("valid")).toBeNull();
    expect(dom.getByText("logo.png (asset)")).toBeTruthy();

    // Back on the text tab, its unsaved edit is still there to save.
    await select("script.lua");
    expect(dom.getByRole("button", { name: "Save" })).toBeEnabled();
    expect(editor.active.dirty).toBe(true);
  });
});

describe("rename", () => {
  it("renames a binary and the tab follows", async () => {
    const ren = vi.spyOn(api, "renameFile").mockImplementation(async (r, from, to) => {
      listing = listing.map((f) => (f.name === from ? { ...f, name: to } : f));
    });
    render(EditorPane);
    await select("logo.png");
    await fireEvent.click(dom.getByRole("button", { name: "Rename file" }));
    const form = await dom.findByRole("form", { name: "Rename file" });
    const input = within(form).getByRole("textbox");
    expect(input.value).toBe("logo.png");
    await fireEvent.input(input, { target: { value: "brand.png" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(ren).toHaveBeenCalledWith("local/a", "logo.png", "brand.png"));
    await waitFor(() => expect(dom.getByRole("tab", { name: "brand.png" })).toHaveAttribute("aria-selected", "true"));
  });

  it("renames a clean open text tab and keeps it open under the new name", async () => {
    vi.spyOn(api, "renameFile").mockImplementation(async (r, from, to) => {
      listing = listing.map((f) => (f.name === from ? { ...f, name: to } : f));
    });
    listing.push({ name: "helper.lua", size: 1, text: true });
    editor.files = listing;
    render(EditorPane);
    await select("helper.lua");
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));
    await fireEvent.click(dom.getByRole("button", { name: "Rename file" }));
    const input = within(await dom.findByRole("form", { name: "Rename file" })).getByRole("textbox");
    await fireEvent.input(input, { target: { value: "util.lua" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(editor.active?.name).toBe("util.lua"));
    expect(editor.tabs).toHaveLength(1);
  });

  it("is disabled while the text buffer is dirty", async () => {
    listing.push({ name: "helper.lua", size: 1, text: true });
    editor.files = listing;
    render(EditorPane);
    await select("helper.lua");
    await waitFor(() => expect(editor.active?.name).toBe("helper.lua"));
    editor.setText("changed");
    flushSync();
    expect(dom.getByRole("button", { name: "Rename file" })).toBeDisabled();
  });

  it("a 412 shows the error and keeps the form", async () => {
    vi.spyOn(api, "renameFile").mockRejectedValue(new api.ConflictError());
    render(EditorPane);
    await select("logo.png");
    await fireEvent.click(dom.getByRole("button", { name: "Rename file" }));
    const form = await dom.findByRole("form", { name: "Rename file" });
    const input = within(form).getByRole("textbox");
    await fireEvent.input(input, { target: { value: "font.ttf" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(dom.getByText(/font\.ttf already exists/)).toBeTruthy());
    expect(dom.getByRole("form", { name: "Rename file" })).toBeTruthy();
  });

  it("is refused for the core files", async () => {
    render(EditorPane);
    await select("script.lua");
    expect(dom.getByRole("button", { name: "Rename file" })).toBeDisabled();
    expect(dom.getByRole("button", { name: "Delete file" })).toBeDisabled();
  });
});

describe("upload", () => {
  async function openUpload() {
    await fireEvent.click(dom.getByRole("button", { name: "Add file" }));
    return await dom.findByLabelText("Upload files");
  }
  const png = () => new File([new Uint8Array([1, 2, 3])], "new.png", { type: "image/png" });

  it("PUTs each file with If-Match * (null etag), then refreshes and selects it", async () => {
    const put = vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      listing.push({ name: n, size: 3, text: false });
      return { etag: '"e"' };
    });
    render(EditorPane);
    const input = await openUpload();
    await fireEvent.change(input, { target: { files: [png()] } });
    await waitFor(() => expect(put).toHaveBeenCalledTimes(1));
    expect(put.mock.calls[0][0]).toBe("local/a");
    expect(put.mock.calls[0][1]).toBe("new.png");
    expect(put.mock.calls[0][3]).toBeNull();
    await waitFor(() => expect(dom.getByRole("tab", { name: "new.png" })).toHaveAttribute("aria-selected", "true"));
  });

  it("one batch bumps diskRevision once", async () => {
    vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      listing.push({ name: n, size: 3, text: false });
      return { etag: '"e"' };
    });
    render(EditorPane);
    const input = await openUpload();
    const before = editor.diskRevision;
    await fireEvent.change(input, {
      target: { files: [png(), new File([new Uint8Array([9])], "two.png")] },
    });
    await waitFor(() => expect(dom.getByRole("tab", { name: "two.png" })).toBeTruthy());
    expect(editor.diskRevision).toBe(before + 1);
  });

  it("412 asks Replace? and yes re-sends with the current etag", async () => {
    const put = vi
      .spyOn(api, "putRaw")
      .mockRejectedValueOnce(new api.ConflictError())
      .mockResolvedValueOnce({ etag: '"n"' });
    vi.spyOn(api, "rawEtag").mockResolvedValue('"cur"');
    render(EditorPane);
    const input = await openUpload();
    await fireEvent.change(input, { target: { files: [new File([new Uint8Array([1])], "logo.png")] } });
    const form = await dom.findByRole("form", { name: "Confirm replace" });
    expect(within(form).getByText(/Replace logo\.png\?/)).toBeTruthy();
    await fireEvent.click(within(form).getByRole("button", { name: "Replace" }));
    await waitFor(() => expect(put).toHaveBeenCalledTimes(2));
    expect(put.mock.calls[1][3]).toBe('"cur"');
  });

  it("a text upload becomes a normal text tab", async () => {
    vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      listing.push({ name: n, size: 5, text: true });
      return { etag: '"e"' };
    });
    render(EditorPane);
    const input = await openUpload();
    await fireEvent.change(input, { target: { files: [new File(["-- hi"], "util.lua")] } });
    await waitFor(() => expect(editor.active?.name).toBe("util.lua"));
    expect(editor.asset).toBeNull();
  });
});

describe("read-only screen", () => {
  it("views binaries but offers no Add/Rename/Delete", async () => {
    editor.ref = "examples/a";
    render(EditorPane);
    await select("logo.png");
    expect(dom.getByRole("img", { name: "logo.png" })).toBeTruthy();
    for (const n of ["Add file", "Rename file", "Delete file"]) {
      expect(dom.getByRole("button", { name: n })).toBeDisabled();
    }
  });
});

describe("upload batch lifecycle (fix round 1)", () => {
  const f = (n) => new File([new Uint8Array([1])], n);
  async function pick(files) {
    await fireEvent.click(dom.getByRole("button", { name: "Add file" }));
    const input = await dom.findByLabelText("Upload files");
    await fireEvent.change(input, { target: { files } });
  }

  it("uses the batch's screen for every file after a screen switch", async () => {
    let release;
    const gate = new Promise((r) => (release = r));
    const put = vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      if (n === "one.png") await gate;
      return { etag: '"e"' };
    });
    render(EditorPane);
    await pick([f("one.png"), f("two.png")]);
    await waitFor(() => expect(put).toHaveBeenCalledTimes(1));
    editor.ref = "local/b";
    release();
    await waitFor(() => expect(put).toHaveBeenCalledTimes(2));
    expect(put.mock.calls.map((c) => c[0])).toEqual(["local/a", "local/a"]);
  });

  it("a cancelled Replace queue still refreshes and renders once for what was written", async () => {
    vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      if (n === "logo.png") throw new api.ConflictError();
      listing.push({ name: n, size: 1, text: false });
      return { etag: '"e"' };
    });
    render(EditorPane);
    const before = editor.diskRevision;
    await pick([f("new.png"), f("logo.png")]);
    await dom.findByRole("form", { name: "Confirm replace" });
    expect(editor.diskRevision).toBe(before);
    // Another inline form opens: the question is dropped.
    await fireEvent.click(dom.getByRole("button", { name: "Rename file" }).disabled ? dom.getByRole("button", { name: "Add file" }) : dom.getByRole("button", { name: "Rename file" }));
    await waitFor(() => expect(dom.getByRole("tab", { name: "new.png" })).toBeTruthy());
    expect(editor.diskRevision).toBe(before + 1);
  });

  it("a screen switch during the Replace question finishes without touching the new screen", async () => {
    vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      if (n === "logo.png") throw new api.ConflictError();
      return { etag: '"e"' };
    });
    render(EditorPane);
    const before = editor.diskRevision;
    await pick([f("new.png"), f("logo.png")]);
    await dom.findByRole("form", { name: "Confirm replace" });
    editor.ref = "local/b";
    await waitFor(() => expect(dom.queryByRole("form", { name: "Confirm replace" })).toBeNull());
    expect(editor.diskRevision).toBe(before);
  });

  it("No skips that file and the batch still renders once", async () => {
    const put = vi.spyOn(api, "putRaw").mockImplementation(async (r, n) => {
      if (n === "logo.png") throw new api.ConflictError();
      listing.push({ name: n, size: 1, text: false });
      return { etag: '"e"' };
    });
    render(EditorPane);
    const before = editor.diskRevision;
    await pick([f("new.png"), f("logo.png")]);
    const form = await dom.findByRole("form", { name: "Confirm replace" });
    await fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(editor.diskRevision).toBe(before + 1));
    expect(put).toHaveBeenCalledTimes(2);
  });

  it("Escape on the question behaves like No", async () => {
    vi.spyOn(api, "putRaw").mockRejectedValue(new api.ConflictError());
    const etag = vi.spyOn(api, "rawEtag");
    render(EditorPane);
    const before = editor.diskRevision;
    await pick([f("logo.png")]);
    const form = await dom.findByRole("form", { name: "Confirm replace" });
    await fireEvent.keyDown(form, { key: "Escape" });
    await waitFor(() => expect(dom.queryByRole("form", { name: "Confirm replace" })).toBeNull());
    expect(etag).not.toHaveBeenCalled();
    expect(editor.diskRevision).toBe(before);
  });
});
