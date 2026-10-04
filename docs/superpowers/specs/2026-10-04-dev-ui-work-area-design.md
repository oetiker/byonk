# Dev UI Work Area — Design

Date: 2026-10-04 (revised same day after review)
Status: design, not yet planned
Depends on: `2026-10-04-render-resolution-cascade-design.md` — specifically its
piece 2 (the provenance payload and the POST render). The ladder widget, this
design's centre, cannot be built before that lands.
Supersedes the work-area layout in `2026-09-09-dev-ui-svelte-design.md` §295
and the panel docking in `2026-09-10-dev-ui-color-tuning-design.md` §8. Those
remain the historical record of what was built; they are not rewritten.

> **Revision note.** Review found four things in this design that could not be
> built from the server as it stands, each now an owned requirement in §8 rather
> than an implementer's discovery: mode a's "render this screen as that device"
> (`dev.rs:488` is strictly either/or), mode b's whole-rung Push (`panel_colors`
> holds one string), the two-rung draft (no wire format), and the ladder
> widget's data when the Preview panel is closed. The "it reverts on its own"
> copy was a promise the server does not keep, and the layout-migration claim
> was simply wrong.

## 1. Problem

The work area has five toggleable panels and three of them are all "the
preview": `PreviewPane`, `SimulateControls` and `TunePane`. They overlap:

1. **Two panel selectors and two device selectors.** Simulate has one pair,
   Tune another, and they are deliberately not linked — `TunePane.svelte:23`
   copies the panel once, on mount. You can tune panel A while the preview
   renders panel B, with nothing to tell you.
2. **Three lifetimes, no visual difference.** Simulate's dither numbers are
   thrown away; Tune's "Push to panel" is a temporary server override; Tune's
   "Save to config" is permanent. All three look like the same kind of control.
3. **Picking a device silently changes what is rendered.** The server resolves
   the screen from the device and ignores the editor's. One paragraph says so.
4. **Simulate mixes three unrelated things**: which screen to render, how the
   hardware renders it, and fake sensor inputs.
5. **Only colours can be saved.** Dither, strength, noise scale and max error
   are real per-device and per-panel config, but the UI offers no way to keep
   them — only to fiddle.
6. **Config fields have no home at all**: a device's `screen` assignment,
   `name`, `params`, `refresh` and `temperature_profile` are unreachable from
   the dev UI, only from the admin API and the MCP tool.

## 2. The organising idea

The admin UI serves three distinct jobs, and they differ in **what the subject
is**:

| | a) design a screen | b) calibrate a panel type | c) configure one display |
|---|---|---|---|
| subject | a screen | a panel profile | a device |
| written to | files in a screen repo | `panels.<id>.*` | `devices.<key>.*` |
| blast radius | that screen | every device of that type | one device |
| needs real hardware | no | **yes, always** | optional |
| which screen renders | the subject | borrowed | the device's assignment |
| which hardware renders | **you switch it freely** | the subject | the subject |

So the UI carries one new piece of state, and the mode is not a separate
variable, because there is nothing a mode could say that the subject does not:

```js
// webui/src/lib/state/subject.svelte.js
{ kind: "screen" | "panel" | "device", id }
```

This replaces `screens.selected` as the application's single truth.

Two rules follow, and they are the whole point:

> **What we SEE** is the Preview panel. Always.
> **What we MODIFY** is the Edit panel. Always, and it names its subject in its
> own header.

> **The Preview panel never writes anything.** No config, no override, no
> server state.

Rule 2 is what makes rule 1 true rather than aspirational. Its server half is
already done — `cd01ce9b` made `GET /dev/api/render` read-only (cascade spec
§5.1/§5.2); the UI half is §9's test.

## 3. Three panels

Five toggles become three: `layout.PANELS` becomes
`["library", "editor", "preview"]`.

No storage migration is needed. `layout.load()` iterates `for (const p of
PANELS)` and copies only keys in the *current* list, so stale `tune` and
`simulate` keys are already dropped and `STORAGE_KEY` need not be bumped. (The
first draft claimed otherwise.) The one real effect is trivial: renaming
`files` → `library` resets that panel to its default-open state for existing
users, once.

