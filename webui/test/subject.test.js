import { describe, it, expect, beforeEach } from "vitest";
import { subject, DEFAULT_BORROWED_SCREEN } from "../src/lib/state/subject.svelte.js";

beforeEach(() => subject.reset());

describe("subject", () => {
  it("starts empty, with mode b borrowing the tone calibration screen", () => {
    expect(subject.kind).toBe(null);
    expect(subject.key).toBe(null);
    expect(subject.borrowedScreen).toBe(DEFAULT_BORROWED_SCREEN);
    expect(DEFAULT_BORROWED_SCREEN).toBe("byonk-builtin/calibration/tone");
  });
  it("selects a kind and an id, and names the screen only for a screen", () => {
    subject.select("screen", "local/a");
    expect(subject.key).toBe("screen:local/a");
    expect(subject.screenRef).toBe("local/a");
    subject.select("panel", "e1004");
    expect(subject.screenRef).toBe(null);
    expect(subject.is("panel", "e1004")).toBe(true);
  });
  it("an empty id clears instead of selecting nothing", () => {
    subject.select("screen", "local/a");
    subject.select("screen", null);
    expect(subject.kind).toBe(null);
  });
  it("keeps the mode b screen choice across subject switches", () => {
    subject.borrowedScreen = "byonk-builtin/calibration/gamut";
    subject.select("screen", "local/a");
    subject.select("panel", "e1004");
    expect(subject.borrowedScreen).toBe("byonk-builtin/calibration/gamut");
  });
});
