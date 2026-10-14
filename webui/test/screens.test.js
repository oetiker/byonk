import { describe, it, expect, vi, beforeEach } from "vitest";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => vi.restoreAllMocks());

describe("screens store", () => {
  it("lists a writable repo that holds no screens yet", async () => {
    // Regression guard for ccc4f47. `local` on a fresh install is exactly
    // this case, and grouping the screen list instead of the repo list is how
    // the bug comes back.
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "local", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "byonk-builtin/hello" }],
      devices: [],
      panels: [],
      dither_algorithms: [],
    });

    await screens.refresh();

    const handles = screens.grouped.map((g) => g.handle);
    expect(handles).toContain("local");
    expect(screens.grouped.find((g) => g.handle === "local").screens).toEqual([]);
  });

  it("knows which refs are writable", async () => {
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
    expect(screens.writable("local/a")).toBe(true);
    expect(screens.writable("byonk-builtin/b")).toBe(false);
  });
});
