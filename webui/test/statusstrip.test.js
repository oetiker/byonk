import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, cleanup, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import StatusStrip from "../src/lib/components/StatusStrip.svelte";
import { layout } from "../src/lib/state/layout.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import * as api from "../src/lib/api.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";

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
  auth.reset();
});

afterEach(() => {
  cleanup();
});

beforeEach(() => {
  configDraft.reset();
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
});

function closeEditor() {
  if (layout.isOpen("editor")) layout.toggle("editor");
  expect(layout.isOpen("editor")).toBe(false);
}

describe("the error/override strip is visible independent of the Editor panel", () => {
  it("shows editor.error and screens.error and simulate.error and auth.error with the Editor panel CLOSED, none clobbering another", async () => {
    closeEditor();
    // Keep AppShell's own whoami from failing later and replacing auth.error.
    vi.spyOn(api, "whoami").mockReturnValue(new Promise(() => {}));

    render(AppShell);
    // Let AppShell's own loads (whoami, repos) fail and settle first: they
    // write screens.error / auth.error, which would replace ours mid-test.
    await new Promise((r) => setTimeout(r, 0));
    // Set AFTER mount, unlike the other three fields above: AppShell's own
    // Task-1 effect calls `editor.loadScreen(subject.screenRef, ...)` on
    // mount, and `loadScreen()` starts with `editor.reset()` (screens.error/
    // simulate.error/auth.error live on other stores untouched by that
    // reset -- fix round 2 moved `loadScreen()`'s own `screens.error = null`
    // below its `!ref` early return specifically so a mount with nothing
    // selected does not reach into another store's state at all). Setting
    // editor.error before mount would have it wiped by that same mount,
    // which is not what this test is about.
    editor.error = { op: "save", message: "save-msg" };
    // auth.load() (AppShell) fails in jsdom and would replace auth.error,
    // so it is set after mount as well.
    auth.error = { op: "logout", message: "auth-msg" };
    screens.error = { op: "delete", message: "screens-msg" };
    simulate.error = { op: "render", message: "simulate-msg" };
    await tick();

    expect(screen.queryByRole("tablist")).toBeNull();
    // One line shows the most important message plus a count; opening the
    // panel lists all of them.
    expect(screen.getByText(/save-msg/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "+3" }));
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
    configDraft.reset();
    screens.error = null;
    simulate.error = null;
    });

  afterEach(() => {
    cleanup();
  });

  it("truncates a long message, full text in the tooltip, expandable by button; a new message starts collapsed (U12)", async () => {
    const long = "x".repeat(300);
    editor.error = { op: "save", message: long };
    render(StatusStrip);
    const btn = screen.getByRole("button", { name: new RegExp(long) });
    expect(btn.getAttribute("title")).toBe(`save: ${long}`);
    expect(btn.className.split(/\s+/)).toContain("truncate");
    await fireEvent.click(btn);
    const panel = screen.getByTestId("status-panel");
    expect(panel.className.split(/\s+/)).toContain("bottom-full");
    expect(panel.textContent).toContain(long);
    editor.error = { op: "save", message: "other" };
    await tick();
    expect(screen.queryByTestId("status-panel")).toBeNull();
    expect(screen.getByRole("button", { name: /other/ }).getAttribute("aria-expanded")).toBe("false");
  });

  it("a message set that returns after a change comes back collapsed (U24)", async () => {
    editor.error = { op: "save", message: "first" };
    render(StatusStrip);
    await fireEvent.click(screen.getByRole("button", { name: /first/ }));
    expect(screen.getByTestId("status-panel")).toBeTruthy();
    editor.error = { op: "save", message: "second" };
    await tick();
    editor.error = { op: "save", message: "first" };
    await tick();
    expect(screen.queryByTestId("status-panel")).toBeNull();
  });

  it("'+N' says what it controls and the panel closes on Escape and on an outside click (U24)", async () => {
    editor.error = { op: "save", message: "one" };
    screens.error = { op: "list", message: "two" };
    render(StatusStrip);
    const more = screen.getByRole("button", { name: "+1" });
    expect(more.getAttribute("aria-expanded")).toBe("false");
    expect(more.getAttribute("aria-controls")).toBe("status-panel");
    await fireEvent.click(more);
    const panel = screen.getByTestId("status-panel");
    expect(panel.id).toBe("status-panel");
    expect(screen.getByRole("button", { name: /Hide 2 messages/ }).getAttribute("aria-controls")).toBe("status-panel");

    await fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("status-panel")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "+1" }));
    expect(screen.getByTestId("status-panel")).toBeTruthy();
    // A click inside the panel keeps it open; one elsewhere closes it.
    await fireEvent.pointerDown(screen.getByTestId("status-panel"));
    expect(screen.getByTestId("status-panel")).toBeTruthy();
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId("status-panel")).toBeNull();
  });

  it("keeps a fixed-height bar when idle (U12)", () => {
    render(StatusStrip);
    expect(screen.getByTestId("status-bar").className.split(/\s+/)).toContain("h-(--status-h)");
  });

  it("shows no text when there is nothing to report", () => {
    const { container } = render(StatusStrip);
    expect(container.textContent.trim()).toBe("");
  });

  it("renders all four error surfaces at once, none clobbering another", async () => {
    editor.error = { op: "save", message: "save-msg" };
    screens.error = { op: "delete", message: "screens-msg" };
    simulate.error = { op: "render", message: "simulate-msg" };
    auth.error = { op: "logout", message: "auth-msg" };

    render(StatusStrip);

    expect(screen.getByText(/save-msg/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "+3" }));
    expect(screen.getByText(/screens-msg/)).toBeTruthy();
    expect(screen.getByText(/simulate-msg/)).toBeTruthy();
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
  it("lists a live override from before a reload, and its Discard clears it", async () => {
    configDraft.live = { panels: [{ key: "e1004", values: {}, expires_in_secs: 100 }], devices: [], tuning_sessions: [] };
    const discard = vi.spyOn(configDraft, "discardLive").mockResolvedValue();
    render(StatusStrip);
    expect(screen.getByRole("status").textContent).toContain("◉ Live on e1004");
    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(discard).toHaveBeenCalledWith("panel", "e1004");
  });

  it("lists a live device override too", async () => {
    configDraft.live = { panels: [], devices: [{ key: "kitchen", values: {}, expires_in_secs: 100 }], tuning_sessions: [] };
    const discard = vi.spyOn(configDraft, "discardLive").mockResolvedValue();
    render(StatusStrip);
    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(discard).toHaveBeenCalledWith("device", "kitchen");
  });

  it("renders configDraft.error as its own surface", () => {
    configDraft.error = { op: "save", message: "409 Conflict" };
    render(StatusStrip);
    expect(screen.getByRole("alert").textContent.trim()).toBe("config save: 409 Conflict");
  });
});
