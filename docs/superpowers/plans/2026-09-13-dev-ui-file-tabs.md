# Plan — restore the screen→files→editor wiring in the Svelte dev UI

Spec: `docs/superpowers/specs/2026-09-09-dev-ui-svelte-design.md`
(§1 panel table: **Files** holds "the screen list, grouped by repo"; **Editor**
holds "file tabs, CodeMirror, status bar").

## Why this plan exists

The Svelte rewrite (`docs/superpowers/plans/2026-09-09-dev-ui-svelte.md`)
decomposed one task per component and never assigned the seam between them.
Task 6 built `FilesPane`, Task 7 built `EditorPane`, and nothing was ever
specified to connect "a screen was selected" to "list its files and open one".
The plan's own `FilesPane` code is `onclick={() => screens.select(s.ref)}` and
stops there.

Result, found by Task 14's browser walk on 2026-09-12: selecting a screen
renders its preview but produces no file tabs and no editable buffer, so the
dev UI cannot edit anything. `api.listFiles` has zero callers in `src/`;
`editor.open` has zero callers in `src/`.

A sibling defect in the same class — nothing called `screens.refresh()` on
boot, so every list in the app was empty — is already fixed in `32106892`.

## Authority: the deleted implementation

The behaviour being restored shipped in 0.16.0 and is in git. Read it:

```
git show ae3393152d208fc64ac20814abe5b57877b61eb7^:static/dev/dev.js
```

The relevant parts are `doSelectScreen` (~:413), `renderTabs` (:509) and
`openFile` (:451). That file is the authority on *behaviour*. It is NOT a
style model — do not port its DOM-string rendering, its global `state`
object, or its `prompt()`/`confirm()` dialogs.

## Global Constraints

- **pnpm only.** Never `npm`/`yarn`. Do not move any dependency pin;
  `webui/package.json` is the authority, not any plan's version table.
- **Svelte 5 runes.** A plain `let` creates no dependency — reactive fields
  are `$state()`. Derived values are `$derived`.
- **`cleanup()` in `beforeEach` is mandatory** in every test file; this
  project configures no auto-cleanup.
- **jsdom under vitest has no `EventSource` and no `matchMedia`**, and does
  have a live `BroadcastChannel`.
- **Every URL the browser sees must be relative.** No leading `/`.
  `tests/dev_static_test.rs` guards this against the built bundle.
- **Never `git add -A` or `git add .`** in this repo. Stage by explicit path.
  Four files are permanently modified in the maintainer's tree and must never
  be staged: `config.yaml`, `docs/generate-samples.sh`,
  `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`.
- **Verification is five direct commands, never `make check` alone** (it has
  reported exit 0 while tests failed). From the repo root:
  `make webui`, then `cargo fmt --check`, then
  `cargo clippy --workspace --all-targets -- -D warnings`, then
  `cargo test --workspace`, then `(cd docs && mdbook build)`.
  For UI-only changes, `make webui-test` is the fast inner loop, but the five
  still gate the commit.
- **Do not amend a spec to match the implementation.**

## Controller rulings made before dispatch

- **R1 — the wiring lives in `AppShell`, not in `EditorPane` or `FilesPane`.**
  `EditorPane` unmounts whenever the Editor panel is toggled off (spec §285),
  so wiring placed there would silently stop loading files for anyone working
  with that panel closed. `FilesPane` unmounts the same way. `AppShell` is the
  application root and is never unmounted. Cost if wrong: the effect lives one
  component higher than strictly needed.
- **R2 — `editor.tabs` keeps its current meaning (loaded buffers), and the
  screen's file listing becomes a new field.** Re-pointing `tabs` at the
  listing would invalidate the store's existing tests and `close()`/`save()`/
  conflict logic at the end of a branch. Cost if wrong: two collections where
  a redesign might have one.
- **R3 — files are fetched lazily, one click at a time, not eagerly on
  selection.** This is what the old UI did, and eager-loading every file of a
  screen would fetch assets nobody opened. The single exception is the
  auto-open below. Cost if wrong: one extra click to see a file's content.
- **R4 — binary entries (`text: false`) render as disabled tabs showing name
  and size**, exactly as the old UI did (`dev.js:520-523`). Hiding them would
  make a file that exists on disk invisible in the only UI that lists it.
  Cost if wrong: a tab nobody can click.
- **R5 — the "+" new-file tab and the `prompt()`/`confirm()` replacement are
  OUT OF SCOPE.** The maintainer has not settled that scope, and
  `api.deleteFile` stays orphaned for now — recorded, not fixed here. Cost if
  wrong: a follow-up task.

---

### Task 1: Selecting a screen lists its files and opens one

