import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor, within } from "@testing-library/svelte";
import LibraryPane from "../src/lib/components/LibraryPane.svelte";
import PreviewPane from "../src/lib/components/PreviewPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import * as api from "../src/lib/api.js";

/**
 * The New-screen journey, across the LibraryPane/PreviewPane seam.
 *
 * Every defect found on this branch so far lived BETWEEN two components that
 * each had a green suite of their own: LibraryPane's tests assert the selection
 * moves, PreviewPane's assert it renders what is selected, and Finding J sat
 * in the gap -- the selection moved, the render fired, and the preview still
 * showed the previous screen because an older in-flight render answered last.
 * This file owns that gap: it drives the real button and asserts what the
 * user actually sees at the end.
 */
beforeEach(async () => {
  cleanup();
  vi.restoreAllMocks();
  subject.reset();
  screens.error = null;
  simulate.reset();
  simulate.error = null;
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  await screens.refresh();
});

describe("New screen", () => {
  it("leaves the preview showing the new screen, not the one it replaced", async () => {
    // Finding J, as the user met it: with `local/a` selected, click New. The
    // create refreshes the store and then selects -- which starts a render of
    // `local/a` and a render of `local/fresh`. `local/a` runs a real script
    // and answers second; the starter answers first. The preview must follow
    // the SELECTION, so the late answer has to be discarded.
    const refOf = new WeakMap();
    vi.spyOn(URL, "createObjectURL").mockImplementation((b) => `blob:${refOf.get(b)}`);
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});

    // Every render is held open until its ref is released, so the answers can
    // be landed in a chosen order instead of raced against a timer.
    const release = {};
    vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init) => {
      const ref = JSON.parse(init.body).screen;
      await new Promise((r) => (release[ref] = r));
      const blob = new Blob([ref]);
      refOf.set(blob, ref);
      return { ok: true, blob: async () => blob };
    });

    subject.select("screen", "local/a");
    render(PreviewPane);
    render(LibraryPane);
    const img = dom.getByRole("img", { name: /screen preview/i });
    await waitFor(() => expect(release["local/a"]).toBeTypeOf("function"));
    release["local/a"]();
    delete release["local/a"];
    await waitFor(() => expect(img.src).toBe("blob:local/a"));

    vi.spyOn(api, "createScreen").mockResolvedValue({ ref: "local/fresh" });
    api.listScreens.mockResolvedValue({
      screens: [{ name: "local/a" }, { name: "local/fresh" }],
      devices: [],
      panels: [],
      dither_algorithms: [],
    });

    await fireEvent.click(dom.getByRole("button", { name: "New" }));
    const form = await dom.findByRole("form", { name: "New screen name" });
    const input = within(form).getByRole("textbox");
    await fireEvent.input(input, { target: { value: "fresh" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(subject.screenRef).toBe("local/fresh"));
    await waitFor(() => expect(release["local/fresh"]).toBeTypeOf("function"));

    // The starter answers first, the screen it replaced answers second.
    release["local/fresh"]();
    await waitFor(() => expect(img.src).toBe("blob:local/fresh"));
    for (const ref of Object.keys(release)) if (ref !== "local/fresh") release[ref]();
    await new Promise((r) => setTimeout(r, 20));

    expect(img.src).toBe("blob:local/fresh");
  });
});
