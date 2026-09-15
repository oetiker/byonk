import "@testing-library/jest-dom/vitest";

/**
 * jsdom gaps that stop a `bits-ui` overlay (Select, and anything else built
 * on the same trigger) from ever opening.
 *
 * Without these the trigger's own `pointerdown` handler throws
 * `target?.hasPointerCapture is not a function` (select.svelte.js:744) BEFORE
 * it can toggle the menu. The throw surfaces as an unhandled exception rather
 * than a test failure, so a test that merely looks for the menu afterwards
 * reports "found 0 items" and can be misread as "the control does nothing" --
 * which is very close to how the theme control was nearly filed as broken.
 *
 * `??=` so a test that wants its own stub (or a future jsdom that implements
 * these for real) wins over ours.
 *
 * NOTE for anyone reading a green suite here: jsdom runs no animations, so an
 * overlay unmounts immediately on close. A real browser waits for the exit
 * animation first, and a browser tab that is not being PAINTED never finishes
 * one -- that is the trap that produced Finding K. jsdom cannot see that
 * class of problem; only a visible browser window can.
 */
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
