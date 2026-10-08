# Dev UI Work Area — Webui (Plan 2 of 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the five overlapping work-area panels with three — Library, Edit, Preview — around one piece of state, the **subject** (`screen` | `panel` | `device`), with a schema-driven config editor whose ladder widget shows every cascade rung and its status.

**Architecture:** `subject.svelte.js` replaces `screens.selected` as the single truth. The Preview panel builds its render request from the subject through one pure function (`render-request.js`) and writes the `X-Byonk-Resolution` payload into a shared `resolution` store. The Edit panel routes on `subject.kind`: `EditorPane` for screens, `ConfigEditor` (one component, two schemas) for panels and devices, backed by one `configDraft` store that owns drafts, Push, Save, Discard and the server's live-override list. `tune.svelte.js`, `TunePane` and `SimulateControls` dissolve.

**Tech Stack:** Svelte 5 runes, Vite 8, vitest 5 + jsdom + @testing-library/svelte, pnpm, `$lib/ui` (bits-ui 2.19.0, @lucide/svelte 1.42.0).

**Spec:** `docs/superpowers/specs/2026-10-04-dev-ui-work-area-design.md` §2–§7, §9 (and §10 rulings). Server contract: Plan 1, `docs/superpowers/plans/2026-10-08-dev-ui-server.md` — **executed first**; every route below is cited from its Interfaces blocks. Controller brief: `.superpowers/sdd/dev-ui-planning-brief.md`.

## Rulings (read before Task 1)

Controller rulings (binding; where they differ from the spec, the ruling wins):

