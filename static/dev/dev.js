// Byonk dev-mode editor: three-column screen editor page.
//
// Every URL in this file is relative (`./api/...`, `dev.css`, `login`,
// `logout`) — this page is served both at `http://host:3000/dev/` and,
// under Home Assistant Ingress, at a Supervisor-rewritten prefix the server
// never sees. An origin-absolute URL (anything starting with `/`) would
// escape the ingress iframe onto Home Assistant's own frontend.

"use strict";

// Escapes text that lands in innerHTML — error messages, file names and
// screen refs all originate outside the page.
function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// Extracts the `error` field from a JSON error envelope. Every non-2xx
// response from /dev/api/* is `{"status": N, "error": "..."}`; fall back to
// the status text if the body isn't JSON (e.g. a network-level failure).
async function jsonError(r) {
  try {
    const body = await r.json();
    return body.error || r.statusText;
  } catch (e) {
    return r.statusText;
  }
}

// --- api: fetch helpers over /dev/api/* --------------------------------

const api = {
  async json(path, opts = {}) {
    const r = await fetch(path, opts);
    if (r.status === 401) { location.href = "login"; throw new Error("login required"); }
    if (!r.ok) throw new Error(await jsonError(r));
    return r.status === 204 ? null : r.json();
  },
  repos: () => api.json("./api/repos"),
  screens: () => api.json("./api/screens"),
  files: (ref) => api.json(`./api/files?ref=${encodeURIComponent(ref)}`),
  async readFile(ref, name) {
    const r = await fetch(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`);
    if (r.status === 401) { location.href = "login"; throw new Error("login required"); }
    if (!r.ok) throw new Error(await jsonError(r));
    return { text: await r.text(), etag: r.headers.get("ETag") };
  },
  async writeFile(ref, name, text, etag) {
    const r = await fetch(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`,
      { method: "PUT", headers: { "If-Match": etag, "Content-Type": "text/plain" }, body: text });
    if (r.status === 401) { location.href = "login"; throw new Error("login required"); }
    if (r.status === 412) return { conflict: true };
    if (!r.ok) throw new Error(await jsonError(r));
    return { etag: r.headers.get("ETag") };
  },
  deleteFile: (ref, name) => api.json(`./api/file?ref=${encodeURIComponent(ref)}&name=${encodeURIComponent(name)}`, { method: "DELETE" }),
  create: (handle, name) => api.json("./api/screen", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ handle, name, starter: "minimal" }) }),
  copy: (ref, to_handle, to_name) => api.json("./api/screen/copy", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, to_handle, to_name }) }),
  rename: (ref, name) => api.json("./api/screen/rename", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, name }) }),
  deleteScreen: (ref) => api.json(`./api/screen?ref=${encodeURIComponent(ref)}`, { method: "DELETE" }),
  validate: (ref) => api.json("./api/screen/validate", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref }) }),
  check: (ref, file, text) => api.json("./api/screen/check", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ref, file, text }) }),
  whoami: () => api.json("./api/whoami"),
};

// --- state ---------------------------------------------------------------

const state = {
  repos: [],          // [{handle, writable}]
  screens: [],         // from /api/screens (also devices, panels for the drawer)
  ref: null,           // selected screen ref
  writable: false,
  files: [],           // [{name, size, text}]
  open: new Map(),     // name -> {host, editor, etag, dirty, saved}
  active: null,        // file name

  // --- simulate drawer ---
  timeLocked: false,   // false = the time field follows the clock
  colorPopup: { visible: false, colorIndex: -1, originalHex: "" },
  colorOverrides: {},        // device key -> colors_actual string
  ditherTuningOverrides: {}, // algorithm -> {noiseScale, maxError}
};
window.byonkDev = { state, api, render: null };

// --- localStorage, guarded ------------------------------------------------
//
// Touching localStorage is not safe on this page. With "block all site data"
// set (and in Safari's stricter modes) the ACCESSOR ITSELF throws -- note
// that Chrome's and Firefox's private windows do not: they hand out ephemeral
// storage, so this is not reproducible by opening an incognito window.
//
// An unguarded throw here is not a degraded feature, it aborts whatever was
// half-done around it: `doSelectScreen` would have thrown after setting
// `state.ref` but before painting the tabs, the editor and the preview,
// leaving no way to reach the editor at all, and `init` would have thrown
// before `connectSSE()` and `setupLens()`.
//
// Losing the value is fine. Every caller must work without it.

function safeGet(key) {
  try {
    return localStorage.getItem(key);
  } catch (e) {
    return null;
  }
}

function safeSet(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch (e) { /* the value just will not survive this page */ }
}

function safeRemove(key) {
  try {
    localStorage.removeItem(key);
  } catch (e) { /* nothing was stored, or it cannot be reached */ }
}

// --- colour scheme --------------------------------------------------------
//
// Three states, not a toggle: "system" (the default), "light" and "dark". A
// two-state switch cannot express "follow the system again", so a user who
// touched it could never get back to the default.
//
// "system" is the ABSENCE of `data-theme` on <html>, never the string
// "system" -- dev.css's dark block is
// `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`,
// which only sees an attribute that is missing or "light"/"dark".
//
// The same preference is applied before first paint by a small inline script
// in the <head> of index.html and login.html; this code owns it afterwards.

const THEME_KEY = "byonk-dev-theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

// Anything that is not exactly "light" or "dark" -- including a missing or
// unreadable value -- means System, so the page renders correctly whether
// storage is empty or unreachable.
function readThemeChoice() {
  const v = safeGet(THEME_KEY);
  return v === "light" || v === "dark" ? v : "system";
}

// A choice that cannot be persisted still applies for this page's lifetime.
function storeThemeChoice(choice) {
  if (choice === "system") safeRemove(THEME_KEY);
  else safeSet(THEME_KEY, choice);
}

let themeChoice = readThemeChoice();

// What the page is actually painting right now: "light" or "dark". The
// CodeMirror themes need the resolved value, not the choice.
function currentScheme() {
  if (themeChoice === "light" || themeChoice === "dark") return themeChoice;
  return darkQuery.matches ? "dark" : "light";
}