**Files:**
- `webui/src/lib/state/editor.svelte.js`
- `webui/src/lib/components/AppShell.svelte`
- `webui/src/lib/components/EditorPane.svelte`
- `webui/test/editor.test.js`
- `webui/test/editorpane.test.js`
- `webui/test/appshell.test.js`

**Step 1 — the store holds the screen's file listing.**

Add to the `Editor` class in `editor.svelte.js`:

- `files = $state([])` — the entries `api.listFiles(ref)` returned for the
  currently selected screen. Each entry is `{ name, size, text }` exactly as
  the server sends it. Document in a comment that this is the screen's file
  LISTING, distinct from `tabs`, which holds buffers that have been loaded.
- `async loadScreen(ref, { readOnly = false } = {})`:
  - `reset()` first — switching screens must not carry another screen's
    buffers, conflict banner or error across. (`reset()` also clears
    `watchConnected`; re-read it from the live SSE state is NOT required —
    see Step 2's note.)
  - `if (!ref) { this.files = []; return; }`
  - `this.files = await api.listFiles(ref)`
  - auto-open `script.lua` when the listing contains it with `text: true`,
    via the existing `open(ref, "script.lua", { readOnly })`. This mirrors
    `dev.js:436-438`.
  - Errors: catch, set `this.error = { message }`, and leave `files` empty.
    `StatusStrip` already renders `editor.error`. Do not let a rejection
    escape — a failed listing must not take the page down.

**Do not re-sort `files`.** The server already ranks `meta.yaml`,
`script.lua`, `screen.svg` first and the rest alphabetically
(`src/services/screen_store.rs:1074`). The old UI sorted again client-side;
that duplication is not worth porting. Add a comment saying the order is the
server's, and naming that file, so the next reader does not "fix" it.

**Step 2 — `AppShell` drives it from the selection.**

Add an `$effect` that reads `screens.selected` and calls
`editor.loadScreen(ref, { readOnly: !screens.writable(ref) })`.

Two properties the effect must have, both testable:

- It must not re-run when unrelated `screens` fields change. Read
  `screens.selected` (and compute `readOnly` inside `untrack`, or read
  `writable` outside the reactive read) so a `refresh()` that replaces the
  repo list does not reload the open screen and discard unsaved buffers.
- `watchConnected` is owned by the SSE `onMount` block, and `reset()` clears
  it. Re-assert it after `loadScreen`, or scope `reset()` so it does not touch
  `watchConnected`. Choose one, and say in a comment which and why. The
  observable requirement: after switching screens, the status line still reads
  `watch: connected`, not `watch: disconnected`.

**Step 3 — `EditorPane` renders one tab per listed file.**

The `role="tablist"` block at `EditorPane.svelte:109` currently iterates
`editor.tabs`. Iterate `editor.files` instead:

- Binary entry (`text: false`): render a non-interactive element showing
  `name (size)`, visibly disabled, not clickable, not focusable.
- Text entry: a `role="tab"` button. `aria-selected` is true when the active
  loaded tab has this name. Show a dirty marker when a loaded tab with this
  name is dirty. Clicking calls `editor.open(ref, name, { readOnly })` — which
  already no-ops into activation when the file is loaded already.
- The close control keeps working and must close the loaded buffer without
  removing the file from `files` — closing a buffer does not delete a file.
  A closed file's tab stays listed and can be reopened.

Keep the existing keyboard behaviour and ARIA roles; do not regress
`editorpane.test.js`.

**Step 4 — tests.**

In `editor.test.js`:
- `loadScreen` populates `files` from `api.listFiles`.
- `loadScreen` auto-opens `script.lua` when present and text.
- `loadScreen` does NOT auto-open when `script.lua` is absent, and does not
  auto-open a `script.lua` whose entry is `text: false`.
- `loadScreen(null)` empties `files`.
- switching screens clears the previous screen's buffers.
- a rejecting `listFiles` sets `editor.error` and leaves `files` empty,
  and does not reject.

In `editorpane.test.js`:
- a binary entry renders disabled and does not open on click.
- a text entry opens on click.
- the dirty marker follows the loaded buffer, not the listing.

In `appshell.test.js` — **the journey test, and the point of this plan:**
- Mount `AppShell` with `api.listRepos`, `api.listScreens`, `api.listFiles`
  and `api.readFile` mocked, select a screen through `screens.select(...)`,
  and assert that file tabs appear and `script.lua` becomes the active
  buffer. **The test must not call `screens.refresh()`, `editor.loadScreen()`
  or `editor.open()` itself.** Every existing component test hand-feeds the
  store in `beforeEach`, which is exactly why the whole suite passed over a
  UI that loaded nothing — this test exists to make that impossible to repeat.

**Step 5 — verify.** Run `make webui-test` while iterating, then the five
commands from the repo root before committing. Report anything you could not
test.