- **R-A** One PR; this plan runs after Plan 1 and consumes its routes verbatim.
- **R-B (overrides spec §8.2's "draft feeds the override rung")**: the preview's draft **replaces** the rung being edited (`draft.panel`, `draft.base`, `draft.final`), so the ladder shows the user's value on its own rung with its real status. Push is what puts values on the `override` rung.
- **R-C** "It reverts on its own" is true now (30-min TTL). UI copy: **"It reverts on its own within 30 minutes, or on Discard."** A live override is listed by `GET api/overrides` and shown again after a reload.
- **R-F** Usage copy: **"⚠ 3 devices use this panel"** and, when non-zero, **"(2 not seen since byonk started)"**.
- **R-H** (1) mode b defaults to `byonk-builtin/calibration/tone` and remembers a later choice for the page session; (2) the panel is named **"Library"**; (3) `prompt()`/`confirm()` are **not** touched; (4) Finding H gets a test: after deleting the selected screen, the editor and the preview are empty.
- **R-I** No CHANGES.md entry. `docs/src/guide/dev-mode.md`'s UI sections are rewritten (Task 10).

Rulings this plan makes:

- **Ruling (controller) C1 — overrides may carry `dither`.** A panel and a device override both accept `colors_actual`, `dither`, `max_error`, `noise_scale`, `chroma_clamp`, `strength`, `gamut`; both refuse `colors` and `refresh` (Plan 1 Task 2/3). So a device Push carries `dither`.
- **P2-1 Push carries dirty, set, pushable fields only.** Pushing a whole rung would put unchanged config values on the override rung, where they would beat the screen and change the render for fields the user never touched. Not pushable, listed in the footer as "applies on Save only": removals, a device's `colors` and `refresh`, and the plain device fields.
- **P2-1a (new, follows from C1) The panel editor has no algorithm field.** `panels.<id>` has no algorithm key, so Save could not persist one; a panel's "dither" is its tuning block (numbers, gamut, per-algorithm sub-maps). The panel Push therefore carries `colors_actual`, the four numbers and `gamut`; the per-algorithm sub-maps are carried unchanged by the render draft and by Save, and cannot be pushed (the override is flat). A pushed panel `dither` remains available through the API.
- **P2-2 A device push merges the two rungs**, final over base, into one override (there is one override rung).
- **P2-3 The tuning session starts on Push, not on choosing "watch on".** Spec §2 rule 2: the Preview panel never writes anything. "Watch on [device]" only chooses which device mode b renders as; the Edit panel's Push starts/re-touches that device's session; Save/Discard/release end it. Mode c starts no session (spec §6 mentions none).
- **P2-4 Drafts are kept per subject** (`kind:id`) in `configDraft`, so a detour to another subject and back keeps the draft (spec §6 "the draft survives in the store").
- **Ruling (controller) C3 — device Save is one atomic PATCH.** Plan 1 Task 5 makes `apply_device_patch` clear first and set second, so one `DeviceWrite` carries every `clear` and every set. A removed single gamut knob clears as `gamut.<knob>` / `final.gamut.<knob>`; dirty params are sent as `clear: ["params"]` plus the full new `params` (clear then set ⇒ wholesale replacement).
- **Ruling (controller) C2 — panel Save is written in place on the server** (Plan 1 Task 4); the client still sends the whole rung.
- **P2-6 Mode b with a watched device renders as that device** (`device` + `panel` in the request, the subject panel winning), so the preview matches the display being watched; without one, no device is sent.
- **P2-7 Draft-driven re-renders are debounced 300 ms**; subject, target and simulated-input changes render at once.
- **P2-8 The editor remembers which screen it loaded** (`editor.ref`); `editor.validate/addFile/deleteFile` use it instead of `screens.selected`.
- **Ruling (controller, pre-flight) PF3 — the journey harness grows with the modules.** Task 2's and Task 6's journey files import and reset only modules that exist at that task; Task 3 adds `resolution`'s import and reset to every journey file existing by then (`journey-finding-h`), Task 7 adds `configDraft`'s (`journey-finding-h`, `journey-subject-buffers`). The shared block below tags those lines. Each task's "all PASS" step is true at that task.
- **Ruling (controller, pre-flight) PF4 — `ladder.test.js` cleans up.** It imports `cleanup` and runs `beforeEach(() => cleanup())` like the other render tests (this vitest setup has no auto-cleanup).
- **Ruling (controller, pre-flight) PF5 — Push never shows what Save will not (amends P2-2).** `pushBody` skips a dirty base value whenever the field's EFFECTIVE final value (draft or config) is non-null, and `unpushable` reports it as "`<field>` (outranked by final)"; the Edit footer lists it with the other unpushable fields. A `configDraft` test pins it (Task 7).
- **Ruling (controller, pre-flight) PF8 — no duplicated lookup.** Task 6's `LibraryPane` imports `devicePanel` from Task 5's `render-request.js` instead of writing the device→panel lookup again (Plan 1's PF8 covers the server side).
- **Ruling (controller, pre-flight) PF9 — stale lines fixed.** `NOT_PUSHABLE`'s comment cites ruling C1 (not the non-existent "Plan 1 P1-2"); the Task 7 test misnamed "a changed final value is sent as a set" is renamed to describe `device_base` ("a changed base gamut value is sent as a set on the base rung, with no clear"). The Plan 1 citations of PF9 (`dev.rs` line numbers) are applied in Plan 1.
- **Ruling (controller, pre-flight) PF10 — the panel journey changes a swatch.** Task 10's mode-b journey drives one `#swatch-hex` edit end to end (spec §9 "change a swatch"): hex edit → draft → render request → Push → Save.

## Global Constraints

- Every task's requirements include this section and the rulings above.
- **Verification order (never `make check` alone):** `make webui`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `(cd docs && mdbook build)`, `make webui-test` — Rust suite and `make webui-test` **one after the other, never in parallel** (journeys time out at 5 s under load). Per-task inner loop: `cd webui && pnpm vitest run <files>`.
- **Never `git add -A`/`.`**; never stage `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. `git diff --cached --stat` before each commit.
- pnpm only. Dependency pins do not move (`@lucide/svelte` 1.42.0, `bits-ui` 2.19.0). Buttons come from `$lib/ui/button` (`Button`), text fields from `$lib/ui/input`, labels from `$lib/ui/label`; native `<select>` with the existing `selectClass` string is the established select pattern.
- Every URL relative (no leading `/`); `api.js` is the only module that knows a URL. Build emits exactly `index.html`, `app.js`, `app.css`. No nested page route under `/dev`.
- `tests/dev_auth_test.rs` stays byte-identical to `5d6323c`.
- Spec §10 rulings carried: **R2** (`editor.tabs` = loaded buffers, `editor.files` = listing), **R3** (lazy load, only `script.lua` auto-opens), **R4** (binary entries are disabled tabs), **R7** (`screens.error = null` stays below `loadScreen()`'s `!ref` return; a subject switch away from screens never clears a user-visible error), **57** (`postFrame()` posts `{url, width, height}`, size read live, `renderSeq` guard first), **63** (Logout only when `whoami.can_logout`, no DOM node otherwise), **66** (independent error surfaces: `editor.error`, `screens.error`, `simulate.error`, `configDraft.error`, `auth.error` never clear each other). Push is a button, never a debounce.
- `min-w-0` on the editor pane and its toolbar row is load-bearing (Finding M); keep it on `EditPane`'s wrapper too.
- User-facing UI copy is English. Comments/identifiers English.
- `api.test.js`'s `urlCoverage` table gains an entry for every new export (completeness assertion).

## Review Focus

1. **Push while the screen sets the same field.** The override rung must win in the ladder ("override ← wins") and the user's panel rung must say outranked; a widget that keys "winner" on the edited rung shows green while the glass disagrees. [Task 8, fixture `override wins over everything`]
2. **A six-entry calibration on a sixteen-entry render.** Rejected rung must show its reason, not sit silently on its rung. [Task 8, fixture `rejected`]
3. **A dirty screen buffer, then a panel, then the same screen.** Buffers and dirty flag must survive; a stray `loadScreen()` erases them. [Task 6, buffer-survival journey]
4. **Removing a single `gamut` sub-field on a device rung and saving.** Must produce `clear: ["gamut.knee"]` in the one request, or the removed value stays in config. [Task 7, `a removed gamut sub-field is cleared by name in the one request`]
5. **A reload while a Push is live.** The strip must still say "Live on …" with a working Discard. [Task 9, `a live override from before a reload is shown and can be discarded`]

## File Structure

**New — state**
- `webui/src/lib/state/subject.svelte.js` — the single truth (kind/id) + per-mode preview choices (target, borrowed screen, watched device).
- `webui/src/lib/state/resolution.svelte.js` — the last render's provenance payload and state (none/ok/failed).
- `webui/src/lib/state/configDraft.svelte.js` — schema-driven drafts for panel and device, Push/Save/Discard/release, live overrides, panel usage.
- `webui/src/lib/render-request.js` — pure: subject + drafts + simulated inputs → `POST api/render` body and size.

**New — components**
- `webui/src/lib/components/EditPane.svelte` — router on `subject.kind`.
- `webui/src/lib/components/ConfigEditor.svelte` — one editor, two schemas.
- `webui/src/lib/components/Ladder.svelte` — one setting's rungs.
- `webui/src/lib/components/SwatchEditor.svelte` — swatches + H/S/L + hex (lifted from `TunePane`).
- `webui/src/lib/components/SimulatedInputs.svelte` — replaces `SimulateControls`.
- `webui/src/lib/components/LibraryPane.svelte` — replaces `FilesPane`.

**Modified** — `api.js`, `editor.svelte.js`, `screens.svelte.js`, `simulate.svelte.js`, `layout.svelte.js`, `AppShell.svelte`, `IconRail.svelte`, `EditorPane.svelte`, `PreviewPane.svelte`, `StatusStrip.svelte`, `docs/src/guide/dev-mode.md`.

**Deleted** — `tune.svelte.js`, `TunePane.svelte`, `SimulateControls.svelte`, `FilesPane.svelte`, and `test/tune.test.js`, `test/tunepane.test.js`, `test/simulatecontrols.test.js`, `test/filespane.test.js` (its cases move to `test/librarypane.test.js`).

**New tests** — `test/subject.test.js`, `test/resolution.test.js`, `test/render-request.test.js`, `test/configdraft.test.js`, `test/ladder.test.js`, `test/configeditor.test.js`, `test/librarypane.test.js`, `test/journey-subject-buffers.test.js`, `test/journey-finding-h.test.js`, `test/journey-mode-screen.test.js`, `test/journey-mode-panel.test.js`, `test/journey-mode-device.test.js`, `test/journey-preview-writes-nothing.test.js`, `test/journey-whole-feature.test.js`, `test/orphan-api.test.js`.

### Shared test harness (used by every journey below)

Each journey file starts with this block (copy it; tasks are read independently). **The block is the final form and grows with the modules (ruling PF3):** the two lines tagged `// [Task 3]` and the two tagged `// [Task 7]` exist only from those tasks on. A journey file created before then omits them; the task that creates the module adds its import and reset to every journey file existing by then (Task 3 → `journey-finding-h`; Task 7 → `journey-finding-h` and `journey-subject-buffers`). Every task's "all PASS" step is true at that task.

```js
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen as dom, cleanup, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import AppShell from "../src/lib/components/AppShell.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { editor } from "../src/lib/state/editor.svelte.js";
import { layout } from "../src/lib/state/layout.svelte.js";
import { auth } from "../src/lib/state/auth.svelte.js";
import { theme } from "../src/lib/state/theme.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js"; // [Task 3]
import { configDraft } from "../src/lib/state/configDraft.svelte.js"; // [Task 7]
import * as api from "../src/lib/api.js";

const PANELS = [
  { id: "e1004", name: "reTerminal E1004", width: 800, height: 480,
    colors: "#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00",
    colors_actual: "#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D",
    dither: { strength: 0.8 } },
  { id: "x", name: "TRMNL X", width: 1600, height: 1200, colors: "#000000,#FFFFFF", colors_actual: null, dither: null },
];
const DEVICES = [
  { id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: { greeting: "hi" },
    base: { dither: "floyd-steinberg", colors: null, colors_actual: null, max_error: null, noise_scale: null,
            chroma_clamp: null, strength: null, refresh: null, gamut: {} },
    final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null },
];
const RESOLUTION = {
  dither: { effective: "atkinson", from: "screen",
    order: ["override", "device_final", "screen", "device_base", "panel", "fallback"],
    rungs: { screen: { value: "atkinson", status: "winner" }, fallback: { value: "atkinson", status: "outranked" } } },
};

/** Every fetch the app makes, as [method, url, body]. Renders answer a PNG with the header. */
function stubServer() {
  const calls = [];
  vi.spyOn(globalThis, "fetch").mockImplementation(async (url, init = {}) => {
    calls.push([init.method ?? "GET", url, init.body ? JSON.parse(init.body) : undefined]);
    return {
      ok: true, status: 200,
      headers: { get: (h) => (h.toLowerCase() === "x-byonk-resolution" ? JSON.stringify(RESOLUTION) : null) },
      blob: async () => new Blob(["png"], { type: "image/png" }),
      text: async () => "", json: async () => ({}),
    };
  });
  return calls;
}

beforeEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  layout.reset(); theme.reset(); auth.reset(); editor.reset(); editor.files = [];
  simulate.reset(); simulate.error = null;
  subject.reset();
  resolution.reset(); // [Task 3]
  configDraft.reset(); // [Task 7]
  screens.repos = []; screens.list = [];
  screens.meta = { devices: [], panels: [], dither_algorithms: [] };
  vi.spyOn(api, "listRepos").mockResolvedValue([{ handle: "local", writable: true }]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }, { name: "local/b" }, { name: "byonk-builtin/calibration/tone" }],
    devices: DEVICES, panels: PANELS,
    dither_algorithms: [{ name: "atkinson", defaults: { max_error: 1, noise_scale: 0 } },
                        { name: "floyd-steinberg", defaults: { max_error: 1, noise_scale: 0 } }],
  });
  vi.spyOn(api, "whoami").mockResolvedValue({ user: "u", via: "cookie", can_logout: true });
  vi.spyOn(api, "listFiles").mockResolvedValue([
    { name: "script.lua", size: 1, text: true }, { name: "screen.svg", size: 1, text: true }]);
  vi.spyOn(api, "readFile").mockImplementation(async (_ref, name) => ({ text: `-- ${name}`, etag: '"1"' }));
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"], x: [] }, unseen: ["attic"] });
});
afterEach(() => cleanup());

async function mount() {
  render(AppShell);
  await waitFor(() => expect(screens.meta.panels).toHaveLength(2));
  await tick();
}
```

(`stubServer()` replaces `fetch` for the render; the `api.*` spies above cover everything else. A journey that asserts on writes spies the specific `api.*` function.)

---

### Task 1: The server contract in `api.js`

**Files:**
- Modify: `webui/src/lib/api.js` (after `endTuningSession`, `:172-174`)
- Test: `webui/test/api.test.js` (`urlCoverage`, `:24-162`)

**Interfaces:**
- Consumes (Plan 1): `GET api/overrides`; `PUT|DELETE api/overrides/panels/{panel}`; `PUT|DELETE api/overrides/devices/{device}`; `POST api/panels/{panel}/save`; `PATCH api/devices/{key}`; `GET api/panel-usage`.
- Produces:
  - `listOverrides(): Promise<{panels:[{key,values,expires_in_secs}], devices:[…], tuning_sessions:[{key,expires_in_secs}]}>`
  - `pushPanelOverride(panel, values)`, `releasePanelOverride(panel)`, `pushDeviceOverride(device, values)`, `releaseDeviceOverride(device)` → `null`
  - `savePanel(panel, {colors_actual?, dither?})` → `null`
  - `saveDevice(key, deviceWrite)` → `{key, screen}`
  - `panelUsage(): Promise<{panels: {[id]: string[]}, unseen: string[]}>`
  - The old `setPanelOverride`, `clearPanelOverride`, `savePanelColors` stay until Task 4 removes them with their only consumer.

- [ ] **Step 1: Write the failing test entries**

Add to `urlCoverage` in `webui/test/api.test.js`:

```js
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
```

And a body-shape test:

```js
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd webui && pnpm vitest run test/api.test.js`
Expected: FAIL — `api.listOverrides is not a function`.

- [ ] **Step 3: Implement**

In `webui/src/lib/api.js`, after `del()` add:

```js
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
```

After `endTuningSession`:

```js
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
```

- [ ] **Step 4: Run** — `cd webui && pnpm vitest run test/api.test.js` → PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/api.js webui/test/api.test.js
git commit -m "feat(webui): api.js knows the override, save and panel-usage routes"
```

---

### Task 2: The subject replaces `screens.selected`

**Files:**
- Create: `webui/src/lib/state/subject.svelte.js`, `webui/test/subject.test.js`, `webui/test/journey-finding-h.test.js`
- Modify: `webui/src/lib/state/screens.svelte.js` (drop `selected`, `select`), `webui/src/lib/state/editor.svelte.js` (`ref`, `:116-136` reset, `:171-` loadScreen, `:286`, `:325`, `:349`), `webui/src/lib/components/AppShell.svelte` (`:59-73`), `EditorPane.svelte` (`:124-126`, `:187-188`), `FilesPane.svelte`, `PreviewPane.svelte` (`:188`, `:230`)
- Modify tests: every file under `webui/test/` that uses `screens.selected`/`screens.select(` (appshell, editor, editorpane, filespane, conflict, journey-*, preview, statusbar).

**Interfaces:**
- Produces:
  ```js
  export const DEFAULT_BORROWED_SCREEN = "byonk-builtin/calibration/tone";
  subject.kind: "screen"|"panel"|"device"|null; subject.id: string|null
  subject.target: null | {kind:"panel"|"device", id}   // mode a hardware
  subject.borrowedScreen: string                       // mode b screen
  subject.watchDevice: string                          // mode b, "" = none
  subject.key: "kind:id"|null; subject.screenRef: string|null
  subject.select(kind, id); subject.clear(); subject.is(kind,id); subject.reset()
  editor.ref: string|null   // the screen loadScreen() last loaded
  ```

- [ ] **Step 1: Write the failing tests**

`webui/test/subject.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { subject, DEFAULT_BORROWED_SCREEN } from "../src/lib/state/subject.svelte.js";

beforeEach(() => subject.reset());

describe("subject", () => {
  it("starts empty, with mode b borrowing the tone calibration screen", () => {
    expect(subject.kind).toBe(null);
    expect(subject.key).toBe(null);
    expect(subject.borrowedScreen).toBe(DEFAULT_BORROWED_SCREEN);
    expect(DEFAULT_BORROWED_SCREEN).toBe("byonk-builtin/calibration/tone");
  });
  it("selects a kind and an id, and names the screen only for a screen", () => {
    subject.select("screen", "local/a");
    expect(subject.key).toBe("screen:local/a");
    expect(subject.screenRef).toBe("local/a");
    subject.select("panel", "e1004");
    expect(subject.screenRef).toBe(null);
    expect(subject.is("panel", "e1004")).toBe(true);
  });
  it("an empty id clears instead of selecting nothing", () => {
    subject.select("screen", "local/a");
    subject.select("screen", null);
    expect(subject.kind).toBe(null);
  });
  it("keeps the mode b screen choice across subject switches", () => {
    subject.borrowedScreen = "byonk-builtin/calibration/gamut";
    subject.select("screen", "local/a");
    subject.select("panel", "e1004");
    expect(subject.borrowedScreen).toBe("byonk-builtin/calibration/gamut");
  });
});
```

`webui/test/journey-finding-h.test.js` (harness block from the top of this plan **without** the `// [Task 3]` and `// [Task 7]` lines, ruling PF3, then):

```js
describe("Finding H: deleting the selected screen", () => {
  it("leaves the editor and the preview empty", async () => {
    stubServer();
    vi.spyOn(api, "deleteScreen").mockResolvedValue(undefined);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    const img = () => document.querySelector('img[alt="Screen preview"]');
    await waitFor(() => expect(img().getAttribute("src")).toBeTruthy());

    await fireEvent.click(dom.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(subject.kind).toBe(null));
    expect(editor.tabs).toHaveLength(0);
    expect(editor.files).toHaveLength(0);
    expect(editor.active).toBe(null);
    await waitFor(() => expect(img().getAttribute("src")).toBeFalsy());
  });
});
```

(Screen buttons in the Library/Files pane are labelled by the screen's label, e.g. `a` for `local/a`.)

Add to `webui/test/appshell.test.js`, replacing the describe "AppShell wires screens.selected to editor.loadScreen" setup to use the subject, and add:

```js
  it("selecting a panel does not reload the editor, and coming back to the same screen does not either", async () => {
    const listFiles = vi.spyOn(api, "listFiles").mockResolvedValue([{ name: "script.lua", size: 1, text: true }]);
    render(AppShell);
    subject.select("screen", "local/a");
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("dirty");
    subject.select("panel", "e1004");
    await tick();
    subject.select("screen", "local/a");
    await tick();
    expect(listFiles).toHaveBeenCalledTimes(1);
    expect(editor.active.text).toBe("dirty");
    expect(editor.active.dirty).toBe(true);
  });

  it("a subject switch away from screens leaves screens.error alone (R7)", async () => {
    render(AppShell);
    screens.error = { op: "files", message: "boom" };
    subject.select("device", "kitchen");
    await tick();
    expect(screens.error).toEqual({ op: "files", message: "boom" });
  });
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd webui && pnpm vitest run test/subject.test.js test/journey-finding-h.test.js test/appshell.test.js`
Expected: FAIL — module `subject.svelte.js` not found.

- [ ] **Step 3: Implement the store**

`webui/src/lib/state/subject.svelte.js`:

```js
// What the work area is about: a screen, a panel profile, or a device
// (spec §2). The mode is not a separate variable -- there is nothing a mode
// could say that the subject does not.
//
// The three per-mode preview choices live here too, because they outlive a
// subject switch: mode a's hardware target, mode b's borrowed screen
// (remembered for the page session, R-H 1) and mode b's watched device.

export const DEFAULT_BORROWED_SCREEN = "byonk-builtin/calibration/tone";

class Subject {
  kind = $state(null);
  id = $state(null);
  /** Mode a: render the screen as `{kind: "panel"|"device", id}`, or null for the 800x480 default. */
  target = $state(null);
  /** Mode b: which screen renders on the panel being calibrated. */
  borrowedScreen = $state(DEFAULT_BORROWED_SCREEN);
  /** Mode b: which device of that panel the user watches ("" = none). */
  watchDevice = $state("");

  get key() {
    return this.kind ? `${this.kind}:${this.id}` : null;
  }

  /** The screen being EDITED -- only in mode a. */
  get screenRef() {
    return this.kind === "screen" ? this.id : null;
  }

  is(kind, id) {
    return this.kind === kind && this.id === id;
  }

  select(kind, id) {
    if (!kind || id === null || id === undefined || id === "") {
      this.clear();
      return;
    }
    this.kind = kind;
    this.id = id;
  }

  clear() {
    this.kind = null;
    this.id = null;
  }

  reset() {
    this.clear();
    this.target = null;
    this.borrowedScreen = DEFAULT_BORROWED_SCREEN;
    this.watchDevice = "";
  }
}

export const subject = new Subject();
```

- [ ] **Step 4: Rewire the consumers**

`screens.svelte.js`: delete `selected = $state(null);` and `select(ref) { … }`; update the class doc to "Repos, screens and their metadata. Which one is selected lives in `subject`."

`editor.svelte.js`:
- add field `ref = $state(null);` with doc `/** The screen \`loadScreen()\` last loaded -- what the tab strip, validate, add and delete act on (P2-8). */`
- in `reset()` add `this.ref = null;`
- in `loadScreen()`, right after `this.reset();`, add `this.ref = ref ?? null;`
- `async validate(ref = screens.selected)` → `async validate(ref = this.ref)`; the two `const ref = screens.selected;` (`:325`, `:349`) → `const ref = this.ref;`

`AppShell.svelte`: replace the effect at `:59-73` with:

```js
  // Spec §7's trap: switching subject must not discard open buffers.
  // `loadScreen()` resets every buffer, so it runs only when the subject is
  // a screen AND a different one than the editor holds. A detour to a panel
  // or a device, and back, leaves the buffers alone. Reading only
  // `subject.screenRef` keeps `screens.refresh()` from re-running this.
  $effect(() => {
    const ref = subject.screenRef;
    untrack(() => {
      if (!ref || ref === editor.ref) return;
      editor.loadScreen(ref, { readOnly: !screens.writable(ref) });
    });
  });
```

and import `subject`.

`EditorPane.svelte`: `!!screens.selected && screens.writable(screens.selected)` → `!!editor.ref && screens.writable(editor.ref)`; the tab `onclick` → `editor.open(editor.ref, f.name, { readOnly: !screens.writable(editor.ref) })`.

`FilesPane.svelte`: import `subject` and `editor`; `canEdit` → `$derived(screens.writable(subject.screenRef))`; every `screens.selected` → `subject.screenRef`; every `screens.select(ref)` → `subject.select("screen", ref)`; the highlight test → `subject.is("screen", s.ref)`; in `deleteScreen()` replace `screens.select(null);` with:

```js
      // Finding H: the deleted screen's buffers and render must go too. The
      // subject effect never reloads for "no screen", so drop them here.
      subject.clear();
      await editor.loadScreen(null);
```

`PreviewPane.svelte`: `const ref = screens.selected;` → `const ref = subject.screenRef;`; in the effect `screens.selected;` → `subject.screenRef;`; import `subject`.

- [ ] **Step 5: Migrate the existing tests**

Run `grep -ln "screens.select" webui/test` and, in each file listed, apply:
- `import { subject } from "../src/lib/state/subject.svelte.js";` added.
- `screens.selected = null;` → `subject.reset();`
- `screens.selected = "X";` and `screens.select("X")` → `subject.select("screen", "X")`
- `screens.select(null)` → `subject.clear()`
- `expect(screens.selected)` → `expect(subject.screenRef)`
- where a test set `screens.selected` only so that `editor.validate()`, `editor.addFile()` or `editor.deleteFile()` find a ref, set `editor.ref = "X";` instead.

In `test/screens.test.js` nothing uses `selected` (verify). Delete the appshell test "does not re-run when screens.refresh() replaces repos/list…" only if it asserts on `screens.selected` internals; otherwise migrate it.

- [ ] **Step 6: Run** — `cd webui && pnpm vitest run` → all PASS.

- [ ] **Step 7: Commit**

```bash
git add webui/src/lib/state/subject.svelte.js webui/src/lib/state/screens.svelte.js webui/src/lib/state/editor.svelte.js webui/src/lib/components/AppShell.svelte webui/src/lib/components/EditorPane.svelte webui/src/lib/components/FilesPane.svelte webui/src/lib/components/PreviewPane.svelte webui/test/
git commit -m "feat(webui): the subject replaces screens.selected; a detour keeps the buffers"
```

(Stage `webui/test/` file by file if `git status` shows anything there you did not touch.)

---

### Task 3: The shared resolution store

**Files:**
- Create: `webui/src/lib/state/resolution.svelte.js`, `webui/test/resolution.test.js`
- Modify: `webui/src/lib/components/PreviewPane.svelte` (`renderNow`, `:187-226`)
- Test: `webui/test/preview.test.js`

**Interfaces:**
- Consumes: `X-Byonk-Resolution` (Plan 1 / cascade §4.5): `{[setting]: {effective, from, order: Rung[], rungs: {[rung]: {value?, status: "winner"|"outranked"|"rejected", reason?}}}}`; absent rungs are missing from `rungs`.
- Produces:
  ```js
  resolution.state: "none" | "ok" | "failed"
  resolution.payload: object | null; resolution.failure: string | null
  resolution.subjectKey: string | null   // which subject the payload describes
  resolution.record(payload, subjectKey); resolution.fail(message, subjectKey); resolution.reset()
  resolution.setting(key) -> SettingReport | null
  export function parseResolutionHeader(raw: string|null): object|null  // throws on bad JSON
  ```

- [ ] **Step 1: Write the failing tests**

`webui/test/resolution.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { resolution, parseResolutionHeader } from "../src/lib/state/resolution.svelte.js";

beforeEach(() => resolution.reset());

describe("resolution store", () => {
  it("starts with no render", () => {
    expect(resolution.state).toBe("none");
    expect(resolution.setting("dither")).toBe(null);
  });
  it("records a payload for a subject", () => {
    resolution.record({ dither: { effective: "atkinson", from: "screen", order: [], rungs: {} } }, "screen:local/a");
    expect(resolution.state).toBe("ok");
    expect(resolution.setting("dither").effective).toBe("atkinson");
    expect(resolution.subjectKey).toBe("screen:local/a");
  });
  it("a failure drops the payload rather than leaving stale rungs", () => {
    resolution.record({ dither: {} }, "screen:local/a");
    resolution.fail("Render error: boom", "screen:local/a");
    expect(resolution.state).toBe("failed");
    expect(resolution.payload).toBe(null);
    expect(resolution.failure).toBe("Render error: boom");
  });
  it("parses the header, and a missing header is null", () => {
    expect(parseResolutionHeader(null)).toBe(null);
    expect(parseResolutionHeader('{"refresh":{"effective":60}}').refresh.effective).toBe(60);
    expect(() => parseResolutionHeader("{nope")).toThrow();
  });
});
```

Append to `webui/test/preview.test.js` (its own setup; add `import { resolution } …` and `import { subject } …`, `resolution.reset()` in its `beforeEach`):

```js
  it("writes a successful render's X-Byonk-Resolution into the shared store", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue({
      ok: true,
      headers: { get: () => '{"dither":{"effective":"atkinson","from":"screen","order":[],"rungs":{}}}' },
      blob: async () => new Blob(["png"]),
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(resolution.state).toBe("ok"));
    expect(resolution.setting("dither").from).toBe("screen");
    expect(resolution.subjectKey).toBe("screen:local/a");
  });

  it("a failed render marks the store failed, with the server's message", async () => {
    vi.spyOn(api, "renderPreview").mockResolvedValue({
      ok: false, status: 500, statusText: "x",
      json: async () => ({ error: "Render error", details: "boom" }),
    });
    subject.select("screen", "local/a");
    render(PreviewPane);
    await waitFor(() => expect(resolution.state).toBe("failed"));
    expect(resolution.failure).toBe("Render error: boom");
  });
```

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run test/resolution.test.js test/preview.test.js` → FAIL (module missing).

- [ ] **Step 3: Implement**

`webui/src/lib/state/resolution.svelte.js`:

```js
// The last render's provenance (spec §5): written by the Preview panel, read
// by the Edit panel's ladder. A shared store, not PreviewPane state, because
// Preview can be closed while Edit is open -- the ladder then shows the last
// payload it received, labelled as such.

export function parseResolutionHeader(raw) {
  if (!raw) return null;
  return JSON.parse(raw);
}

class Resolution {
  /** "none" before any render for this subject, "ok", or "failed". */
  state = $state("none");
  payload = $state(null);
  failure = $state(null);
  subjectKey = $state(null);

  record(payload, subjectKey) {
    this.state = "ok";
    this.payload = payload;
    this.failure = null;
    this.subjectKey = subjectKey;
  }

  /** A failed render carries no header; showing the previous rungs would lie. */
  fail(message, subjectKey) {
    this.state = "failed";
    this.payload = null;
    this.failure = message;
    this.subjectKey = subjectKey;
  }

  setting(key) {
    return this.payload?.[key] ?? null;
  }

  reset() {
    this.state = "none";
    this.payload = null;
    this.failure = null;
    this.subjectKey = null;
  }
}

export const resolution = new Resolution();
```

In `PreviewPane.svelte` `renderNow()`: import `resolution, parseResolutionHeader`. In the `!ref` branch add `resolution.reset();` inside the `untrack`. After `const blob = await res.blob(); if (seq !== renderSeq) return;` insert:

```js
      let payload;
      try {
        payload = parseResolutionHeader(res.headers?.get?.("X-Byonk-Resolution") ?? null);
      } catch {
        throw new Error("the render's X-Byonk-Resolution header is not valid JSON");
      }
      resolution.record(payload, subject.key);
```

and in the `catch`, after the superseded check: `resolution.fail(e.message, subject.key);`.

Then extend the existing journey file (ruling PF3): in `webui/test/journey-finding-h.test.js` add the harness's `// [Task 3]` lines, `import { resolution } from "../src/lib/state/resolution.svelte.js";` after the `subject` import and `resolution.reset();` after `subject.reset();` in `beforeEach`.

- [ ] **Step 4: Run** — `pnpm vitest run` → PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/state/resolution.svelte.js webui/src/lib/components/PreviewPane.svelte webui/test/resolution.test.js webui/test/preview.test.js webui/test/journey-finding-h.test.js
git commit -m "feat(webui): the render's provenance lands in a shared store"
```

---

### Task 4: Three panels; Tune and the old Simulate drawer dissolve

**Files:**
- Modify: `webui/src/lib/state/layout.svelte.js`, `IconRail.svelte`, `AppShell.svelte` (markup `:182-217`), `StatusStrip.svelte`, `webui/src/lib/api.js` (remove `setPanelOverride`, `clearPanelOverride`, `savePanelColors`)
- Create: `webui/src/lib/components/EditPane.svelte`
- Delete: `webui/src/lib/state/tune.svelte.js`, `webui/src/lib/components/TunePane.svelte`, `webui/test/tune.test.js`, `webui/test/tunepane.test.js`
- Modify tests: `layout.test.js`, `iconrail.test.js`, `journey-panel-toggles.test.js`, `appshell.test.js`, `statusstrip.test.js`, `api.test.js`, `journey-layout-persistence.test.js`, `auth.svelte.js`'s doc comment mentioning `tune.error`

**Interfaces:**
- Produces: `PANELS = ["library", "editor", "preview"]`, defaults all open; IconRail labels `Library`, `Edit`, `Preview`. `EditPane` renders `EditorPane` for `subject.kind === "screen"`, `ConfigEditor` placeholder text for panel/device until Task 9, and "Pick a screen, panel or device in the Library." when empty. The y split in `AppShell` now holds `PreviewPane` (top) and `SimulateControls` (bottom, unchanged for one task; Task 5 replaces it).

- [ ] **Step 1: Update the tests first**

`layout.test.js`: replace the first test, the "corrupt" loop and the two `tune` tests with:

```js
  it("has three panels, all open by default", () => {
    expect(PANELS).toEqual(["library", "editor", "preview"]);
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(true);
  });

  it("drops stale keys from the five-panel layout without a storage bump", () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({
      panels: { files: false, editor: false, tune: true, preview: true, simulate: false },
      sizes: { x: 500, y: 200 },
    }));
    layout.load();
    expect(layout.isOpen("editor")).toBe(false);
    expect(layout.isOpen("library")).toBe(true); // renamed from files: default-open once
    expect(layout.panels.tune).toBeUndefined();
    expect(layout.size("x")).toBe(500);
  });
```

and in the remaining tests use `"preview"` where they toggled `"simulate"`, and `expect(layout.isOpen(p)).toBe(true)` where they compared against `p !== "tune"`.

`iconrail.test.js`: delete "shows one toggle per panel, including tune" and "toggles tune"; in "offers one labelled toggle per panel" expect labels `["Library", "Edit", "Preview"]`.

`journey-panel-toggles.test.js`: panel list becomes `[["library", "Library"], ["editor", "Edit"], ["preview", "Preview"]]` (adapt its table); delete the `tune` test; "with all five closed" → "with all three closed".

`appshell.test.js`: delete "mounts TunePane only while the tune panel is open" and "keeps the horizontal splitter alive when only tune is open on the right"; add:

```js
  it("the Edit panel asks for a subject when there is none", async () => {
    render(AppShell);
    expect(await dom.findByText("Pick a screen, panel or device in the Library.")).toBeTruthy();
  });
```

`statusstrip.test.js`: delete every test that sets `tune.*` ("shows a live tune.pushed override…", "shows a live tune.error…", "warns when a panel colour override is live…", "shows no override warning…", "appears and disappears reactively as tune.pushed changes"); in the "all five error surfaces" test drop the tune line and expect four.

`api.test.js`: remove the `setPanelOverride`, `clearPanelOverride`, `savePanelColors` coverage entries and the "tolerates the empty 200 body savePanelColors' handler actually returns" test (re-point it at `savePanel`: `await expect(api.savePanel("e1004", {})).resolves.toBeNull();` with a `new Response(null, {status: 200})` mock).

`git rm webui/test/tune.test.js webui/test/tunepane.test.js`.

- [ ] **Step 2: Run to verify the new expectations fail** — `pnpm vitest run test/layout.test.js test/iconrail.test.js test/appshell.test.js` → FAIL.

- [ ] **Step 3: Implement**

`layout.svelte.js`:

```js
export const PANELS = ["library", "editor", "preview"];
export const STORAGE_KEY = "byonk.layout.v1";

const DEFAULTS = {
  panels: { library: true, editor: true, preview: true },
  sizes: { x: 420, y: 260 },
};
```

(No storage bump: `load()` copies only keys in `PANELS`, spec §3.)

`IconRail.svelte`:

```js
  import { Library, Code, Monitor, LogOut } from "@lucide/svelte";
  …
  const ICONS = { library: Library, editor: Code, preview: Monitor };
  const LABELS = { library: "Library", editor: "Edit", preview: "Preview" };
```

`EditPane.svelte`:

```svelte
<script>
  // What we MODIFY is the Edit panel, always, and it names its subject
  // (spec §2). Routes on the subject's kind.
  import { subject } from "../state/subject.svelte.js";
  import EditorPane from "./EditorPane.svelte";
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  {#if subject.kind === "screen"}
    <EditorPane />
  {:else if subject.kind === "panel" || subject.kind === "device"}
    <p class="p-3 text-sm text-[var(--text-muted)]">Config editor for {subject.kind} {subject.id}</p>
  {:else}
    <p class="p-3 text-sm text-[var(--text-muted)]">Pick a screen, panel or device in the Library.</p>
  {/if}
</div>
```

`AppShell.svelte` markup — replace the body from `{#if layout.isOpen("files")}` to the end of the outer `SplitPane` with:

```svelte
    {#if layout.isOpen("library")}
      <div class="w-56 shrink-0 overflow-auto border-r border-[var(--border-color)]">
        <FilesPane />
      </div>
    {/if}

    <SplitPane axis="x" bind:size={xSize} open={layout.isOpen("preview")}>
      {#snippet a()}
        {#if layout.isOpen("editor")}<EditPane />{/if}
      {/snippet}
      {#snippet b()}
        {#if layout.isOpen("preview")}
          <SplitPane axis="y" bind:size={ySize} open={true}>
            {#snippet a()}<PreviewPane />{/snippet}
            {#snippet b()}<SimulateControls />{/snippet}
          </SplitPane>
        {/if}
      {/snippet}
    </SplitPane>
```

remove the `TunePane` import, add `import EditPane from "./EditPane.svelte";`.

`StatusStrip.svelte`: remove the `tune` import, `tune.error`/`tune.pushed` from `hasAnything`, and the two `{#if tune…}` blocks.

`simulate.svelte.js`: delete `colors_actual_override` (field, DEFAULTS entry, reset line, and the `// A live tuning working copy` block in `request()`); `simulate.test.js`: delete the tests about `colors_actual_override`.

`api.js`: delete `setPanelOverride`, `clearPanelOverride`, `savePanelColors` and their doc block.

`git rm webui/src/lib/state/tune.svelte.js webui/src/lib/components/TunePane.svelte`. Update `auth.svelte.js`'s comment listing `tune.error` to `configDraft.error`.

- [ ] **Step 4: Run** — `pnpm vitest run` → PASS. `grep -rn "tune" webui/src webui/test` returns nothing but unrelated words.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/state/layout.svelte.js webui/src/lib/components/IconRail.svelte webui/src/lib/components/AppShell.svelte webui/src/lib/components/EditPane.svelte webui/src/lib/components/StatusStrip.svelte webui/src/lib/state/simulate.svelte.js webui/src/lib/state/auth.svelte.js webui/src/lib/api.js webui/test/
git commit -m "feat(webui): three panels -- Library, Edit, Preview; Tune dissolves"
```

---

### Task 5: The Preview renders the subject

**Files:**
- Create: `webui/src/lib/render-request.js`, `webui/src/lib/components/SimulatedInputs.svelte`, `webui/test/render-request.test.js`
- Modify: `webui/src/lib/state/simulate.svelte.js` (shrinks), `PreviewPane.svelte` (header, toolbar, request), `AppShell.svelte` (y split second pane)
- Delete: `SimulateControls.svelte`, `webui/test/simulatecontrols.test.js`
- Modify tests: `simulate.test.js` (rewrite), `preview.test.js`, `journey-panel-profile-popout.test.js` (drive the new target select), `journey-new-screen.test.js`

**Interfaces:**
- Consumes: Plan 1 Task 1 wire (`draft.base|final|panel|params`), `subject`, `screens.meta`, and from Task 7 `configDraft.panelWire(id)`/`deviceWire(id)`/`usage` — until Task 7 lands, `render-request.js` takes a `draft` argument, and `PreviewPane` passes an object with those two methods returning the baseline built from `screens.meta` (see `metaDraft` below); Task 7 swaps in `configDraft`.
- Produces:
  ```js
  export const DEFAULT_SIZE = { width: 800, height: 480 };
  export function previewSize({ subject, meta, usage }) -> {width, height}
  export function devicePanel({ meta, usage }, deviceId) -> panelId | null
  export function buildRenderRequest({ subject, draft, simulate, meta, usage })
     -> null | { body, size, screen, as }      // throws on invalid simulated params JSON
  simulate: battery_voltage, rssi, timestamp, timeLocked, params, use_actual, expanded, error
  simulate.simulated({ withParams }) -> {battery_voltage?, rssi?, timestamp?, params?}  // throws on bad JSON
  simulate.fields (snapshot for effects); simulate.pending (any input set); simulate.reset()
  ```

- [ ] **Step 1: Write the failing tests**

`webui/test/render-request.test.js`:

```js
import { describe, it, expect } from "vitest";
import { buildRenderRequest, previewSize, DEFAULT_SIZE } from "../src/lib/render-request.js";

const meta = {
  panels: [{ id: "e1004", width: 800, height: 480 }, { id: "x", width: 1600, height: 1200 }],
  devices: [{ id: "kitchen", panel: "e1004", screen: "local/k" }, { id: "office", panel: null, screen: "local/o" }],
};
const usage = { panels: { x: ["office"], e1004: ["kitchen"] }, unseen: [] };
const sim = (over = {}) => ({ use_actual: true, simulated: () => ({ params: { a: 1 } }), ...over });
const draft = {
  panelWire: (id) => ({ colors: "#000000,#FFFFFF", colors_actual: `actual-${id}` }),
  deviceWire: (id) => ({ screen: `draft-screen-${id}`, params: { p: 1 }, base: { dither: "stucki" }, final: {} }),
};

describe("buildRenderRequest", () => {
  it("returns null with no subject", () => {
    expect(buildRenderRequest({ subject: { kind: null }, draft, simulate: sim(), meta, usage })).toBe(null);
  });

  it("mode a, no target: the screen at the default size, with simulated params", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: null }, draft, simulate: sim(), meta, usage });
    expect(r.body).toEqual({ screen: "local/a", ...DEFAULT_SIZE, simulated: { params: { a: 1 } }, view: { use_actual: true } });
    expect(r.as).toBe(null);
  });

  it("mode a as a panel: that panel and its size, no draft", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: { kind: "panel", id: "x" } }, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({ screen: "local/a", panel: "x", width: 1600, height: 1200 });
    expect(r.body.draft).toBeUndefined();
  });

  it("mode a as a device: screen AND device together, the device's panel size", () => {
    const r = buildRenderRequest({ subject: { kind: "screen", id: "local/a", target: { kind: "device", id: "office" } }, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({ screen: "local/a", device: "office", width: 1600, height: 1200 });
  });

  it("mode b: the borrowed screen on the subject panel with the panel draft; no device unless watched", () => {
    const s = { kind: "panel", id: "e1004", borrowedScreen: "byonk-builtin/calibration/tone", watchDevice: "" };
    const r = buildRenderRequest({ subject: s, draft, simulate: sim(), meta, usage });
    expect(r.body).toMatchObject({
      screen: "byonk-builtin/calibration/tone", panel: "e1004", width: 800, height: 480,
      draft: { panel: { colors: "#000000,#FFFFFF", colors_actual: "actual-e1004" } },
    });
    expect(r.body.device).toBeUndefined();
    const w = buildRenderRequest({ subject: { ...s, watchDevice: "kitchen" }, draft, simulate: sim(), meta, usage });
    expect(w.body.device).toBe("kitchen");
  });

  it("mode c: the draft's screen assignment, both device rungs and params, and no simulated params", () => {
    let asked;
    const simulate = sim({ simulated: ({ withParams }) => ((asked = withParams), {}) });
    const r = buildRenderRequest({ subject: { kind: "device", id: "kitchen" }, draft, simulate, meta, usage });
    expect(asked).toBe(false);
    expect(r.body).toMatchObject({
      device: "kitchen", screen: "draft-screen-kitchen", width: 800, height: 480,
      draft: { base: { dither: "stucki" }, final: {}, params: { p: 1 } },
    });
  });

  it("never sends override: the preview writes and pushes nothing", () => {
    for (const s of [{ kind: "screen", id: "local/a" }, { kind: "panel", id: "e1004", borrowedScreen: "b" }, { kind: "device", id: "kitchen" }]) {
      expect(buildRenderRequest({ subject: s, draft, simulate: sim(), meta, usage }).body.override).toBeUndefined();
    }
  });
});

