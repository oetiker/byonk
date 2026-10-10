import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/svelte";
import { subject } from "../src/lib/state/subject.svelte.js";
import EditPane from "../src/lib/components/EditPane.svelte";

beforeEach(() => {
  cleanup();
  subject.clear();
});
afterEach(cleanup);

describe("EditPane empty state", () => {
  it("still draws the pane header row, so the line across the panes is unbroken", () => {
    const { container } = render(EditPane);
    const header = container.querySelector("[data-slot=pane-header]");
    expect(header).not.toBeNull();
    expect(header.textContent).toContain("Edit");
    expect(container.textContent).toContain("Pick a screen, panel or device");
  });
});