### 3.1 Library (today's `FilesPane`)

Renamed: it never listed files. It listed screens, and the file tabs live in the
Edit panel. It becomes the only place a subject is chosen.

```
▾ Screens
  ▾ local                      writable
      hello
  ▸ examples                   read-only
  ▸ byonk-builtin              read-only
      calibration/tone
      calibration/gamut
      calibration/inkfield
▾ Panels
    TRMNL OG (4-grey)
    TRMNL X
▾ Devices
    kitchen-display    · TRMNL X
    hallway            · TRMNL OG
```

Screen create / copy / rename / delete stay here. Panels and devices are not
created or deleted here — a panel profile is a hand-authored config block, and a
device appears by registering itself.

### 3.2 Edit — a router on `subject.kind`

| kind | editor | writes through |
|---|---|---|
| `screen` | today's `EditorPane`: tabs, CodeMirror, validate, add/delete file | `writeFile` with etag |
| `panel` | `ConfigEditor` with the panel schema | `savePanelColors`, generalised to the whole panel rung |
| `device` | `ConfigEditor` with the device schema | existing `apply_device_patch` |

`ConfigEditor` is **one component with two schemas**, not two components. Both
jobs are "edit one rung of a config object": load the resolved ladder, show each
setting's rungs, mark dirty, Push / Save / Discard.

The device schema also carries the fields that have no rung — `screen`
assignment, `name`, `params`, `temperature_profile`, `maximum_compatibility`,
`min_png_bytes` — as plain form fields above the cascade section. `refresh` is
*in* the cascade (cascade spec §2), so it appears in the ladder section with a
`final` rung of its own.

### 3.3 Preview

```
┌─ PREVIEW ─────────────────────────────────────────────┐
│  examples/hello   as kitchen-display   800×480  ⬈ ⌕ ◐ │
├───────────────────────────────────────────────────────┤
│                                                       │
│                  [  the render  ]                     │
│                                                       │
╞════════════════════ drag ═════════════════════════════╡
│  ▸ Simulated inputs                        not saved  │
└───────────────────────────────────────────────────────┘
```

The header states the two facts and never lets them disagree. The toolbar
carries the **view options** — pop out, the 1:1 lens, "show measured inks",
device frame.

`use_actual` is a view option, not a setting: it changes how the result is
*depicted*, by repainting the indexed image in measured inks. It does not change
what the device does, so it belongs next to the lens.

Simulated inputs collapse to one line by default, and the collapsed row keeps
its badge, so nothing pending can hide.

## 4. What each mode does

|  | renders | hardware | Preview also offers |
|---|---|---|---|
| **a** screen | the subject | **you pick** — any panel or device | simulated inputs, incl. `params` |
| **b** panel | **you pick**, defaulting to `byonk-builtin/calibration/tone` | the subject panel | **"watch on [device ▾]"**, simulated inputs incl. `params` |
| **c** device | the device's assigned screen, read from the **draft** so changing the assignment re-renders at once | the subject device | simulated inputs, **no `params`** |

**Mode a needs a server change, and it is not optional.** `dev.rs:488-520` is
strictly either/or: when `mac` is present, `screen` *and* `panel` are discarded
and everything comes from the device entry. So "render *this* screen as *that*
device" — the mode a screen author's whole workflow, and the thing that makes
mode a trustworthy — is unbuildable today. Cascade spec §4.6 owns the fix; §8.1
below restates it. Without it, an implementer will quietly reduce mode a to
"pick a panel only", which deletes the design's main promise.

**Mode b needs a device, and it is not the old selector returning.** A panel
*type* is not a thing you can look at; to calibrate you must watch one physical
display of that type, which is what `startTuningSession` exists for — it makes
that device poll at `TUNING_REFRESH_SECS`. The control means "which display do
I watch", never "what am I simulating". Different question, different label.

**Mode c has no simulated `params`.** The device owns real `params` in config,
so the Edit panel owns that field and a draft change re-renders the preview at
once. That gives try-before-save without a second, conflicting params box — and
it depends entirely on §8.2's wire format.

