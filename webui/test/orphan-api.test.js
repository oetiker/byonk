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
  "POST api/panels": "createPanel",
  "POST api/devices": "createDevice",
  "GET api/pending-devices": "listPendingDevices",
  "DELETE api/panels/{panel}": "deletePanel",
  "DELETE api/devices/{key}": "deleteDevice",
  "PUT api/tuning-session/{device}": "startTuningSession",
  "DELETE api/tuning-session/{device}": "endTuningSession",
  "POST api/render": "renderPreview",
  "GET api/screens": "listScreens",
  "GET api/events": "eventsUrl",
  "GET api/whoami": "whoami",
  "GET api/repos": "listRepos",
  "GET api/files": "listFiles",
  "GET api/file": "readFile",
  "PUT api/file": "writeFile",
  "DELETE api/file": "deleteFile",
  "POST api/screen": "createScreen",
  "DELETE api/screen": "deleteScreen",
  "POST api/screen/copy": "copyScreen",
  "POST api/screen/move": "moveScreen",
  "POST api/screen/validate": "validateScreen",
  "POST api/screen/check": "checkFile",
  "GET api/screen-repos": "listScreenRepos",
  "POST api/screen-repos": "addScreenRepo",
  "PATCH api/screen-repos/{handle}": "patchScreenRepo",
  "DELETE api/screen-repos/{handle}": "deleteScreenRepo",
  "POST api/screen-repos/{handle}/update": "updateScreenRepo",
};

// Routes the router registers that the SPA deliberately does not call.
// Every entry needs a reason.
const ALLOWED_UNCALLED = {
  "/resolve-mac": "server-only since before this work: scripted use, documented in dev-mode.md",
  "/": "the dev page itself, served as the document, not called through api.js",
  "/app.js": "the SPA bundle, loaded by index.html",
  "/app.css": "the SPA stylesheet, loaded by index.html",
  "/lens.js": "static script loaded by the preview page, not an API",
  "/preview.html": "static preview page loaded in the iframe, not an API",
};

/** Every `.route("<path>"` in a Rust source (the path may follow a newline). */
function routePaths(file) {
  const text = readFileSync(join(__dirname, "../../src/api", file), "utf8");
  const paths = [...text.matchAll(/\.route\(\s*"([^"]+)"/g)].map((m) => m[1]);
  // A router refactor must not turn this sweep into a silent no-op.
  expect(paths.length, `no .route("...") found in src/api/${file}`).toBeGreaterThan(0);
  return paths;
}

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

  it("every route the server registers is mapped to an export or explicitly allowed", () => {
    const mapped = new Set(Object.keys(ROUTES).map((k) => "/" + k.split(" ")[1].replace(/^api\//, "")));
    const registered = [...routePaths("dev.rs"), ...routePaths("dev_files.rs"), ...routePaths("dev_repos.rs")];
    const unmapped = registered.filter((p) => !mapped.has(p) && !(p in ALLOWED_UNCALLED));
    expect(unmapped, `server routes with no api.js export: ${unmapped.join(", ")}`).toEqual([]);
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
