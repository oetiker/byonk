import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, cleanup, screen } from "@testing-library/svelte";
import { flushSync, tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import StatusStrip from "../src/lib/components/StatusStrip.svelte";
import { layout } from "../src/lib/state/layout.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { tune } from "../src/lib/state/tune.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";

// Ruling 49 (review finding 2): editor.error/screens.error/simulate.error/
// tune.error and tune.pushed used to render only inside StatusBar, and
// StatusBar was mounted only inside EditorPane -- {#if layout.isOpen
// ("editor")} in AppShell.svelte UNMOUNTS EditorPane (and everything inside
// it) the moment the Editor panel is toggled closed. All five signals went
// dark together. The decisive spec requirement is
// docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md §8: "An
// active override must be visible from outside the Tune panel... StatusBar
// shows that an override is live and for which panel" -- a surface reachable
// only by ALSO opening the Editor does not satisfy "from outside the Tune
// panel", least of all when the Editor is the panel least likely to be open
// during a colour-tuning session.
//
// This test is mounted at the level that matters -- the real AppShell, with
// the Editor panel closed -- specifically so it fails against the old
// EditorPane-only arrangement and only passes once the four error fields
// and tune.pushed live in something AppShell always renders.
beforeEach(() => {
  cleanup();
  localStorage.clear();
  layout.reset();
  editor.reset();
  screens.error = null;
  simulate.error = null;
  tune.reset();
  auth.reset();
});

afterEach(() => {
  cleanup();
});

function closeEditor() {
  if (layout.isOpen("editor")) layout.toggle("editor");
  expect(layout.isOpen("editor")).toBe(false);
}

describe("the error/override strip is visible independent of the Editor panel", () => {
  it("shows a live tune.pushed override, naming its panel, with the Editor panel CLOSED", async () => {
    closeEditor();
    tune.panel = "e1004";
    tune.pushed = true;

    render(AppShell);
    await tick();

    // Confirm the Editor really is closed -- EditorPane is the only thing
    // that renders role="tablist" -- so a pass here can't be explained by
    // the Editor having mounted anyway.
    expect(screen.queryByRole("tablist")).toBeNull();

    expect(screen.getByText(/override live on e1004/)).toBeTruthy();
  });

  it("shows a live tune.error with the Editor panel CLOSED", async () => {
    closeEditor();
    tune.error = { op: "push", message: "server unreachable" };

    render(AppShell);
    await tick();

    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByText(/push/)).toBeTruthy();
    expect(screen.getByText(/server unreachable/)).toBeTruthy();
  });

  it("shows editor.error and screens.error and simulate.error and auth.error with the Editor panel CLOSED, none clobbering another", async () => {
    closeEditor();
    screens.error = { op: "delete", message: "screens-msg" };
    simulate.error = { op: "render", message: "simulate-msg" };
    auth.error = { op: "logout", message: "auth-msg" };

    render(AppShell);
    await tick();
    // Set AFTER mount, unlike the other three fields above: AppShell's own
    // Task-1 effect calls `editor.loadScreen(screens.selected, ...)` on
    // mount, and `loadScreen()` starts with `editor.reset()` (screens.error/
    // simulate.error/auth.error live on other stores untouched by that
    // reset -- fix round 2 moved `loadScreen()`'s own `screens.error = null`
    // below its `!ref` early return specifically so a mount with nothing
    // selected does not reach into another store's state at all). Setting
    // editor.error before mount would have it wiped by that same mount,
    // which is not what this test is about.
    editor.error = { op: "save", message: "save-msg" };
    await tick();

    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByText(/save-msg/)).toBeTruthy();
    expect(screen.getByText(/screens-msg/)).toBeTruthy();
    expect(screen.getByText(/simulate-msg/)).toBeTruthy();
    expect(screen.getByText(/auth-msg/)).toBeTruthy();
  });

  // Ruling 66: restoring Logout (Task 13a) created a fifth error owner
  // (auth.error, from a failed whoami or a failed logout) and left it with
  // no surface at all -- worse than hidden, since a failed logout with no
  // visible consequence reads to the user as a session that ended when it
  // did not. Same requirement as the four fields above: visible with the
  // Editor panel closed, since a failed Logout click (mounted in IconRail,
  // reachable in every panel combination) has nothing to do with the
  // Editor panel being open.
  it("shows a live auth.error with the Editor panel CLOSED", async () => {
    closeEditor();
    auth.error = { op: "logout", message: "session did not end" };

    render(AppShell);
    await tick();

    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByText(/logout/)).toBeTruthy();
    expect(screen.getByText(/session did not end/)).toBeTruthy();
  });

  it("renders nothing extra when there is nothing to report (no empty bar eating height)", async () => {
    closeEditor();
    render(AppShell);
    await tick();

    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
  });
});