**One behaviour moves.** `SimulateControls`' ad-hoc `max_error`,
`noise_scale`, `chroma_clamp` and `strength` leave mode a. Mode a renders
through each target's real configured settings, which is what makes it
trustworthy: "this is what it will actually look like on that device."
Adjusting those numbers is calibration, so it moves to modes b and c where it
has a Save button and a rung. Nothing is deleted; "tweak strength without
committing to a panel" is lost.

**Mode a's target switcher is weaker than it looks, and must say so.** A screen
can win `colors` and `colors_actual` outright, so switching from TRMNL OG to
TRMNL X may change only the size. The header states when the screen is in
charge.

## 5. The ladder widget

This is the design's centre. For each cascade setting, the Edit panel shows
every rung with its value **and its status**, built from the
`X-Byonk-Resolution` payload (cascade spec §4.5).

```
┌EDIT──── panel TRMNL X ──────────────────────────────┐
│ ⚠ 3 devices use this panel                          │
│                                                     │
│ colours_actual                                      │
│   ● panel          #1E5645,#00994D,…   ← editing    │
│     device base    —                                │
│     screen tone    —                                │
│     device final   —                                │
│     override       —                                │
│   ▶ effective      #1E5645,#00994D,…   from panel   │
│                                                     │
│ dither                                              │
│     panel          floyd-steinberg     outranked    │
│     device base    —                                │
│   ⚠ screen tone    atkinson            ← wins       │
│     device final   —                                │
│   ▶ effective      atkinson            from screen  │
│     your panel value is not used in this preview    │
│                                                     │
│ ● 2 unsaved      [Push to panel] [Save] [Discard]   │
└─────────────────────────────────────────────────────┘
```

Three questions it answers at a glance:

- **what we see** — the `effective` row
- **what we are modifying** — the `●` row
- **why nothing is happening** — a rung above yours carries a value, *or* your
  own value was rejected

**That last clause is why `RungStatus` exists** (cascade spec §4.4). Mode b's
worst failure is not only "the screen outranks you". It is also "your six-entry
calibration was discarded because this render has a sixteen-entry palette" —
`resolve_measured_colors` supplies-and-discards on a length mismatch
(`display.rs:284-316`). Without a `Rejected` status the widget would show the
user's value sitting on their own rung, nothing above it, and no explanation,
in the one mode where every attempt costs a minute of panel repaint. The first
draft's payload could not express this.

**The override rung must be shown, or the widget lies while a Push is live.**
Push puts a draft on real hardware (§6). With only four rungs the `effective`
row would report the config winner while the panel shows something else —
wrong in precisely the two modes where Push exists. Cascade spec §2 adds the
rung for this reason.

**Where the data comes from, and what happens when it is absent.** The payload
rides on the Preview panel's render request, but the ladder lives in the Edit
panel, and Preview is a panel the user can close. So:

- the provenance payload lands in a **shared store**, not in `PreviewPane`
  local state, and Edit reads it from there;
- the ladder has an explicit **"no render yet"** state (first entry, before any
  render completes) and a **"last render failed"** state — `PreviewPane`'s
  `renderNow()` throws on `!res.ok` (`PreviewPane.svelte:195-200`), so no
  header is read and the ladder must say so rather than show stale rungs;
- with Preview closed, the ladder shows the last payload it received, labelled
  as such.

**The "N devices use this panel" count must come from the server.** Device-to-
panel resolution is partly by `match_pattern` against the firmware `Board`
header (`resolve_panel`), so a device with no explicit `panel:` cannot be
counted client-side. §8.4 asks for it.

## 6. Push and Save

| | Push | Save |
|---|---|---|
| mode b (panel) | panel-keyed override of the whole draft rung. Any matching device picks it up at its own refresh pace; the watched device polls fast. | `panels.<id>.*` in `config.yaml` |
| mode c (device) | device-keyed override of the whole draft rung | `apply_device_patch` |

