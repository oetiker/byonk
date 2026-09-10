import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import FilesPane from "../src/lib/components/FilesPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(async () => {
  // No project-wide auto-cleanup is configured (see iconrail.test.js,
  // splitpane.test.js) -- without this, the second test's render leaves the
  // first test's DOM in place and getByRole matches more than one element.
  cleanup();
  vi.restoreAllMocks();
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
});