describe("previewSize", () => {
  it("follows the subject without simulated inputs", () => {
    expect(previewSize({ subject: { kind: "panel", id: "x" }, meta, usage })).toEqual({ width: 1600, height: 1200 });
    expect(previewSize({ subject: { kind: "device", id: "office" }, meta, usage })).toEqual({ width: 1600, height: 1200 });
    expect(previewSize({ subject: { kind: null }, meta, usage })).toEqual(DEFAULT_SIZE);
  });
});
```

Rewrite `webui/test/simulate.test.js` to:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { simulate } from "../src/lib/state/simulate.svelte.js";

beforeEach(() => simulate.reset());

describe("simulated inputs", () => {
  it("leave blank fields out and send numbers as numbers", () => {
    simulate.battery_voltage = "3.9";
    expect(simulate.simulated({ withParams: true })).toEqual({ battery_voltage: 3.9, params: {} });
  });
  it("send the clock only while it is frozen", () => {
    simulate.timestamp = 1700000000;
    expect(simulate.simulated({ withParams: false }).timestamp).toBeUndefined();
    simulate.timeLocked = true;
    expect(simulate.simulated({ withParams: false }).timestamp).toBe(1700000000);
  });
  it("throw on invalid params JSON, and leave params out in mode c", () => {
    simulate.params = "{nope";
    expect(() => simulate.simulated({ withParams: true })).toThrow(/Invalid JSON in parameters/);
    expect(simulate.simulated({ withParams: false }).params).toBeUndefined();
  });
  it("report pending when any input is set", () => {
    expect(simulate.pending).toBe(false);
    simulate.rssi = -70;
    expect(simulate.pending).toBe(true);
  });
});
```

In `journey-panel-profile-popout.test.js`: `panelSelect()` returns `document.querySelector("#preview-target")`, the option values become `panel:og` / `panel:x` (set `sel.value = "panel:x"`), `mountWithScreen()` selects via `subject.select("screen", "local/a")`, and `PANELS_META` stays. The body assertion becomes `expect(bodies.at(-1)).toMatchObject({ panel: "x", width: 1600, height: 1200 })`. Replace the "in-app preview header reports the new size" `getByText("1600 × 1200")` with `await waitFor(() => expect(document.querySelector('[data-testid="preview-size"]').textContent).toBe("1600 × 1200"))`.

`git rm webui/test/simulatecontrols.test.js`.

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run test/render-request.test.js test/simulate.test.js` → FAIL.

- [ ] **Step 3: Implement `render-request.js`**

```js
// One place that turns the subject into a render request (spec §4). Pure:
// the stores are arguments, so each mode's body is a unit-testable value.
// The preview never sends `override` -- Push is the only way onto that rung,
// and it is the Edit panel's (spec §2 rule 2).

export const DEFAULT_SIZE = { width: 800, height: 480 };

function sizeOf(meta, panelId) {
  const p = (meta.panels ?? []).find((x) => x.id === panelId);
  return p && p.width && p.height ? { width: p.width, height: p.height } : DEFAULT_SIZE;
}

