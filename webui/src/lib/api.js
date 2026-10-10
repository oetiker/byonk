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
  // The rule, not a list of today's endpoints: a handler may answer with a
  // body or without one, so read the text and only parse when there is
  // something to parse. An earlier version enumerated the endpoints instead
  // and asserted they "all return 200 with JSON"; `savePanel`'s handler
  // returns a bare `Ok(StatusCode::OK)` with an empty body,
  // and `res.json()` rejected on every successful save. `put()` and `del()`
  // below already work this way -- all three verbs now obey one rule.
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

export const createScreen = (handle, name, starter = "minimal") =>
  post("api/screen", { handle, name, starter });
export const copyScreen = (ref, to_handle, to_name) =>
  post("api/screen/copy", { ref, to_handle, to_name });
export const renameScreen = (ref, name) => post("api/screen/rename", { ref, name });
export const checkFile = (ref, file, text) => post("api/screen/check", { ref, file, text });

/**
 * Validate a whole screen on disk, as opposed to `checkFile`'s single draft
 * buffer.
 *
 * Answers `{ok, issues}` where each issue carries a `location` -- usually the
 * screen-relative file it belongs to, but sometimes the screen ref itself for
 * a finding no single file owns (src/services/screen_store.rs). Spec
 * 2026-09-07-dev-mode-editor-design.md:315 runs this after every successful
 * save; :326 turns its result into one green tick per clean file.
 */
export const validateScreen = (ref) => post("api/screen/validate", { ref });

export async function deleteScreen(ref) {
  const res = await fetch(`api/screen?${qs({ ref })}`, { method: "DELETE" });
  if (!res.ok) throw await fail(res);
}

/**
 * PUT/DELETE counterparts to `post()`, in the same shape: throw `fail()`'s
 * error on a non-OK status so the caller's error handling is uniform across
 * verbs.
 *
 * The colour-tuning routes below answer with a bare `StatusCode` and no body
 * (`handle_set_panel_colors` et al., `src/api/dev.rs`), so an empty 200 body
 * must not be handed to `res.json()`, which throws on empty input. `post()`
 * reads its body the same way for the same reason.
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

async function patch(url, body) {
  const res = await fetch(url, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
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
 * §7). The tuning session is keyed by **device** (only a device can be
 * standing in front of, and polls); panel overrides and saves are keyed by
 * panel (see `pushPanelOverride` / `savePanel` below).
 */
export const startTuningSession = (device) =>
  put(`api/tuning-session/${encodeURIComponent(device)}`);
export const endTuningSession = (device) => del(`api/tuning-session/${encodeURIComponent(device)}`);

const enc = encodeURIComponent;

/**
 * Dev overrides (Plan 1 Tasks 2-3): a Push puts dirty draft values on the
 * cascade's override rung for every device on a panel, or for one device.
 * Each expires 30 minutes after its last PUT.
 */
export const listOverrides = () => json("api/overrides");
export const pushPanelOverride = (panel, values) => put(`api/overrides/panels/${enc(panel)}`, values);
export const releasePanelOverride = (panel) => del(`api/overrides/panels/${enc(panel)}`);
export const pushDeviceOverride = (device, values) => put(`api/overrides/devices/${enc(device)}`, values);
export const releaseDeviceOverride = (device) => del(`api/overrides/devices/${enc(device)}`);

/** Save a panel's whole rung (`colors_actual` + `dither`); absent = removed. */
export const savePanel = (panel, rung) => post(`api/panels/${enc(panel)}/save`, rung);
/** Save a device: the admin PATCH body (`DeviceWrite`), behind the dev login. */
export const saveDevice = (key, body) => patch(`api/devices/${enc(key)}`, body);
/** Which devices use which panel, as the server resolves them. */
export const panelUsage = () => json("api/panel-usage");

/** Create `panels.<id>` with `{ id, name, colors }`; 409 when the id is taken. */
export const createPanel = (body) => post("api/panels", body);
/** Add a device: the admin POST body (`DeviceWrite`, with `key`); 409 on a duplicate. */
export const createDevice = (body) => post("api/devices", body);
/** Devices byonk has seen that no `devices:` entry configures yet. */
export const listPendingDevices = () => json("api/pending-devices");

/**
 * POST a preview render (see docs/src/guide/dev-mode.md, "Rendering a
 * preview"). Relative, for HA Ingress. Returns the raw `Response`, so the
 * caller reads the PNG blob and the `X-Byonk-Resolution` header.
 */
export const renderPreview = (body) =>
  fetch("api/render", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
export const eventsUrl = () => "api/events";
