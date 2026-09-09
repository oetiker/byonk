import { describe, it, expect } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// src/api/dev.rs embeds these three paths with include_str! at compile time.
// A content hash in any of them is a compile error in Rust, discovered far
// from its cause, so it is caught here instead.
describe("build output", () => {
  const names = ["dist/index.html", "dist/app.js", "dist/app.css"];

  it.each(names)("emits %s under a fixed name", (n) => {
    expect(existsSync(resolve(root, n))).toBe(true);
  });

  it("references its assets relatively, never from the root", () => {
    const html = readFileSync(resolve(root, "dist/index.html"), "utf8");
    // Under Ingress the Supervisor strips a prefix the server never sees, so
    // a src="/app.js" would 404 inside the HA iframe.
    expect(html).not.toMatch(/(?:src|href)="\//);
    expect(html).toMatch(/app\.js/);
    expect(html).toMatch(/app\.css/);
  });
});
