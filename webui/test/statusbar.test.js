import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import * as api from "../src/lib/api.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { tune } from "../src/lib/state/tune.svelte.js";
import StatusBar from "../src/lib/components/StatusBar.svelte";

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  editor.reset();
  screens.error = null;
  simulate.error = null;
  tune.reset();
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

  // --- Rulings 20, 39, 41/43, tune amendment: render every captured error ---
  //
  // Each of these was captured by an earlier task onto a store field and
  // never read by anything -- a captured-but-never-displayed error is worse
  // than the unhandled rejection it replaced, because it looks handled.
  it("renders a save failure from editor.error (Ruling 39)", () => {
    editor.error = { op: "save", message: "disk full" };
    render(StatusBar);
    expect(screen.getByText(/disk full/)).toBeTruthy();
  });

  it("renders a failed file operation from screens.error (Ruling 20)", () => {
    screens.error = { op: "rename", message: "name already exists" };
    render(StatusBar);
    expect(screen.getByText(/rename/)).toBeTruthy();
    expect(screen.getByText(/name already exists/)).toBeTruthy();
  });

  it("renders a failed render from simulate.error (Rulings 41, 43)", () => {
    simulate.error = { op: "render", message: "bad panel id" };
    render(StatusBar);
    expect(screen.getByText(/bad panel id/)).toBeTruthy();
  });

  it("renders a failed tune operation from tune.error (tune amendment)", () => {
    tune.error = { op: "push", message: "server unreachable" };
    render(StatusBar);
    expect(screen.getByText(/push/)).toBeTruthy();
    expect(screen.getByText(/server unreachable/)).toBeTruthy();
  });

  it("renders all four error surfaces at once, none clobbering another", () => {
    editor.error = { op: "save", message: "save-msg" };
    screens.error = { op: "delete", message: "screens-msg" };
    simulate.error = { op: "render", message: "simulate-msg" };
    tune.error = { op: "save", message: "tune-msg" };

    render(StatusBar);

    expect(screen.getByText(/save-msg/)).toBeTruthy();
    expect(screen.getByText(/screens-msg/)).toBeTruthy();
    expect(screen.getByText(/simulate-msg/)).toBeTruthy();
    expect(screen.getByText(/tune-msg/)).toBeTruthy();
  });

  it("warns when a panel colour override is live and nobody may be watching (tune.pushed)", () => {
    tune.panel = "e1004";
    tune.pushed = true;

    render(StatusBar);

    expect(screen.getByText(/override live on e1004/)).toBeTruthy();
  });

  it("shows no override warning when nothing is pushed", () => {
    tune.pushed = false;
    render(StatusBar);
    expect(screen.queryByText(/override live/)).toBeNull();
  });
});
