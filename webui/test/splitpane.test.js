import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import Harness from "./SplitPaneHarness.svelte";

// Stub for a container with a known, measurable extent. SplitPane's
// horizontal (axis="x") maxSize math is `rect.width - min`; move() derives
// the dragged position from `rect.right - event.clientX`.
function stubRect({ width = 500, height = 500 } = {}) {
  return () => ({
    width,
    height,
    top: 0,
    left: 0,
    right: width,
    bottom: height,
    x: 0,
    y: 0,
    toJSON() {},
  });
}

describe("SplitPane", () => {
  let getBoundingClientRectSpy;

  beforeEach(() => {
    cleanup();
  });

  afterEach(() => {
    getBoundingClientRectSpy?.mockRestore();
    getBoundingClientRectSpy = undefined;
  });

  it("shows a handle when both sides are open", () => {
    render(Harness, { props: { open: true } });
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("retires the handle when a neighbour is hidden", () => {
    // Spec §1: a splitter must not survive as a handle that drags nothing.
    render(Harness, { props: { open: false } });
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });

  it("does not claim a false upper bound before the container is measurable", () => {
    // Regression test for the aria-valuemax="9999" fallback bug: with
    // jsdom's default all-zero getBoundingClientRect (nothing stubbed
    // here), there is no real bound to report. The old code served a magic
    // "9999" fallback on every mount; the fix must not serve any invented
    // number in its place.
    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    expect(separator).toHaveAttribute("aria-valuenow");
    expect(separator).toHaveAttribute("aria-valuemin");
    expect(separator).not.toHaveAttribute("aria-valuemax");
  });

  it("exposes a real aria-valuemax once the container is measurable", () => {
    // width 500, default min 120 -> maxSize = 500 - 120 = 380.
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 }));

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    expect(separator).toHaveAttribute("aria-valuemax", "380");
    expect(Number(separator.getAttribute("aria-valuenow"))).toBeLessThanOrEqual(380);
  });

  it("clamps a grow keypress at the upper bound instead of inverting to min", async () => {
    // Requirement 2's headline failure mode: maxSize going negative made
    // the outer Math.max(min, ...) snap size to min on every move/key --
    // including a GROW press. Start well below the bound and grow past it.
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 })); // maxSize = 380

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    for (let i = 0; i < 10; i++) {
      await fireEvent.keyDown(separator, { key: "ArrowRight", shiftKey: true }); // +48 each
    }

    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(380);
  });

  it("grows on a single ArrowRight press even when the container is unmeasurable", async () => {
    // Requirement 2, the exact jsdom manifestation: unstubbed
    // getBoundingClientRect is all zeros, so maxSize would be negative
    // under the old code, and Math.max(min, ...) would snap size down to
    // min on this very grow press instead of growing it.
    render(Harness, { props: { open: true } }); // Harness starts size at 300
    const separator = screen.getByRole("separator");

    await fireEvent.keyDown(separator, { key: "ArrowRight" }); // +12, unshifted

    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(312);
  });

  it("clamps a shrink keypress at the lower bound", async () => {
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 }));

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    for (let i = 0; i < 10; i++) {
      await fireEvent.keyDown(separator, { key: "ArrowLeft", shiftKey: true }); // -48 each
    }

    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(120); // default min
  });

  it("clamps a drag at the upper bound", async () => {
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 })); // maxSize = 380

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    await fireEvent.pointerDown(separator, { pointerId: 1, clientX: 200, clientY: 0 });
    // rect.right(500) - clientX(-1000) = 1500, well past maxSize (380).
    await fireEvent.pointerMove(window, { pointerId: 1, clientX: -1000, clientY: 0 });
    await fireEvent.pointerUp(window, { pointerId: 1 });

    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(380);
  });

  it("clamps a drag at the lower bound", async () => {
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 }));

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");

    await fireEvent.pointerDown(separator, { pointerId: 1, clientX: 200, clientY: 0 });
    // rect.right(500) - clientX(10000) is deeply negative, well past min.
    await fireEvent.pointerMove(window, { pointerId: 1, clientX: 10000, clientY: 0 });
    await fireEvent.pointerUp(window, { pointerId: 1 });

    expect(Number(separator.getAttribute("aria-valuenow"))).toBe(120); // default min
  });

  it("releases pointer capture on the separator that received it, not on host", async () => {
    getBoundingClientRectSpy = vi
      .spyOn(Element.prototype, "getBoundingClientRect")
      .mockImplementation(stubRect({ width: 500 }));

    render(Harness, { props: { open: true } });
    const separator = screen.getByRole("separator");
    const host = separator.parentElement;

    const separatorSetSpy = vi.fn();
    const separatorReleaseSpy = vi.fn();
    const hostReleaseSpy = vi.fn();
    separator.setPointerCapture = separatorSetSpy;
    separator.releasePointerCapture = separatorReleaseSpy;
    host.releasePointerCapture = hostReleaseSpy;

    await fireEvent.pointerDown(separator, { pointerId: 7, clientX: 200, clientY: 0 });
    expect(separatorSetSpy).toHaveBeenCalledWith(7);

    await fireEvent.pointerUp(window, { pointerId: 7 });

    expect(separatorReleaseSpy).toHaveBeenCalledWith(7);
    expect(hostReleaseSpy).not.toHaveBeenCalled();
  });
});

describe("SplitPane second pane layout contract", () => {
  beforeEach(() => cleanup());
  afterEach(() => cleanup());

  // jsdom has no layout. A nested flex child (the inner SplitPane's flex-1
  // root) only fills the second pane if that pane is itself a flex
  // container, the way the first pane already is.
  it("makes the second pane a flex container so a flex-1 child fills it", () => {
    render(Harness);
    const wrapper = screen.getByText("right").parentElement;
    expect(wrapper.className.split(/\s+/)).toContain("flex");
  });
});

