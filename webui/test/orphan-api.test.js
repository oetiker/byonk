import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import * as api from "../src/lib/api.js";

/** Every .js/.svelte source under src/lib except api.js itself. */
function sources(dir) {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return sources(p);
    return /\.(js|svelte)$/.test(n) && !p.endsWith("api.js") ? [readFileSync(p, "utf8")] : [];
  });
}

// Plan 1's routes (docs/superpowers/plans/2026-10-08-dev-ui-server.md) and
// the export that calls each. A route here with no export, or an export
// with no consumer below, is an orphan.
const ROUTES = {
  "GET api/overrides": "listOverrides",
  "PUT api/overrides/panels/{panel}": "pushPanelOverride",
  "DELETE api/overrides/panels/{panel}": "releasePanelOverride",
  "PUT api/overrides/devices/{device}": "pushDeviceOverride",
  "DELETE api/overrides/devices/{device}": "releaseDeviceOverride",
  "POST api/panels/{panel}/save": "savePanel",
  "PATCH api/devices/{key}": "saveDevice",
  "GET api/panel-usage": "panelUsage",
  "PUT api/tuning-session/{device}": "startTuningSession",
  "DELETE api/tuning-session/{device}": "endTuningSession",
  "POST api/render": "renderPreview",
};

describe("no orphan APIs", () => {
  const src = sources(join(__dirname, "../src/lib")).join("\n");

  it("every new server route has an api.js export", () => {
    for (const [route, name] of Object.entries(ROUTES)) {
      expect(typeof api[name], `${route} -> ${name}`).toBe("function");
    }
  });

  it("the tuning-session calls are consumed by app code (configDraft push/release)", () => {
    expect(src).toMatch(/api\.startTuningSession\b/);
    expect(src).toMatch(/api\.endTuningSession\b/);
  });

  it("every api.js export has a consumer in the app", () => {
    const excluded = ["ConflictError", "PreconditionRequiredError"];
    const unused = Object.keys(api).filter(
      (n) => !excluded.includes(n) && !new RegExp(`api\\.${n}\\b`).test(src),
    );
    expect(unused, `exports nothing calls: ${unused.join(", ")}`).toEqual([]);
  });

  it("the removed panel-colors routes are gone from the client", () => {
    expect(src).not.toMatch(/panel-colors/);
    expect(readFileSync(join(__dirname, "../src/lib/api.js"), "utf8")).not.toMatch(/panel-colors/);
  });
});
