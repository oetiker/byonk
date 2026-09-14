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
    // completed -- `blobUrl` is still null at that point, so the
    // component's (correct) `if (e.data?.hello && blobUrl)` guard posts
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

    // Ruling 57: this used to accept `"blob" in m || "url" in m`, which
    // reports green whether PreviewPane posts the real, only-ever-
    // understood-by-preview.html contract (`{url, width, height}`) or the
    // `{blob}` shape a rewrite regression actually shipped. Pin the exact
    // shape, on the render broadcast itself first...
    expect(posted[0]).toEqual({
      url: expect.stringMatching(/^blob:/),
      width: simulate.size.width,
      height: simulate.size.height,
    });
    posted.length = 0;

    // ...and again on the hello reply, since the two are built from the
    // same call in the component but that is an implementation detail this
    // test must not assume -- each wire message is asserted independently.
    fake.onmessage?.({ data: { hello: true } });
    expect(posted).toEqual([
      {
        url: expect.stringMatching(/^blob:/),
        width: simulate.size.width,
        height: simulate.size.height,
      },
    ]);
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

  // --- Controller Ruling 42: device frame + zoom are named spec ---------
  // --- deliverables (design doc:290), not just prose in the brief -------

  it("frames a >1000px panel with the \"x\" bezel class", () => {
    // Ported from the old UI's `updateDeviceFrame()` (static/dev/dev.js:262):
    // hi-res panels get the "x" bezel, everything else gets "og".
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ id: "trmnl_x", width: 1872, height: 1404 }],
    };
    simulate.panel = "trmnl_x";
    const { container } = render(PreviewPane);
    const frame = container.querySelector(".device-frame");
    expect(frame.classList.contains("x")).toBe(true);
    expect(frame.classList.contains("og")).toBe(false);
  });

  it("frames the 800x480 default with the \"og\" bezel class", () => {
    const { container } = render(PreviewPane);
    const frame = container.querySelector(".device-frame");
    expect(frame.classList.contains("og")).toBe(true);
    expect(frame.classList.contains("x")).toBe(false);
  });

  it("shows the 1:1 pixel lens on hover, but only once a render has landed", async () => {
    // Mirrors `lens.js`'s own guard against magnifying nothing -- keyed off
    // `blobUrl` rather than `img.naturalWidth`/`.complete` (what `lens.js`
    // checks), because jsdom never populates either for a `blob:` URL
    // (confirmed directly: both stay 0/false indefinitely no matter how
    // long you wait -- see the report). `blobUrl` is this component's own
    // "a render actually completed" signal, and the only readiness check a
    // test here can drive.
    const { container } = render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });

    // No render has happened yet: hovering shows nothing.
    await fireEvent.mouseEnter(img);
    expect(container.querySelector(".lens")).toBeNull();

    const blob = new Blob(["fake-png-bytes"], { type: "image/png" });
    vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: true, blob: async () => blob });
    screens.selected = "local/a";
    await waitFor(() => expect(img.src).toMatch(/^blob:/));

    // Now that a render has landed, hovering shows the lens...
    await fireEvent.mouseEnter(img);
    expect(container.querySelector(".lens")).not.toBeNull();

    // ...and it goes away again on mouseleave.
    await fireEvent.mouseLeave(img);
    expect(container.querySelector(".lens")).toBeNull();
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

  // --- Finding J: out-of-order render responses -------------------------

  it("ignores a render response that arrives after a newer render started", async () => {
    // Finding J. `renderNow()` writes whatever comes back, so two renders in
    // flight are resolved by WHICHEVER SERVER ANSWER LANDS LAST -- not by
    // which screen is selected. A screen that fetches real data is slower
    // than a freshly created starter, so after New the old screen's render
    // routinely lands second and paints over the new one, permanently.
    //
    // This fails without a sequence guard in `renderNow()`, and only that
    // guard makes it pass: nothing else in the component knows that an
    // in-flight render has been superseded.
    //
    // `img.src` is an opaque `blob:` URL, so map each Blob back to the ref
    // it was fetched for and let `createObjectURL` report that instead.
    const refOf = new WeakMap();
    vi.spyOn(URL, "createObjectURL").mockImplementation((b) => `blob:${refOf.get(b)}`);
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});

    // Each render is held open until its ref is released, so the two
    // responses can be landed in a chosen order with no timing guesswork.
    const release = {};
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url) => {
      const ref = new URL(url, "http://localhost/").searchParams.get("screen");
      await new Promise((r) => (release[ref] = r));
      const blob = new Blob([ref]);
      refOf.set(blob, ref);
      return { ok: true, blob: async () => blob };
    });

    screens.selected = "local/slow";
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    await waitFor(() => expect(release["local/slow"]).toBeTypeOf("function"));

    // Selection moves on while that first render is still in flight.
    screens.selected = "local/fresh";
    await waitFor(() => expect(release["local/fresh"]).toBeTypeOf("function"));

    // The newly selected screen answers first...
    release["local/fresh"]();
    await waitFor(() => expect(img.src).toBe("blob:local/fresh"));

    // ...and the superseded one answers second. It must not win.
    release["local/slow"]();
    await new Promise((r) => setTimeout(r, 20));
    expect(img.src).toBe("blob:local/fresh");
  });

  it("does not surface a render failure that belongs to a superseded render", async () => {
    // Same guard, other branch: a stale render that fails must not raise an
    // error banner over a newer render that succeeded.
    const release = {};
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url) => {
      const ref = new URL(url, "http://localhost/").searchParams.get("screen");
      await new Promise((r) => (release[ref] = r));
      if (ref === "local/slow") throw new Error("gone");
      return { ok: true, blob: async () => new Blob([ref]) };
    });

    screens.selected = "local/slow";
    render(PreviewPane);
    await waitFor(() => expect(release["local/slow"]).toBeTypeOf("function"));
    screens.selected = "local/fresh";
    await waitFor(() => expect(release["local/fresh"]).toBeTypeOf("function"));

    release["local/fresh"]();
    await waitFor(() => expect(simulate.error).toBeNull());
    release["local/slow"]();
    await new Promise((r) => setTimeout(r, 20));
    expect(simulate.error).toBeNull();
  });
});
