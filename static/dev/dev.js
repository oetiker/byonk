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
};
window.byonkDev = { state, api, render: null };

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

// --- lens (1:1 pixel viewing over the device preview) -----------------

let lensCanvas = null;
let lensCtx = null;
let fullResImage = null;
let lensElement = null;

function setupLens() {
  lensElement = document.createElement("div");
  lensElement.id = "lens";
  lensElement.className = "lens hidden";
  document.body.appendChild(lensElement);

  lensCanvas = document.createElement("canvas");
  lensCanvas.width = 200;
  lensCanvas.height = 200;
  lensElement.appendChild(lensCanvas);
  lensCtx = lensCanvas.getContext("2d");

  fullResImage = new Image();

  const img = document.getElementById("display-image");
  img.addEventListener("mouseenter", showLens);
  img.addEventListener("mouseleave", hideLens);
  img.addEventListener("mousemove", moveLens);
}

function updateLensImage(url) {
  if (fullResImage) {
    fullResImage.src = url;
  }
}

function showLens(e) {
  if (!fullResImage.complete || !fullResImage.naturalWidth) return;
  lensElement.classList.remove("hidden");
  moveLens(e);
}

function hideLens() {
  lensElement.classList.add("hidden");
}

function moveLens(e) {
  if (!fullResImage.complete || !fullResImage.naturalWidth) return;

  const img = document.getElementById("display-image");
  const rect = img.getBoundingClientRect();
  const lensSize = 200;
  const lensRadius = lensSize / 2;
  const magnification = 2;
  const sourceSize = lensSize / magnification;

  const lensX = e.clientX + 20;
  const lensY = e.clientY - lensSize - 10;
  lensElement.style.left = `${lensX}px`;
  lensElement.style.top = `${lensY}px`;

  const imgX = e.clientX - rect.left;
  const imgY = e.clientY - rect.top;

  const scaleX = fullResImage.naturalWidth / rect.width;
  const scaleY = fullResImage.naturalHeight / rect.height;

  const srcW = sourceSize * scaleX;
  const srcH = sourceSize * scaleY;
  const srcX = (imgX * scaleX) - (srcW / 2);
  const srcY = (imgY * scaleY) - (srcH / 2);

  lensCtx.clearRect(0, 0, lensSize, lensSize);
  lensCtx.save();
  lensCtx.beginPath();
  lensCtx.arc(lensRadius, lensRadius, lensRadius - 2, 0, Math.PI * 2);
  lensCtx.clip();
  lensCtx.fillStyle = "#fff";
  lensCtx.fillRect(0, 0, lensSize, lensSize);
  lensCtx.imageSmoothingEnabled = false;
  lensCtx.drawImage(fullResImage, srcX, srcY, srcW, srcH, 0, 0, lensSize, lensSize);
  lensCtx.restore();
}

// --- device frame -----------------------------------------------------
//
// `updateDeviceFrame` is kept for Task 9: once the drawer exists, changing
// its panel select should update the frame class ahead of the network
// round-trip. Until then nothing calls it — `#panel-select` doesn't exist
// yet, so `getEffectiveDimensions` degrades to the defaults below. The
// authoritative update (used by this task) is `updateDisplaySizeFromImage`,
// driven by the actual rendered PNG's dimensions once it loads.

const DEFAULTS = { width: "800", height: "480" };

function getEffectiveDimensions() {
  const panelSelect = document.getElementById("panel-select");
  const opt = panelSelect && panelSelect.selectedOptions[0];
  if (opt && opt.value && opt.dataset.width) {
    return { width: opt.dataset.width, height: opt.dataset.height };
  }
  return { width: DEFAULTS.width, height: DEFAULTS.height };
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
  localStorage.setItem("byonkDevRef", ref);

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
      localStorage.removeItem("byonkDevRef");
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

// --- render ---------------------------------------------------------------

let previewChannel = null;
try { previewChannel = new BroadcastChannel("byonk-preview"); } catch (e) { previewChannel = null; }

// Filled in by Task 9 with the drawer's simulate parameters (device/panel,
// dither tuning, battery, RSSI, time, custom params). Until then every
// render uses server-side defaults for everything but the selected screen.
function drawerParams() {
  return {};
}

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

    if (previewChannel) {
      const dims = getEffectiveDimensions();
      previewChannel.postMessage({ url: objUrl, width: dims.width, height: dims.height });
    }
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
  try {
    const who = await api.whoami();
    document.getElementById("whoami").textContent = who.user || "";
    document.getElementById("logout-form").hidden = !who.can_logout;
  } catch (e) {
    // api.whoami() already redirects to login on 401.
  }

  await loadScreens();

  const saved = localStorage.getItem("byonkDevRef");
  if (saved && (state.screens.screens || []).some((s) => s.name === saved)) {
    await selectScreen(saved);
  }

  document.getElementById("display-image").addEventListener("load", updateDisplaySizeFromImage);

  connectSSE();
  setupLens();
}

init();