**The server state for mode b does not exist yet.** The first draft said "the
server state exists; the endpoints do not" — true for mode c, false for mode b.
`DevOverrides.panel_colors` (`server.rs:40-52`) holds one `colors_actual`
*string* per panel key; there is no panel-keyed map for `dither` or the tuning
numbers, and `display.rs:841-845` reads it only into the measured-colour chain.
Shipping §8 as first drafted would give mode b a Save button and a silently
dead Push button, in the mode that needs real hardware *always*. Cascade spec
§4.7 adds the map; §8.3 restates it.

A live override is stated plainly, because it is reaching real hardware:

```
◉ Live on TRMNL X — the real panel is showing this draft.
  It reverts on its own, or on Discard.
```

**"It reverts on its own" is only true once the overrides have a TTL.** None of
the three override maps expires today; only `tuning_sessions` does
(`server.rs:66`), and that governs the refresh *rate*. Release is
`onDestroy`-only (`TunePane.svelte:83-85`), which does not run on tab close,
crash or navigation, and `tune.svelte.js:144-146` already concedes "a failed
clear can leave…". So a closed tab can leave a physical panel rendering an
uncommitted draft indefinitely — the very bug rule 2 exists to prevent, wearing
a reassuring label. Cascade spec §4.7 adds the TTL; until it lands the copy
must read "it stays until you Discard, or byonk restarts", and a live override
must be visible again after a reload.

Closing or switching subject releases the override, as `TunePane`'s `onDestroy`
does today. The draft survives in the store, as it does today.

## 7. State modules

| module | change |
|---|---|
| `subject.svelte.js` | **new** — the single truth |
| `resolution.svelte.js` | **new** — the shared provenance store the ladder reads (§5) |
| `screens.svelte.js` | keeps repos/screens/meta; loses `selected` to `subject` |
| `editor.svelte.js` | unchanged behaviour. **`tabs` must survive a subject switch** |
| `configDraft.svelte.js` | **new** — one schema-driven draft store, serving both the panel and device editors |
| `tune.svelte.js` | **dissolved** into `configDraft` + the push calls |
| `simulate.svelte.js` | shrinks to simulated inputs + view options; loses device, panel and the four tuning knobs |
| `layout.svelte.js` | three panels (no storage migration — §3) |

### The trap

**Switching subject must not discard open editor buffers.** R2/R3 make
`editor.tabs` the loaded buffers, and `loadScreen()` calls `reset()`
unconditionally (`editor.svelte.js:171-172`), which discards every unsaved one.
`AppShell`'s current `$effect` (`AppShell.svelte:68-73`) calls
`editor.loadScreen(screens.selected)` on every change; under `subject` it must
fire only when the subject is a screen **and a different one than is loaded**.

R7 also still binds: `screens.error = null` lives below `loadScreen()`'s `!ref`
early return. A subject switch away from screens must not clear a user-visible
error.

## 8. Server additions this design needs

Beyond the cascade spec's own work, these are this design's hard requirements.
Each was an unowned assumption in the first draft.

1. **`POST /dev/api/render` accepting `screen` and `device` together**, taking
   the screen from `screen` and every render setting from `device`
   (`dev.rs:488-520` is either/or today). Without it, mode a does not exist.
   Cascade spec §4.6.
2. **The draft wire format.** `RenderQuery` (`dev.rs:30-68`) is a flat GET query
   with one slot per setting, no `gamut`, and no way to name a rung. Modes b and
   c preview an uncommitted two-rung draft; there is no transport for that.
   Cascade spec §4.6 makes the dev render a POST whose `draft` feeds the
   `override` rung.
3. **A panel-keyed `CascadeValues` override map**, plus its PUT/DELETE routes
   and its reading as the `override` rung for every panel-rung field — not just
   measured colours (§6). Plus the TTL, so §6's copy is true. Cascade spec §4.7.
4. **Device-keyed override routes** for mode c, replacing the side effect
   `cd01ce9b` deleted.
5. **`handle_save_panel_colors` generalised** to the whole panel rung.
6. **Thicker list payloads.** `PanelInfo` (`dev.rs:117`) gains the `dither`
   block; `DeviceInfo` (`dev.rs:108`) gains both cascade rungs plus `name`,
   `params`, `temperature_profile`. Both carry only what the old Simulate
   drawer's selects needed.
