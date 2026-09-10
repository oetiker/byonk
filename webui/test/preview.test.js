import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import PreviewPane from "../src/lib/components/PreviewPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";

beforeEach(() => {
  // No project-wide auto-cleanup is configured (see filespane.test.js,
  // iconrail.test.js) -- without this, a later test's render sees an
  // earlier test's mounted PreviewPane (and its onMount side effects) still
  // in the DOM.
  cleanup();
  vi.restoreAllMocks();
  // `screens` and `simulate` are module singletons: reset the fields tests
  // below mutate so one test's selection, params or captured error can't
  // leak into the next (same reasoning as filespane.test.js).
  screens.selected = null;
  simulate.reset();
  simulate.error = null;
});

describe("PreviewPane", () => {
  it("opens the pop-out at a relative URL, in a named window", async () => {
    // Proven under HA Ingress on 2026-09-09. An origin-absolute URL here
    // leaves the HA iframe; an unnamed window stacks a new one per click.
    const open = vi.spyOn(window, "open").mockReturnValue({ closed: false });
    render(PreviewPane);
    await fireEvent.click(screen.getByRole("button", { name: /pop ?out/i }));

    const [url, name] = open.mock.calls[0];
    expect(url.startsWith("/")).toBe(false);
    expect(url.startsWith("http")).toBe(false);
    expect(url).toBe("preview.html");
    expect(name).toBe("byonk-preview");
  });

  it("replies to the pop-out's hello handshake with the current render", async () => {
    // Spec §8.4: a reloaded pop-out must recover, not sit blank.
    //
    // Controller Ruling 29: the brief's original version fired `hello`
    // immediately after `render(PreviewPane)`, before any render had
    // completed -- `lastBlob` is still null at that point, so the
    // component's (correct) `if (e.data?.hello && lastBlob)` guard posts
    // nothing and the assertion could never pass. Fixed here by mocking
    // `fetch` and selecting a screen so one render actually completes
    // first, THEN sending `hello`.
    const posted = [];
    const fake = { postMessage: (m) => posted.push(m), close() {}, onmessage: null };
    // `new BroadcastChannel(...)` requires the mock to be usable as a
    // constructor -- an arrow-function implementation (as in the brief)
    // throws "is not a constructor" the moment the component calls `new`
    // on it (confirmed: this is exactly what happened on the first run of
    // this test, before switching to `function` here; see the report).
    vi.spyOn(globalThis, "BroadcastChannel").mockImplementation(function () {
      return fake;
    });
    const blob = new Blob(["fake-png-bytes"], { type: "image/png" });
    vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: true, blob: async () => blob });
    screens.selected = "local/a";

    render(PreviewPane);
    // Wait for the mount-time $effect's renderNow() to finish and post the
    // render itself, before touching the handshake.
    await waitFor(() => expect(posted.length).toBeGreaterThan(0));
    posted.length = 0;

    fake.onmessage?.({ data: { hello: true } });
    expect(posted.some((m) => "blob" in m || "url" in m)).toBe(true);
  });

  it("fits the device to its box without distorting the aspect ratio", () => {
    // Controller Ruling 30: jsdom never runs Tailwind, so
    // `getComputedStyle(img).objectFit` cannot reflect the `object-contain`
    // class. Verified directly: jsdom reports the CSS *initial* value,
    // "fill", for every `<img>` regardless of its class list (checked with
    // and without the class present, and via both the camelCase accessor
    // and `getPropertyValue("object-fit")` -- all four say "fill"). That is
    // even less permissive than Ruling 30's framing ("always ''") but the
    // same conclusion: the brief's `expect(["contain", ""]).toContain(fit)`
    // cannot tell a correct implementation from an incorrect one -- "fill"
    // is in neither branch, so with the real component it would now FAIL
    // outright rather than pass vacuously. Assert the class itself instead,
    // which jsdom does expose accurately; the true aspect-ratio check (an
    // upright E1004 at 1200x1600 must not be squashed) moves to Task 14's
    // browser list.
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    expect(img.className).toContain("object-contain");
  });

  // --- Controller Ruling 41: renderNow() must catch, not swallow --------

  it("captures an invalid-JSON render failure on simulate.error", async () => {
    // `simulate.url()` deliberately THROWS on invalid params JSON (Task 8
    // rule (e), simulate.svelte.js) so the user is never shown a render
    // made from parameters they didn't write. The brief called it inside
    // an `$effect` with no try/catch, turning that throw into an unhandled
    // rejection -- the preview would just silently stop updating.
    screens.selected = "local/a";
    simulate.params = "{not json";

    render(PreviewPane);
    await waitFor(() => expect(simulate.error).not.toBeNull());
    expect(simulate.error.op).toBe("render");
    expect(simulate.error.message).toMatch(/Invalid JSON/);
  });

  it("captures a server-side render failure (RenderErrorResponse) on simulate.error", async () => {
    // The brief's `if (!res.ok) return;` silently swallowed this, even
    // though the server sends a real message back on failure:
    // `RenderErrorResponse { error, details }` (src/api/dev.rs:157-161).
    screens.selected = "local/a";
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: false,
      status: 400,
      statusText: "Bad Request",
      json: async () => ({ error: "boom", details: "bad params" }),
    });

    render(PreviewPane);
    await waitFor(() => expect(simulate.error).not.toBeNull());
    expect(simulate.error).toEqual({ op: "render", message: "boom: bad params" });
  });

  it("clears a stale render error on the next successful render", async () => {
    simulate.error = { op: "render", message: "a previous failure" };
    const blob = new Blob(["fake-png-bytes"], { type: "image/png" });
    vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: true, blob: async () => blob });
    screens.selected = "local/a";

    render(PreviewPane);
    await waitFor(() => expect(simulate.error).toBeNull());
  });
});
