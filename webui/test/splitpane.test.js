import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/svelte";
import SplitPane from "../src/lib/components/SplitPane.svelte";
import Harness from "./SplitPaneHarness.svelte";

describe("SplitPane", () => {
  beforeEach(() => {
    cleanup();
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
});
