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
    // the exact expected URL, and optionally a custom response.
    const urlCoverage = [
      {
        name: "whoami",
        call: () => api.whoami(),
        expectedUrl: "api/whoami",
      },
      {
        name: "listRepos",
        call: () => api.listRepos(),
        expectedUrl: "api/repos",
      },
      {
        name: "listScreens",
        call: () => api.listScreens(),
        expectedUrl: "api/screens",
      },
      {
        name: "listFiles",
        call: () => api.listFiles("local/hello"),
        expectedUrl: "api/files?ref=local%2Fhello",
      },
      {
        name: "readFile",
        call: () => api.readFile("local/hello", "script.lua"),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
      },
      {
        name: "writeFile",
        call: () => api.writeFile("local/hello", "script.lua", "x", null),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
        responseStatus: 204,
        responseHeaders: { ETag: '"new"' },
      },
      {
        name: "deleteFile",
        call: () => api.deleteFile("local/hello", "script.lua"),
        expectedUrl: "api/file?ref=local%2Fhello&name=script.lua",
      },
      {
        name: "createScreen",
        call: () => api.createScreen("local", "test"),
        expectedUrl: "api/screen",
        responseBody: '{"ref":"local/test"}',
      },
      {
        name: "deleteScreen",
        call: () => api.deleteScreen("local/test"),
        expectedUrl: "api/screen",
        responseBody: '{"ref":"local/test"}',
      },
      {
        name: "copyScreen",
        call: () => api.copyScreen("local/src", "dest", "copy"),
        expectedUrl: "api/screen/copy",
        responseBody: '{"ref":"local/dest/copy"}',
      },
      {
        name: "renameScreen",
        call: () => api.renameScreen("local/test", "renamed"),
        expectedUrl: "api/screen/rename",
        responseBody: '{"ref":"local/renamed"}',
      },
      {
        name: "checkFile",
        call: () => api.checkFile("local/test", "script.lua", "x"),
        expectedUrl: "api/screen/check",
        responseBody: '{"ok":true,"issues":[]}',
      },
      {
        name: "setPanelOverride",
        call: () => api.setPanelOverride("e1004", "#101010,#E8E8E8"),
        expectedUrl: "api/panel-colors/e1004",
      },
      {
        name: "clearPanelOverride",
        call: () => api.clearPanelOverride("e1004"),
        expectedUrl: "api/panel-colors/e1004",
      },
      {
        name: "savePanelColors",
        call: () => api.savePanelColors("e1004", "#101010,#E8E8E8"),
        expectedUrl: "api/panel-colors/e1004/save",
      },
      {
        name: "startTuningSession",
        call: () => api.startTuningSession("kitchen"),
        expectedUrl: "api/tuning-session/kitchen",
      },
      {
        name: "endTuningSession",
        call: () => api.endTuningSession("kitchen"),
        expectedUrl: "api/tuning-session/kitchen",
      },
      {
        name: "renderUrl",
        call: () => api.renderUrl({ screen: "local/hello" }),
        expectedUrl: "api/render?screen=local%2Fhello",
        isStringBuilder: true,
      },
      {
        name: "eventsUrl",
        call: () => api.eventsUrl(),
        expectedUrl: "api/events",
        isStringBuilder: true,
      },
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
        // HTTP requests: await and verify the URL
        await result;
        const calledUrl = globalThis.fetch.mock.calls.at(-1)[0];
        expect(calledUrl).toBe(entry.expectedUrl);
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

  it("omits empty render params rather than sending blanks", () => {
    const u = api.renderUrl({ screen: "local/hello", panel: "", dither: "atkinson" });
    expect(u).toContain("screen=local%2Fhello");
    expect(u).toContain("dither=atkinson");
    expect(u).not.toContain("panel=");
  });

  it("throws on unparseable JSON response from POST", async () => {
    // A 200 response with invalid JSON must throw, not silently return null.
    // This ensures server errors are diagnosable rather than silently becoming
    // downstream TypeErrors far from the actual cause.
    mockFetch(async () => new Response("not valid json", { status: 200 }));
    await expect(api.checkFile("local/test", "script.lua", "x")).rejects.toThrow();
  });
});
