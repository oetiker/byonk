import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import FilesPane from "../src/lib/components/FilesPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(async () => {
  // No project-wide auto-cleanup is configured (see iconrail.test.js,
  // splitpane.test.js) -- without this, the second test's render leaves the
  // first test's DOM in place and getByRole matches more than one element.
  cleanup();
  vi.restoreAllMocks();
  // `screens` is a module singleton: reset the fields tests below mutate so
  // one test's selection or captured error can't leak into the next.
  subject.reset();
  screens.error = null;
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

describe("FilesPane", () => {
  it("disables Rename and Delete on a read-only screen, keeps Copy and New", async () => {
    // Spec §8.6.
    subject.select("screen", "byonk-builtin/b");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /copy/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /new/i })).toBeEnabled();
  });

  it("enables Rename and Delete on a writable screen", () => {
    subject.select("screen", "local/a");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeEnabled();
  });

  it("selects the new screen after a successful create", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("fresh");
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(subject.screenRef).toBe("local/fresh"));
  });

  it("selects the copy after a successful copy", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("dup");
    vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "local/dup" });
    subject.select("screen", "byonk-builtin/b");
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await waitFor(() => expect(subject.screenRef).toBe("local/dup"));
  });

  it("selects the renamed screen after a successful rename", async () => {
    // This is the behaviour the rename-reselect deviation (task-6-report.md)
    // exists for: without it, `subject.screenRef` would stay pinned to the
    // pre-rename ref, which no longer names anything after the rename.
    vi.spyOn(window, "prompt").mockReturnValue("renamed");
    vi.spyOn(api, "renameScreen").mockResolvedValue({ ref: "local/renamed" });
    subject.select("screen", "local/a");
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
    await waitFor(() => expect(subject.screenRef).toBe("local/renamed"));
  });

  it("captures a rejected create on screens.error instead of an unhandled rejection", async () => {
    // Finding 1: none of the four handlers used to catch a rejected api.*
    // call, so a 409/dropped-connection/401 became an unhandled promise
    // rejection with no user-visible trace -- the dialog just closed. This
    // is the regression guard; see task-6-report.md's fix-round-1 section
    // for the break/restore transcript proving it fails without the catch.
    vi.spyOn(window, "prompt").mockReturnValue("boom");
    vi.spyOn(api, "createScreen").mockRejectedValue(new Error("409 Conflict: name taken"));
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => {
      expect(screens.error).toEqual({ op: "create", message: "409 Conflict: name taken" });
    });
  });

  it("clears a stale error when the next operation starts", async () => {
    screens.error = { op: "create", message: "a previous failure" };
    vi.spyOn(window, "prompt").mockReturnValue(null); // user cancels the dialog
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(screens.error).toBeNull());
  });
});

describe("FilesPane with more than one writable repo", () => {
  // Finding G. The suite above seeds exactly ONE writable repo, so which one
  // gets picked is never observable there. A real install has at least two --
  // `local` and the seeded `examples` -- and /api/repos sorts them
  // writable-first then handle-ascending (src/api/dev_files.rs:149), so
  // `examples` always arrives first. This fixture reproduces that order.
  beforeEach(async () => {
    cleanup();
    vi.restoreAllMocks();
    subject.reset();
    screens.error = null;
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
    vi.spyOn(window, "prompt").mockReturnValue("fresh");
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("local");
  });

  it("copies into local, not into the alphabetically-first examples", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("dup");
    const copy = vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "local/dup" });
    subject.select("screen", "byonk-builtin/b");
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await waitFor(() => expect(copy).toHaveBeenCalled());
    expect(copy.mock.calls[0][1]).toBe("local");
  });

  it("falls back to the first writable repo when local is absent", async () => {
    // `local` is registered from `screens_dir`, but a deployment that points
    // every repo elsewhere can leave it out. The default must degrade, not
    // break New entirely.
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "examples", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    await screens.refresh();
    vi.spyOn(window, "prompt").mockReturnValue("fresh");
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "examples/fresh" });
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("examples");
  });

  it("falls back to the first writable repo when local is read-only", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "examples", writable: true },
      { handle: "local", writable: false },
    ]);
    await screens.refresh();
    vi.spyOn(window, "prompt").mockReturnValue("fresh");
    const create = vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "examples/fresh" });
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toBe("examples");
  });
});