function applyTheme() {
  const root = document.documentElement;
  if (themeChoice === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", themeChoice);
  // Every open buffer, not just the active one: the others are only hidden,
  // and a stale theme would show the moment their tab is clicked.
  const scheme = currentScheme();
  for (const e of state.open.values()) e.editor.setTheme(scheme);
}

const themeSelect = document.getElementById("theme-select");
themeSelect.value = themeChoice;
themeSelect.addEventListener("change", () => {
  themeChoice = themeSelect.value;
  storeThemeChoice(themeChoice);
  applyTheme();
});

// The system preference can change while the page is open (a scheduled
// switch at dusk, or the OS setting being flipped). The CSS follows it on its
// own; the editors have to be told.
darkQuery.addEventListener("change", () => {
  if (themeChoice === "system") applyTheme();
});

applyTheme();

// --- console ---------------------------------------------------------------

function consoleClear() {
  document.getElementById("console-output").textContent = "";
}

function consoleLog(msg) {
  const time = new Date().toLocaleTimeString();
  const out = document.getElementById("console-output");
  const line = document.createElement("span");
  line.className = "console-info";
  line.textContent = `[${time}] ${msg}\n`;
  out.appendChild(line);
  out.parentElement.scrollTop = out.parentElement.scrollHeight;
}

function consoleError(msg) {
  const time = new Date().toLocaleTimeString();
  const out = document.getElementById("console-output");
  const line = document.createElement("span");
  line.className = "console-error";
  line.textContent = `[${time}] ERROR: ${msg}\n`;
  out.appendChild(line);
  out.parentElement.scrollTop = out.parentElement.scrollHeight;
}

// The 1:1 pixel lens (`setupLens`, `updateLensImage`) lives in lens.js, which
// index.html loads before this file: the pop-out preview window needs the
// same magnifier and must not carry a second copy of it.

// --- device frame -----------------------------------------------------
//
// Two functions, two jobs, and they must not be confused:
//
// `getEffectiveDimensions` reads the selected panel profile. It is what the
// page ASKS for — the width/height in the render query, and the frame class
// `updateDeviceFrame` applies the moment the panel select changes, ahead of
// the network round-trip.
//
// `updateDisplaySizeFromImage` reads the PNG that came back. It is the truth
// about what is on screen — the server may render at a size the request did
// not ask for — so the displayed size and the pop-out broadcast come from it
// and never from the panel select.

const DEFAULTS = {
  width: "800",
  height: "480",
  // Mirrors `DEFAULT_COLORS` in src/api/display.rs: the palette the server
  // falls back to for the `og` model when no panel is selected. Only the
  // swatch strip uses it — the render query omits `colors` entirely and lets
  // the server apply the same fallback.
  colors: "#000000,#555555,#AAAAAA,#FFFFFF",
};

function getEffectiveDimensions() {
  const panelSelect = document.getElementById("panel-select");
  const opt = panelSelect && panelSelect.selectedOptions[0];
  if (opt && opt.value && opt.dataset.width) {
    return { width: opt.dataset.width, height: opt.dataset.height };
  }
  return { width: DEFAULTS.width, height: DEFAULTS.height };
}

function getEffectiveColors() {
  const panelSelect = document.getElementById("panel-select");
  const opt = panelSelect && panelSelect.selectedOptions[0];
  if (opt && opt.value && opt.dataset.colors) return opt.dataset.colors;
  return DEFAULTS.colors;
}

function updateDeviceFrame() {
  const dims = getEffectiveDimensions();
  const frameClass = (parseInt(dims.width) > 1000 || parseInt(dims.height) > 1000) ? "x" : "og";
  const frame = document.getElementById("device-frame");
  frame.classList.remove("og", "x");
  frame.classList.add(frameClass);
  document.getElementById("display-size").textContent = `${dims.width} x ${dims.height}`;
}

function updateDisplaySizeFromImage() {
  const img = document.getElementById("display-image");
  if (!img.naturalWidth || !img.naturalHeight) return;
  document.getElementById("display-size").textContent = `${img.naturalWidth} x ${img.naturalHeight}`;
  const frame = document.getElementById("device-frame");
  const hiRes = img.naturalWidth > 1000 || img.naturalHeight > 1000;
  frame.classList.toggle("x", hiRes);
  frame.classList.toggle("og", !hiRes);
}

// The preview image has decoded, so its real size is finally known. Both the
// size readout and the pop-out broadcast hang off this event: at the point
// `render()` finishes, `img.naturalWidth` still belongs to the PREVIOUS
// image, so a message posted there would carry the wrong dimensions on every
// render but the first.
function onPreviewImageLoad() {
  updateDisplaySizeFromImage();
  broadcastPreview();
}

// The only message the pop-out ever receives. Both the `load` event above and
// a newly-opened pop-out's `hello` come through here.
//
// It reads the LIVE <img> every time and never a remembered copy of an
// earlier message. The URL is a blob this page owns, and `render()` revokes
// the previous blob in the same breath as it assigns the next one, so a
// remembered URL can already be dead — most plainly when a render lands but
// its PNG never decodes: `load` does not fire, the element moves on, and the
// remembered URL names a blob that was revoked. `img.src` is by construction
// the one blob this page still holds, and `naturalWidth`/`naturalHeight`
// belong to that same image, so the pop-out can always fetch what it is told
// about. Before the first image has decoded there is nothing to say and the
// message is skipped; the `load` that follows reaches every listener, the
// new pop-out included.
function broadcastPreview() {
  const img = document.getElementById("display-image");
  if (!previewChannel || !img.naturalWidth || !img.naturalHeight) return;
  previewChannel.postMessage({
    url: img.src,
    width: img.naturalWidth,
    height: img.naturalHeight,
  });
}

// --- screens list -------------------------------------------------------

async function loadScreens() {
  try {
    const [repos, screens] = await Promise.all([api.repos(), api.screens()]);
    state.repos = repos;
    state.screens = screens;
  } catch (e) {
    consoleError("Failed to load screens: " + e.message);
    return;
  }
  renderScreenList();
}

function renderScreenList() {
  const container = document.getElementById("screen-list");
  const byHandle = new Map();
  for (const s of state.screens.screens || []) {
    const slash = s.name.indexOf("/");
    const handle = slash === -1 ? s.name : s.name.slice(0, slash);
    const list = byHandle.get(handle) || [];
    list.push(s);
    byHandle.set(handle, list);
  }

  let html = "";
  const renderRepo = (handle, writable, screens) => {
    const ro = writable ? "" : " ro";
    html += `<div class="repo${ro}">${escapeHtml(handle)}</div>`;
    for (const s of screens) {
      const sel = s.name === state.ref ? " sel" : "";
      const label = s.name.slice(handle.length + 1);
      html += `<div class="scr${sel}${ro}" data-ref="${escapeHtml(s.name)}">${escapeHtml(label)}</div>`;
    }
  };

  for (const repo of state.repos) {
    renderRepo(repo.handle, repo.writable, byHandle.get(repo.handle) || []);
    byHandle.delete(repo.handle);
  }
  // Screens whose handle wasn't in /api/repos (shouldn't normally happen).
  for (const [handle, screens] of byHandle) {
    renderRepo(handle, false, screens);
  }

  container.innerHTML = html;
  container.querySelectorAll(".scr").forEach((el) => {
    el.addEventListener("click", () => selectScreen(el.dataset.ref));
  });
}

function hasDirtyBuffers() {
  return [...state.open.values()].some((e) => e.dirty);
}

// The single place that answers "are there unsaved changes?" in the UI. The
// Save button must be enabled exactly when clicking it would write
// something — it saves *every* dirty buffer, not just the active one — and
// the `#current-ref` dot and the beforeunload guard use the same predicate.
function syncDirtyIndicators() {
  const anyDirty = hasDirtyBuffers();
  document.getElementById("save-btn").disabled = !state.writable || !anyDirty;
  window.onbeforeunload = anyDirty ? () => true : null;
  document.getElementById("current-ref").classList.toggle("dirty", anyDirty);
}

// Destroys every open editor and its buffer. Only called where the user has
// already agreed to lose the edits: a screen switch or a rename confirmed
// through `confirmDiscard`, or a screen deletion.
function discardBuffers() {
  for (const e of state.open.values()) e.editor.destroy();
  state.open.clear();
  document.getElementById("editor-host").innerHTML = "";
  state.active = null;
  syncDirtyIndicators();
}

function confirmDiscard(onConfirm) {
  const p = document.getElementById("problems");
  p.innerHTML = '<span class="err">Unsaved changes.</span> ';
  const discard = Object.assign(document.createElement("button"), { textContent: "Discard" });
  discard.onclick = () => { showProblems([]); onConfirm(); };
  const cancel = Object.assign(document.createElement("button"), { textContent: "Cancel" });
  cancel.onclick = () => showProblems([]);
  p.append(discard, " ", cancel);
}

async function selectScreen(ref) {
  if (ref === state.ref) return;
  if (hasDirtyBuffers()) {
    confirmDiscard(() => doSelectScreen(ref));
    return;
  }
  await doSelectScreen(ref);
}

async function doSelectScreen(ref) {
  discardBuffers();
  clearFailure();

  state.ref = ref;
  const repo = state.repos.find((r) => ref.startsWith(r.handle + "/"));
  state.writable = !!(repo && repo.writable);

  try {
    state.files = await api.files(ref);
  } catch (e) {
    // Not the console: the `render()` at the end of this function clears it.
    showFailure("Failed to load files: " + e.message);
    state.files = [];
  }

  document.getElementById("current-ref").textContent = ref;
  document.getElementById("copy-btn").disabled = false;
  document.getElementById("rename-btn").disabled = !state.writable;
  document.getElementById("delete-btn").disabled = !state.writable;
  syncDirtyIndicators();
  safeSet("byonkDevRef", ref);

  renderScreenList();
  renderTabs();
  showProblems([]);

  if (state.files.some((f) => f.name === "script.lua" && f.text)) {
    await openFile("script.lua");
  }
  render();
}

// --- tabs and editor instances -------------------------------------------

function languageFor(name) {
  if (name.endsWith(".lua")) return "lua";
  if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
  if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
  return "text";
}

async function openFile(name) {
  let entry = state.open.get(name);
  if (!entry) {
    let text, etag;
    try {
      ({ text, etag } = await api.readFile(state.ref, name));
    } catch (e) {
      // Not the console: `doSelectScreen`'s `render()` clears it right after.
      showFailure(`Failed to open ${name}: ` + e.message);
      return;
    }
    const host = document.createElement("div");
    host.className = "editor-instance";
    document.getElementById("editor-host").appendChild(host);
    entry = { host, etag, dirty: false, saved: text, editor: null };
    entry.editor = byonkEditor.create(host, {
      language: languageFor(name), doc: text, readOnly: !state.writable,
      // A buffer opened after the user switched schemes must start in the
      // current one; `setTheme` only reaches buffers that already exist.
      theme: currentScheme(),
      onChange: () => { markDirty(name, entry.editor.getValue() !== entry.saved); scheduleCheck(name); },
      onSave: () => saveFile(name),
    });
    state.open.set(name, entry);
  }
  state.active = name;
  for (const [n, e] of state.open) e.host.hidden = n !== name;
  renderTabs();
  entry.editor.focus();
}

function closeTab(name) {
  const e = state.open.get(name);
  if (!e) return;
  const doClose = () => {
    e.editor.destroy();
    e.host.remove();
    state.open.delete(name);
    if (state.active === name) {
      state.active = null;
      const remaining = [...state.open.keys()];
      if (remaining.length) {
        openFile(remaining[remaining.length - 1]);
      } else {
        renderTabs();
      }
    } else {
      renderTabs();
    }
    syncDirtyIndicators();
  };
  if (e.dirty) {
    confirmDiscard(doClose);
  } else {
    doClose();
  }
}

function renderTabs() {
  const el = document.getElementById("tabs");
  const core = ["meta.yaml", "script.lua", "screen.svg"];
  const files = [...state.files].sort((a, b) => {
    const ia = core.indexOf(a.name), ib = core.indexOf(b.name);
    if (ia !== -1 || ib !== -1) return (ia === -1 ? core.length : ia) - (ib === -1 ? core.length : ib);
    return a.name.localeCompare(b.name);
  });

  let html = "";
  for (const f of files) {
    if (!f.text) {
      const size = f.size != null ? ` (${f.size})` : "";
      html += `<span class="tab disabled">${escapeHtml(f.name)}${escapeHtml(size)}</span>`;
      continue;
    }
    const open = state.open.get(f.name);
    const on = state.active === f.name ? " on" : "";
    const dirty = open && open.dirty ? " dirty" : "";
    html += `<span class="tab${on}${dirty}" data-name="${escapeHtml(f.name)}">` +
      `${escapeHtml(f.name)}<button type="button" class="tab-close" data-name="${escapeHtml(f.name)}">&times;</button></span>`;
  }
  html += `<span class="tab" id="new-file-tab">+</span>`;
  el.innerHTML = html;

  el.querySelectorAll(".tab[data-name]").forEach((t) => {
    t.addEventListener("click", (e) => {
      if (e.target.closest(".tab-close")) return;
      openFile(t.dataset.name);
    });
  });
  el.querySelectorAll(".tab-close").forEach((btn) => {
    btn.addEventListener("click", (e) => { e.stopPropagation(); closeTab(btn.dataset.name); });
  });
  const newTab = document.getElementById("new-file-tab");
  if (newTab) newTab.addEventListener("click", () => showNewFileForm());
}

function markDirty(name, dirty) {
  const e = state.open.get(name);
  if (!e || e.dirty === dirty) return;
  e.dirty = dirty;
  renderTabs();
  syncDirtyIndicators();
}

let checkTimer = null;
function scheduleCheck(name) {
  clearTimeout(checkTimer);
  checkTimer = setTimeout(async () => {
    const e = state.open.get(name);
    if (!e) return;
    try {
      const report = await api.check(state.ref, name, e.editor.getValue());
      e.editor.setDiagnostics(report.issues);
      showProblems(report.issues.map((i) => ({ ...i, location: name })));
    } catch (err) {
      consoleError("Check failed: " + err.message);
    }
  }, 400);
}

// --- failures that must survive the next repaint -------------------------
//
// A failed write (read-only repo, a 409 race, a 5xx, a dropped connection)
// has to stay in front of the user until they do something about it. Neither
// existing region can hold it: `#console` is emptied by `consoleClear()` at
// the top of every `render()`, and `render()` runs on every SSE `file-change`
// and `refresh`; `#problems` is overwritten 400 ms after the next keystroke
// by `scheduleCheck`'s `showProblems`, and also carries the conflict banner
// and the discard prompt. So failures get a fourth, single-purpose region
// that only the user's dismiss button or the next successful write clears.

function showFailure(msg) {
  // Unhide before filling: the bar is a live region, and an assistive
  // technology only announces text that changes while the region is visible.
  document.getElementById("failure-bar").hidden = false;
  document.getElementById("failure-text").textContent = msg;
  consoleError(msg); // keep the timestamped trail in the console as well
}

function clearFailure() {
  document.getElementById("failure-bar").hidden = true;
  document.getElementById("failure-text").textContent = "";
}

document.getElementById("failure-dismiss").addEventListener("click", clearFailure);

// --- saving ---------------------------------------------------------------

// Writes one buffer. Returns true only when the bytes really reached disk;
// every false return has already put a message on screen (the failure bar,
// or the conflict banner for a 412).
async function writeBuffer(name) {
  const e = state.open.get(name);
  if (!e) return false;
  if (!state.writable) {
    // Reachable via the editor's Cmd/Ctrl-S, which fires even in a read-only
    // buffer. Silence here would be one more invisible write failure.
    showFailure(`${state.ref} is read-only — ${name} was not saved.`);
    return false;
  }
  const text = e.editor.getValue();
  let r;
  try {
    r = await api.writeFile(state.ref, name, text, e.etag);
  } catch (err) {
    showFailure(`Saving ${name} failed: ` + err.message);
    return false;
  }
  if (r.conflict) { showConflict(name); return false; }
  e.etag = r.etag; e.saved = text; markDirty(name, false);
  clearFailure();
  return true;
}

// Runs after a write actually succeeded — never before, so "✔ all files
// valid" can never appear over a buffer that failed to save.
async function afterWrite() {
  try {
    const report = await api.validate(state.ref);
    showProblems(report.issues, report.ok);
  } catch (err) {
    showFailure("Validate failed: " + err.message);
  }
  render();
}

// Single-file save: the editor's Cmd/Ctrl-S and the conflict banner's
// Overwrite, both of which name the file they act on.
async function saveFile(name) {
  if (await writeBuffer(name)) await afterWrite();
}

// The toolbar's Save button. It is enabled whenever *any* buffer is dirty,
// so it has to save all of them: saving only `state.active` re-wrote an
// unchanged file, bumped its etag and painted "all files valid" while the
// dirty buffer stayed on the floor.
async function saveAllDirty() {
  const names = [...state.open].filter(([, e]) => e.dirty).map(([n]) => n);
  if (!names.length) return;
  for (const name of names) {
    // Stop at the first refusal: its banner or failure bar needs the user's
    // decision before the remaining buffers follow it to disk.
    if (!(await writeBuffer(name))) return;
  }
  await afterWrite();
}

function showConflict(name) {
  const p = document.getElementById("problems");
  p.innerHTML = `<span class="err">${escapeHtml(name)} changed on disk.</span> `;
  const reload = Object.assign(document.createElement("button"), { textContent: "Reload" });
  // Both handlers are async and both can fail — a dropped connection, or the
  // 401 path, which throws after starting the redirect. Without the catch
  // the click was an unhandled rejection and the user saw nothing happen.
  // The buffer can also be gone by now: the tab is still closable while the
  // banner is up.
  reload.onclick = async () => {
    const e = state.open.get(name);
    if (!e) { showProblems([]); return; }
    try {
      const f = await api.readFile(state.ref, name);
      e.editor.setValue(f.text); e.etag = f.etag; e.saved = f.text;
      markDirty(name, false); showProblems([]);
    } catch (err) {
      showFailure(`Reloading ${name} failed: ` + err.message);
    }
  };
  const over = Object.assign(document.createElement("button"), { textContent: "Overwrite" });
  over.onclick = async () => {
    const e = state.open.get(name);
    if (!e) { showProblems([]); return; }
    try {
      const f = await api.readFile(state.ref, name);
      e.etag = f.etag;
      await saveFile(name);
    } catch (err) {
      showFailure(`Overwriting ${name} failed: ` + err.message);
    }
  };
  p.append(reload, " ", over);
}

function showProblems(issues, ok) {
  const p = document.getElementById("problems");
  if (!issues.length) { p.innerHTML = ok === undefined ? "" : `<span class="ok">✔ all files valid</span>`; return; }
  p.innerHTML = issues.map((i) => `<div class="err">✖ ${escapeHtml(i.location)}${i.line ? ":" + i.line : ""} ${escapeHtml(i.message)}</div>`).join("");
}

// --- toolbar: New / Copy / Rename / Delete + inline form -----------------

const inlineForm = document.getElementById("inline-form");
const inlineTitle = document.getElementById("inline-title");
const inlineRepo = document.getElementById("inline-repo");
const inlineName = document.getElementById("inline-name");
let inlineMode = null;

function showInline(mode, opts = {}) {
  inlineMode = mode;
  inlineTitle.textContent = opts.title || "";
  inlineName.value = opts.name || "";
  inlineName.placeholder = opts.placeholder || "name";

  const showRepo = mode === "new" || mode === "copy";
  inlineRepo.hidden = !showRepo;
  const showName = mode !== "delete";
  inlineName.hidden = !showName;
  // `hidden` does not exempt a control from constraint validation: leaving
  // `required` set on the hidden, empty name input made the Delete form
  // unsubmittable (the browser refuses, then cannot focus the field to say
  // why). The flag has to track visibility.
  inlineName.required = showName;

  if (showRepo) {
    inlineRepo.innerHTML = state.repos
      .filter((r) => r.writable)
      .map((r) => `<option value="${escapeHtml(r.handle)}">${escapeHtml(r.handle)}</option>`)
      .join("");
  }

  inlineForm.hidden = false;
  (showName ? inlineName : inlineForm.querySelector("button[type=submit]")).focus();
}

function hideInline() {
  inlineForm.hidden = true;
  inlineMode = null;
}

document.getElementById("inline-cancel").addEventListener("click", hideInline);

inlineForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const mode = inlineMode;
  const repoHandle = inlineRepo.value;
  const name = inlineName.value.trim();
  // A rename moves the screen directory out from under every open buffer:
  // the moment the server returns, the ref those buffers were read from no
  // longer exists, so an unsaved edit becomes unsavable (every later PUT
  // 404s) and copy-paste is the only way out. Ask *before* the rename, while
  // the old ref is still there to save into. `selectScreen`'s own check
  // fires too late — after the rename has already happened.
  if (mode === "rename" && hasDirtyBuffers()) {
    confirmDiscard(() => runInline(mode, repoHandle, name));
    return;
  }
  runInline(mode, repoHandle, name);
});