/** The device's explicit panel, else the panel the server says it resolves to. */
export function devicePanel({ meta, usage }, deviceId) {
  const d = (meta.devices ?? []).find((x) => x.id === deviceId);
  if (d?.panel) return d.panel;
  for (const [panel, keys] of Object.entries(usage?.panels ?? {})) {
    if (keys.includes(deviceId)) return panel;
  }
  return null;
}

export function previewSize({ subject, meta, usage }) {
  switch (subject.kind) {
    case "screen": {
      const t = subject.target;
      if (t?.kind === "panel") return sizeOf(meta, t.id);
      if (t?.kind === "device") return sizeOf(meta, devicePanel({ meta, usage }, t.id));
      return DEFAULT_SIZE;
    }
    case "panel":
      return sizeOf(meta, subject.id);
    case "device":
      return sizeOf(meta, devicePanel({ meta, usage }, subject.id));
    default:
      return DEFAULT_SIZE;
  }
}

export function buildRenderRequest({ subject, draft, simulate, meta, usage }) {
  if (!subject.kind) return null;
  const size = previewSize({ subject, meta, usage });
  const view = { use_actual: !!simulate.use_actual };
  if (subject.kind === "screen") {
    const simulated = simulate.simulated({ withParams: true });
    const body = { screen: subject.id, ...size, simulated, view };
    const t = subject.target;
    if (t?.kind === "panel") body.panel = t.id;
    if (t?.kind === "device") body.device = t.id;
    return { body, size, screen: subject.id, as: t ? t.id : null };
  }
  if (subject.kind === "panel") {
    const simulated = simulate.simulated({ withParams: true });
    const body = {
      screen: subject.borrowedScreen,
      panel: subject.id,
      ...size,
      draft: { panel: draft.panelWire(subject.id) },
      simulated,
      view,
    };
    // P2-6: watching a device renders as that device; the panel still wins.
    if (subject.watchDevice) body.device = subject.watchDevice;
    return { body, size, screen: subject.borrowedScreen, as: subject.id };
  }
  // Mode c: no simulated params -- the device owns real ones (spec §4).
  const simulated = simulate.simulated({ withParams: false });
  const w = draft.deviceWire(subject.id);
  const body = {
    device: subject.id,
    screen: w.screen,
    ...size,
    draft: { base: w.base, final: w.final, params: w.params },
    simulated,
    view,
  };
  return { body, size, screen: w.screen, as: subject.id };
}
```

- [ ] **Step 4: Shrink `simulate.svelte.js`**

Replace the file with:

```js
// Simulated inputs and view options (spec §3.3, §7). Nothing here is a
// render setting: what the device would report about itself, and how the
// answer is depicted. Device, panel and the tuning knobs moved to the
// subject and the config editor.

const DEFAULTS = {
  use_actual: true,
  battery_voltage: "",
  rssi: "",
  timestamp: "",
  timeLocked: false,
  params: "{}",
  expanded: false,
};

/** "" (untouched text field), null (cleared number field) or undefined. */
function isBlank(v) {
  return v === "" || v === undefined || v === null;
}

class Simulate {
  /** The last failed render `{op: "render", message}` (Ruling 41/66). */
  error = $state(null);
  use_actual = $state(DEFAULTS.use_actual);
  battery_voltage = $state(DEFAULTS.battery_voltage);
  rssi = $state(DEFAULTS.rssi);
  timestamp = $state(DEFAULTS.timestamp);
  timeLocked = $state(DEFAULTS.timeLocked);
  params = $state(DEFAULTS.params);
  /** Whether the Simulated inputs drawer is open. */
  expanded = $state(DEFAULTS.expanded);

  reset() {
    for (const [k, v] of Object.entries(DEFAULTS)) this[k] = v;
  }

  /** A snapshot an effect reads to re-render on any change. */
  get fields() {
    return {
      use_actual: this.use_actual,
      battery_voltage: this.battery_voltage,
      rssi: this.rssi,
      timestamp: this.timestamp,
      timeLocked: this.timeLocked,
      params: this.params,
    };
  }

  /** Anything the user set that a real device would not report. */
  get pending() {
    return (
      !isBlank(this.battery_voltage) ||
      !isBlank(this.rssi) ||
      this.timeLocked ||
      (this.params.trim() !== "" && this.params.trim() !== "{}")
    );
  }

  /**
   * The `simulated` object of `POST api/render`. Throws on invalid params
   * JSON so the user never sees a render made from parameters they did not
   * write; `renderNow()` catches it.
   */
  simulated({ withParams }) {
    const out = {};
    if (!isBlank(this.battery_voltage)) out.battery_voltage = Number(this.battery_voltage);
    if (!isBlank(this.rssi)) out.rssi = Number(this.rssi);
    if (this.timeLocked && !isBlank(this.timestamp)) out.timestamp = Number(this.timestamp);
    if (withParams) {
      try {
        out.params = JSON.parse(this.params || "{}");
      } catch (e) {
        throw new Error("Invalid JSON in parameters: " + e.message);
      }
    }
    return out;
  }
}

export const simulate = new Simulate();
```

- [ ] **Step 5: `SimulatedInputs.svelte`**

Move from `SimulateControls.svelte` exactly these controls, unchanged in ids and bindings: Battery (`#sim-battery`, `bind:value={simulate.battery_voltage}`), WiFi (`#sim-rssi`), Current Time (`#sim-time`, the live-clock `$effect`, `onTimeChange`, `resetTime`), Custom Parameters (`#sim-params`, `bind:value={simulate.params}`) — the params block wrapped in `{#if subject.kind !== "device"}` (spec §4: mode c has no simulated params). Root element: `<div class="flex flex-col gap-3 overflow-auto min-h-0 p-3 text-sm" data-testid="simulated-inputs">`. `git rm webui/src/lib/components/SimulateControls.svelte`.

- [ ] **Step 6: `PreviewPane.svelte` — request, header, toolbar**

Script changes:

```js
  import { subject } from "../state/subject.svelte.js";
  import { buildRenderRequest, previewSize } from "../render-request.js";

  // Until Task 7 the draft comes from config as-is; Task 7 swaps in configDraft.
  const metaDraft = {
    panelWire(id) {
      const p = (screens.meta.panels ?? []).find((x) => x.id === id);
      return p ? { colors: p.colors, colors_actual: p.colors_actual ?? undefined, dither: p.dither ?? undefined } : {};
    },
    deviceWire(id) {
      const d = (screens.meta.devices ?? []).find((x) => x.id === id);
      return { screen: d?.screen, params: d?.params ?? {}, base: d?.base ?? {}, final: d?.final ?? {} };
    },
  };
  const usage = { panels: {}, unseen: [] };

  const size = $derived(previewSize({ subject, meta: screens.meta, usage }));
```

Every `simulate.size` becomes `size` (frame class, `postFrame()`; Ruling 57 still holds: `postFrame` reads `size` live and the `renderSeq` guard runs first). In `renderNow()`:

```js
    let req;
    try {
      req = buildRenderRequest({ subject, draft: metaDraft, simulate, meta: screens.meta, usage });
    } catch (e) {
      simulate.error = { op: "render", message: e.message };
      resolution.fail(e.message, subject.key);
      return;
    }
    if (!req) { /* the existing "nothing selected" branch: drop the image, tell the pop-out, resolution.reset() */ }
    …
    const res = await api.renderPreview(req.body);
```

The re-render effect:

```js
  $effect(() => {
    subject.key;
    subject.target;
    subject.borrowedScreen;
    subject.watchDevice;
    simulate.fields;
    screens.meta;
    renderNow();
  });
```

Header + toolbar markup (replace the current top row):

```svelte
  <div class="flex min-w-0 flex-wrap items-center gap-2 border-b border-[var(--border-color)] px-2 py-1 text-xs">
    <span data-testid="preview-title" class="truncate font-mono">{title}</span>
    <span data-testid="preview-size" class="font-mono">{size.width} × {size.height}</span>
    {#if subject.kind === "screen"}
      <label for="preview-target" class="sr-only">Render as</label>
      <select id="preview-target" class={selectClass} value={targetValue} onchange={onTarget}>
        <option value="">default 800×480</option>
        <optgroup label="Panels">
          {#each screens.meta.panels ?? [] as p (p.id)}<option value={`panel:${p.id}`}>{p.name}</option>{/each}
        </optgroup>
        <optgroup label="Devices">
          {#each screens.meta.devices ?? [] as d (d.id)}<option value={`device:${d.id}`}>{d.name ?? d.id}</option>{/each}
        </optgroup>
      </select>
    {:else if subject.kind === "panel"}
      <label for="preview-screen" class="sr-only">Screen</label>
      <select id="preview-screen" class={selectClass} bind:value={subject.borrowedScreen}>
        {#each screens.list as s (s.ref)}<option value={s.ref}>{s.ref}</option>{/each}
      </select>
      <label for="preview-watch">watch on</label>
      <select id="preview-watch" class={selectClass} bind:value={subject.watchDevice}>
        <option value="">no device</option>
        {#each screens.meta.devices ?? [] as d (d.id)}<option value={d.id}>{d.name ?? d.id}</option>{/each}
      </select>
    {/if}
    {#if screenInCharge}
      <span data-testid="screen-in-charge" class="text-[var(--text-muted)]">the screen sets its own colours</span>
    {/if}
    <label class="ml-auto flex items-center gap-1">
      <input type="checkbox" bind:checked={simulate.use_actual} /> show measured inks
    </label>
    <Button type="button" size="xs" variant="outline" onclick={popOut}>Pop out</Button>
  </div>
```

with, in the script:

```js
  import { Button } from "$lib/ui/button/index.js";
  import { resolution, parseResolutionHeader } from "../state/resolution.svelte.js";
  const selectClass =
    "h-7 min-w-0 rounded-lg border border-input bg-transparent px-2 text-xs text-foreground";
  const targetValue = $derived(subject.target ? `${subject.target.kind}:${subject.target.id}` : "");
  function onTarget(e) {
    const v = e.currentTarget.value;
    if (!v) { subject.target = null; return; }
    const i = v.indexOf(":");
    subject.target = { kind: v.slice(0, i), id: v.slice(i + 1) };
  }
  const asLabel = $derived(
    subject.kind === "screen"
      ? (subject.target ? subject.target.id : "default")
      : subject.id,
  );
  const shownScreen = $derived(
    subject.kind === "screen" ? subject.id
    : subject.kind === "panel" ? subject.borrowedScreen
    : (screens.meta.devices ?? []).find((d) => d.id === subject.id)?.screen,
  );
  const title = $derived(subject.kind ? `${shownScreen ?? "?"} as ${asLabel}` : "nothing selected");
  // Spec §4: a screen can win colors outright, so switching hardware may change only the size.
  const screenInCharge = $derived(
    resolution.setting("colors")?.from === "screen" || resolution.setting("colors_actual")?.from === "screen",
  );
```

Footer row (always rendered, under the image area):

```svelte
  <button type="button" class="flex items-center gap-2 border-t border-[var(--border-color)] px-2 py-1 text-xs"
          aria-expanded={simulate.expanded} onclick={() => (simulate.expanded = !simulate.expanded)}>
    {simulate.expanded ? "▾" : "▸"} Simulated inputs
    {#if simulate.pending}<span data-testid="sim-badge" class="text-[var(--danger-color)]">not saved</span>{/if}
  </button>
```

(This is a raw `<button>` like `IconRail`'s and `FilesPane`'s list rows: a disclosure row, not an action.)

`AppShell.svelte`: the y split becomes `open={simulate.expanded}` with `{#snippet b()}<SimulatedInputs />{/snippet}`; import `simulate` and `SimulatedInputs`.

Delete `preview.test.js` cases that assert `simulate.request`/`simulate.size`-driven behaviour no longer present, and adapt the rest to `subject.select("screen", "local/a")`; the frame-class tests set `subject.select("panel", "x")` with `screens.meta.panels` holding `x` 1600×1200.

- [ ] **Step 7: Run** — `pnpm vitest run` → PASS.

- [ ] **Step 8: Commit**

```bash
git add webui/src/lib/render-request.js webui/src/lib/state/simulate.svelte.js webui/src/lib/components/SimulatedInputs.svelte webui/src/lib/components/PreviewPane.svelte webui/src/lib/components/AppShell.svelte webui/test/
# SimulateControls.svelte and simulatecontrols.test.js were removed with `git rm` in Steps 1 and 5, so they are already staged.
git commit -m "feat(webui): the Preview renders the subject, with its target in its header"
```

---

### Task 6: The Library, and the buffer-survival journey

**Files:**
- Create: `webui/src/lib/components/LibraryPane.svelte`, `webui/test/librarypane.test.js`, `webui/test/journey-subject-buffers.test.js`
- Delete: `webui/src/lib/components/FilesPane.svelte`, `webui/test/filespane.test.js`
- Modify: `AppShell.svelte` (mount `LibraryPane`)

**Interfaces:**
- Consumes: `subject`, `screens`, `api.panelUsage` (Task 1; until Task 7, LibraryPane loads usage itself into a local `$state`; Task 7 moves it to `configDraft.usage`).
- Produces: three sections with headings `Screens`, `Panels`, `Devices`; a panel row is a button named by `panel.name`; a device row a button named `"{name ?? id} · {panel name}"`; selected row carries `aria-current="true"`. New/Copy/Rename/Delete act on `subject.screenRef` exactly as `FilesPane` did (prompt/confirm untouched, R-H 3).

- [ ] **Step 1: Write the failing tests**

`webui/test/librarypane.test.js` — port every case from `filespane.test.js` (rename imports to `LibraryPane`, replace `screens.select` per Task 2), then add:

```js
describe("LibraryPane subjects", () => {
  beforeEach(() => {
    screens.meta = {
      panels: [{ id: "e1004", name: "reTerminal E1004" }],
      devices: [{ id: "kitchen", name: "Kitchen", panel: "e1004" }, { id: "attic", name: null, panel: null }],
      dither_algorithms: [],
    };
    vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: { e1004: ["kitchen"] }, unseen: ["attic"] });
  });

  it("lists panels and devices and makes each the subject", async () => {
    render(LibraryPane);
    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    expect(subject.key).toBe("panel:e1004");
    await fireEvent.click(await dom.findByRole("button", { name: "Kitchen · reTerminal E1004" }));
    expect(subject.key).toBe("device:kitchen");
    expect(dom.getByRole("button", { name: "Kitchen · reTerminal E1004" }).getAttribute("aria-current")).toBe("true");
  });

  it("names a device's panel as the server resolves it, and says when it is unknown", async () => {
    render(LibraryPane);
    expect(await dom.findByRole("button", { name: "attic · panel unknown" })).toBeTruthy();
  });

  it("offers no create or delete for panels and devices", () => {
    render(LibraryPane);
    subject.select("panel", "e1004");
    expect(dom.getByRole("button", { name: "Delete" }).disabled).toBe(true);
  });
});
```

`webui/test/journey-subject-buffers.test.js` (harness block **with** the `// [Task 3]` lines and **without** the `// [Task 7]` ones, ruling PF3, then):

```js
describe("spec §9: the buffer-survival journey", () => {
  it("two open files, one dirty, survive a detour through a panel and back", async () => {
    stubServer();
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    await fireEvent.click(dom.getByRole("button", { name: "screen.svg" }));
    await waitFor(() => expect(editor.tabs).toHaveLength(2));
    editor.setText("-- edited");
    expect(editor.active.dirty).toBe(true);

    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    await waitFor(() => expect(subject.kind).toBe("panel"));
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/a"));

    expect(editor.tabs.map((t) => t.name)).toEqual(["script.lua", "screen.svg"]);
    const svg = editor.tabs.find((t) => t.name === "screen.svg");
    expect(svg.text).toBe("-- edited");
    expect(svg.dirty).toBe(true);
    expect(api.listFiles).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run test/librarypane.test.js test/journey-subject-buffers.test.js` → FAIL.

- [ ] **Step 3: Implement `LibraryPane.svelte`**

Start from `FilesPane.svelte` (`git mv webui/src/lib/components/FilesPane.svelte webui/src/lib/components/LibraryPane.svelte`, `git mv webui/test/filespane.test.js webui/test/librarypane.test.js`), keep its script (`defaultWritableHandle`, `run`, the four handlers as rewired in Task 2), and add:

