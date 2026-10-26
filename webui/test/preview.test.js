import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import PreviewPane from "../src/lib/components/PreviewPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import * as api from "../src/lib/api.js";

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
  subject.reset();
  resolution.reset();
  configDraft.reset();
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
    subject.select("screen", "local/a");

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
      width: 800,
      height: 480,
    });
    posted.length = 0;

    // ...and again on the hello reply, since the two are built from the
    // same call in the component but that is an implementation detail this
    // test must not assume -- each wire message is asserted independently.
    fake.onmessage?.({ data: { hello: true } });
    expect(posted).toEqual([
      {
        url: expect.stringMatching(/^blob:/),
        width: 800,
        height: 480,
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
    subject.select("panel", "trmnl_x");
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
    subject.select("screen", "local/a");
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
    // `simulate.simulated()` deliberately THROWS on invalid params JSON (Task 8
    // rule (e), simulate.svelte.js) so the user is never shown a render
    // made from parameters they didn't write. The brief called it inside
    // an `$effect` with no try/catch, turning that throw into an unhandled
    // rejection -- the preview would just silently stop updating.
    subject.select("screen", "local/a");
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
    subject.select("screen", "local/a");
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
    subject.select("screen", "local/a");

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
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init) => {
      // A POST to the relative `api/render` (Ingress), carrying the ref in
      // its JSON body.
      expect(url).toBe("api/render");
      expect(init.method).toBe("POST");
      const ref = JSON.parse(init.body).screen;
      await new Promise((r) => (release[ref] = r));
      const blob = new Blob([ref]);
      refOf.set(blob, ref);
      return { ok: true, blob: async () => blob };
    });

    subject.select("screen", "local/slow");
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    await waitFor(() => expect(release["local/slow"]).toBeTypeOf("function"));

    // Selection moves on while that first render is still in flight.
    subject.select("screen", "local/fresh");
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
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init) => {
      const ref = JSON.parse(init.body).screen;
      await new Promise((r) => (release[ref] = r));
      if (ref === "local/slow") throw new Error("gone");
      return { ok: true, blob: async () => new Blob([ref]) };
    });

    subject.select("screen", "local/slow");
    render(PreviewPane);
    await waitFor(() => expect(release["local/slow"]).toBeTypeOf("function"));
    subject.select("screen", "local/fresh");
    await waitFor(() => expect(release["local/fresh"]).toBeTypeOf("function"));

    release["local/fresh"]();
    await waitFor(() => expect(simulate.error).toBeNull());
    release["local/slow"]();
    await new Promise((r) => setTimeout(r, 20));
    expect(simulate.error).toBeNull();
  });

  // --- Finding H: a cleared selection must clear the preview -------------

  it("clears the image, and tells the pop-out, when the selection is cleared", async () => {
    // Finding H. Deleting the selected screen sets `subject.screenRef` to
    // null; the preview kept painting the deleted screen's last render, and
    // so did the pop-out that mirrors it.
    const posted = [];
    const fake = { postMessage: (m) => posted.push(m), close() {}, onmessage: null };
    vi.spyOn(globalThis, "BroadcastChannel").mockImplementation(function () {
      return fake;
    });
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      blob: async () => new Blob(["png"], { type: "image/png" }),
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    await waitFor(() => expect(img.getAttribute("src")).toMatch(/^blob:/));

    posted.length = 0;
    subject.reset();
    await waitFor(() => expect(img.getAttribute("src")).toBeNull());
    expect(posted).toEqual([{ clear: true }]);

    // A reloaded pop-out asking for "the current render" gets none.
    posted.length = 0;
    fake.onmessage?.({ data: { hello: true } });
    expect(posted).toEqual([]);
  });

  it("drops a render that lands after the selection was cleared", async () => {
    // The same sequence guard as Finding J: a render still in flight when
    // the screen is deleted must not repaint it afterwards.
    let release;
    vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
      await new Promise((r) => (release = r));
      return { ok: true, blob: async () => new Blob(["png"]) };
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    await waitFor(() => expect(release).toBeTypeOf("function"));

    subject.reset();
    await new Promise((r) => setTimeout(r, 0));
    release();
    await new Promise((r) => setTimeout(r, 20));
    expect(img.getAttribute("src")).toBeNull();
  });
  it("writes a successful render's X-Byonk-Resolution into the shared store", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue({
      ok: true,
      headers: { get: () => '{"dither":{"effective":"atkinson","from":"screen","order":[],"rungs":{}}}' },
      blob: async () => new Blob(["png"]),
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(resolution.state).toBe("ok"));
    expect(resolution.setting("dither").from).toBe("screen");
    expect(resolution.subjectKey).toBe("screen:local/a");
  });

  it("a failed render marks the store failed, with the server's message", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue({
      ok: false, status: 500, statusText: "x",
      json: async () => ({ error: "Render error", details: "boom" }),
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(resolution.state).toBe("failed"));
    expect(resolution.failure).toBe("Render error: boom");
  });

  it("mode b renders the borrowed calibration screen on the subject panel, and as the watched device", async () => {
    screens.meta = {
      devices: [{ id: "kitchen", panel: "x", screen: "local/k" }],
      dither_algorithms: [],
      panels: [{ id: "x", name: "X", width: 1600, height: 1200 }],
    };
    const bodies = [];
    vi.spyOn(api, "renderPreview").mockImplementation(async (b) => {
      bodies.push(b);
      return { ok: true, headers: { get: () => null }, blob: async () => new Blob(["png"]) };
    });
    subject.select("panel", "x");
    render(PreviewPane);
    await waitFor(() => expect(bodies.length).toBeGreaterThan(0));
    expect(bodies.at(-1)).toMatchObject({ screen: "byonk-builtin/calibration/tone", panel: "x" });
    expect(bodies.at(-1).device).toBeUndefined();
    expect(bodies.at(-1).override).toBeUndefined();

    subject.watchDevice = "kitchen";
    await waitFor(() => expect(bodies.at(-1).device).toBe("kitchen"));
    expect(bodies.at(-1).panel).toBe("x");
  });
});