// Split out of the submit listener so the rename path can also reach it from
// the discard prompt's callback. Handles its own errors; never rejects.
async function runInline(mode, repoHandle, name) {
  try {
    if (mode === "new") {
      const r = await api.create(repoHandle, name);
      hideInline();
      await loadScreens();
      await selectScreen(r.ref);
    } else if (mode === "copy") {
      const r = await api.copy(state.ref, repoHandle, name);
      hideInline();
      await loadScreens();
      await selectScreen(r.ref);
    } else if (mode === "rename") {
      const r = await api.rename(state.ref, name);
      hideInline();
      // Only now that the rename has actually happened do the old ref's
      // buffers become unsavable — if `api.rename` had thrown, they would
      // still belong to a live ref and must survive. `doSelectScreen`, not
      // `selectScreen`: the buffers are gone so there is nothing left to
      // confirm, and it must not no-op if `r.ref` came back unchanged.
      discardBuffers();
      await loadScreens();
      await doSelectScreen(r.ref);
    } else if (mode === "delete") {
      await api.deleteScreen(state.ref);
      hideInline();
      state.ref = null;
      state.writable = false;
      // The file list belongs to the screen that just stopped existing.
      // Leaving it behind made `renderTabs()` redraw the dead screen's tabs,
      // and clicking one issued `./api/file?ref=null&…`.
      state.files = [];
      discardBuffers();
      document.getElementById("current-ref").textContent = "no screen";
      document.getElementById("copy-btn").disabled = true;
      document.getElementById("rename-btn").disabled = true;
      document.getElementById("delete-btn").disabled = true;
      safeRemove("byonkDevRef");
      renderTabs();
      showProblems([]);
      await loadScreens();
    } else if (mode === "newfile") {
      // `If-Match: *` means "create, and fail with 412 if it already
      // exists" (src/api/dev_files.rs), so a name collision comes back as a
      // conflict rather than a throw — without this it silently opened the
      // existing file as if it had just been created.
      const w = await api.writeFile(state.ref, name, "", "*");
      if (w.conflict) { showFailure(`${name} already exists.`); return; }
      hideInline();
      clearFailure();
      state.files = await api.files(state.ref);
      renderTabs();
      await openFile(name);
    }
  } catch (err) {
    // These are all write-path failures; `showProblems` would lose them to
    // the next keystroke's check.
    showFailure(`${mode}${name ? " " + name : ""} failed: ` + err.message);
  }
}

