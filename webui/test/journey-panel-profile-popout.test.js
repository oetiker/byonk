import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * Changing the Panel Profile, and the pop-out following it.
 *
 * The three pieces are each covered and the chain between them is not.
 * `simulatecontrols.test.js` drives the select and asserts the `simulate`
 * store; `simulate.test.js` asserts the request `request()` builds; and
 * `preview.test.js` asserts the pop-out's URL and its hello handshake. Nobody
 * has changed the Panel Profile in the real shell and checked what the
 * SECOND window is then shown.
 *
 * It matters because the pop-out is fed by a single broadcast carrying
 * `{url, width, height}` (Ruling 57): the size travels with the render, so a
 * panel change that re-renders without re-broadcasting -- or broadcasts a
 * fresh image with the PREVIOUS dimensions -- leaves the pop-out drawing the
 * new picture at the old size, with nothing on screen to explain it. That is
 * Finding J's family exactly: two correct halves and a wrong result.
 */

/** A BroadcastChannel that records what the app sends the pop-out. */
function captureChannel() {
  const posted = [];
  const fake = {
    postMessage: (m) => posted.push(m),
    close() {},
    onmessage: null,
  };
  vi.spyOn(globalThis, "BroadcastChannel").mockImplementation(function () {
    return fake;
  });
  return { posted, fake };
}

/** A render server that answers every request with a distinguishable blob. */
function stubRenders() {
  const bodies = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init) => {
    bodies.push(JSON.parse(init.body));
    return { ok: true, blob: async () => new Blob([String(url)], { type: "image/png" }) };
  });
  return bodies;
}

const PANELS_META = [
  { id: "og", name: "TRMNL OG", width: 800, height: 480 },
  { id: "x", name: "TRMNL X", width: 1600, height: 1200 },
];

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset();
  theme.reset();
  auth.reset();
  editor.reset();
  editor.files = [];
  simulate.reset();
  simulate.error = null;
  subject.reset();
  screens.repos = [];
  screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }],
    devices: [],
    panels: PANELS_META,
    dither_algorithms: ["floyd-steinberg"],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "-- lua", etag: '"1"' });
});

afterEach(() => {
  cleanup();
});

/** The Preview header's "render as" select. */
function panelSelect() {
  return document.querySelector("#preview-target");
}

async function mountWithScreen() {
  render(AppShell);
  await waitFor(() => expect(screens.repos).toHaveLength(1));
  subject.select("screen", "local/a");
  await waitFor(() => expect(subject.screenRef).toBe("local/a"));
  await tick();
}

