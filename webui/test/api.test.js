import { describe, it, expect, vi, beforeEach } from "vitest";
import * as api from "../src/lib/api.js";

function mockFetch(impl) {
  globalThis.fetch = vi.fn(impl);
}

beforeEach(() => vi.restoreAllMocks());

describe("api", () => {
  it("never builds a root-absolute URL", async () => {
    // Under Ingress the Supervisor strips a prefix the server never sees. A
    // leading slash escapes the HA iframe, so this is a correctness test, not
    // a style test. This test MUST fail if any export is added without being
    // added to this coverage table — proving the guard works via the
    // completeness assertion at the end.

    // Coverage table: each entry specifies a function, how to call it,
    // the exact expected URL and HTTP method, and optionally a custom
    // response. `expectedMethod` matters as much as `expectedUrl` --
    // `pushPanelOverride`/`releasePanelOverride` share a URL and differ only by
    // verb (PUT vs DELETE), so a URL-only check can't tell a push from a
    // discard, or catch the two being swapped outright.
    const urlCoverage = [
      {
        name: "whoami",
        call: () => api.whoami(),
        expectedUrl: "api/whoami",
        expectedMethod: "GET",
      },
      {
        name: "listRepos",
        call: () => api.listRepos(),
        expectedUrl: "api/repos",
        expectedMethod: "GET",
      },
      {
        name: "listScreens",
        call: () => api.listScreens(),
        expectedUrl: "api/screens",
        expectedMethod: "GET",
      },
      {
        name: "listFiles",
        call: () => api.listFiles("local/hello"),
        expectedUrl: "api/files?ref=local%2Fhello",
        expectedMethod: "GET",
      },
      {
        name: "readFile",
        call: () => api.readFile("local/hello", "script.lua"),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
        expectedMethod: "GET",
      },
      {
        name: "writeFile",
        call: () => api.writeFile("local/hello", "script.lua", "x", null),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
        expectedMethod: "PUT",
        responseStatus: 204,
        responseHeaders: { ETag: '"new"' },
      },
      {
        name: "deleteFile",
        call: () => api.deleteFile("local/hello", "script.lua"),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
        expectedMethod: "DELETE",
      },
      {
        name: "createScreen",
        call: () => api.createScreen("local", "test"),
        expectedUrl: "api/screen",
        expectedMethod: "POST",
        responseBody: '{"ref":"local/test"}',
      },
      {
        name: "deleteScreen",
        call: () => api.deleteScreen("local/test"),
        expectedUrl: "api/screen?ref=local%2Ftest",
        expectedMethod: "DELETE",
        responseBody: '{"ref":"local/test"}',
      },
      {
        name: "copyScreen",
        call: () => api.copyScreen("local/src", "dest", "copy"),
        expectedUrl: "api/screen/copy",
        expectedMethod: "POST",
        responseBody: '{"ref":"local/dest/copy"}',
      },
      {
        name: "moveScreen",
        call: () => api.moveScreen("local/test", "shared", "renamed"),
        expectedUrl: "api/screen/move",
        expectedMethod: "POST",
        responseBody: '{"ref":"shared/renamed","rewritten":[]}',
      },
      {
        name: "checkFile",
        call: () => api.checkFile("local/test", "script.lua", "x"),
        expectedUrl: "api/screen/check",
        expectedMethod: "POST",
        responseBody: '{"ok":true,"issues":[]}',
      },
      {
        name: "validateScreen",
        call: () => api.validateScreen("local/test"),
        expectedUrl: "api/screen/validate",
        expectedMethod: "POST",
        responseBody: '{"ok":true,"issues":[]}',
      },
      {
        name: "startTuningSession",
        call: () => api.startTuningSession("kitchen"),
        expectedUrl: "api/tuning-session/kitchen",
        expectedMethod: "PUT",
      },
      {
        name: "endTuningSession",
        call: () => api.endTuningSession("kitchen"),
        expectedUrl: "api/tuning-session/kitchen",
        expectedMethod: "DELETE",
      },
      {
        name: "renderPreview",
        call: () => api.renderPreview({ screen: "local/hello" }),
        expectedUrl: "api/render",
        expectedMethod: "POST",
      },
      {
        name: "eventsUrl",
        call: () => api.eventsUrl(),
        expectedUrl: "api/events",
        isStringBuilder: true,
      },
      {
        // Ruling 63: the SPA never called this at all. `/logout` is a
        // sibling of `/dev`, not `/dev/api/...` (dev_auth_router() merges
        // it into the nest root, unlike whoami/screens/etc which live
        // under `/api` -- see src/api/dev.rs's dev_router()), so the
        // relative URL is "logout", not "api/logout".
        name: "logout",
        call: () => api.logout(),
        expectedUrl: "logout",
        expectedMethod: "POST",
      },
      { name: "listOverrides", call: () => api.listOverrides(), expectedUrl: "api/overrides", expectedMethod: "GET" },
      { name: "pushPanelOverride", call: () => api.pushPanelOverride("e1004", { strength: 0.5 }),
        expectedUrl: "api/overrides/panels/e1004", expectedMethod: "PUT" },
      { name: "releasePanelOverride", call: () => api.releasePanelOverride("e1004"),
        expectedUrl: "api/overrides/panels/e1004", expectedMethod: "DELETE" },
      { name: "pushDeviceOverride", call: () => api.pushDeviceOverride("AA:BB", { dither: "stucki" }),
        expectedUrl: "api/overrides/devices/AA%3ABB", expectedMethod: "PUT" },
      { name: "releaseDeviceOverride", call: () => api.releaseDeviceOverride("AA:BB"),
        expectedUrl: "api/overrides/devices/AA%3ABB", expectedMethod: "DELETE" },
      { name: "savePanel", call: () => api.savePanel("e1004", { colors_actual: "#000000,#FFFFFF" }),
        expectedUrl: "api/panels/e1004/save", expectedMethod: "POST" },
      { name: "saveDevice", call: () => api.saveDevice("kitchen", { name: "K" }),
        expectedUrl: "api/devices/kitchen", expectedMethod: "PATCH", responseBody: '{"key":"kitchen","screen":"local/a"}' },
      { name: "panelUsage", call: () => api.panelUsage(), expectedUrl: "api/panel-usage", expectedMethod: "GET" },
      { name: "createPanel", call: () => api.createPanel({ id: "bw", name: "BW", colors: "#000000,#FFFFFF" }),
        expectedUrl: "api/panels", expectedMethod: "POST", responseBody: '{"id":"bw"}' },
      { name: "createDevice", call: () => api.createDevice({ key: "hall", screen: "local/a" }),
        expectedUrl: "api/devices", expectedMethod: "POST", responseBody: '{"key":"hall","screen":"local/a"}' },
      { name: "listPendingDevices", call: () => api.listPendingDevices(),
        expectedUrl: "api/pending-devices", expectedMethod: "GET", responseBody: "[]" },
      { name: "deletePanel", call: () => api.deletePanel("e1004"),
        expectedUrl: "api/panels/e1004", expectedMethod: "DELETE", responseBody: '{"deleted":"e1004"}' },
      { name: "deleteDevice", call: () => api.deleteDevice("AA:BB"),
        expectedUrl: "api/devices/AA%3ABB", expectedMethod: "DELETE", responseBody: '{"deleted":"AA:BB"}' },
    ];

    // Mock factory: store the current entry so the mock can respond accordingly
    let currentEntry;
    mockFetch(async (url) => {
      expect(url.startsWith("/")).toBe(false);

      if (!currentEntry) return new Response('{}', { status: 200 });

      const status = currentEntry.responseStatus || 200;
      const headers = currentEntry.responseHeaders || {};
      // 204 No Content must not have a body per RFC 7231
      const body = status === 204 ? null : (currentEntry.responseBody || '{}');

      return new Response(body, { status, headers });
    });

    // Test each coverage entry
    for (const entry of urlCoverage) {
      currentEntry = entry;
      const result = entry.call();

      if (entry.isStringBuilder) {
        // String builders don't fetch, just return the URL
        expect(result).toBe(entry.expectedUrl);
      } else {
        // HTTP requests: await and verify the URL AND the method. URL alone
        // can't tell `pushPanelOverride` (PUT) from `releasePanelOverride`
        // (DELETE) -- they share a URL and differ only by verb.
        await result;
        const [calledUrl, calledInit] = globalThis.fetch.mock.calls.at(-1);
        expect(calledUrl).toBe(entry.expectedUrl);
        // A call made with no `init.method` is a GET by fetch's own default;
        // this codebase's helpers only ever omit it for GETs, so `?? "GET"`
        // reflects that rather than papering over a missing assertion.
        expect(calledInit?.method ?? "GET").toBe(entry.expectedMethod);
      }
    }

    // Completeness assertion: every exported function must be covered.
    // This ensures adding a new function to api.js without adding it to the
    // coverage table will make the test fail with the uncovered name.
    const exportedNames = Object.keys(api);
    const coveredNames = urlCoverage.map((e) => e.name);
    // Error classes are explicitly excluded (not HTTP-calling functions).
    const excluded = ["ConflictError", "PreconditionRequiredError"];

    const uncovered = exportedNames.filter(
      (name) => !coveredNames.includes(name) && !excluded.includes(name)
    );

    expect(
      uncovered,
      uncovered.length > 0
        ? `Uncovered exports: ${uncovered.join(", ")}. Add them to urlCoverage.`
        : ""
    ).toEqual([]);
  });

  it("sends a device Save as a JSON PATCH and a push as a JSON PUT", async () => {
    mockFetch(async () => new Response('{"key":"kitchen","screen":"local/a"}', { status: 200 }));
    await api.saveDevice("kitchen", { final: { dither: "atkinson" } });
    let [, init] = globalThis.fetch.mock.calls.at(-1);
    expect(init.method).toBe("PATCH");
    expect(init.headers["Content-Type"]).toBe("application/json");
    expect(JSON.parse(init.body)).toEqual({ final: { dither: "atkinson" } });

    mockFetch(async () => new Response(null, { status: 200 }));
    await expect(api.pushPanelOverride("e1004", { strength: 0.5 })).resolves.toBeNull();
    [, init] = globalThis.fetch.mock.calls.at(-1);
    expect(JSON.parse(init.body)).toEqual({ strength: 0.5 });
  });

  it("returns the etag alongside the text when reading a file", async () => {
    mockFetch(
      async () =>
        new Response("local x = 1", { status: 200, headers: { ETag: "\"abc\"" } }),
    );
    expect(await api.readFile("local/hello", "script.lua")).toEqual({
      text: "local x = 1",
      etag: '"abc"',
    });
  });

  it("sends If-Match: * to create and the etag to update", async () => {
    mockFetch(async () => new Response(null, { status: 204, headers: { ETag: '"new"' } }));
    await api.writeFile("local/hello", "script.lua", "x", null);
    expect(globalThis.fetch.mock.calls[0][1].headers["If-Match"]).toBe("*");

    await api.writeFile("local/hello", "script.lua", "x", '"old"');
    expect(globalThis.fetch.mock.calls[1][1].headers["If-Match"]).toBe('"old"');
  });

  it("distinguishes 412 from 428", async () => {
    mockFetch(async () => new Response("", { status: 412 }));
    await expect(api.writeFile("r/s", "f", "x", '"e"')).rejects.toBeInstanceOf(
      api.ConflictError,
    );

    mockFetch(async () => new Response("", { status: 428 }));
    await expect(api.writeFile("r/s", "f", "x", '"e"')).rejects.toBeInstanceOf(
      api.PreconditionRequiredError,
    );
  });

  it("POSTs the render request as a JSON body and returns the raw Response", async () => {
    const response = new Response("png", { status: 200, headers: { "X-Byonk-Resolution": "{}" } });
    mockFetch(async () => response);
    const body = { screen: "local/hello", override: { dither: "atkinson" } };
    const res = await api.renderPreview(body);
    // The raw Response, so the caller can read the blob AND the header.
    expect(res).toBe(response);
    const [url, init] = globalThis.fetch.mock.calls.at(-1);
    expect(url).toBe("api/render");
    expect(init.method).toBe("POST");
    expect(init.headers["Content-Type"]).toBe("application/json");
    expect(JSON.parse(init.body)).toEqual(body);
  });

  it("moveScreen POSTs {ref, to_handle, to_name} and returns {ref, rewritten}", async () => {
    mockFetch(async () => new Response('{"ref":"shared/b","rewritten":["kitchen"]}', { status: 200 }));
    const out = await api.moveScreen("local/a", "shared", "b");
    expect(out).toEqual({ ref: "shared/b", rewritten: ["kitchen"] });
    const [url, init] = globalThis.fetch.mock.calls.at(-1);
    expect(url).toBe("api/screen/move");
    expect(JSON.parse(init.body)).toEqual({ ref: "local/a", to_handle: "shared", to_name: "b" });
  });

  it("sends deleteScreen's ref as a query parameter, not a JSON body", async () => {
    // Regression test: the server's delete_screen handler extracts
    // Query<RefQuery> (src/api/dev_files.rs) and has no Json extractor at
    // all, so a JSON body -- what this used to send -- always fails
    // deserialization server-side. The coverage table above only pins the
    // URL string; this test pins the *shape* of the request (no body, no
    // Content-Type) so that regression can't come back unnoticed.
    mockFetch(async () => new Response(null, { status: 204 }));
    await api.deleteScreen("local/zz-delete-me");

    const [calledUrl, calledInit] = globalThis.fetch.mock.calls[0];
    expect(calledUrl).toBe("api/screen?ref=local%2Fzz-delete-me");
    expect(calledInit.method).toBe("DELETE");
    expect(calledInit.body).toBeUndefined();
    expect(calledInit.headers).toBeUndefined();
  });

  it("throws on unparseable JSON response from POST", async () => {
    // A 200 response with invalid JSON must throw, not silently return null.
    // This ensures server errors are diagnosable rather than silently becoming
    // downstream TypeErrors far from the actual cause.
    mockFetch(async () => new Response("not valid json", { status: 200 }));
    await expect(api.checkFile("local/test", "script.lua", "x")).rejects.toThrow();
  });

  it("tolerates the empty 200 body the panel-save handler returns", async () => {
    // Regression test: handle_save_panel_colors (src/api/dev.rs) returns
    // Result<StatusCode, ApiError> -> Ok(StatusCode::OK), i.e. 200 with an
    // EMPTY body. post() used to hand that straight to res.json(), which
    // rejects on empty input, so every *successful* save threw. tune.svelte.js
    // catches that throw and reports a failure for a calibration that had
    // already been written to config.yaml -- and never ends the tuning session.
    // The coverage table can't catch this: entries with no `responseBody` get
    // a fabricated '{}' from the mock factory, a body the real server never
    // sends.
    mockFetch(async () => new Response(null, { status: 200 }));
    await expect(api.savePanel("e1004", {})).resolves.toBeNull();
  });
});
