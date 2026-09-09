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
    // a style test.
    mockFetch(async () => new Response("[]", { status: 200 }));
    await api.listRepos();
    const url = globalThis.fetch.mock.calls[0][0];
    expect(url.startsWith("/")).toBe(false);
    expect(url).toBe("api/repos");

    expect(api.renderUrl({ screen: "local/hello" }).startsWith("/")).toBe(false);
    expect(api.eventsUrl().startsWith("/")).toBe(false);
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
});