document.getElementById("new-btn").addEventListener("click", () => showInline("new", { title: "New screen" }));

document.getElementById("copy-btn").addEventListener("click", () => {
  if (!state.ref) return;
  const srcName = state.ref.slice(state.ref.indexOf("/") + 1);
  showInline("copy", { title: `Copy ${state.ref}`, name: srcName });
});

document.getElementById("rename-btn").addEventListener("click", () => {
  if (!state.ref) return;
  const srcName = state.ref.slice(state.ref.indexOf("/") + 1);
  showInline("rename", { title: `Rename ${state.ref}`, name: srcName });
});

document.getElementById("delete-btn").addEventListener("click", () => {
  if (!state.ref) return;
  showInline("delete", { title: `Delete ${state.ref}?` });
});

function showNewFileForm() {
  if (!state.ref || !state.writable) return;
  showInline("newfile", { title: "New file", placeholder: "filename" });
}

document.getElementById("save-btn").addEventListener("click", () => {
  saveAllDirty();
});

// --- simulate drawer ------------------------------------------------------
//
// The old single-column page's configuration panel, moved into a drawer that
// slides over the preview column. The Screen select it used to carry is gone
// — the screens column owns that — and the Device select that lived inside it
// stands on its own.
//
// Visibility is the native `hidden` attribute throughout: dev.css has no
// generic `.hidden` rule (only `.lens.hidden`, `.color-popup.hidden` and the
// loading overlay), so the class the old page toggled would do nothing here.