```js
  import { onMount } from "svelte";
  import { subject } from "../state/subject.svelte.js";
  import { devicePanel } from "../render-request.js"; // Task 5's lookup; not written twice (ruling PF8)

  // Which panel each device uses comes from the server: a device without
  // `panel:` resolves through the headers it polls with (spec §5).
  let usage = $state({ panels: {}, unseen: [] });
  onMount(async () => {
    try {
      usage = await api.panelUsage();
    } catch (e) {
      screens.error = { op: "panel usage", message: e?.message ?? String(e) };
    }
  });

  function panelName(id) {
    return (screens.meta.panels ?? []).find((p) => p.id === id)?.name ?? id;
  }
  function devicePanelLabel(d) {
    const id = devicePanel({ meta: screens.meta, usage }, d.id);
    return id ? panelName(id) : "panel unknown";
  }
  const rowClass = (on) =>
    "truncate px-2 py-0.5 text-left hover:bg-[var(--panel-bg)] " +
    (on ? "bg-[var(--accent-color)] text-white" : "");
```

Markup after the existing toolbar:

```svelte
  <h2 class="px-2 pt-2 text-[10px] font-medium uppercase text-[var(--text-muted)]">Screens</h2>
  {#each screens.grouped as g (g.handle)}
    <div class="px-2 pt-1 font-mono text-[10px] uppercase text-[var(--text-muted)]">
      {g.handle}{#if !g.writable}<span class="ml-1">read-only</span>{/if}
    </div>
    {#each g.screens as s (s.ref)}
      <button type="button" class={rowClass(subject.is("screen", s.ref))}
              aria-current={subject.is("screen", s.ref) ? "true" : undefined}
              onclick={() => subject.select("screen", s.ref)}>{s.label}</button>
    {/each}
  {/each}

  <h2 class="px-2 pt-3 text-[10px] font-medium uppercase text-[var(--text-muted)]">Panels</h2>
  {#each screens.meta.panels ?? [] as p (p.id)}
    <button type="button" class={rowClass(subject.is("panel", p.id))}
            aria-current={subject.is("panel", p.id) ? "true" : undefined}
            onclick={() => subject.select("panel", p.id)}>{p.name}</button>
  {/each}

  <h2 class="px-2 pt-3 text-[10px] font-medium uppercase text-[var(--text-muted)]">Devices</h2>
  {#each screens.meta.devices ?? [] as d (d.id)}
    <button type="button" class={rowClass(subject.is("device", d.id))}
            aria-current={subject.is("device", d.id) ? "true" : undefined}
            onclick={() => subject.select("device", d.id)}>{d.name ?? d.id} · {devicePanelLabel(d)}</button>
  {/each}
```

`AppShell.svelte`: `import LibraryPane from "./LibraryPane.svelte";` and mount it in the `library` slot.

- [ ] **Step 4: Run** — `pnpm vitest run` → PASS (journeys that clicked FilesPane buttons still find the same names).

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/components/LibraryPane.svelte webui/src/lib/components/AppShell.svelte webui/test/librarypane.test.js webui/test/journey-subject-buffers.test.js
git add -u webui/src/lib/components/FilesPane.svelte webui/test/filespane.test.js
git commit -m "feat(webui): the Library chooses screens, panels and devices; buffers survive a detour"
```

---

### Task 7: `configDraft` — one draft store for both schemas

**Files:**
- Create: `webui/src/lib/state/configDraft.svelte.js`, `webui/test/configdraft.test.js`
- Modify: `PreviewPane.svelte` (use `configDraft` instead of `metaDraft`; debounced draft re-render, P2-7), `LibraryPane.svelte` (use `configDraft.usage`), `AppShell.svelte` (boot: `configDraft.load()`)

**Interfaces:**
- Consumes: `screens.meta` (Plan 1 Task 6 payloads), Task 1's API functions.
- Produces:
  ```js
  export const NUMBER_FIELDS, PANEL_FIELDS, DEVICE_CASCADE_FIELDS, DEVICE_PLAIN_FIELDS, SCHEMAS
  // SCHEMAS.panel = {rungs: ["panel"], fields: PANEL_FIELDS, plain: []}
  // SCHEMAS.device = {rungs: ["device_base","device_final"], fields: DEVICE_CASCADE_FIELDS, plain: DEVICE_PLAIN_FIELDS}
  configDraft.value(kind,id,rung,field); configDraft.set(kind,id,rung,field,value)
  configDraft.plainValue(kind,id,field); configDraft.setPlain(kind,id,field,value)
  configDraft.dirty(kind,id) -> [{rung|null, field}]; dirtyCount(kind,id); isDirty(kind,id,rung,field)
  configDraft.panelWire(id) -> {colors, colors_actual?, dither?}
  configDraft.deviceWire(id) -> {screen, params, base, final}       // throws on bad params JSON
  configDraft.pushBody(kind,id) -> CascadeValues; unpushable(kind,id) -> string[]
  configDraft.savePanelBody(id); configDraft.saveDeviceBody(id) -> DeviceWrite|null   // one atomic PATCH (C3)
  async push(kind,id,{watchDevice}); save(kind,id); discard(kind,id); release(); discardLive(kind,key)
  async refreshLive(); loadUsage(); load()  // load = refreshLive + loadUsage
  configDraft.liveFor(kind,id) -> {key,values,expires_in_secs}|null
  configDraft.pushed: {kind,id,device}|null; error: {op,message}|null
  configDraft.live: {panels,devices,tuning_sessions}; usage: {panels,unseen}; revision: number
  ```

- [ ] **Step 1: Write the failing tests**

`webui/test/configdraft.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  configDraft.reset();
  screens.meta = {
    panels: [{ id: "e1004", colors: "#000000,#FFFFFF", colors_actual: "#101010,#E8E8E8",
               dither: { strength: 0.8, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } } }],
    devices: [{ id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: { g: "hi" },
                base: { dither: "floyd-steinberg", gamut: { knee: 0.5, amount: 0.9 } }, final: null,
                temperature_profile: null, maximum_compatibility: null, min_png_bytes: null }],
    dither_algorithms: [],
  };
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue({ panels: {}, unseen: [] });
  vi.spyOn(screens, "refresh").mockResolvedValue();
});

describe("configDraft", () => {
  it("starts from config and is clean", () => {
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.8);
    expect(configDraft.value("panel", "e1004", "panel", "gamut.knee")).toBe(0.6);
    expect(configDraft.value("device", "kitchen", "device_base", "dither")).toBe("floyd-steinberg");
    expect(configDraft.plainValue("device", "kitchen", "name")).toBe("Kitchen");
    expect(configDraft.dirtyCount("panel", "e1004")).toBe(0);
  });

  it("a panel wire carries the whole rung, per-algorithm blocks included", () => {
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    expect(configDraft.panelWire("e1004")).toEqual({
      colors: "#000000,#FFFFFF", colors_actual: "#101010,#E8E8E8",
      dither: { strength: 0.5, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } },
    });
    expect(configDraft.dirtyCount("panel", "e1004")).toBe(1);
  });

  it("a draft survives a detour to another subject (P2-4)", () => {
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    configDraft.set("device", "kitchen", "device_final", "dither", "atkinson");
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.5);
  });

  it("push carries dirty set pushable fields only, final over base (P2-1, P2-2)", () => {
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    configDraft.set("device", "kitchen", "device_final", "strength", 0.7);
    configDraft.set("device", "kitchen", "device_base", "refresh", 60);
    configDraft.set("device", "kitchen", "device_base", "dither", null);
    expect(configDraft.pushBody("device", "kitchen")).toEqual({ strength: 0.7 });
    expect(configDraft.unpushable("device", "kitchen").sort()).toEqual([
      "dither (removal)", "refresh", "strength (outranked by final)",
    ]);
  });

  it("push never shows what Save will not: a base value outranked by the effective final is left out (PF5)", () => {
    // kitchen's configured final sets strength; nobody edited it.
    screens.meta.devices[0].final = { strength: 0.4 };
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    configDraft.set("device", "kitchen", "device_base", "max_error", 0.5);
    expect(configDraft.pushBody("device", "kitchen")).toEqual({ max_error: 0.5 });
    expect(configDraft.unpushable("device", "kitchen")).toEqual(["strength (outranked by final)"]);
  });

  it("push sends the override, starts the watched device's session, and lists it as live", async () => {
    const pushP = vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    const sess = vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.push("panel", "e1004", { watchDevice: "kitchen" });
    expect(pushP).toHaveBeenCalledWith("e1004", { strength: 0.5 });
    expect(sess).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toEqual({ kind: "panel", id: "e1004", device: "kitchen" });
  });

  it("panel save sends the whole rung and ends the session", async () => {
    vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue(null);
    const save = vi.spyOn(api, "savePanel").mockResolvedValue(null);
    configDraft.set("panel", "e1004", "panel", "colors_actual", null);
    await configDraft.push("panel", "e1004", { watchDevice: "kitchen" });
    await configDraft.save("panel", "e1004");
    expect(save).toHaveBeenCalledWith("e1004", {
      dither: { strength: 0.8, gamut: { knee: 0.6 }, atkinson: { noise_scale: 2 } },
    });
    expect(end).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toBe(null);
  });

  it("a removed gamut sub-field is cleared by name in the one request (C3)", () => {
    configDraft.set("device", "kitchen", "device_base", "gamut.knee", null);
    configDraft.set("device", "kitchen", "device_final", "dither", "atkinson");
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({
      clear: ["gamut.knee"], final: { dither: "atkinson" } });
  });

  it("dirty params are cleared and set in one request, so the server replaces them wholesale", () => {
    configDraft.setPlain("device", "kitchen", "params", '{"other": 1}');
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({ clear: ["params"], params: { other: 1 } });
  });

  it("a changed base gamut value is sent as a set on the base rung, with no clear", () => {
    configDraft.set("device", "kitchen", "device_base", "gamut.amount", 0.7);
    expect(configDraft.saveDeviceBody("kitchen")).toEqual({ gamut: { amount: 0.7 } });
  });

  it("device save is ONE PATCH carrying the clears and the sets", async () => {
    const calls = [];
    vi.spyOn(api, "saveDevice").mockImplementation(async (k, b) => (calls.push(b), { key: k }));
    configDraft.set("device", "kitchen", "device_base", "dither", null);
    configDraft.setPlain("device", "kitchen", "name", "Cuisine");
    await configDraft.save("device", "kitchen");
    expect(calls).toEqual([{ clear: ["dither"], name: "Cuisine" }]);
    expect(configDraft.dirtyCount("device", "kitchen")).toBe(0);
  });

  it("discard reverts the draft and clears a live override even after a reload", async () => {
    vi.spyOn(api, "listOverrides").mockResolvedValue({
      panels: [{ key: "e1004", values: { strength: 0.5 }, expires_in_secs: 100 }], devices: [], tuning_sessions: [],
    });
    const rel = vi.spyOn(api, "releasePanelOverride").mockResolvedValue(null);
    await configDraft.refreshLive();
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.discard("panel", "e1004");
    expect(rel).toHaveBeenCalledWith("e1004");
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.8);
  });

  it("release gives up a pushed override but keeps the draft", async () => {
    vi.spyOn(api, "pushDeviceOverride").mockResolvedValue(null);
    const rel = vi.spyOn(api, "releaseDeviceOverride").mockResolvedValue(null);
    configDraft.set("device", "kitchen", "device_base", "strength", 0.3);
    await configDraft.push("device", "kitchen");
    await configDraft.release();
    expect(rel).toHaveBeenCalledWith("kitchen");
    expect(configDraft.pushed).toBe(null);
    expect(configDraft.value("device", "kitchen", "device_base", "strength")).toBe(0.3);
  });

  it("a failed push is its own error and leaves nothing marked pushed", async () => {
    vi.spyOn(api, "pushPanelOverride").mockRejectedValue(new Error("404 Not Found"));
    configDraft.set("panel", "e1004", "panel", "strength", 0.5);
    await configDraft.push("panel", "e1004");
    expect(configDraft.error).toEqual({ op: "push", message: "404 Not Found" });
    expect(configDraft.pushed).toBe(null);
  });
});
```

- [ ] **Step 2: Run to verify they fail** — `pnpm vitest run test/configdraft.test.js` → FAIL (module missing).

- [ ] **Step 3: Implement**

`webui/src/lib/state/configDraft.svelte.js`:

```js
// One draft store for both config editors (spec §3.2, §7): "edit one rung of
// a config object". Panel schema: the panel rung. Device schema: the base and
// final rungs plus the fields with no rung. Drafts are kept per subject
// (P2-4); the baseline is always today's config from `screens.meta`, so a
// Save followed by `screens.refresh()` leaves the draft clean.

import * as api from "../api.js";
import { screens } from "./screens.svelte.js";

export const NUMBER_FIELDS = [
  "max_error", "noise_scale", "chroma_clamp", "strength",
  "gamut.knee", "gamut.amount", "gamut.max_compression",
];
export const PANEL_FIELDS = ["colors_actual", ...NUMBER_FIELDS];
export const DEVICE_CASCADE_FIELDS = ["colors", "colors_actual", "dither", ...NUMBER_FIELDS, "refresh"];
export const DEVICE_PLAIN_FIELDS = [
  "screen", "name", "params", "temperature_profile", "maximum_compatibility", "min_png_bytes",
];
export const SCHEMAS = {
  panel: { rungs: ["panel"], fields: PANEL_FIELDS, plain: [] },
  device: { rungs: ["device_base", "device_final"], fields: DEVICE_CASCADE_FIELDS, plain: DEVICE_PLAIN_FIELDS },
};
const NUMERIC = new Set([...NUMBER_FIELDS, "refresh", "min_png_bytes"]);
/** A device override cannot carry these (ruling C1: an override takes `dither`, never `colors` or `refresh`; Plan 1 Tasks 2 and 3). */
const NOT_PUSHABLE = new Set(["colors", "refresh"]);

function norm(field, v) {
  if (v === "" || v === undefined || v === null) return null;
  if (NUMERIC.has(field)) {
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  }
  return typeof v === "string" ? v.trim() || null : v;
}

/** `{field: value|null}` over `fields`, reading gamut sub-fields from `src.gamut`. */
function flatten(src, fields) {
  const out = {};
  for (const f of fields) {
    out[f] = f.startsWith("gamut.") ? (src?.gamut?.[f.slice(6)] ?? null) : (src?.[f] ?? null);
  }
  return out;
}

/** The inverse: nulls dropped, gamut nested (omitted when empty). */
function nest(flat) {
  const out = {};
  const gamut = {};
  for (const [f, v] of Object.entries(flat)) {
    if (v === null) continue;
    if (f.startsWith("gamut.")) gamut[f.slice(6)] = v;
    else out[f] = v;
  }
  if (Object.keys(gamut).length) out.gamut = gamut;
  return out;
}

const SCALAR_DITHER_KEYS = new Set(["max_error", "noise_scale", "chroma_clamp", "strength", "gamut"]);
function algorithmsOf(dither) {
  const out = {};
  for (const [k, v] of Object.entries(dither ?? {})) if (!SCALAR_DITHER_KEYS.has(k)) out[k] = v;
  return out;
}

const keyOf = (kind, id) => `${kind}:${id}`;

class ConfigDraft {
  drafts = $state({});
  pushed = $state(null);
  error = $state(null);
  live = $state({ panels: [], devices: [], tuning_sessions: [] });
  usage = $state({ panels: {}, unseen: [] });
  /** Bumped on every draft edit; the Preview re-renders on it (P2-7). */
  revision = $state(0);

