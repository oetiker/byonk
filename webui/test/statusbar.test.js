import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import StatusBar from "../src/lib/components/StatusBar.svelte";

// StatusBar is scoped to exactly what spec §1's table names for the
// Editor's own status bar: validity, cursor position, watch state. The
// four captured-error fields (editor.error/screens.error/simulate.error/
// tune.error) and tune.pushed moved to StatusStrip (AppShell-level,
// review finding 2 / "Ruling 49") -- see statusstrip.test.js for those.

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  editor.reset();
});

afterEach(() => {
  cleanup();
});

describe("StatusBar", () => {
  it("shows validity, language and cursor position for the active tab", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.setCursor(3, 22);

    render(StatusBar);
    flushSync();

    expect(screen.getByText("✓ valid")).toBeTruthy();
    expect(screen.getByText("lua 3:22")).toBeTruthy();
  });

  it("shows an issue count instead of valid when the active tab has issues", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.active.issues = [
      { severity: "error", location: "script.lua", message: "boom", line: 1 },
    ];

    render(StatusBar);
    flushSync();

    expect(screen.queryByText("✓ valid")).toBeNull();
    expect(screen.getByText(/1 issue/)).toBeTruthy();
  });

  it("reports no file open when nothing is active", () => {
    render(StatusBar);
    expect(screen.getByText("no file open")).toBeTruthy();
  });

  it("reflects watch connection state (Ruling 40)", async () => {
    render(StatusBar);
    expect(screen.getByText(/watch: disconnected/)).toBeTruthy();

    editor.watchConnected = true;
    flushSync();
    expect(screen.getByText(/watch: connected/)).toBeTruthy();
  });

  it("does not render any of the error/override fields -- those live in StatusStrip now", () => {
    editor.error = { op: "save", message: "should-not-appear-here" };
    render(StatusBar);
    expect(screen.queryByText(/should-not-appear-here/)).toBeNull();
  });
});