// Component-level coverage of StatusStrip's own rendering logic, faster and
// more precise than driving it through the full AppShell.
describe("StatusStrip", () => {
  beforeEach(() => {
    cleanup();
    editor.reset();
    screens.error = null;
    simulate.error = null;
    tune.reset();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders nothing when there is nothing to report", () => {
    const { container } = render(StatusStrip);
    expect(container.textContent.trim()).toBe("");
  });

  it("renders all five error surfaces at once, none clobbering another", () => {
    editor.error = { op: "save", message: "save-msg" };
    screens.error = { op: "delete", message: "screens-msg" };
    simulate.error = { op: "render", message: "simulate-msg" };
    tune.error = { op: "save", message: "tune-msg" };
    auth.error = { op: "logout", message: "auth-msg" };

    render(StatusStrip);

    expect(screen.getByText(/save-msg/)).toBeTruthy();
    expect(screen.getByText(/screens-msg/)).toBeTruthy();
    expect(screen.getByText(/simulate-msg/)).toBeTruthy();
    expect(screen.getByText(/tune-msg/)).toBeTruthy();
    expect(screen.getByText(/auth-msg/)).toBeTruthy();
  });

  // Every fixture in this file used to set `op: "save"` on editor.error, and
  // the component hard-coded a "save:" label -- so a failed Reload told the
  // user a save had failed, and no test could see it. `editor.error` is
  // raised by four different operations; all four must name themselves.
  it.each(["save", "reload", "overwrite", "watch-reload"])(
    "labels editor.error with its own op (%s), not always 'save'",
    (op) => {
      editor.error = { op, message: "network down" };
      render(StatusStrip);
      expect(screen.getByText(new RegExp(`${op}: network down`))).toBeTruthy();
    },
  );

  it("renders auth.error, labelled with its own op", () => {
    auth.error = { op: "logout", message: "network down" };
    render(StatusStrip);
    expect(screen.getByText(/logout: network down/)).toBeTruthy();
  });

  it("shows no auth error when nothing failed", () => {
    render(StatusStrip);
    expect(screen.queryByText(/logout/)).toBeNull();
  });

  // Minor from the review: StatusBar.svelte used to hardcode "render:"
  // instead of using simulate.error.op the way the other three blocks use
  // theirs. Trivial to fix while already in this file for the move --
  // taken, per the review's own instruction to take a trivial minor when
  // already touching the file.
  it("labels a simulate error with its own op, not a hardcoded 'render:'", () => {
    simulate.error = { op: "not-render", message: "whatever" };
    render(StatusStrip);
    expect(screen.getByText(/not-render: whatever/)).toBeTruthy();
  });

  it("warns when a panel colour override is live, naming the panel (spec §8)", () => {
    tune.panel = "e1004";
    tune.pushed = true;

    render(StatusStrip);

    expect(screen.getByText(/override live on e1004/)).toBeTruthy();
  });

  it("shows no override warning when nothing is pushed", () => {
    tune.pushed = false;
    render(StatusStrip);
    expect(screen.queryByText(/override live/)).toBeNull();
  });

  it("appears and disappears reactively as tune.pushed changes", () => {
    render(StatusStrip);
    expect(screen.queryByRole("status")).toBeNull();

    tune.pushed = true;
    flushSync();
    expect(screen.queryByRole("status")).not.toBeNull();

    tune.pushed = false;
    flushSync();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