  #info(kind, id) {
    const list = kind === "panel" ? screens.meta.panels : screens.meta.devices;
    return (list ?? []).find((x) => x.id === id) ?? null;
  }

  baseline(kind, id) {
    const info = this.#info(kind, id);
    if (kind === "panel") {
      return {
        values: { panel: { colors_actual: info?.colors_actual ?? null, ...flatten(info?.dither, NUMBER_FIELDS) } },
        plain: {},
      };
    }
    return {
      values: {
        device_base: flatten(info?.base, DEVICE_CASCADE_FIELDS),
        device_final: flatten(info?.final, DEVICE_CASCADE_FIELDS),
      },
      plain: {
        screen: info?.screen ?? null,
        name: info?.name ?? null,
        params: JSON.stringify(info?.params ?? {}, null, 2),
        temperature_profile: info?.temperature_profile ?? null,
        maximum_compatibility: info?.maximum_compatibility ?? null,
        min_png_bytes: info?.min_png_bytes ?? null,
      },
    };
  }

  #current(kind, id) {
    return this.drafts[keyOf(kind, id)] ?? this.baseline(kind, id);
  }

  #edit(kind, id, fn) {
    const k = keyOf(kind, id);
    const entry = structuredClone($state.snapshot(this.drafts[k] ?? this.baseline(kind, id)));
    fn(entry);
    this.drafts = { ...this.drafts, [k]: entry };
    this.revision += 1;
  }

  value(kind, id, rung, field) {
    return this.#current(kind, id).values[rung]?.[field] ?? null;
  }
  set(kind, id, rung, field, v) {
    this.#edit(kind, id, (e) => (e.values[rung][field] = norm(field, v)));
  }
  plainValue(kind, id, field) {
    return this.#current(kind, id).plain[field] ?? null;
  }
  setPlain(kind, id, field, v) {
    this.#edit(kind, id, (e) => (e.plain[field] = field === "params" ? v : norm(field, v)));
  }

  dirty(kind, id) {
    const cur = this.#current(kind, id);
    const base = this.baseline(kind, id);
    const out = [];
    for (const rung of SCHEMAS[kind].rungs) {
      for (const f of SCHEMAS[kind].fields) {
        if ((cur.values[rung][f] ?? null) !== (base.values[rung][f] ?? null)) out.push({ rung, field: f });
      }
    }
    for (const f of SCHEMAS[kind].plain) {
      if ((cur.plain[f] ?? null) !== (base.plain[f] ?? null)) out.push({ rung: null, field: f });
    }
    return out;
  }
  dirtyCount(kind, id) {
    return this.dirty(kind, id).length;
  }
  isDirty(kind, id, rung, field) {
    return this.dirty(kind, id).some((d) => d.rung === rung && d.field === field);
  }

  /** `draft.panel` of the render request: the whole rung. */
  panelWire(id) {
    const info = this.#info("panel", id);
    const v = this.#current("panel", id).values.panel;
    const { colors_actual, ...numbers } = v;
    const dither = { ...nest(numbers), ...algorithmsOf(info?.dither) };
    const out = { colors: info?.colors };
    if (colors_actual !== null) out.colors_actual = colors_actual;
    if (Object.keys(dither).length) out.dither = dither;
    return out;
  }

  /** `screen`, `draft.base|final|params` of a mode c render request. */
  deviceWire(id) {
    const cur = this.#current("device", id);
    let params;
    try {
      params = JSON.parse(cur.plain.params || "{}");
    } catch (e) {
      throw new Error("Invalid JSON in the device's params: " + e.message);
    }
    return {
      screen: cur.plain.screen,
      params,
      base: nest(cur.values.device_base),
      final: nest(cur.values.device_final),
    };
  }

  /**
   * P2-1/P2-2: dirty, set, pushable values; a device's final over its base.
   * Ruling PF5: Push never shows what Save will not. A dirty base value is
   * skipped whenever the field's EFFECTIVE final value (draft or config) is
   * non-null, because Save leaves that final winning; `unpushable()` lists it
   * as "outranked by final".
   */
  pushBody(kind, id) {
    const cur = this.#current(kind, id);
    const flat = {};
    const rungs = kind === "panel" ? ["panel"] : ["device_base", "device_final"];
    for (const { rung, field } of this.dirty(kind, id)) {
      if (!rungs.includes(rung) || NOT_PUSHABLE.has(field)) continue;
      const v = cur.values[rung][field];
      if (v === null) continue;
      if (rung === "device_base" && cur.values.device_final[field] !== null) continue;
      flat[field] = v;
    }
    return nest(flat);
  }

  /** What a Push leaves out, for the footer's "applies on Save only". */
  unpushable(kind, id) {
    const cur = this.#current(kind, id);
    const out = new Set();
    for (const { rung, field } of this.dirty(kind, id)) {
      if (rung === null) out.add(field);
      else if (NOT_PUSHABLE.has(field)) out.add(field);
      else if (cur.values[rung][field] === null) out.add(`${field} (removal)`);
      else if (rung === "device_base" && cur.values.device_final[field] !== null) out.add(`${field} (outranked by final)`);
    }
    return [...out];
  }

  savePanelBody(id) {
    const { colors_actual, dither } = this.panelWire(id);
    const out = {};
    if (colors_actual !== undefined) out.colors_actual = colors_actual;
    if (dither !== undefined) out.dither = dither;
    return out;
  }

  /**
   * The device Save, as ONE `DeviceWrite` (controller ruling C3): the server
   * clears first and sets second, so a removed value goes in `clear` and a
   * changed one is set, with no ordering for the client to get wrong. Only
   * dirty fields travel; `gamut` sets carry just the dirty knobs (the server
   * merges them over what is there).
   */
  saveDeviceBody(id) {
    const cur = this.#current("device", id);
    const clear = [];
    const body = {};
    const final = {};
    const gamut = { device_base: {}, device_final: {} };
    for (const { rung, field } of this.dirty("device", id)) {
      if (rung === null) {
        const v = cur.plain[field];
        if (field === "params") {
          clear.push("params");
          body.params = JSON.parse(v || "{}");
        } else if (v === null) clear.push(field);
        else body[field] = v;
        continue;
      }
      const v = cur.values[rung][field];
      const prefix = rung === "device_final" ? "final." : "";
      if (v === null) {
        clear.push(prefix + field);
      } else if (field.startsWith("gamut.")) {
        gamut[rung][field.slice(6)] = v;
      } else if (rung === "device_final") {
        final[field] = v;
      } else {
        body[field] = v;
      }
    }
    if (Object.keys(gamut.device_base).length) body.gamut = gamut.device_base;
    if (Object.keys(gamut.device_final).length) final.gamut = gamut.device_final;
    if (Object.keys(final).length) body.final = final;
    if (clear.length) body.clear = clear;
    return Object.keys(body).length ? body : null;
  }

  liveFor(kind, id) {
    const list = kind === "panel" ? this.live.panels : this.live.devices;
    return (list ?? []).find((o) => o.key === id) ?? null;
  }

  async refreshLive() {
    try {
      this.live = await api.listOverrides();
    } catch (e) {
      this.error = { op: "overrides", message: e.message };
    }
  }

  async loadUsage() {
    try {
      this.usage = await api.panelUsage();
    } catch (e) {
      this.error = { op: "panel usage", message: e.message };
    }
  }

  async load() {
    await Promise.all([this.refreshLive(), this.loadUsage()]);
  }

  /** Stage 1: put the dirty values on real hardware. A button, never a debounce. */
  async push(kind, id, { watchDevice = "" } = {}) {
    const body = this.pushBody(kind, id);
    try {
      if (kind === "panel") await api.pushPanelOverride(id, body);
      else await api.pushDeviceOverride(id, body);
    } catch (e) {
      this.error = { op: "push", message: e.message };
      return;
    }
    this.pushed = { kind, id, device: kind === "panel" ? watchDevice : "" };
    this.error = null;
    // P2-3: the session starts here, never from the Preview.
    if (kind === "panel" && watchDevice) {
      try {
        await api.startTuningSession(watchDevice);
      } catch (e) {
        this.error = { op: "session", message: e.message };
      }
    }
    await this.refreshLive();
  }

  /** Stage 2: write config.yaml. The server drops the override itself. */
  async save(kind, id) {
    try {
      if (kind === "panel") {
        await api.savePanel(id, this.savePanelBody(id));
      } else {
        const body = this.saveDeviceBody(id);
        if (body) await api.saveDevice(id, body);
      }
    } catch (e) {
      this.error = { op: "save", message: e.message };
      return;
    }
    this.error = null;
    const { [keyOf(kind, id)]: _gone, ...rest } = this.drafts;
    this.drafts = rest;
    this.revision += 1;
    await this.#endSessionFor(kind, id);
    await screens.refresh();
    await this.load();
  }

  async discard(kind, id) {
    const { [keyOf(kind, id)]: _gone, ...rest } = this.drafts;
    this.drafts = rest;
    this.revision += 1;
    if (this.liveFor(kind, id) || (this.pushed?.kind === kind && this.pushed?.id === id)) {
      try {
        if (kind === "panel") await api.releasePanelOverride(id);
        else await api.releaseDeviceOverride(id);
        this.error = null;
      } catch (e) {
        this.error = { op: "discard", message: e.message };
      }
    }
    await this.#endSessionFor(kind, id);
    await this.refreshLive();
  }

  /** Subject switch or Edit closed: give up server state, keep the draft. */
  async release() {
    const p = this.pushed;
    if (!p) return;
    try {
      if (p.kind === "panel") await api.releasePanelOverride(p.id);
      else await api.releaseDeviceOverride(p.id);
    } catch {
      // Best effort; the strip still lists what is live, and the TTL ends it.
    }
    await this.#endSessionFor(p.kind, p.id);
    await this.refreshLive();
  }

  /** The strip's Discard for an override this page did not push (a reload). */
  async discardLive(kind, key) {
    try {
      if (kind === "panel") await api.releasePanelOverride(key);
      else await api.releaseDeviceOverride(key);
    } catch (e) {
      this.error = { op: "discard", message: e.message };
    }
    if (this.pushed?.kind === kind && this.pushed?.id === key) await this.#endSessionFor(kind, key);
    await this.refreshLive();
  }

  async #endSessionFor(kind, id) {
    const p = this.pushed;
    if (!p || p.kind !== kind || p.id !== id) return;
    this.pushed = null;
    if (!p.device) return;
    try {
      await api.endTuningSession(p.device);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  reset() {
    this.drafts = {};
    this.pushed = null;
    this.error = null;
    this.live = { panels: [], devices: [], tuning_sessions: [] };
    this.usage = { panels: {}, unseen: [] };
    this.revision = 0;
  }
}

export const configDraft = new ConfigDraft();
```

- [ ] **Step 4: Wire it in**

`PreviewPane.svelte`: delete `metaDraft` and the local `usage`; pass `draft: configDraft, usage: configDraft.usage` to `buildRenderRequest`/`previewSize`. Add the debounced draft effect (P2-7):

```js
  const DRAFT_RENDER_DEBOUNCE_MS = 300;
  let lastRevision = configDraft.revision;
  $effect(() => {
    const rev = configDraft.revision;
    if (rev === lastRevision) return;
    const id = setTimeout(() => {
      lastRevision = rev;
      renderNow();
    }, DRAFT_RENDER_DEBOUNCE_MS);
    return () => clearTimeout(id);
  });
```

`LibraryPane.svelte`: delete its local `usage` and `onMount`; read `configDraft.usage` (so `devicePanelLabel` calls `devicePanel({ meta: screens.meta, usage: configDraft.usage }, d.id)`). `AppShell.svelte`: after `screens.load();` add `configDraft.load();`.

Journey harness (ruling PF3): in `webui/test/journey-finding-h.test.js` and `webui/test/journey-subject-buffers.test.js` add the harness's `// [Task 7]` lines: `import { configDraft } from "../src/lib/state/configDraft.svelte.js";` after the `resolution` import and `configDraft.reset();` after `resolution.reset();` in `beforeEach`.

- [ ] **Step 5: Run** — `pnpm vitest run` → PASS.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/state/configDraft.svelte.js webui/src/lib/components/PreviewPane.svelte webui/src/lib/components/LibraryPane.svelte webui/src/lib/components/AppShell.svelte webui/test/configdraft.test.js webui/test/journey-finding-h.test.js webui/test/journey-subject-buffers.test.js
git commit -m "feat(webui): one schema-driven draft store for panels and devices"
```

---

### Task 8: The ladder widget

**Files:**
- Create: `webui/src/lib/components/Ladder.svelte`, `webui/test/ladder.test.js`

**Interfaces:**
- Consumes: a `SettingReport` (Task 3), `resolution.state`/`failure`.
- Produces: `<Ladder setting editing={[rung,…]} report state failure stale />`. Renders rungs lowest-first (`fallback` … `override`, spec §5's mock), each `<li data-rung data-status>`; the edited rung(s) marked `●` with "← editing"; the winner "← wins"; `data-testid="effective"` row `▶ effective {value} from {rung label}`; a `role="status"` note when an edited rung is outranked ("your {rung} value is not used in this preview") or rejected ("your {rung} value was rejected: {reason}"). States: `none` → "No render yet."; `failed` → `role="alert"` "Last render failed: {failure}" and no rungs; `stale` → "From the last render — Preview is closed."

- [ ] **Step 1: Write the failing tests**

`webui/test/ladder.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen as dom, cleanup } from "@testing-library/svelte";
import Ladder from "../src/lib/components/Ladder.svelte";

// No auto-cleanup in this vitest setup (no globals): without it the loop below
// would stack ladders and `getByTestId("effective")` would match several (PF4).
beforeEach(() => cleanup());

const ORDER = ["override", "device_final", "screen", "device_base", "panel", "fallback"];
function report(from, rungs, effective = "v") {
  return { effective, from, order: ORDER, rungs };
}
const status = (rung) => document.querySelector(`[data-rung="${rung}"]`).dataset.status;