describe("PreviewPane render-in-progress indicator", () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const okResponse = () => ({ ok: true, headers: { get: () => null }, blob: async () => new Blob(["png"]) });
  // A render the test settles by hand.
  function deferred() {
    let resolve, reject;
    const promise = new Promise((res, rej) => ((resolve = res), (reject = rej)));
    return { promise, resolve, reject };
  }
  const region = () => screen.getByTestId("preview-region");
  const label = () => screen.queryByRole("status");

  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    subject.reset();
    screens.meta = { devices: [], dither_algorithms: [], panels: [] };
    configDraft.reset();
    simulate.reset();
    simulate.error = null;
    subject.select("screen", "local/a");
  });

  it("stays absent for a render that finishes under the delay", async () => {
    vi.spyOn(api, "renderPreview").mockImplementation(async () => {
      await wait(30);
      return okResponse();
    });
    render(PreviewPane);
    await wait(100);
    expect(label()).toBeNull();
    expect(region().getAttribute("aria-busy")).not.toBe("true");
    await wait(300);
    expect(label()).toBeNull();
  });

  it("shows after the delay while pending, and clears on resolve", async () => {
    const d = deferred();
    vi.spyOn(api, "renderPreview").mockReturnValue(d.promise);
    render(PreviewPane);
    await wait(100);
    expect(label()).toBeNull();
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    expect(label().textContent).toMatch(/Rendering/);
    expect(region().getAttribute("aria-busy")).toBe("true");
    d.resolve(okResponse());
    await waitFor(() => expect(label()).toBeNull());
    expect(region().getAttribute("aria-busy")).not.toBe("true");
  });

  it("clears when the render fails", async () => {
    const d = deferred();
    vi.spyOn(api, "renderPreview").mockReturnValue(d.promise);
    render(PreviewPane);
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    d.reject(new Error("boom"));
    await waitFor(() => expect(label()).toBeNull());
    expect(region().getAttribute("aria-busy")).not.toBe("true");
  });

  it("a stale response does not clear the indicator of a newer request", async () => {
    const first = deferred();
    const second = deferred();
    const spy = vi.spyOn(api, "renderPreview");
    // Re-selecting may start more than one follow-up render; all of them share `second`.
    spy.mockReturnValueOnce(first.promise).mockReturnValue(second.promise);
    render(PreviewPane);
    await wait(10);
    subject.select("screen", "local/b");
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    first.resolve(okResponse());
    await wait(50);
    expect(label()).not.toBeNull();
    expect(region().getAttribute("aria-busy")).toBe("true");
    second.resolve(okResponse());
    await waitFor(() => expect(label()).toBeNull());
  });

  it("stays visible without a gap when a newer request supersedes a visible one", async () => {
    const first = deferred();
    const second = deferred();
    const spy = vi.spyOn(api, "renderPreview");
    spy.mockReturnValueOnce(first.promise).mockReturnValue(second.promise);
    render(PreviewPane);
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    // Sample the DOM on every mutation: a gap of any length is a blink.
    let gaps = 0;
    const mo = new MutationObserver(() => {
      if (label() === null) gaps++;
    });
    mo.observe(region(), { childList: true, subtree: true, attributes: true });
    subject.select("screen", "local/b");
    await waitFor(() => expect(spy.mock.calls.length).toBeGreaterThanOrEqual(2));
    await wait(400); // longer than the 250 ms delay
    expect(gaps).toBe(0);
    expect(label()).not.toBeNull();
    first.resolve(okResponse());
    second.resolve(okResponse());
    await waitFor(() => expect(label()).toBeNull());
    mo.disconnect();
  });

  it("stays visible across back-to-back renders where the next starts just after the last ends", async () => {
    const ds = [];
    vi.spyOn(api, "renderPreview").mockImplementation(() => {
      const d = deferred();
      ds.push(d);
      return d.promise;
    });
    render(PreviewPane);
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    let gaps = 0;
    const mo = new MutationObserver(() => {
      if (label() === null) gaps++;
    });
    mo.observe(region(), { childList: true, subtree: true, attributes: true });
    // The latest render ends and a new one starts moments later (a burst of
    // renders is one slow wait to the user, not one per request).
    ds.forEach((d) => d.resolve(okResponse()));
    await wait(40);
    subject.select("screen", "local/b");
    await wait(400);
    expect(gaps).toBe(0);
    expect(label()).not.toBeNull();
    ds.forEach((d) => d.resolve(okResponse()));
    await waitFor(() => expect(label()).toBeNull());
    mo.disconnect();
  });

  it("shows in the empty frame when there is no previous image", async () => {
    vi.spyOn(api, "renderPreview").mockReturnValue(deferred().promise);
    render(PreviewPane);
    await waitFor(() => expect(label()).not.toBeNull(), { timeout: 1000 });
    expect(screen.getByRole("img", { name: /screen preview/i }).getAttribute("src")).toBeNull();
  });
});