const sim = {
  root: document.getElementById("drawer"),
  toggle: document.getElementById("simulate-toggle"),
  device: document.getElementById("device-select"),
  deviceInfo: document.getElementById("device-info"),
  panel: document.getElementById("panel-select"),
  swatches: document.getElementById("color-swatches"),
  useActual: document.getElementById("use-actual"),
  useActualLabel: document.getElementById("use-actual-label"),
  dither: document.getElementById("dither-select"),
  maxError: document.getElementById("max-error"),
  chromaClamp: document.getElementById("chroma-clamp"),
  noiseScale: document.getElementById("noise-scale"),
  strength: document.getElementById("strength"),
  battery: document.getElementById("battery-input"),
  rssi: document.getElementById("rssi-input"),
  time: document.getElementById("time-input"),
  timeMode: document.getElementById("time-mode"),
  timeReset: document.getElementById("time-reset"),
  params: document.getElementById("params-input"),
};

const popup = {
  root: document.getElementById("color-popup"),
  close: document.getElementById("color-popup-close"),
  original: document.getElementById("popup-original-color"),
  live: document.getElementById("popup-live-color"),
  hue: document.getElementById("popup-hue"),
  sat: document.getElementById("popup-sat"),
  lit: document.getElementById("popup-lit"),
  hueVal: document.getElementById("popup-hue-val"),
  satVal: document.getElementById("popup-sat-val"),
  litVal: document.getElementById("popup-lit-val"),
  hex: document.getElementById("popup-hex-input"),
  reset: document.getElementById("popup-reset-btn"),
};

function setDrawerOpen(open) {
  if (open) {
    sim.root.hidden = false;
    // Flush layout between un-hiding and the class change: without a start
    // value in the same frame the browser has nothing to slide from.
    void sim.root.offsetWidth;
    sim.root.classList.add("open");
  } else {
    sim.root.classList.remove("open");
    // Stay in the layout until the slide-out has run; `hidden` immediately
    // would cut it short. A timer rather than `transitionend`, which never
    // arrives when the transition is suppressed.
    setTimeout(() => {
      if (!sim.root.classList.contains("open")) sim.root.hidden = true;
    }, 250);
  }
  sim.toggle.setAttribute("aria-expanded", open ? "true" : "false");
}

// A select whose value matches no option goes blank. Device configs name
// panels and algorithms that a later edit of config.yaml can remove, so an
// unknown value leaves the current choice standing instead.
function selectIfPresent(select, value) {
  if (!value) return false;
  if (![...select.options].some((o) => o.value === value)) return false;
  select.value = value;
  return true;
}

function splitColors(s) {
  return s ? s.split(",").map((c) => c.trim()).filter((c) => c) : [];
}

// --- drawer: time ---------------------------------------------------------

function formatDatetimeLocal(d) {
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function tickTime() {
  if (!state.timeLocked) sim.time.value = formatDatetimeLocal(new Date());
}

function setTimeLocked(locked) {
  state.timeLocked = locked;
  sim.timeMode.textContent = locked ? "frozen" : "live";
  sim.timeMode.className = "time-mode " + (locked ? "frozen" : "live");
  sim.timeReset.hidden = !locked;
  if (!locked) tickTime();
}

// --- drawer: option lists -------------------------------------------------
//
// All three come from `/dev/api/screens` and are built once: devices, panels
// and algorithms all live in config.yaml, which this page cannot change.

function populateDeviceSelect() {
  const devices = [...((state.screens && state.screens.devices) || [])].sort((a, b) =>
    a.id.localeCompare(b.id));
  for (const d of devices) {
    const o = document.createElement("option");
    o.value = d.id;
    o.textContent = `${d.id} → ${d.screen}`;
    if (d.panel) o.dataset.panel = d.panel;
    if (d.dither) o.dataset.dither = d.dither;
    sim.device.appendChild(o);
  }
}

function populatePanelSelect() {
  for (const p of (state.screens && state.screens.panels) || []) {
    const o = document.createElement("option");
    o.value = p.id;
    o.textContent = p.name;
    o.dataset.colors = p.colors;
    if (p.colors_actual) o.dataset.colorsActual = p.colors_actual;
    if (p.width) o.dataset.width = p.width;
    if (p.height) o.dataset.height = p.height;
    sim.panel.appendChild(o);
  }
}

function populateDitherSelect() {
  const list = (state.screens && state.screens.dither_algorithms) || [];
  sim.dither.innerHTML = "";
  for (const a of list) {
    const o = document.createElement("option");
    // The canonical name verbatim: it is exactly what goes in config.yaml
    // and in `?dither=`, so a prettified label would hide the one string
    // that matters when the setting is copied out of here.
    o.value = a.name;
    o.textContent = a.name;
    sim.dither.appendChild(o);
  }
  // The engine's own fallback, so an untouched drawer asks for what the
  // server would have rendered anyway.
  selectIfPresent(sim.dither, "atkinson");
}

// --- drawer: dither tuning ------------------------------------------------
//
// The defaults come from the server (`DitherAlgorithm::defaults()`), not from
// a table in this file: the copy the old page carried could disagree with the
// engine, and then the preview lied about what the device would show.

function ditherDefaults(algo) {
  const list = (state.screens && state.screens.dither_algorithms) || [];
  const hit = list.find((a) => a.name === algo);
  return hit ? hit.defaults : null;
}

function applyDitherDefaults() {
  const algo = sim.dither.value;
  const override = state.ditherTuningOverrides[algo] || {};
  const d = ditherDefaults(algo) || {};
  if (override.noiseScale !== undefined) sim.noiseScale.value = override.noiseScale;
  else if (d.noise_scale !== undefined) sim.noiseScale.value = d.noise_scale;
  if (override.maxError !== undefined) sim.maxError.value = override.maxError;
  else if (d.max_error !== undefined) sim.maxError.value = d.max_error;
}

function saveDitherTuningOverride() {
  state.ditherTuningOverrides[sim.dither.value] = {
    noiseScale: sim.noiseScale.value,
    maxError: sim.maxError.value,
  };
}

// --- drawer: colour swatches ----------------------------------------------

function updateColorSwatches() {
  const colors = splitColors(getEffectiveColors());
  const opt = sim.panel.selectedOptions[0];
  const actual = splitColors(opt && opt.dataset.colorsActual);

  if (!colors.length) { sim.swatches.innerHTML = ""; return; }

  let html = '<div class="swatch-label-row"><span class="swatch-label">Official</span></div>' +
    '<div class="swatch-row">';
  colors.forEach((c, i) => {
    const title = `Official: ${c}` + (actual[i] ? `\nActual: ${actual[i]}` : "");
    html += `<span class="swatch" style="background:${escapeHtml(c)}" title="${escapeHtml(title)}"></span>`;
  });
  html += "</div>";

  if (actual.length) {
    html += '<div class="swatch-label-row"><span class="swatch-label">Actual</span>' +
      '<button type="button" class="copy-colors-btn">Copy color config</button></div>' +
      '<div class="swatch-row actual">';
    actual.forEach((c, i) => {
      html += `<span class="swatch" data-index="${i}" style="background:${escapeHtml(c)}" ` +
        `title="${escapeHtml(`Actual: ${c} (click to adjust)`)}"></span>`;
    });
    html += "</div>";
  }
  sim.swatches.innerHTML = html;

  // Listeners rather than the old inline `onclick=`, which needed the
  // handlers to be globals; this file puts exactly one object on `window`.
  const copyBtn = sim.swatches.querySelector(".copy-colors-btn");
  if (copyBtn) copyBtn.addEventListener("click", copyActualColors);
  sim.swatches.querySelectorAll(".swatch-row.actual .swatch").forEach((el) => {
    el.addEventListener("click", () => openColorPopup(el, Number(el.dataset.index)));
  });
}

// The checkbox only means something for a panel that has measured colours.
function updateUseActualVisibility() {
  const opt = sim.panel.selectedOptions[0];
  sim.useActualLabel.hidden = !(opt && opt.dataset.colorsActual);
}

// Restored overrides have to reach the server too, so the production
// `/api/display` handler renders the real device with the colours the drawer
// is showing.
function applyRestoredColorOverrides() {
  const deviceKey = sim.device.value;
  if (!deviceKey) return;
  const override = state.colorOverrides[deviceKey];
  if (!override) return;

  const opt = sim.panel.selectedOptions[0];
  if (opt && opt.value) opt.dataset.colorsActual = override;

  fetch("./api/panel-colors", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ device: deviceKey, colors_actual: override }),
  }).catch((e) => consoleError("Failed to restore panel colors: " + e.message));
}