describe("Ladder", () => {
  for (const rung of ORDER) {
    it(`shows ${rung} winning`, () => {
      const rungs = { [rung]: { value: rung, status: "winner" } };
      if (rung !== "fallback") rungs.fallback = { value: "fb", status: "outranked" };
      render(Ladder, { setting: "dither", editing: [], state: "ok", report: report(rung, rungs, rung) });
      expect(status(rung)).toBe("winner");
      expect(dom.getByTestId("effective").textContent).toContain(rung === "device_final" ? "device final" : rung.replace("_", " "));
      for (const other of ORDER.filter((r) => r !== rung && r !== "fallback")) expect(status(other)).toBe("absent");
    });
  }

  it("override wins over everything, and an edited panel rung says so", () => {
    render(Ladder, {
      setting: "strength", editing: ["panel"], state: "ok",
      report: report("override", {
        override: { value: 0.5, status: "winner" },
        screen: { value: 0.9, status: "outranked" },
        panel: { value: 0.5, status: "outranked" },
      }, 0.5),
    });
    expect(status("override")).toBe("winner");
    expect(dom.getByRole("status").textContent).toContain("your panel value is not used in this preview");
    expect(document.querySelector('[data-rung="panel"]').textContent).toContain("●");
  });

  it("explains a rejected rung with its reason", () => {
    render(Ladder, {
      setting: "colors_actual", editing: ["panel"], state: "ok",
      report: report("fallback", {
        panel: { value: ["#101010", "#E8E8E8"], status: "rejected", reason: "has 2 usable entries but the resolved palette has 16" },
      }, null),
    });
    expect(status("panel")).toBe("rejected");
    expect(dom.getByRole("status").textContent).toContain(
      "your panel value was rejected: has 2 usable entries but the resolved palette has 16");
  });

  it("says no render yet, and shows no rungs", () => {
    render(Ladder, { setting: "dither", editing: [], state: "none", report: null });
    expect(dom.getByText("No render yet.")).toBeTruthy();
    expect(document.querySelector("[data-rung]")).toBe(null);
  });

  it("says the last render failed instead of showing stale rungs", () => {
    render(Ladder, { setting: "dither", editing: [], state: "failed", failure: "Render error: boom", report: null });
    expect(dom.getByRole("alert").textContent).toContain("Last render failed: Render error: boom");
    expect(document.querySelector("[data-rung]")).toBe(null);
  });

  it("labels a payload shown while Preview is closed", () => {
    render(Ladder, { setting: "dither", editing: [], state: "ok", stale: true,
      report: report("fallback", { fallback: { value: "atkinson", status: "winner" } }, "atkinson") });
    expect(dom.getByText("From the last render — Preview is closed.")).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run** — FAIL (component missing).

- [ ] **Step 3: Implement `Ladder.svelte`**

```svelte
<script>
  // One cascade setting, every rung with its value and status (spec §5):
  // what we see (effective), what we modify (●), and why nothing happens
  // (outranked, or rejected with a reason).
  let { setting, editing = [], report = null, state = "none", failure = null, stale = false } = $props();

  const LABEL = {
    override: "override", device_final: "device final", screen: "screen",
    device_base: "device base", panel: "panel", fallback: "fallback",
  };
  const DEFAULT_ORDER = ["override", "device_final", "screen", "device_base", "panel", "fallback"];

  const order = $derived([...(report?.order ?? DEFAULT_ORDER)].reverse());
  function fmt(v) {
    if (v === undefined || v === null) return "—";
    if (Array.isArray(v)) return v.join(",");
    return String(v);
  }
  const notes = $derived(
    editing
      .map((rung) => {
        const r = report?.rungs?.[rung];
        if (r?.status === "outranked") return `your ${LABEL[rung]} value is not used in this preview`;
        if (r?.status === "rejected") return `your ${LABEL[rung]} value was rejected: ${r.reason}`;
        return null;
      })
      .filter(Boolean),
  );
</script>

<section class="flex flex-col gap-0.5 text-xs" data-testid={`ladder-${setting}`} aria-label={`${setting} ladder`}>
  {#if state === "none"}
    <p class="text-[var(--text-muted)]">No render yet.</p>
  {:else if state === "failed"}
    <p role="alert" class="text-[var(--danger-color)]">Last render failed: {failure}</p>
  {:else}
    {#if stale}<p class="text-[var(--text-muted)]">From the last render — Preview is closed.</p>{/if}
    <ol class="flex flex-col font-mono">
      {#each order as rung (rung)}
        {@const r = report?.rungs?.[rung]}
        <li data-rung={rung} data-status={r?.status ?? "absent"}
            class="grid grid-cols-[1rem_7rem_1fr_auto] gap-1 {r?.status === 'winner' ? 'font-semibold' : ''}">
          <span>{editing.includes(rung) ? "●" : ""}</span>
          <span>{LABEL[rung]}</span>
          <span class="truncate" title={fmt(r?.value)}>{fmt(r?.value)}</span>
          <span class="text-[var(--text-muted)]">
            {#if r?.status === "winner"}← wins{:else if r?.status === "rejected"}rejected{:else if r?.status === "outranked"}outranked{/if}
            {#if editing.includes(rung)} ← editing{/if}
          </span>
        </li>
      {/each}
      <li data-testid="effective">▶ effective {fmt(report?.effective)} from {LABEL[report?.from] ?? "—"}</li>
    </ol>
    {#each notes as n (n)}<p role="status" class="text-[var(--danger-color)]">{n}</p>{/each}
  {/if}
</section>
```

- [ ] **Step 4: Run** — `pnpm vitest run test/ladder.test.js` → PASS.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/components/Ladder.svelte webui/test/ladder.test.js
git commit -m "feat(webui): the ladder widget -- every rung, its value and its status"
```

---

### Task 9: `ConfigEditor` — one editor, two schemas; live overrides on the strip

**Files:**
- Create: `webui/src/lib/components/ConfigEditor.svelte`, `webui/src/lib/components/SwatchEditor.svelte`, `webui/test/configeditor.test.js`
- Modify: `EditPane.svelte` (route panel/device to `ConfigEditor`), `StatusStrip.svelte` (`configDraft.error`, live lines with Discard), `AppShell.svelte` (release + resolution reset on subject switch)
- Test: `webui/test/statusstrip.test.js`

**Interfaces:**
- Consumes: `configDraft` (Task 7), `Ladder` (Task 8), `resolution`, `subject`, `layout`, `screens.meta`.
- Produces:
  - Control ids: `cfg-{rung}-{field}` with dots replaced by `-` (e.g. `cfg-device_final-dither`, `cfg-panel-gamut-knee`); plain fields `cfg-plain-{field}`.
  - Buttons: `Push to panel` / `Push to device`, `Save`, `Discard`; footer text `● {n} unsaved`.
  - Copy (verbatim): `⚠ {n} devices use this panel` (singular `⚠ 1 device uses this panel`), `({m} not seen since byonk started)`; live banner `◉ Live on {name} — the real panel is showing this draft.` / `◉ Live on {name} — the real display is showing this draft.` followed by `It reverts on its own within 30 minutes, or on Discard.`
  - StatusStrip: one `role="status"` line per live override `◉ Live on {key}` with a `Discard` button calling `configDraft.discardLive(kind, key)`; `configDraft.error` rendered as `config {op}: {message}` (its own surface, Ruling 66).

- [ ] **Step 1: Write the failing tests**

`webui/test/configeditor.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen as dom, fireEvent, waitFor, cleanup } from "@testing-library/svelte";
import ConfigEditor from "../src/lib/components/ConfigEditor.svelte";
import { configDraft } from "../src/lib/state/configDraft.svelte.js";
import { subject } from "../src/lib/state/subject.svelte.js";
import { resolution } from "../src/lib/state/resolution.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  cleanup(); vi.restoreAllMocks();
  configDraft.reset(); subject.reset(); resolution.reset();
  screens.list = [{ ref: "local/a" }, { ref: "local/b" }];
  screens.meta = {
    panels: [{ id: "e1004", name: "reTerminal E1004", colors: "#000000,#FFFFFF",
               colors_actual: "#101010,#E8E8E8", dither: null }],
    devices: [{ id: "kitchen", screen: "local/a", panel: "e1004", name: "Kitchen", params: {},
                base: {}, final: null, temperature_profile: null, maximum_compatibility: null, min_png_bytes: null }],
    dither_algorithms: [{ name: "atkinson", defaults: {} }, { name: "stucki", defaults: {} }],
  };
  configDraft.usage = { panels: { e1004: ["kitchen", "hall", "office"] }, unseen: ["attic", "porch"] };
  vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
  vi.spyOn(api, "panelUsage").mockResolvedValue(configDraft.usage);
});

describe("ConfigEditor, panel schema", () => {
  beforeEach(() => subject.select("panel", "e1004"));

  it("names its subject and warns how many devices it reaches", () => {
    render(ConfigEditor);
    expect(dom.getByRole("heading", { name: /panel reTerminal E1004/ })).toBeTruthy();
    expect(dom.getByText("⚠ 3 devices use this panel")).toBeTruthy();
    expect(dom.getByText("(2 not seen since byonk started)")).toBeTruthy();
  });

  it("an edit marks one unsaved change and enables Push and Save", async () => {
    render(ConfigEditor);
    expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(true);
    const strength = document.querySelector("#cfg-panel-strength");
    strength.value = "0.5";
    await fireEvent.input(strength);
    expect(dom.getByText("● 1 unsaved")).toBeTruthy();
    expect(dom.getByRole("button", { name: "Push to panel" }).disabled).toBe(false);
    expect(dom.getByRole("button", { name: "Save" }).disabled).toBe(false);
  });

  it("shows a ladder per setting from the shared store", () => {
    resolution.record({ strength: { effective: 1, from: "fallback", order: [], rungs: { fallback: { value: 1, status: "winner" } } } }, "panel:e1004");
    render(ConfigEditor);
    expect(document.querySelector('[data-testid="ladder-strength"] [data-rung="fallback"]').dataset.status).toBe("winner");
  });

  it("a ladder ignores a payload that belongs to another subject", () => {
    resolution.record({ strength: { effective: 1, from: "fallback", order: [], rungs: {} } }, "screen:local/a");
    render(ConfigEditor);
    expect(document.querySelector('[data-testid="ladder-strength"]').textContent).toContain("No render yet.");
  });

  it("states a live push plainly, with the TTL copy", async () => {
    configDraft.live = { panels: [{ key: "e1004", values: { strength: 0.5 }, expires_in_secs: 1700 }], devices: [], tuning_sessions: [] };
    render(ConfigEditor);
    expect(dom.getByText("◉ Live on reTerminal E1004 — the real panel is showing this draft.")).toBeTruthy();
    expect(dom.getByText("It reverts on its own within 30 minutes, or on Discard.")).toBeTruthy();
  });
});

describe("ConfigEditor, device schema", () => {
  beforeEach(() => subject.select("device", "kitchen"));

  it("carries the plain fields above the cascade, and both rungs per setting", () => {
    render(ConfigEditor);
    for (const f of ["screen", "name", "params", "temperature_profile", "maximum_compatibility", "min_png_bytes"]) {
      expect(document.querySelector(`#cfg-plain-${f}`), f).toBeTruthy();
    }
    expect(document.querySelector("#cfg-device_base-dither")).toBeTruthy();
    expect(document.querySelector("#cfg-device_final-dither")).toBeTruthy();
    expect(document.querySelector("#cfg-device_final-refresh")).toBeTruthy();
  });

  it("says which dirty fields a Push cannot carry", async () => {
    render(ConfigEditor);
    const r = document.querySelector("#cfg-device_base-refresh");
    r.value = "60";
    await fireEvent.input(r);
    expect(dom.getByText(/applies on Save only: refresh/)).toBeTruthy();
    expect(dom.getByRole("button", { name: "Push to device" }).disabled).toBe(true);
  });
});
```

Add to `statusstrip.test.js`:

```js
  it("lists a live override from before a reload, and its Discard clears it", async () => {
    configDraft.live = { panels: [{ key: "e1004", values: {}, expires_in_secs: 100 }], devices: [], tuning_sessions: [] };
    const discard = vi.spyOn(configDraft, "discardLive").mockResolvedValue();
    render(StatusStrip);
    expect(dom.getByRole("status").textContent).toContain("◉ Live on e1004");
    await fireEvent.click(dom.getByRole("button", { name: "Discard" }));
    expect(discard).toHaveBeenCalledWith("panel", "e1004");
  });

  it("renders configDraft.error as its own surface", () => {
    configDraft.error = { op: "save", message: "409 Conflict" };
    render(StatusStrip);
    expect(dom.getByRole("alert").textContent).toBe("config save: 409 Conflict");
  });
```

- [ ] **Step 2: Run** — FAIL (component missing).

- [ ] **Step 3: `SwatchEditor.svelte`**

Lift `TunePane`'s swatch radiogroup, H/S/L sliders, hex box and original/live pair (from `git show HEAD~N:webui/src/lib/components/TunePane.svelte`, the commit before Task 4; or `git log --diff-filter=D -- webui/src/lib/components/TunePane.svelte`) into a component with props `{ value, baseline, onchange }` where `value`/`baseline` are comma-separated strings and `onchange(newCommaString)` is called on every valid edit. Keep ids `tune-hue`, `tune-sat`, `tune-light`, `tune-hex` renamed to `swatch-hue`, `swatch-sat`, `swatch-light`, `swatch-hex`; the selected index is local `$state(0)`; uses `hexToHsl`/`hslToHex` from `../utils/color.js` and `Input`/`Label` from `$lib/ui`.

- [ ] **Step 4: `ConfigEditor.svelte`**

Script:

```js
  import { onDestroy } from "svelte";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft, SCHEMAS } from "../state/configDraft.svelte.js";
  import { resolution } from "../state/resolution.svelte.js";
  import { layout } from "../state/layout.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import Ladder from "./Ladder.svelte";
  import SwatchEditor from "./SwatchEditor.svelte";
  import { Button } from "$lib/ui/button/index.js";
  import { Input } from "$lib/ui/input/index.js";
  import { Label } from "$lib/ui/label/index.js";

  const selectClass =
    "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm " +
    "text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 " +
    "focus-visible:ring-ring/50 dark:bg-input/30";
  const RUNG_LABEL = { panel: "panel", device_base: "device base", device_final: "device final" };
  const STEP = { max_error: 0.05, noise_scale: 0.5, chroma_clamp: 0.05, strength: 0.05,
                 "gamut.knee": 0.05, "gamut.amount": 0.05, "gamut.max_compression": 0.1, refresh: 60 };

  const kind = $derived(subject.kind);
  const id = $derived(subject.id);
  const schema = $derived(SCHEMAS[kind]);
  const info = $derived(
    (kind === "panel" ? screens.meta.panels : screens.meta.devices)?.find((x) => x.id === id) ?? null,
  );
  const displayName = $derived(kind === "panel" ? (info?.name ?? id) : (info?.name ?? id));
  const users = $derived(configDraft.usage.panels?.[id]?.length ?? 0);
  const unseen = $derived(configDraft.usage.unseen?.length ?? 0);
  const dirtyCount = $derived((configDraft.revision, configDraft.dirtyCount(kind, id)));
  const pushBody = $derived((configDraft.revision, configDraft.pushBody(kind, id)));
  const unpushable = $derived((configDraft.revision, configDraft.unpushable(kind, id)));
  const live = $derived(configDraft.liveFor(kind, id));
  const ladderState = $derived(resolution.subjectKey === subject.key ? resolution.state : "none");

  const cid = (rung, field) => `cfg-${rung}-${field.replaceAll(".", "-")}`;
  const val = (rung, field) => (configDraft.revision, configDraft.value(kind, id, rung, field));
  const plain = (field) => (configDraft.revision, configDraft.plainValue(kind, id, field));

  function onField(rung, field, e) {
    configDraft.set(kind, id, rung, field, e.currentTarget.value);
  }
  function onPlain(field, e) {
    const t = e.currentTarget;
    let v = t.value;
    if (field === "maximum_compatibility") v = v === "" ? null : v === "true";
    configDraft.setPlain(kind, id, field, v);
  }
  async function push() {
    await configDraft.push(kind, id, { watchDevice: kind === "panel" ? subject.watchDevice : "" });
  }

  // Closing the Edit panel gives up a pushed override (spec §6); the draft stays.
  onDestroy(() => {
    configDraft.release();
  });