describe("PreviewPane with the draft store (Task 7)", () => {
  const META = {
    panels: [
      { id: "e1004", name: "E1004", width: 800, height: 480, colors: "#000000,#FFFFFF", colors_actual: null, dither: null },
      { id: "x", name: "X", width: 1600, height: 1200, colors: "#000000,#FFFFFF", colors_actual: null, dither: null },
    ],
    devices: [
      { id: "kitchen", name: "Kitchen", screen: "local/a", panel: "e1004", params: {}, base: {}, final: null },
      { id: "attic", name: "Attic", screen: "local/a", panel: null, params: {}, base: {}, final: null },
    ],
    dither_algorithms: [],
  };

  function spyRender() {
    return vi.spyOn(api, "renderPreview").mockResolvedValue({
      ok: true,
      headers: { get: () => null },
      blob: async () => new Blob(["png"], { type: "image/png" }),
    });
  }
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));

  beforeEach(() => {
    cleanup();
    screens.meta = structuredClone(META);
    configDraft.reset();
  });

  it("a draft edit re-renders only after the debounce", async () => {
    const r = spyRender();
    subject.select("panel", "e1004");
    render(PreviewPane);
    await waitFor(() => expect(r).toHaveBeenCalledTimes(1));
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await wait(100);
    expect(r).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(r).toHaveBeenCalledTimes(2), { timeout: 1500 });
    expect(r.mock.calls[1][0].draft.panel.dither).toEqual({ strength: 0.5 });
  });

  it("a burst of draft edits renders once", async () => {
    const r = spyRender();
    subject.select("panel", "e1004");
    render(PreviewPane);
    await waitFor(() => expect(r).toHaveBeenCalledTimes(1));
    configDraft.set("panel", "e1004", "panel", "strength", 0.4);
    await wait(50);
    configDraft.set("panel", "e1004", "panel", "strength", 0.6);
    await waitFor(() => expect(r).toHaveBeenCalledTimes(2), { timeout: 1500 });
    await wait(450);
    expect(r).toHaveBeenCalledTimes(2);
    expect(r.mock.calls[1][0].draft.panel.dither).toEqual({ strength: 0.6 });
  });

  it("a subject switch renders at once, without waiting for the debounce", async () => {
    const r = spyRender();
    subject.select("panel", "e1004");
    render(PreviewPane);
    await waitFor(() => expect(r).toHaveBeenCalledTimes(1));
    subject.select("panel", "x");
    await waitFor(() => expect(r).toHaveBeenCalledTimes(2), { timeout: 150 });
    expect(r.mock.calls[1][0].panel).toBe("x");
  });

  it("a device without an explicit panel renders at the size of the panel it was sighted on", async () => {
    const r = spyRender();
    configDraft.usage = { panels: { x: ["attic"] }, unseen: [] };
    subject.select("device", "attic");
    render(PreviewPane);
    await waitFor(() => expect(r).toHaveBeenCalled());
    expect(r.mock.calls[0][0]).toMatchObject({ device: "attic", width: 1600, height: 1200 });
    expect(screen.getByTestId("preview-size").textContent).toContain("1600");
  });

  it("lists only the devices that use the panel under 'watch on', and drops a watched device that does not", async () => {
    spyRender();
    configDraft.usage = { panels: { e1004: ["kitchen"], x: ["attic"] }, unseen: [] };
    subject.watchDevice = "attic";
    subject.select("panel", "e1004");
    render(PreviewPane);
    const options = () => [...screen.getByLabelText("watch on").querySelectorAll("option")].map((o) => o.value);
    expect(options()).toEqual(["", "kitchen"]);
    await waitFor(() => expect(subject.watchDevice).toBe(""));
  });

  it("does not claim the screen sets its colours from a payload recorded for another subject", async () => {
    spyRender();
    resolution.record({ colors: { effective: "#000", from: "screen" } }, "screen:local/a");
    subject.select("panel", "e1004");
    render(PreviewPane);
    expect(screen.queryByTestId("screen-in-charge")).toBeNull();
    cleanup();
    resolution.record({ colors: { effective: "#000", from: "screen" } }, "panel:e1004");
    render(PreviewPane);
    expect(screen.queryByTestId("screen-in-charge")).not.toBeNull();
  });
});