// --- drawer: persistence --------------------------------------------------

const SIM_STATE_KEY = "byonk-dev-state";

function saveState() {
  safeSet(SIM_STATE_KEY, JSON.stringify({
    device: sim.device.value,
    panel: sim.panel.value,
    battery: sim.battery.value,
    rssi: sim.rssi.value,
    timeLocked: state.timeLocked,
    time: state.timeLocked ? sim.time.value : "",
    params: sim.params.value,
    useActual: sim.useActual.checked,
    dither: sim.dither.value,
    maxError: sim.maxError.value,
    chromaClamp: sim.chromaClamp.value,
    noiseScale: sim.noiseScale.value,
    strength: sim.strength.value,
    colorOverrides: state.colorOverrides,
    ditherTuningOverrides: state.ditherTuningOverrides,
  }));
}

function loadSavedState() {
  const raw = safeGet(SIM_STATE_KEY);
  if (!raw) return;
  let data;
  try {
    data = JSON.parse(raw);
  } catch (e) {
    return; // a half-written or foreign value: start from the defaults
  }
  if (!data || typeof data !== "object") return;

  selectIfPresent(sim.device, data.device);
  selectIfPresent(sim.panel, data.panel);
  if (data.battery) sim.battery.value = data.battery;
  if (data.rssi) sim.rssi.value = data.rssi;
  if (data.timeLocked && data.time) {
    sim.time.value = data.time;
    setTimeLocked(true);
  }
  if (data.params) sim.params.value = data.params;
  if (typeof data.useActual === "boolean") sim.useActual.checked = data.useActual;
  selectIfPresent(sim.dither, data.dither);
  if (data.maxError) {
    sim.maxError.value = data.maxError;
  } else if (data.errorClamp) {
    // Deliberately NOT migrated to maxError. In 0.18.0 this knob stopped
    // capping the pixel value and started capping the accumulated error,
    // moving its useful range from ~0.1 to ~1.0 while keeping its name —
    // which is the whole reason it was renamed. A saved value cannot be
    // dated, so carrying it across would risk reviving a number chosen for
    // the old meaning, the exact failure the rename exists to prevent. The
    // server discards `error_clamp` for the same reason; the dev UI matches
    // it, but says so rather than dropping it in silence.
    console.warn(
      `byonk dev: ignoring saved error_clamp=${data.errorClamp}. ` +
      `It was renamed to max_error in 0.18.0 and changed meaning ` +
      `(useful range ~0.1 -> ~1.0), so the old value is not carried ` +
      `over. Set max_error yourself if you still want an override.`
    );
  }
  if (typeof data.chromaClamp === "string") sim.chromaClamp.value = data.chromaClamp;
  if (data.noiseScale) sim.noiseScale.value = data.noiseScale;
  if (data.strength) sim.strength.value = data.strength;
  if (data.colorOverrides && typeof data.colorOverrides === "object") {
    state.colorOverrides = data.colorOverrides;
  }
  if (data.ditherTuningOverrides && typeof data.ditherTuningOverrides === "object") {
    state.ditherTuningOverrides = data.ditherTuningOverrides;
  }

  sim.deviceInfo.hidden = !sim.device.value;
  applyRestoredColorOverrides();
}

// --- drawer: colour conversion --------------------------------------------

function hexToRgb(hex) {
  hex = hex.replace("#", "");
  return {
    r: parseInt(hex.substring(0, 2), 16),
    g: parseInt(hex.substring(2, 4), 16),
    b: parseInt(hex.substring(4, 6), 16),
  };
}

function rgbToHex(r, g, b) {
  return "#" + [r, g, b]
    .map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0"))
    .join("").toUpperCase();
}

function rgbToHsl(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  let h, s;
  const l = (max + min) / 2;
  if (max === min) {
    h = s = 0;
  } else {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r: h = ((g - b) / d + (g < b ? 6 : 0)) / 6; break;
      case g: h = ((b - r) / d + 2) / 6; break;
      default: h = ((r - g) / d + 4) / 6; break;
    }
  }
  return { h: Math.round(h * 360), s: Math.round(s * 100), l: Math.round(l * 100) };
}

function hslToRgb(h, s, l) {
  h /= 360; s /= 100; l /= 100;
  let r, g, b;
  if (s === 0) {
    r = g = b = l;
  } else {
    const hue2rgb = (p, q, t) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hue2rgb(p, q, h + 1 / 3);
    g = hue2rgb(p, q, h);
    b = hue2rgb(p, q, h - 1 / 3);
  }
  return { r: Math.round(r * 255), g: Math.round(g * 255), b: Math.round(b * 255) };
}

function hexToHsl(hex) {
  const { r, g, b } = hexToRgb(hex);
  return rgbToHsl(r, g, b);
}

function hslToHex(h, s, l) {
  const { r, g, b } = hslToRgb(h, s, l);
  return rgbToHex(r, g, b);
}

// --- drawer: colour popup -------------------------------------------------

