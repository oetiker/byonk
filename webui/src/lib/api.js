// The only module that knows a URL.
//
// Every path here is RELATIVE, with no leading slash. Under Home Assistant
// Ingress the Supervisor strips a path prefix before byonk sees the request,
// so the server cannot reconstruct it and a root-absolute URL leaves the HA
// iframe. `api.test.js` pins this.

export class ConflictError extends Error {
  constructor(msg = "the file changed on disk") {
    super(msg);
    this.name = "ConflictError";
  }
}

/** 428: the If-Match header was missing, which is not a mismatch. */
export class PreconditionRequiredError extends Error {
  constructor(msg = "If-Match header required") {
    super(msg);
    this.name = "PreconditionRequiredError";
  }
}

async function fail(res) {
  const body = await res.text().catch(() => "");
  return new Error(`${res.status} ${res.statusText}${body ? `: ${body}` : ""}`);
}

async function json(url, init) {
  const res = await fetch(url, init);
  if (!res.ok) throw await fail(res);
  return res.json();
}

function qs(params) {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    u.set(k, String(v));
  }
  return u.toString();
}

export const whoami = () => json("api/whoami");

/**
 * `POST /logout` (src/api/dev_auth.rs's `logout()`) is a sibling of `/dev`,
 * not `/dev/api/...` -- `dev_auth_router()` merges it into the nest root
 * alongside `/login`, deliberately outside `require_dev`, so the relative
 * URL here is "logout", not "api/logout". The handler answers with a
 * `Set-Cookie` clearing the session and a redirect to `login`; nothing in
 * that body is needed here; only that the round trip actually happened.
 */
export async function logout() {
  const res = await fetch("logout", { method: "POST" });
  if (!res.ok) throw await fail(res);
}

export const listRepos = () => json("api/repos");
export const listScreens = () => json("api/screens");
export const listFiles = (ref) => json(`api/files?${qs({ ref })}`);

export async function readFile(ref, name) {
  const res = await fetch(`api/file?${qs({ ref, name })}`);
  if (!res.ok) throw await fail(res);
  return { text: await res.text(), etag: res.headers.get("ETag") };
}

/**
 * Write one file.
 *
 * byonk's If-Match semantics are its own, not RFC 9110's: `*` means "create,
 * and fail with 412 if it already exists", so a null etag is how a new file
 * is written. A missing header is 428 -- "you forgot it" -- which is
 * deliberately a different failure from a mismatch.
 */
export async function writeFile(ref, name, text, etag) {
  const res = await fetch(`api/file?${qs({ ref, name })}`, {
    method: "PUT",
    headers: { "If-Match": etag ?? "*", "Content-Type": "text/plain" },
    body: text,
  });
  if (res.status === 412) throw new ConflictError();
  if (res.status === 428) throw new PreconditionRequiredError();
  if (!res.ok) throw await fail(res);
  return { etag: res.headers.get("ETag") };
}

export async function deleteFile(ref, name) {
  const res = await fetch(`api/file?${qs({ ref, name })}`, { method: "DELETE" });
  if (!res.ok) throw await fail(res);
}

async function post(url, body) {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw await fail(res);
  // Current endpoints (create, copy, rename, check via dev_files.rs:286,303,313,346)
  // all return 200 with JSON. The 204 branch is kept as a guard for potential
  // future endpoints that may return no content.
  return res.status === 204 ? null : res.json();
}

export const createScreen = (handle, name, starter = "minimal") =>
  post("api/screen", { handle, name, starter });
export const copyScreen = (ref, to_handle, to_name) =>
  post("api/screen/copy", { ref, to_handle, to_name });
export const renameScreen = (ref, name) => post("api/screen/rename", { ref, name });
export const checkFile = (ref, file, text) => post("api/screen/check", { ref, file, text });

export async function deleteScreen(ref) {
  const res = await fetch("api/screen", {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ref }),
  });
  if (!res.ok) throw await fail(res);
}

/**
 * PUT/DELETE counterparts to `post()`, in the same shape: throw `fail()`'s
 * error on a non-OK status so the caller's error handling is uniform across
 * verbs.
 *
 * Unlike `post()`'s endpoints, the colour-tuning routes below answer with a
 * bare `StatusCode` and no body (`handle_set_panel_colors` et al.,
 * `src/api/dev.rs`), so -- unlike `post()`, which can assume a 200 always
 * carries JSON -- an empty 200 body must not be handed to `res.json()`,
 * which throws on empty input.
 */
async function put(url, body) {
  const res = await fetch(url, {
    method: "PUT",
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw await fail(res);
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

async function del(url) {
  const res = await fetch(url, { method: "DELETE" });
  if (!res.ok) throw await fail(res);
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

/**
 * Colour tuning (`docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md`
 * §7). Note the asymmetry: the override and the save are keyed by **panel**
 * (a calibration belongs to the panel profile, shared by every device using
 * it); the tuning session is keyed by **device** (only a device can be
 * standing in front of, and polls). Mixing the two keys is the defect Task
 * 9b's fix round eliminated -- keep it apart here too.
 */
export const setPanelOverride = (panel, colors_actual) =>
  put(`api/panel-colors/${encodeURIComponent(panel)}`, { colors_actual });
export const clearPanelOverride = (panel) => del(`api/panel-colors/${encodeURIComponent(panel)}`);
export const savePanelColors = (panel, colors_actual) =>
  post(`api/panel-colors/${encodeURIComponent(panel)}/save`, { colors_actual });
export const startTuningSession = (device) =>
  put(`api/tuning-session/${encodeURIComponent(device)}`);
export const endTuningSession = (device) => del(`api/tuning-session/${encodeURIComponent(device)}`);

/** Params are RenderQuery in src/api/dev.rs:29. Blanks are omitted, not sent. */
export const renderUrl = (params) => `api/render?${qs(params)}`;
export const eventsUrl = () => "api/events";
