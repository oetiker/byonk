import { describe, it, expect, beforeEach } from "vitest";
import { theme, CHANNEL_NAME } from "../src/lib/state/theme.svelte.js";

beforeEach(() => {
  localStorage.clear();
  document.documentElement.className = "";
  theme.reset();
});

describe("theme", () => {
  it("defaults to system", () => {
    expect(theme.mode).toBe("system");
  });

  it("puts .dark on the root for an explicit dark choice", () => {
    theme.set("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    theme.set("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("remembers the choice across a reload", () => {
    theme.set("dark");
    theme.reset();
    expect(theme.mode).toBe("system");
    theme.load();
    expect(theme.mode).toBe("dark");
  });

  it("announces the choice so the pop-out can follow", () => {
    const seen = [];
    theme.onBroadcast((m) => seen.push(m));
    theme.set("dark");
    expect(seen).toContain("dark");
  });

  // Ruling 51: the local `onBroadcast` seam above is not proof anything
  // reaches the pop-out -- it never touches a wire. This test listens on a
  // REAL BroadcastChannel, the same way static/dev/preview.html does, and
  // pins the actual on-the-wire shape: `{theme: mode}` on "byonk-preview".
  //
  // Mutation performed to confirm this can fail (then reverted): changed
  // `set()` to `this.#channel?.postMessage(mode)` (posting the bare string
  // instead of `{theme: mode}`). Result: this test failed with
  //   AssertionError: expected [ 'dark' ] to deeply equal [ { theme: 'dark' } ]
  // The `onBroadcast` test above stayed green throughout that mutation,
  // since it only inspects the local seam and never touches the wire --
  // exactly the blind spot this test exists to close.
  it("puts {theme: mode} on the real byonk-preview BroadcastChannel", async () => {
    const listener = new BroadcastChannel(CHANNEL_NAME);
    const received = [];
    const seen = new Promise((resolve) => {
      listener.onmessage = (e) => {
        received.push(e.data);
        resolve();
      };
    });

    theme.set("dark");
    await seen;
    listener.close();

    expect(received).toEqual([{ theme: "dark" }]);
  });
});