function openColorPopup(swatchEl, index) {
  const panelOption = sim.panel.selectedOptions[0];
  const actual = splitColors(panelOption && panelOption.dataset.colorsActual);
  const hex = actual[index];
  if (!hex) return;

  state.colorPopup.visible = true;
  state.colorPopup.colorIndex = index;
  state.colorPopup.originalHex = hex;

  const rect = swatchEl.getBoundingClientRect();
  const popupWidth = 280;
  let left = rect.left;
  let top = rect.bottom + 8;
  if (left + popupWidth > window.innerWidth - 16) left = window.innerWidth - popupWidth - 16;
  if (top + 340 > window.innerHeight) top = rect.top - 340;
  popup.root.style.left = `${left}px`;
  popup.root.style.top = `${top}px`;

  popup.original.style.background = hex;
  popup.live.style.background = hex;

  const hsl = hexToHsl(hex);
  popup.hue.value = hsl.h;
  popup.sat.value = hsl.s;
  popup.lit.value = hsl.l;
  popup.hueVal.textContent = hsl.h;
  popup.satVal.textContent = hsl.s + "%";
  popup.litVal.textContent = hsl.l + "%";
  popup.hex.value = hex;

  popup.root.classList.remove("hidden");
}

function closeColorPopup() {
  state.colorPopup.visible = false;
  popup.root.classList.add("hidden");
}

function updateColorFromSliders() {
  const h = parseInt(popup.hue.value);
  const s = parseInt(popup.sat.value);
  const l = parseInt(popup.lit.value);
  popup.hueVal.textContent = h;
  popup.satVal.textContent = s + "%";
  popup.litVal.textContent = l + "%";

  const hex = hslToHex(h, s, l);
  popup.hex.value = hex;
  popup.live.style.background = hex;
  applyColorChange(hex);
}

function updateColorFromHex() {
  let hex = popup.hex.value.trim();
  if (!hex.startsWith("#")) hex = "#" + hex;
  if (!/^#[0-9A-Fa-f]{6}$/.test(hex)) return;

  hex = hex.toUpperCase();
  const hsl = hexToHsl(hex);
  popup.hue.value = hsl.h;
  popup.sat.value = hsl.s;
  popup.lit.value = hsl.l;
  popup.hueVal.textContent = hsl.h;
  popup.satVal.textContent = hsl.s + "%";
  popup.litVal.textContent = hsl.l + "%";
  popup.live.style.background = hex;
  applyColorChange(hex);
}

let applyDebounceTimer = null;

function applyColorChange(newHex) {
  const panelOption = sim.panel.selectedOptions[0];
  if (!panelOption || !panelOption.dataset.colorsActual) return;

  const actual = splitColors(panelOption.dataset.colorsActual);
  actual[state.colorPopup.colorIndex] = newHex;
  panelOption.dataset.colorsActual = actual.join(",");
  updateColorSwatches();

  const deviceKey = sim.device.value;
  if (deviceKey) {
    state.colorOverrides[deviceKey] = actual.join(",");
    saveState();
  }

  clearTimeout(applyDebounceTimer);
  applyDebounceTimer = setTimeout(async () => {
    if (deviceKey) {
      try {
        await fetch("./api/panel-colors", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ device: deviceKey, colors_actual: actual.join(",") }),
        });
      } catch (e) {
        consoleError("Failed to update panel colors: " + e.message);
      }
    }
    render();
  }, 300);
}

function copyActualColors() {
  const panelOption = sim.panel.selectedOptions[0];
  if (!panelOption || !panelOption.dataset.colorsActual) return;
  copyToClipboard(panelOption.dataset.colorsActual).then(() => {
    const btn = sim.swatches.querySelector(".copy-colors-btn");
    if (!btn) return;
    btn.textContent = "Copied!";
    btn.classList.add("copied");
    setTimeout(() => {
      btn.textContent = "Copy color config";
      btn.classList.remove("copied");
    }, 1500);
  });
}