7. **A count (or list) of devices resolving to a given panel**, for §5's
   warning, since `match_pattern` resolution is not visible client-side.

## 9. Testing

jsdom has no layout engine, so no vitest assertion can catch a layout defect —
every width there is 0. That is unchanged, and it is why the browser walk
exists.

- **Journey tests per mode**, mounting the real app root and driving real
  controls: pick a screen → edit → see it render; pick a panel → change a
  swatch → Push → Save; pick a device → reassign its screen → the preview
  follows.
- **The buffer-survival test** (§7): open two files, dirty one, select a panel,
  select the screen again, assert both buffers and the dirty flag survive. The
  regression nobody will notice by hand.
- **The ladder widget against fixtures**: one per rung winning, the override
  rung winning, a `Rejected` rung, "no render yet", and "last render failed".
- **The Preview-writes-nothing test**: render in each mode and assert no
  mutating request is issued. The server half is already covered by
  `tests/dev_render_is_read_only_test.rs`; this is the client half.
- **`api.test.js`'s `urlCoverage` table** must gain every new endpoint — that
  completeness assertion is what caught `validateScreen` being added without an
  entry. Keep it fed.
- **Browser walk, with a long-lined screen.** `examples/hello` shows no
  overflow; `examples/swiss-departure-board` does. Finding M was a whole button
  made invisible by a clipping ancestor, and `min-w-0` on the editor and its
  toolbar row is load-bearing, not tidiness.
- Still untested in a browser, carried forward: a read-only screen's editor,
  the 5 MB limit, and deleting a file with unsaved edits elsewhere.

## 10. Rulings carried forward

Still binding, re-stated because this design moves the code they constrain:

- **R2** — `editor.tabs` is loaded buffers, `editor.files` is the listing. Two
  collections, deliberately.
- **R3** — files load lazily on click; only `script.lua` auto-opens.
- **R4** — binary entries (`text:false`) render as disabled tabs, not hidden.
- **R7** — see §7.
- **Ruling 57** — `PreviewPane` posts `{url, width, height}` to the pop-out, and
  `postFrame()` reads `simulate.size` live, so the `renderSeq` guard is what
  keeps the pop-out's size matched to its image.
- **Ruling 63** — Logout is gated on `whoami.can_logout` and renders **no DOM
  node** under Ingress.
- **Ruling 66** — the independent error surfaces stay independent. Merging three
  panels into one must not merge their error state; a render failure and a draft
  save failure are different failures with different remedies.
- **Colour tuning's two stages exist because e-ink is reflective**: tuning must
  reach real hardware (push) before it becomes configuration (save). Push is a
  button, never a debounce.
- Use `$lib/ui` components, not raw `<button>`. Dependency pins do not move;
  `@lucide/svelte` 1.42.0 and `bits-ui` 2.19.0 were stepped down deliberately.
  pnpm only.
- Every URL the browser sees must be relative (`tests/dev_static_test.rs`
  guards this against the built bundle). No nested page route under `/dev`. The
  build emits exactly `index.html`, `app.js`, `app.css`.
- `tests/dev_auth_test.rs` stays byte-identical to `5d6323c`.
- No CHANGES.md entry for dev-UI work: the whole feature is unreleased. The
  cascade's `dither` flip **is** a CHANGES.md entry, in that spec.

## 11. Open questions

1. **Mode b's default screen** — `calibration/tone`, or whatever was last open?
   This design defaults to `calibration/tone` on first entry and remembers a
   later choice for the session.
2. **The Library panel's name.** "Library" here; "Browse" and "Explorer" were
   the alternatives.
3. **`prompt()`/`confirm()`** — `FilesPane` and `EditorPane` both use them, and
   they freeze the Chrome extension during a browser walk. Whether they become
   inline forms is still unruled; if ruled, all of them move together.
4. **Finding H, still open** — after deleting the selected screen, CodeMirror
   keeps painting the deleted file's text and the preview its last render. The
   subject rework touches exactly this code path and may absorb it.
