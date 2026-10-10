import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/svelte";
import { Check } from "@lucide/svelte";
import IconButton from "../src/lib/components/IconButton.svelte";
import HeaderAction from "../src/lib/components/HeaderAction.svelte";

beforeEach(cleanup);
afterEach(cleanup);

// bits-ui's tooltip trigger puts its own onclick (close the tooltip) in the
// props it hands to the button; a caller's onclick must run in addition to it.
describe.each([
  ["IconButton", IconButton, { icon: Check, label: "Go" }],
  ["HeaderAction", HeaderAction, { label: "Go" }],
])("%s", (_name, Component, props) => {
  it("runs the caller's onclick", async () => {
    const onclick = vi.fn();
    render(Component, { ...props, onclick });
    await fireEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onclick).toHaveBeenCalledTimes(1);
  });

  it("keeps the trigger's own click handler (tooltip trigger state)", async () => {
    const onclick = vi.fn();
    render(Component, { ...props, onclick });
    const btn = screen.getByRole("button", { name: "Go" });
    // Hover opens the tooltip; the click must close it again.
    await fireEvent.pointerMove(btn, { pointerType: "mouse" });
    await fireEvent.focus(btn);
    await new Promise((r) => setTimeout(r, 50));
    await fireEvent.click(btn);
    expect(onclick).toHaveBeenCalled();
    expect(btn.getAttribute("data-state")).not.toBe("delayed-open");
    expect(btn.getAttribute("data-state")).not.toBe("instant-open");
  });
});