```

Markup structure (every control and its binding):

```svelte
<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3 text-sm" data-testid="config-editor">
  <h2 class="font-medium">Edit — {kind} {displayName}</h2>

  {#if kind === "panel"}
    <p class="text-xs">{users === 1 ? "⚠ 1 device uses this panel" : `⚠ ${users} devices use this panel`}</p>
    {#if unseen > 0}<p class="text-xs text-[var(--text-muted)]">({unseen} not seen since byonk started)</p>{/if}
  {/if}

  {#if live}
    <div role="status" class="my-2 rounded border border-[var(--danger-color)] p-2 text-xs">
      <p>◉ Live on {displayName} — the real {kind === "panel" ? "panel" : "display"} is showing this draft.</p>
      <p>It reverts on its own within 30 minutes, or on Discard.</p>
    </div>
  {/if}

  {#if kind === "device"}
    <section class="flex flex-col gap-2 py-2" aria-label="Device">
      <Label for="cfg-plain-screen">Screen</Label>
      <select id="cfg-plain-screen" class={selectClass} value={plain("screen")} onchange={(e) => onPlain("screen", e)}>
        {#each screens.list as s (s.ref)}<option value={s.ref}>{s.ref}</option>{/each}
      </select>
      <Label for="cfg-plain-name">Name</Label>
      <Input id="cfg-plain-name" value={plain("name") ?? ""} oninput={(e) => onPlain("name", e)} />
      <Label for="cfg-plain-params">Params (JSON)</Label>
      <textarea id="cfg-plain-params" class={selectClass + " h-20 py-1.5 font-mono"}
                value={plain("params")} oninput={(e) => onPlain("params", e)}></textarea>
      <Label for="cfg-plain-temperature_profile">Temperature profile</Label>
      <select id="cfg-plain-temperature_profile" class={selectClass} value={plain("temperature_profile") ?? ""}
              onchange={(e) => onPlain("temperature_profile", e)}>
        <option value="">(unset)</option><option value="default">default</option>
        <option value="a">a</option><option value="b">b</option>
      </select>
      <Label for="cfg-plain-maximum_compatibility">Maximum compatibility</Label>
      <select id="cfg-plain-maximum_compatibility" class={selectClass}
              value={plain("maximum_compatibility") === null ? "" : String(plain("maximum_compatibility"))}
              onchange={(e) => onPlain("maximum_compatibility", e)}>
        <option value="">(unset)</option><option value="true">on</option><option value="false">off</option>
      </select>
      <Label for="cfg-plain-min_png_bytes">Minimum PNG bytes</Label>
      <Input id="cfg-plain-min_png_bytes" type="number" min="0" value={plain("min_png_bytes") ?? ""}
             oninput={(e) => onPlain("min_png_bytes", e)} />
    </section>
  {/if}

  {#each schema.fields as field (field)}
    <section class="flex flex-col gap-1 border-t border-[var(--border-color)] py-2" aria-label={field}>
      <h3 class="font-mono text-xs">{field}</h3>
      {#each schema.rungs as rung (rung)}
        <Label for={cid(rung, field)}>{field} ({RUNG_LABEL[rung]})</Label>
        {#if field === "dither"}
          <select id={cid(rung, field)} class={selectClass} value={val(rung, field) ?? ""}
                  onchange={(e) => onField(rung, field, e)}>
            <option value="">— (none)</option>
            {#each screens.meta.dither_algorithms ?? [] as a (a.name)}<option value={a.name}>{a.name}</option>{/each}
          </select>
        {:else if field === "colors_actual" && kind === "panel"}
          <Input id={cid(rung, field)} value={val(rung, field) ?? ""} oninput={(e) => onField(rung, field, e)} />
          <SwatchEditor value={val(rung, field) ?? info?.colors ?? ""}
                        baseline={info?.colors_actual ?? info?.colors ?? ""}
                        onchange={(v) => configDraft.set(kind, id, rung, field, v)} />
        {:else if field === "colors" || field === "colors_actual"}
          <Input id={cid(rung, field)} value={val(rung, field) ?? ""} oninput={(e) => onField(rung, field, e)} />
        {:else}
          <Input id={cid(rung, field)} type="number" step={STEP[field]} value={val(rung, field) ?? ""}
                 oninput={(e) => onField(rung, field, e)} />
        {/if}
      {/each}
      <Ladder setting={field} editing={schema.rungs} state={ladderState} failure={resolution.failure}
              stale={!layout.isOpen("preview")}
              report={ladderState === "ok" ? resolution.setting(field) : null} />
    </section>
  {/each}

  <footer class="sticky bottom-0 flex flex-wrap items-center gap-2 border-t border-[var(--border-color)] bg-[var(--bg-color)] py-2">
    <span>● {dirtyCount} unsaved</span>
    <Button type="button" disabled={Object.keys(pushBody).length === 0} onclick={push}>
      {kind === "panel" ? "Push to panel" : "Push to device"}
    </Button>
    <Button type="button" variant="outline" disabled={dirtyCount === 0} onclick={() => configDraft.save(kind, id)}>Save</Button>
    <Button type="button" variant="ghost" disabled={dirtyCount === 0 && !live} onclick={() => configDraft.discard(kind, id)}>Discard</Button>
    {#if unpushable.length}
      <span class="text-xs text-[var(--text-muted)]">applies on Save only: {unpushable.join(", ")}</span>
    {/if}
  </footer>
</div>
```

`EditPane.svelte`: replace the placeholder branch with `{#key subject.key}<ConfigEditor />{/key}` (the `{#key}` remounts per subject, so `onDestroy` releases on every switch) and import it.

`AppShell.svelte`: add

```js
  // A new subject's ladder starts at "no render yet", never at the old
  // subject's rungs (spec §5).
  $effect(() => {
    subject.key;
    untrack(() => resolution.reset());
  });
```

`StatusStrip.svelte`: import `configDraft`; `hasAnything` adds `!!configDraft.error || configDraft.live.panels.length > 0 || configDraft.live.devices.length > 0`; markup:

```svelte
    {#each configDraft.live.panels as o (o.key)}
      <span role="status" class="flex items-center gap-2 text-[var(--danger-color)]">
        ◉ Live on {o.key}
        <Button type="button" size="xs" variant="ghost" onclick={() => configDraft.discardLive("panel", o.key)}>Discard</Button>
      </span>
    {/each}
    {#each configDraft.live.devices as o (o.key)}
      <span role="status" class="flex items-center gap-2 text-[var(--danger-color)]">
        ◉ Live on {o.key}
        <Button type="button" size="xs" variant="ghost" onclick={() => configDraft.discardLive("device", o.key)}>Discard</Button>
      </span>
    {/each}
    {#if configDraft.error}
      <div role="alert" class="text-[var(--danger-color)]">config {configDraft.error.op}: {configDraft.error.message}</div>
    {/if}
```

- [ ] **Step 5: Run** — `pnpm vitest run` → PASS.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/components/ConfigEditor.svelte webui/src/lib/components/SwatchEditor.svelte webui/src/lib/components/EditPane.svelte webui/src/lib/components/StatusStrip.svelte webui/src/lib/components/AppShell.svelte webui/test/configeditor.test.js webui/test/statusstrip.test.js
git commit -m "feat(webui): one config editor for panels and devices, with the ladder and Push/Save"
```

---

### Task 10: Per-mode journeys, Preview-writes-nothing, and the user docs

**Files:**
- Create: `webui/test/journey-mode-screen.test.js`, `webui/test/journey-mode-panel.test.js`, `webui/test/journey-mode-device.test.js`, `webui/test/journey-preview-writes-nothing.test.js`
- Modify: `docs/src/guide/dev-mode.md` — `## The five panels` (`:84-114`), `## Preview and pop-out` (`:205-215`), `## Simulate panel` + `### Live device sync` (`:240-273`), `### Dither Tuning Controls` (`:291-300`), `### Tune: colour calibration` (`:302-337`), `### Calibration Workflow` (`:423-449`), `## Example workflow` steps 4–7 (`:468-494`), the intro paragraph (`:3-8`)

**Interfaces:**
- Consumes: everything from Tasks 1–9 and the harness block.

- [ ] **Step 1: Write the journeys** (each file: harness block, then)

`journey-mode-screen.test.js`:

```js
describe("mode a: pick a screen, edit, see it render, switch hardware", () => {
  it("renders the edited screen, then the same screen as a device", async () => {
    const calls = stubServer();
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    const renders = () => calls.filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
    await waitFor(() => expect(renders().at(-1)).toMatchObject({ screen: "local/a", width: 800, height: 480 }));

    const sel = document.querySelector("#preview-target");
    sel.value = "device:kitchen";
    await fireEvent.change(sel);
    await waitFor(() => expect(renders().at(-1)).toMatchObject({ screen: "local/a", device: "kitchen" }));
    expect(document.querySelector('[data-testid="preview-title"]').textContent).toBe("local/a as kitchen");
    expect(resolution.state).toBe("ok");
  });
});
```

`journey-mode-panel.test.js`:

```js
describe("mode b: pick a panel, change a swatch value, Push, Save", () => {
  it("previews the draft, pushes dirty fields, watches a device, saves the rung", async () => {
    const calls = stubServer();
    const push = vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    const session = vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue(null);
    const save = vi.spyOn(api, "savePanel").mockResolvedValue(null);
    await mount();

    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    await waitFor(() => expect(dom.getByText("⚠ 1 device uses this panel")).toBeTruthy());
    const renders = () => calls.filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
    await waitFor(() => expect(renders().at(-1)).toMatchObject({
      screen: "byonk-builtin/calibration/tone", panel: "e1004" }));

    const watch = document.querySelector("#preview-watch");
    watch.value = "kitchen";
    await fireEvent.change(watch);
    expect(session).not.toHaveBeenCalled(); // P2-3: choosing whom to watch writes nothing

    // Spec §9 "change a swatch" (ruling PF10): the first swatch of the measured
    // colours is selected by default; edit its hex box, not the raw text field.
    const CHANGED = "#111111,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D";
    const hex = await waitFor(() => document.querySelector("#swatch-hex"));
    expect(hex.value.toLowerCase()).toBe("#101010");
    hex.value = "#111111";
    await fireEvent.input(hex);
    await waitFor(() => expect(renders().at(-1).draft.panel.colors_actual).toBe(CHANGED));
    expect(document.querySelector("#cfg-panel-colors_actual").value).toBe(CHANGED);

    await fireEvent.click(dom.getByRole("button", { name: "Push to panel" }));
    await waitFor(() => expect(push).toHaveBeenCalledWith("e1004", { colors_actual: CHANGED }));
    expect(session).toHaveBeenCalledWith("kitchen");

    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith("e1004", {
      colors_actual: CHANGED, dither: { strength: 0.8 } }));
    expect(end).toHaveBeenCalledWith("kitchen");
  });

  it("switching subject releases a pushed override", async () => {
    stubServer();
    vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    const release = vi.spyOn(api, "releasePanelOverride").mockResolvedValue(null);
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    const s = await waitFor(() => document.querySelector("#cfg-panel-strength"));
    s.value = "0.5";
    await fireEvent.input(s);
    await fireEvent.click(dom.getByRole("button", { name: "Push to panel" }));
    await waitFor(() => expect(configDraft.pushed).toBeTruthy());
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(release).toHaveBeenCalledWith("e1004"));
    expect(configDraft.value("panel", "e1004", "panel", "strength")).toBe(0.5);
  });
});
```

`journey-mode-device.test.js`:

```js
describe("mode c: pick a device, reassign its screen, the preview follows", () => {
  it("renders the draft assignment at once, without simulated params, and saves it", async () => {
    const calls = stubServer();
    const save = vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "kitchen", screen: "local/b" });
    await mount();
    await fireEvent.click(dom.getByRole("button", { name: "Kitchen · reTerminal E1004" }));
    const renders = () => calls.filter(([m, u]) => m === "POST" && u === "api/render").map((c) => c[2]);
    await waitFor(() => expect(renders().at(-1)).toMatchObject({ device: "kitchen", screen: "local/a" }));
    expect(renders().at(-1).simulated.params).toBeUndefined();
    expect(document.querySelector("#sim-params")).toBe(null);

    const sel = document.querySelector("#cfg-plain-screen");
    sel.value = "local/b";
    await fireEvent.change(sel);
    await waitFor(() => expect(renders().at(-1)).toMatchObject({ device: "kitchen", screen: "local/b" }));

    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith("kitchen", { screen: "local/b" }));
  });
});
```

`journey-preview-writes-nothing.test.js`:

```js
describe("spec §2 rule 2: the Preview panel never writes anything", () => {
  it("renders in every mode, with every Preview control moved, and issues no mutating request", async () => {
    const calls = stubServer();
    const writers = ["pushPanelOverride", "releasePanelOverride", "pushDeviceOverride", "releaseDeviceOverride",
      "savePanel", "saveDevice", "startTuningSession", "endTuningSession", "writeFile", "deleteFile",
      "createScreen", "copyScreen", "renameScreen", "deleteScreen"].map((n) => vi.spyOn(api, n));
    await mount();

    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    const t = document.querySelector("#preview-target");
    t.value = "panel:x"; await fireEvent.change(t);
    await fireEvent.click(dom.getByRole("button", { name: /Simulated inputs/ }));
    const b = await waitFor(() => document.querySelector("#sim-battery"));
    b.value = "3.7"; await fireEvent.input(b);

    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    const w = await waitFor(() => document.querySelector("#preview-watch"));
    w.value = "kitchen"; await fireEvent.change(w);
    const s = document.querySelector("#preview-screen");
    s.value = "local/b"; await fireEvent.change(s);

    await fireEvent.click(dom.getByRole("button", { name: "Kitchen · reTerminal E1004" }));
    await waitFor(() => expect(calls.some(([, , body]) => body?.device === "kitchen" && !body?.panel)).toBe(true));

    for (const spy of writers) expect(spy, spy.getMockName()).not.toHaveBeenCalled();
    const mutating = calls.filter(([m, u]) => m !== "GET" && u !== "api/render");
    expect(mutating).toEqual([]);
  });
});
```

- [ ] **Step 2: Run** — `pnpm vitest run test/journey-mode-*.test.js test/journey-preview-writes-nothing.test.js`. Expected: PASS. A failure here is a seam bug in Tasks 5–9: fix it in the owning file, not in the test.

- [ ] **Step 3: Rewrite the UI sections of `docs/src/guide/dev-mode.md`**

- Intro (`:3-8`): "…a web page with three toggleable panels — Library, Edit and Preview — behind an icon rail…: pick a screen to write, a panel profile to calibrate, or a device to configure; edit it; and watch the rendered result next to it." Drop the "syncs what you tune there to the physical device" claim.
- `## The five panels` → `## The three panels`: **Library** (screens by repo, then panel profiles, then devices; choosing one makes it the subject; New/Copy/Rename/Delete for screens), **Edit** (names its subject in its header; screen → the file editor; panel or device → the config editor, below), **Preview** (always what is rendered: the header says which screen as which hardware at which size; in screen mode a **Render as** menu picks a panel or a device; in panel mode a **Screen** menu, defaulting to `byonk-builtin/calibration/tone`, and **watch on** a device; **show measured inks**; **Pop out**; the **Simulated inputs** row — battery, WiFi, clock, and custom params except when the subject is a device — badge "not saved" while anything is set). "The Preview panel never writes anything."
- Replace `## Simulate panel`, `### Live device sync`, `### Dither Tuning Controls` (keep its four-row table, re-introduced as "the config editor's tuning numbers") and `### Tune: colour calibration` with `## The config editor`: one section per cascade setting, each with its input(s) for the rung(s) being edited (a panel edits its own rung; a device its base and `final:` rungs, plus screen, name, params, temperature profile, maximum compatibility and minimum PNG bytes above them) and a ladder showing every rung's value and status (winner / outranked / rejected with its reason / absent), the effective value and where it came from; "No render yet" and "Last render failed" states; "the last render, Preview is closed". Then `### Push, Save, Discard`: Push sends the changed values to the real hardware as a temporary override ("◉ Live on …", copy verbatim: "It reverts on its own within 30 minutes, or on Discard."); in panel mode it also makes the watched device poll every minute; what Push cannot carry (removals, a device's colours and refresh, the plain fields) applies on Save only; Save writes `panels.<id>` or `devices.<key>` in `config.yaml`; Discard reverts the draft and clears the override; switching subject or closing Edit clears a pushed override but keeps the draft; a live override is listed in the status strip after a reload, with its own Discard. The panel editor says how many devices use the panel and how many have not been seen since byonk started.
- `### Calibration Workflow`: 1 pick the panel in Library; 2 choose the calibration screen in the Preview header and **watch on** your device; 3 adjust colours and tuning in Edit, watching the ladder; 4 Push, look at the device under its own light, repeat; 5 Save. Replace the YAML comment "copied in by hand" with "written by Save".
- `## Example workflow` steps 4–7: pick a screen in **Library**; edit in **Edit**; use **Render as** and **Simulated inputs** in Preview; pick a panel in **Library** to calibrate it.

Build: `(cd docs && mdbook build)` → no errors.

- [ ] **Step 4: Commit**

```bash
git add webui/test/journey-mode-screen.test.js webui/test/journey-mode-panel.test.js webui/test/journey-mode-device.test.js webui/test/journey-preview-writes-nothing.test.js docs/src/guide/dev-mode.md
git commit -m "test(webui): per-mode journeys and Preview-writes-nothing; docs: the three-panel work area"
```

---

### Task 11: Whole-feature journey and orphan-API sweep

**Files:**
- Create: `webui/test/journey-whole-feature.test.js`, `webui/test/orphan-api.test.js`

**Interfaces:**
- Consumes: everything. Plan 1's route list (its Interfaces blocks) is the server side of the orphan sweep.

- [ ] **Step 1: The whole-feature journey** (harness block, then)

```js
describe("the work area end to end, through AppShell", () => {
  it("screen → panel push → device save → back to the screen, with every surface agreeing", async () => {
    const calls = stubServer();
    vi.spyOn(api, "pushPanelOverride").mockResolvedValue(null);
    vi.spyOn(api, "startTuningSession").mockResolvedValue(null);
    vi.spyOn(api, "endTuningSession").mockResolvedValue(null);
    const release = vi.spyOn(api, "releasePanelOverride").mockResolvedValue(null);
    const saveDevice = vi.spyOn(api, "saveDevice").mockResolvedValue({ key: "kitchen", screen: "local/a" });
    await mount();

    // a) a screen, with a dirty buffer
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(editor.active?.name).toBe("script.lua"));
    editor.setText("-- mine");

    // b) a panel: push, then the strip shows it live
    await fireEvent.click(dom.getByRole("button", { name: "reTerminal E1004" }));
    const s = await waitFor(() => document.querySelector("#cfg-panel-strength"));
    s.value = "0.4"; await fireEvent.input(s);
    vi.spyOn(api, "listOverrides").mockResolvedValue({
      panels: [{ key: "e1004", values: { strength: 0.4 }, expires_in_secs: 1799 }], devices: [], tuning_sessions: [] });
    await fireEvent.click(dom.getByRole("button", { name: "Push to panel" }));
    await waitFor(() => expect(dom.getAllByText(/◉ Live on/).length).toBeGreaterThan(0));
    expect(dom.getByText("It reverts on its own within 30 minutes, or on Discard.")).toBeTruthy();

    // c) a device: leaving the panel released the push; the ladder restarts
    vi.spyOn(api, "listOverrides").mockResolvedValue({ panels: [], devices: [], tuning_sessions: [] });
    await fireEvent.click(dom.getByRole("button", { name: "Kitchen · reTerminal E1004" }));
    await waitFor(() => expect(release).toHaveBeenCalledWith("e1004"));
    const fd = await waitFor(() => document.querySelector("#cfg-device_final-dither"));
    fd.value = "atkinson"; await fireEvent.change(fd);
    await waitFor(() => expect(calls.filter(([, u]) => u === "api/render").at(-1)[2].draft.final).toEqual({ dither: "atkinson" }));
    await fireEvent.click(dom.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(saveDevice).toHaveBeenCalledWith("kitchen", { final: { dither: "atkinson" } }));

    // d) back to the screen: buffer intact, Edit shows the file editor again
    await fireEvent.click(dom.getByRole("button", { name: "a" }));
    await waitFor(() => expect(subject.key).toBe("screen:local/a"));
    expect(editor.active.text).toBe("-- mine");
    expect(document.querySelector('[data-testid="config-editor"]')).toBe(null);
    await waitFor(() => expect(calls.filter(([, u]) => u === "api/render").at(-1)[2]).toMatchObject({ screen: "local/a" }));
  });
});
```

- [ ] **Step 2: The orphan-API sweep**

`webui/test/orphan-api.test.js`:

```js
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
```

On the server side, confirm by reading `dev_router` (`src/api/dev.rs`) that every route it registers under `/api` has an `api.js` export: `screens, events, render, resolve-mac, overrides, overrides/panels/{panel}, overrides/devices/{device}, panels/{panel}/save, devices/{key}, panel-usage, tuning-session/{device}, whoami` plus `files_router()`'s routes. `resolve-mac` has had no SPA consumer since before this work — leave it and record it in the commit message as a pre-existing server-only route (scripted use, documented in dev-mode.md), not an orphan this plan created.

- [ ] **Step 3: Full verification, in order**

```bash
make webui
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd docs && mdbook build)
make webui-test
```

Expected: all green; `tests/dev_static_test.rs` (relative URLs, three-file build) passes against the rebuilt bundle; `git diff 5d6323c -- tests/dev_auth_test.rs` is empty. Then the browser walk (spec §9) is the owner's, at the end — not part of this task.

- [ ] **Step 4: Commit**

```bash
git add webui/test/journey-whole-feature.test.js webui/test/orphan-api.test.js
git diff --cached --stat
git commit -m "test(webui): whole-feature journey and orphan-API sweep"
```