describe("PreviewPane measured inks", () => {
  const blob = new Blob(["png"], { type: "image/png" });
  const answerWith = (actual) =>
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      blob: async () => blob,
      headers: { get: () => JSON.stringify({ colors_actual: { effective: actual, from: "fallback", rungs: {} } }) },
    });
  const box = () => screen.getByRole("checkbox", { name: /measured inks/i });

  it("toggling 'show measured inks' issues a new render carrying the new use_actual", async () => {
    const fetchSpy = answerWith("#111,#eee");
    subject.select("screen", "local/a");
    render(PreviewPane);
    const bodies = () => fetchSpy.mock.calls.map(([, init]) => JSON.parse(init.body).view.use_actual);
    await waitFor(() => expect(bodies()).toEqual([true]));
    await fireEvent.click(box());
    await waitFor(() => expect(bodies()).toEqual([true, false]));
    await fireEvent.click(box());
    await waitFor(() => expect(bodies()).toEqual([true, false, true]));
  });

  it("is disabled, and says why, when no measured colours resolve (the toggle could not change the picture)", async () => {
    answerWith(null);
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(box().disabled).toBe(true));
    expect(box().closest("label").getAttribute("title")).toMatch(/no measured colours/i);
  });

  it("stays enabled when measured colours resolve", async () => {
    answerWith("#111,#eee");
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(resolution.payload).not.toBe(null));
    expect(box().disabled).toBe(false);
  });
});

describe("PreviewPane tall render scrolls inside the region (U21)", () => {
  const okResponse = () => ({ ok: true, headers: { get: () => null }, blob: async () => new Blob(["png"]) });
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
    subject.reset();
    screens.meta = { devices: [], dither_algorithms: [], panels: [] };
    configDraft.reset();
    simulate.reset();
    simulate.error = null;
    subject.select("screen", "local/a");
  });
  const has = (el, cls) => el.classList.contains(cls);

  it("the region scrolls vertically, never horizontally, and clips to itself", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue(okResponse());
    const { container } = render(PreviewPane);
    const scroller = await screen.findByTestId("preview-scroll");
    expect(has(scroller, "overflow-y-auto")).toBe(true);
    expect(has(scroller, "overflow-x-hidden")).toBe(true);
    expect(screen.getByTestId("preview-region").contains(scroller)).toBe(true);
    expect(container.querySelector("img")).not.toBeNull();
  });

  it("the image is fitted to the width, not height-capped to the region", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue(okResponse());
    const { container } = render(PreviewPane);
    await screen.findByTestId("preview-scroll");
    const img = container.querySelector("img");
    expect(has(img, "max-w-full")).toBe(true);
    expect(has(img, "max-h-full")).toBe(false);
  });

  it("the frame uses auto block margins: centred when short, top-anchored when tall", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue(okResponse());
    const { container } = render(PreviewPane);
    await screen.findByTestId("preview-scroll");
    const frame = container.querySelector(".device-frame");
    expect(has(frame, "my-auto")).toBe(true);
    expect(has(frame, "shrink-0")).toBe(true);
    expect(has(screen.getByTestId("preview-scroll"), "items-center")).toBe(false);
  });

  it("the busy badge lives outside the scrolling content", async () => {
    let resolve;
    vi.spyOn(api, "renderPreview").mockReturnValue(new Promise((r) => (resolve = r)));
    render(PreviewPane);
    const badge = await screen.findByRole("status", {}, { timeout: 1000 });
    expect(screen.getByTestId("preview-scroll").contains(badge)).toBe(false);
    expect(screen.getByTestId("preview-region").contains(badge)).toBe(true);
    resolve(okResponse());
  });

  it("a re-render of the same screen keeps the same scroll container", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue(okResponse());
    render(PreviewPane);
    const scroller = await screen.findByTestId("preview-scroll");
    scroller.scrollTop = 120;
    simulate.use_actual = !simulate.use_actual;
    await new Promise((r) => setTimeout(r, 100));
    expect(screen.getByTestId("preview-scroll")).toBe(scroller);
  });
});
