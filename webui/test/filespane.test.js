import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import FilesPane from "../src/lib/components/FilesPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(async () => {
  // No project-wide auto-cleanup is configured (see iconrail.test.js,
  // splitpane.test.js) -- without this, the second test's render leaves the
  // first test's DOM in place and getByRole matches more than one element.
  cleanup();
  vi.restoreAllMocks();
  // `screens` is a module singleton: reset the fields tests below mutate so
  // one test's selection or captured error can't leak into the next.
  screens.selected = null;
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
    screens.select("byonk-builtin/b");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /copy/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /new/i })).toBeEnabled();
  });

  it("enables Rename and Delete on a writable screen", () => {
    screens.select("local/a");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeEnabled();
  });

  it("selects the new screen after a successful create", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("fresh");
    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /new/i }));
    await waitFor(() => expect(screens.selected).toBe("local/fresh"));
  });

  it("selects the copy after a successful copy", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("dup");
    vi.spyOn(api, "copyScreen").mockResolvedValue({ ref: "local/dup" });
    screens.select("byonk-builtin/b");
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /copy/i }));
    await waitFor(() => expect(screens.selected).toBe("local/dup"));
  });

  it("selects the renamed screen after a successful rename", async () => {
    // This is the behaviour the rename-reselect deviation (task-6-report.md)
    // exists for: without it, `screens.selected` would stay pinned to the
    // pre-rename ref, which no longer names anything after the rename.
    vi.spyOn(window, "prompt").mockReturnValue("renamed");
    vi.spyOn(api, "renameScreen").mockResolvedValue({ ref: "local/renamed" });
    screens.select("local/a");
    render(FilesPane);
    await fireEvent.click(screen.getByRole("button", { name: /rename/i }));
    await waitFor(() => expect(screens.selected).toBe("local/renamed"));
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