describe("a Panel Profile change, all the way to the pop-out", () => {
  it("the select offers every panel the server reported", async () => {
    await mountWithScreen();
    const sel = panelSelect();
    expect(sel).toBeTruthy();
    const values = [...sel.options].map((o) => o.value);
    for (const p of PANELS_META) expect(values).toContain(`panel:${p.id}`);
  });

  it("changing the panel re-renders at the new size and tells the pop-out the new size too", async () => {
    const { posted } = captureChannel();
    const bodies = stubRenders();
    await mountWithScreen();

    // The default: no panel chosen, so 800x480 (simulate.svelte.js).
    await waitFor(() => expect(posted.length).toBeGreaterThan(0));
    expect(posted.at(-1)).toMatchObject({ width: 800, height: 480 });

    const before = posted.length;
    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);

    // A fresh render went out, and it asked for the new panel's size.
    await waitFor(() => expect(posted.length).toBeGreaterThan(before));
    expect(bodies.at(-1)).toMatchObject({ panel: "x", width: 1600, height: 1200 });

    // And the pop-out was told the new size, not the old one. The width and
    // height travel WITH the image in a single message, so a stale pair here
    // means the second window draws the new render at the old dimensions.
    expect(posted.at(-1)).toMatchObject({ width: 1600, height: 1200 });
    expect(posted.at(-1).url).toBeTruthy();
  });

  it("the in-app preview header reports the new size as well", async () => {
    captureChannel();
    stubRenders();
    await mountWithScreen();

    const sizeText = () => document.querySelector('[data-testid="preview-size"]').textContent;
    expect(sizeText()).toBe("800 × 480");

    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);

    await waitFor(() => expect(sizeText()).toBe("1600 × 1200"));
  });

  // The device frame is chosen from the REQUESTED size (>1000px on either
  // axis gets the "x" bezel), so it has to move with the panel too -- a
  // 1600x1200 render inside the small OG bezel is a visible mismatch.
  it("the device frame follows the panel across the 1000px boundary", async () => {
    captureChannel();
    stubRenders();
    await mountWithScreen();

    const frame = () => document.querySelector(".device-frame");
    expect(frame().className).toContain("og");

    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);
    await waitFor(() => expect(frame().className).toContain("x"));

    sel.value = "panel:og";
    await fireEvent.change(sel);
    await waitFor(() => expect(frame().className).toContain("og"));
  });

  // Spec §8.4: a pop-out that reloads says hello and must recover the CURRENT
  // frame -- which, after a panel change, means the current size as well.
  it("a pop-out that says hello after a panel change gets the new size", async () => {
    const { posted, fake } = captureChannel();
    stubRenders();
    await mountWithScreen();

    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);
    await waitFor(() => expect(posted.at(-1)).toMatchObject({ width: 1600, height: 1200 }));

    const before = posted.length;
    fake.onmessage({ data: { hello: true } });

    expect(posted.length).toBe(before + 1);
    expect(posted.at(-1)).toMatchObject({ width: 1600, height: 1200 });
  });

  // Ruling 29: there is nothing to recover before the first render, and
  // answering hello with a null url would blank a pop-out that already has a
  // picture.
  it("a hello before any render is answered with nothing, not with a blank frame", async () => {
    const { posted, fake } = captureChannel();
    stubRenders();
    render(AppShell);
    await tick();

    fake.onmessage({ data: { hello: true } });
    expect(posted).toHaveLength(0);
  });

  // `postFrame()` reads `simulate.size` LIVE, at the moment it posts -- not
  // the size the render in hand was actually made with. That is safe only
  // because a superseded render returns at the `renderSeq` guard BEFORE it
  // can post (the Finding J fix). Without that ordering, a render started
  // under the old panel and answered after the change would broadcast the
  // OLD image tagged with the NEW dimensions, and the pop-out would draw a
  // 800x480 picture as though it were 1600x1200.
  //
  // Found while mutation-testing this file: with the re-render suppressed,
  // exactly that mismatched pair reached the channel.
  it("a render superseded by a panel change posts nothing at all, so no frame is mis-sized", async () => {
    const { posted } = captureChannel();

    // Hold the first render open so the panel can change while it is in
    // flight, then let it answer.
    let releaseFirst;
    const firstInFlight = new Promise((r) => (releaseFirst = r));
    let call = 0;
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url) => {
      call += 1;
      if (call === 1) await firstInFlight;
      return { ok: true, blob: async () => new Blob([String(url)], { type: "image/png" }) };
    });

    await mountWithScreen();
    await waitFor(() => expect(call).toBeGreaterThan(0));
    expect(posted).toHaveLength(0); // the first render has not answered yet

    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);
    await waitFor(() => expect(call).toBeGreaterThan(1));

    // The second render answers immediately and is broadcast.
    await waitFor(() => expect(posted).toHaveLength(1));
    expect(posted[0]).toMatchObject({ width: 1600, height: 1200 });

    // Now let the superseded one answer. It must add NOTHING. Counting the
    // posts is the assertion that matters, not their dimensions: the stale
    // frame would carry the CURRENT 1600x1200 (postFrame reads the size
    // live), so a dimension check alone cannot tell the two apart -- it is
    // the old image wearing the new size.
    releaseFirst();
    await new Promise((r) => setTimeout(r, 20));
    expect(posted).toHaveLength(1);
  });

  // The pop-out button must survive a panel change: it is opened by name so
  // that a second click reuses the same window rather than stacking one per
  // click, and relative so it stays inside the HA Ingress iframe.
  it("Pop out still opens the same named, relative window after a panel change", async () => {
    captureChannel();
    stubRenders();
    const open = vi.spyOn(window, "open").mockReturnValue({ closed: false });
    await mountWithScreen();

    const sel = panelSelect();
    sel.value = "panel:x";
    await fireEvent.change(sel);
    await tick();

    await fireEvent.click(dom.getByRole("button", { name: /pop ?out/i }));
    await fireEvent.click(dom.getByRole("button", { name: /pop ?out/i }));

    expect(open).toHaveBeenCalledTimes(2);
    for (const [url, name] of open.mock.calls) {
      expect(url).toBe("preview.html");
      expect(name).toBe("byonk-preview");
    }
  });
});
