import { describe, it, expect } from "vitest";
import { statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// The UI is embedded in the binary, so its size is the binary's size. The
// vanilla UI it replaces was 3,305 lines across five files, about 400 KB
// once the CodeMirror bundle is counted. This budget is deliberately close
// to that: a framework is affordable here, an unbounded one is not.
// Raising it is a decision, not a formality -- say why in the commit.
const BUDGET_BYTES = 900 * 1024;

describe("bundle size", () => {
  it("fits the stated budget", () => {
    const js = statSync(resolve(root, "dist/app.js")).size;
    const css = statSync(resolve(root, "dist/app.css")).size;
    expect(js + css).toBeLessThan(BUDGET_BYTES);
  });
});
