import { describe, it, expect, beforeEach } from "vitest";
import { resolution, parseResolutionHeader } from "../src/lib/state/resolution.svelte.js";

beforeEach(() => resolution.reset());

describe("resolution store", () => {
  it("starts with no render", () => {
    expect(resolution.state).toBe("none");
    expect(resolution.setting("dither")).toBe(null);
  });
  it("records a payload for a subject", () => {
    resolution.record({ dither: { effective: "atkinson", from: "screen", order: [], rungs: {} } }, "screen:local/a");
    expect(resolution.state).toBe("ok");
    expect(resolution.setting("dither").effective).toBe("atkinson");
    expect(resolution.subjectKey).toBe("screen:local/a");
  });
  it("a failure drops the payload rather than leaving stale rungs", () => {
    resolution.record({ dither: {} }, "screen:local/a");
    resolution.fail("Render error: boom", "screen:local/a");
    expect(resolution.state).toBe("failed");
    expect(resolution.payload).toBe(null);
    expect(resolution.failure).toBe("Render error: boom");
  });
  it("parses the header, and a missing header is null", () => {
    expect(parseResolutionHeader(null)).toBe(null);
    expect(parseResolutionHeader('{"refresh":{"effective":60}}').refresh.effective).toBe(60);
    expect(() => parseResolutionHeader("{nope")).toThrow();
  });
});
