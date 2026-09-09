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
    // a style test. This test covers all 14 functions that know URLs.

    // Functions that fetch and use the URL: mock to return valid responses
    mockFetch(async (url) => {
      // Verify URL doesn't start with /
      expect(url.startsWith("/")).toBe(false);

      // Return appropriate response based on the call
      if (url.includes("files?")) return new Response("[]", { status: 200 });
      if (url.includes("file?")) {
        if (url.includes("method") || globalThis.fetch.mock.lastCall?.[1]?.method === "DELETE") {
          return new Response(null, { status: 204 });
        }
        return new Response("text", { status: 200, headers: { ETag: '"abc"' } });
      }
      if (url.includes("screen/check")) return new Response('{"ok":true,"issues":[]}', { status: 200 });
      return new Response('{}', { status: 200 });
    });

    // Test all functions that make HTTP requests
    await api.whoami();
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/whoami");

    await api.listRepos();
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/repos");

    await api.listScreens();
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screens");

    await api.listFiles("local/hello");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/files?ref=local%2Fhello");

    await api.readFile("local/hello", "script.lua");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/file?ref=local%2Fhello&name=script.lua");

    // For writeFile, mock a fresh response
    mockFetch(async () => new Response(null, { status: 204, headers: { ETag: '"new"' } }));
    await api.writeFile("local/hello", "script.lua", "x", null);
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/file?ref=local%2Fhello&name=script.lua");

    // Reset mock for remaining operations
    mockFetch(async (url) => {
      expect(url.startsWith("/")).toBe(false);
      return new Response('{"ref":"local/test"}', { status: 200 });
    });

    await api.deleteFile("local/hello", "script.lua");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/file?ref=local%2Fhello&name=script.lua");

    await api.createScreen("local", "test");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screen");

    await api.deleteScreen("local/test");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screen");

    await api.copyScreen("local/src", "dest", "copy");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screen/copy");

    await api.renameScreen("local/test", "renamed");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screen/rename");

    // Reset for checkFile which expects specific JSON response
    mockFetch(async () => new Response('{"ok":true,"issues":[]}', { status: 200 }));
    await api.checkFile("local/test", "script.lua", "x");
    expect(globalThis.fetch.mock.calls.at(-1)[0]).toBe("api/screen/check");

    // Test URL builders that return strings
    expect(api.renderUrl({ screen: "local/hello" }).startsWith("/")).toBe(false);
    expect(api.renderUrl({ screen: "local/hello" })).toBe("api/render?screen=local%2Fhello");

    expect(api.eventsUrl().startsWith("/")).toBe(false);
    expect(api.eventsUrl()).toBe("api/events");
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