// `navigator.clipboard` needs a secure context (HTTPS or localhost) and the
// dev server usually runs plain HTTP, so fall back to execCommand.
function copyToClipboard(text) {
  if (navigator.clipboard && window.isSecureContext) {
    return navigator.clipboard.writeText(text);
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.left = "-9999px";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
  return Promise.resolve();
}

function setupColorPopup() {
  popup.close.addEventListener("click", closeColorPopup);
  popup.hue.addEventListener("input", updateColorFromSliders);
  popup.sat.addEventListener("input", updateColorFromSliders);
  popup.lit.addEventListener("input", updateColorFromSliders);
  popup.hex.addEventListener("input", updateColorFromHex);
  popup.reset.addEventListener("click", () => {
    popup.hex.value = state.colorPopup.originalHex;
    updateColorFromHex();
  });

  document.addEventListener("click", (e) => {
    if (!state.colorPopup.visible) return;
    if (popup.root.contains(e.target)) return;
    // Not on a swatch either — that click re-opens the popup.
    if (e.target.classList.contains("swatch") && e.target.closest(".swatch-row.actual")) return;
    closeColorPopup();
  });
}

// --- drawer: the render query it contributes ------------------------------

// Everything the drawer adds to `/dev/api/render`; `render()` supplies
// `screen`. An empty field is left out so the server applies its own default
// instead of being pinned to whatever placeholder the markup carries.
//
// Throws when the parameters box does not hold JSON. `render()` catches it
// and puts the message in the console, which is better than rendering with
// parameters the user did not write.
function drawerParams() {
  const p = {};
  const put = (k, v) => {
    if (v !== undefined && v !== null && v !== "") p[k] = String(v);
  };

  // With a device selected the SERVER resolves the screen from its config,
  // so the `screen` render() sends is ignored — that is what the banner in
  // the drawer warns about.
  put("mac", sim.device.value);

  const dims = getEffectiveDimensions();
  put("width", dims.width);
  put("height", dims.height);
  put("battery_voltage", sim.battery.value);
  put("rssi", sim.rssi.value);

  // Only while frozen. A live clock needs no override, and sending one would
  // pin every render to the second the field last ticked.
  if (state.timeLocked && sim.time.value) {
    put("timestamp", Math.floor(new Date(sim.time.value).getTime() / 1000));
  }

  try {
    put("params", JSON.stringify(JSON.parse(sim.params.value || "{}")));
  } catch (e) {
    throw new Error("Invalid JSON in parameters: " + e.message);
  }

  if (sim.panel.value) {
    put("panel", sim.panel.value);
    put("use_actual", sim.useActual.checked ? "true" : "false");
    // Tuned colours travel with the request, so the preview does not depend
    // on the server's override map having been written first.
    const opt = sim.panel.selectedOptions[0];
    put("colors_actual", opt && opt.dataset.colorsActual);
  }

  put("dither", sim.dither.value);

  // Send a tunable only when it differs from the default FOR THE SELECTED
  // ALGORITHM. The fields are populated from those defaults, so comparing
  // against one fixed number would make every render carry an override the
  // user never asked for, and would pin the server's own default to whatever
  // the sentinel happened to be. With no defaults known (the screens fetch
  // failed) every non-empty field is sent — the user's value is the only
  // thing left to trust.
  const d = ditherDefaults(sim.dither.value);
  const differs = (v, dflt) =>
    v !== "" && (dflt === undefined || dflt === null || parseFloat(v) !== parseFloat(dflt));
  if (differs(sim.maxError.value, d && d.max_error)) put("max_error", sim.maxError.value);
  if (differs(sim.noiseScale.value, d && d.noise_scale)) put("noise_scale", sim.noiseScale.value);

  // These two have no per-algorithm default: the engine leaves chroma clamp
  // off and strength at 1.0 whatever the kernel.
  put("chroma_clamp", sim.chromaClamp.value);
  if (sim.strength.value !== "" && parseFloat(sim.strength.value) !== 1) {
    put("strength", sim.strength.value);
  }

  return p;
}

// --- drawer: wiring -------------------------------------------------------

function setupDrawer() {
  const changed = () => { saveState(); render(); };

  sim.toggle.addEventListener("click", () => setDrawerOpen(sim.root.hidden));

  sim.device.addEventListener("change", () => {
    const opt = sim.device.selectedOptions[0];
    sim.deviceInfo.hidden = !sim.device.value;
    if (sim.device.value) {
      selectIfPresent(sim.panel, opt.dataset.panel);
      selectIfPresent(sim.dither, opt.dataset.dither);
      const override = state.colorOverrides[sim.device.value];
      const panelOpt = sim.panel.selectedOptions[0];
      if (override && panelOpt && panelOpt.value) panelOpt.dataset.colorsActual = override;
    }
    updateColorSwatches();
    updateUseActualVisibility();
    updateDeviceFrame();
    changed();
  });

  sim.panel.addEventListener("change", () => {
    // Before the round-trip: the frame reclasses at once, and the size
    // readout is corrected by the image that comes back.
    updateDeviceFrame();
    updateColorSwatches();
    updateUseActualVisibility();
    changed();
  });

  sim.dither.addEventListener("change", () => { applyDitherDefaults(); changed(); });

  // Only these two have per-algorithm defaults worth remembering per
  // algorithm, so only they record an override.
  for (const el of [sim.maxError, sim.noiseScale]) {
    el.addEventListener("change", () => { saveDitherTuningOverride(); changed(); });
  }
  for (const el of [sim.useActual, sim.chromaClamp, sim.strength, sim.battery, sim.rssi]) {
    el.addEventListener("change", changed);
  }

  sim.time.addEventListener("change", () => { setTimeLocked(true); changed(); });
  sim.timeReset.addEventListener("click", () => { setTimeLocked(false); changed(); });

  let paramsTimer = null;
  sim.params.addEventListener("input", () => {
    clearTimeout(paramsTimer);
    paramsTimer = setTimeout(changed, 500);
  });
}

// Runs after `loadScreens()` — every option list is built from its response —
// and before the first `render()`, which reads these controls.
function initDrawer() {
  sim.toggle.setAttribute("aria-expanded", "false");
  populateDeviceSelect();
  populatePanelSelect();
  populateDitherSelect();
  // Defaults first, saved values second: a stored tuning must win, but a
  // page with nothing stored still starts on the engine's numbers rather
  // than on the placeholders in the markup.
  applyDitherDefaults();
  loadSavedState();
  setupDrawer();
  setupColorPopup();
  updateDeviceFrame();
  updateColorSwatches();
  updateUseActualVisibility();
  tickTime();
  setInterval(tickTime, 1000);
}

// --- render ---------------------------------------------------------------

let previewChannel = null;
try { previewChannel = new BroadcastChannel("byonk-preview"); } catch (e) { previewChannel = null; }

if (previewChannel) {
  // A pop-out that has just opened says hello, because renders are driven
  // from this window and it would otherwise stay blank until the next one.
  // `broadcastPreview` answers from the live <img> — see the note there.
  // A `hello` posted by a second pop-out is also delivered to the first one,
  // which ignores it: only messages carrying a `url` mean anything there.
  previewChannel.onmessage = (ev) => {
    if (ev.data && ev.data.hello) broadcastPreview();
  };
}

// A named window, so a second click raises the pop-out that is already open
// instead of scattering copies across the desktop. The URL is relative: this
// page is served from `<prefix>/dev/`, and an origin-absolute
// `/dev/preview.html` would leave the Home Assistant Ingress iframe.
document.getElementById("popout-btn").addEventListener("click", () => {
  const w = window.open("preview.html", "byonk-preview", "width=900,height=700");
  if (w) w.focus();
  else showFailure("The pop-out preview window was blocked by the browser.");
});

async function render() {
  if (!state.ref) return;
  const img = document.getElementById("display-image");
  const overlay = document.getElementById("loading-overlay");
  overlay.classList.remove("hidden");
  consoleClear();
  const start = performance.now();
  try {
    const params = new URLSearchParams({ screen: state.ref, ...drawerParams() });
    const r = await fetch(`./api/render?${params}`);
    if (r.status === 401) { location.href = "login"; return; }
    if (!r.ok) throw new Error(await jsonError(r));

    const blob = await r.blob();
    const objUrl = URL.createObjectURL(blob);
    if (img.src.startsWith("blob:")) URL.revokeObjectURL(img.src);
    img.src = objUrl;
    updateLensImage(objUrl);

    const elapsed = (performance.now() - start).toFixed(0);
    document.getElementById("render-time").textContent = `Rendered in ${elapsed}ms`;
    consoleLog(`Rendered in ${elapsed}ms`);

    // The pop-out is told by `onPreviewImageLoad`, not from here: the image
    // has not decoded yet, so its dimensions are still the previous render's.
  } catch (e) {
    consoleError(e.message);
  } finally {
    overlay.classList.add("hidden");
  }
}
window.byonkDev.render = render;

// --- SSE: live reload -------------------------------------------------

let eventSource = null;

function connectSSE() {
  if (eventSource) eventSource.close();
  eventSource = new EventSource("./api/events");
  const badge = document.getElementById("watch-status");

  eventSource.onopen = () => {
    badge.textContent = "File Watch: Connected";
    badge.classList.remove("disconnected");
    badge.classList.add("connected");
  };

  eventSource.onerror = () => {
    badge.textContent = "File Watch: Disconnected";
    badge.classList.remove("connected");
    badge.classList.add("disconnected");
    setTimeout(connectSSE, 3000);
  };

  eventSource.addEventListener("file-change", async (event) => {
    let paths = [];
    try { paths = JSON.parse(event.data); } catch (e) { paths = []; }
    for (const name of paths) {
      const entry = state.open.get(name);
      if (!entry) continue;
      if (entry.dirty) {
        showConflict(name);
      } else {
        try {
          const f = await api.readFile(state.ref, name);
          entry.editor.setValue(f.text);
          entry.etag = f.etag;
          entry.saved = f.text;
          markDirty(name, false);
        } catch (e) {
          // Not the console: the `render()` below clears it immediately.
          showFailure(`Failed to reload ${name}: ` + e.message);
        }
      }
    }
    render();
  });

  eventSource.addEventListener("refresh", () => render());
}

// --- init ---------------------------------------------------------------

async function init() {
  // Before anything can render: `render()` is fired off without being
  // awaited, so a listener registered after the first one is a race with the
  // image it is supposed to measure.
  document.getElementById("display-image").addEventListener("load", onPreviewImageLoad);

  try {
    const who = await api.whoami();
    document.getElementById("whoami").textContent = who.user || "";
    document.getElementById("logout-form").hidden = !who.can_logout;
  } catch (e) {
    // api.whoami() already redirects to login on 401.
  }

  await loadScreens();
  initDrawer();

  const saved = safeGet("byonkDevRef");
  if (saved && (state.screens.screens || []).some((s) => s.name === saved)) {
    await selectScreen(saved);
  }

  connectSSE();
  setupLens();
}

init();
