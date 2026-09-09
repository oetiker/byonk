# The dev UI as a Svelte 5 application

Date: 2026-09-09
Status: approved design, not yet planned

## Goal

Rebuild the `/dev/` editor as a Svelte 5 single-page application with
shadcn-svelte components, replacing roughly 3,300 lines of hand-written
DOM code, CSS and markup.

This is a **design project, not a port**. The point is not to re-implement
the current widgets on a new foundation but to arrive at an interface that
looks and behaves like a professional tool. Layout, information architecture
and visual language are all redesigned. The capabilities stay the same.

Three outcomes:

1. The editor is built from components, so the next feature is an addition
   rather than a surgery on `dev.js`.
2. It looks like a developer tool that someone designed, in a consistent
   visual language that survives new screens being added to it.
3. The behaviour the current implementation earned through review — the
   security properties, the Ingress guarantee, the conflict handling — is
   preserved and re-proved, not re-derived.

## Supersedes

`2026-09-07-dev-mode-editor-design.md` lists as a non-goal: *"No framework.
The page stays plain HTML, CSS and JS."* This spec reverses that decision
deliberately. Every **other** decision in that spec stands, and its security
and routing properties are binding here — see §8.

That earlier decision was right for shipping the feature. It is wrong for
growing it: the three drivers for this work are room to grow, visual polish,
and maintainability, and hand-written DOM code serves none of them.

## Non-goals

- **No new features.** No device management, no config editing, no screen
  assignment. Same capabilities as the shipped version.
- **No server changes.** Dev auth, routes, the file API, the render endpoint
  and Ingress handling stay exactly as they are. If this work needs a server
  change, that is a signal the design drifted.
- **No new write path.** Every edit still goes through `ScreenStore`.
- **No panel-level write path.** Still a separate piece of work.
- **No multi-user model.** Unchanged.
- **No SSR, no router framework, no SvelteKit.** One page, one bundle,
  embedded in the binary. SvelteKit's server half is meaningless here and its
  routing assumptions fight the Ingress constraint.

## Sequencing

This work continues on **`feat/dev-editor` itself**, and the branch opens a
single PR once the rewrite is done. The owner decided this on 2026-09-09,
reversing the earlier plan of a second branch after a first merge.

Consequences the plan must honour:

- **One final whole-branch review, at the end**, covering the vanilla-JS work
  and the rewrite together. Reviewing the vanilla UI first would spend effort
  on files this work deletes.
- **The vanilla-JS UI is still the reference for §8**, but as a *previous
  commit* rather than as `main`. It runs on this branch today, so every
  behaviour in the checklist can be exercised before it is replaced — do that
  while it is still there, not from the diff afterwards.
- **The intermediate commits stay honest.** The branch passes through a state
  where the old UI is gone and the new one is incomplete; that is expected,
  but each commit must still build, because the dev UI is a compile-time
  dependency (§4).

## 1 · Layout

Settled by mockup review on 2026-09-08.

```
┌──────────────────────────────────────────────────────────────┐
│ BYONK   local/departures                     [Simulate] [Save]│  top bar
├──┬──────────────┬───────────────────────┬─┬─────────────────┬─┤
│  │ (flyout)     │ meta.yaml script.lua  │ │ 1200×1600       │ │
│ic│ LOCAL        ├───────────────────────┤ ├─────────────────┤ │
│on│  departures  │  1  -- departures     │▓│                 │ │
│  │  weather     │  2  local res = ...   │▓│    ┌────────┐   │ │
│ra│ EXAMPLES     │  3  return { ... }    │▓│    │ device │   │ │
│il│  hello       │                       │▓│    └────────┘   │ │
│  │  mandelbrot  │                       │ │                 │ │
│  │              ├───────────────────────┤ ├═════════════════┤ │
│  │              │ ✓ valid  lua 3:22     │ │ Panel  [______] │ │
│  │              │      watch: connected │ │ Dither [______] │ │
└──┴──────────────┴───────────────────────┴─┴─────────────────┴─┘
      ▲                                     ▲         ▲
   flyout, over the editor        vertical splitter   horizontal splitter
```

- **Icon rail**, always visible. Holds navigation only. The screen list is a
  **flyout** over the editor, not a permanent column.
- **Editor** in the middle: file tabs, CodeMirror, a status bar carrying
  validity, cursor position and file-watch state.
- **Vertical splitter** between editor and preview, draggable, position
  persisted.
- **Right pane**, one column: the device on top, its simulate controls
  underneath, always visible — never overlapping the render they change.
- **Horizontal splitter** inside the right pane, between device and controls.
  This is what makes upright panels work: an E1004 is 1200 × 1600, and a
  fixed control block under it would squeeze the device to a stamp. Drag the
  controls shut and the device takes the whole column; drag them open when
  tuning. The controls scroll inside whatever height they are given.

One splitter component serves both axes. That is deliberate: two gestures,
one concept.

### Rejected alternatives

- **Permanent screen-list column** — costs width the code and preview need.
- **Preview-first with a code dock** — says byonk is for designing screens,
  not writing Lua. It is for both, and the Lua is where the time goes.
- **Simulate as a flyout** — you cannot watch a render change while the thing
  changing it sits on top of the render.
- **Bottom tabbed panel** — steals height, the scarce dimension on a laptop,
  and needs the most machinery. Reconsider if a Lua log and a data inspector
  are ever built.
- **Controls that reflow beside the device when the panel is portrait** —
  clever, but a layout that rearranges itself when you change a dropdown is
  the kind of clever that grates on the tenth use.

## 2 · Visual language

**Dense tool.** Tighter rows and smaller type than a marketing-shaped UI, a
real status bar, monospace wherever a value is technical (refs, dimensions,
timings, cursor position). More code and more of the render on screen at once.

The known risk is that dense and cramped are neighbours. The guard is a
defined type and spacing scale in the Tailwind theme, applied everywhere, so
density is a decision made once rather than per component.

The existing **three-way System / Light / Dark** switch carries over
unchanged, including the rule that dark keeps byonk's navy identity. Both
themes are defined as CSS variables on `:root`; no component hard-codes a
colour.

## 3 · Toolchain

| Piece | Version | Why |
|---|---|---|
| Svelte | 5.57 | Runes; fine-grained reactivity without a virtual DOM |
| Vite | 8.2 | Build and dev server |
| Tailwind | 4.3 | Required by shadcn-svelte; carries the density scale |
| Vitest | 5.0 | Component tests |
| CodeMirror | 6 | Kept — it works, and replacing it is unrelated risk |

**shadcn-svelte is not a runtime dependency.** Its CLI copies component source
into the repo, so those components become byonk's own code: reviewable,
editable, no version drift, no upstream breaking change to chase. The real
dependencies it brings are `bits-ui`, `tailwind-variants`, `clsx` and
`tailwind-merge`.

Everything lives in a new `webui/` directory at the repo root. Package
manager is **pnpm**, matching `static/dev/editor/`.

## 4 · Build and embedding

The release image is `FROM scratch`. There is no filesystem to serve from, so
the built assets are compiled into the binary with `include_str!`. That is a
constraint, not a choice.

**Build output is not committed.** `webui/dist/` is gitignored and produced by
the build.

- `make webui` runs Vite into `webui/dist/`.
- Vite emits **fixed filenames** — no content hashes — so the `include_str!`
  paths are stable.
- `make build`, `make release` and `make check` depend on `make webui`.
- `build.rs` fails with a plain message — *"webui/dist missing, run
  `make webui`"* — for anyone who bypasses the Makefile. Rust builds stay pure
  Rust: no hidden pnpm invocation, no Node requirement for a build that
  already has the assets.
- The Dockerfile gains a **Node builder stage** that runs `make webui`; the
  Rust stage copies `webui/dist/` in before `cargo build`.
- CI builds the UI before `cargo test`.

`make editor` and `make editor-check` **retire**. CodeMirror folds into the
Vite build, and with nothing committed there is no stale-bundle failure mode
left to guard against.

### What happens to each existing file

| File | Fate |
|---|---|
| `static/dev/index.html` | deleted — Vite emits the app's shell |
| `static/dev/dev.js` (1677 lines) | deleted — replaced by components |
| `static/dev/dev.css` (1195 lines) | deleted — replaced by the Tailwind theme |
| `static/dev/editor/` + `editor.bundle.js` | deleted — CodeMirror moves into `webui/` |
| `static/dev/login.html` | **stays**, hand-written, `include_str!` unchanged |
| `static/dev/preview.html` + `lens.js` | **stay**, hand-written — the pop-out keeps no framework |

The two survivors are restyled to match the dense visual language, but stay
plain HTML, CSS and JS and stay served the way they are today. They are small
enough that hand-writing them is cheaper than the coupling that would come
from building them with the app.

### The prerequisite fix — done

`Dockerfile` copied only `src` before `cargo build --release`, while
`src/api/dev.rs` and `src/api/dev_auth.rs` pull seven files in through
`include_str!("../../static/dev/…")`. Fixed in `f4feba8`.

Two corrections to what this section originally claimed, both worth carrying:
the break was **not** one line — `crates/` was missing too, so the in-repo
`Dockerfile` had been broken since `eink-dither` was extracted, not by this
branch. Releases were indeed unaffected: `release-publisher.yml` uses
`Dockerfile.release`, which copies a binary CI built on a host that has
`static/`. Whatever this work adds to the build context inherits the same
exposure — a missing directory is a compile error, but an *empty* one is not
(§4).

## 5 · Serving

| Route | Serves | Form |
|---|---|---|
| `/dev/` | `index.html` | embedded, built |
| `/dev/app.js`, `/dev/app.css` | the SPA | embedded, built |
| `/dev/preview.html` | pop-out | its own minimal document |
| `/dev/login` | login form | server-rendered, no framework |

**Login, pop-out and the app stay three separate documents.**

- The login page keeps no framework at all. The whole bundle loading before
  anyone can authenticate is a cost with no benefit, and the 404-when-
  unconfigured behaviour is easier to keep true when the page is a constant.
- The pop-out stays a lightweight window that cannot break when the app does,
  and does not carry the app bundle for the sake of one `<img>`.

**Every URL must be relative.** Under Ingress the Supervisor strips the path
prefix before byonk sees the request, so the server cannot know it. Any
origin-absolute URL ejects the user out of the HA iframe. This applies to
`fetch` targets, the EventSource URL, `window.open`, asset references and
redirects alike. Vite must be configured with a **relative base** (`base: ""`)
so emitted asset paths do not become root-absolute.

## 6 · Component structure

Reusable:

- `SplitPane` — one component, both axes, persists its position per axis.

Shell:

- `AppShell` · `IconRail` · `ScreenList` (flyout) · `StatusBar` · `ThemeSwitch`

Work areas:

- `EditorPane` — file tabs, CodeMirror wrapper, lint markers, dirty state
- `PreviewPane` — device frame, zoom, aspect-ratio handling, pop-out button
- `SimulateControls` — device, panel profile, palette swatches, dither and
  its parameters, battery, wifi, time, custom params
- `ConflictBanner` — the on-disk-changed banner with Reload and Overwrite

State lives in four rune modules — `screens`, `editor`, `simulate`, `layout` —
so no component owns state another component needs. Each answers: what does it
hold, who writes it, who reads it.

## 7 · Testing

**Vitest component tests** for the logic that motivated the rewrite:

- dirty tracking, and the fact that the dirty marker and the error marker are
  distinct signals that happen to look alike
- conflict resolution: both the 412 save path and the SSE `file-change` path
- tab lifecycle: open, switch, close, close-the-active-one
- splitter position persistence, both axes
- the aspect-ratio rules that size the device in its box

**Rust integration tests are unchanged.** They keep covering auth, the
404/401 split, routing depth and the file API. In particular
`tests/dev_auth_test.rs` must stay **byte-identical** — it is the
accountability hook for the `/dev` vs `/dev/` routing defect.

`make check` runs both. No browser automation in CI: Playwright was
considered and rejected as too slow and too flaky for the value, given the
Rust tests already cover the integration seams.

## 8 · Behaviour that must not regress

This list cost three review rounds and a browser walkthrough to establish. It
is a checklist for the implementation plan, not prose.

1. **Unconfigured**: `/dev/`, `/dev` and `/dev/login` return **404**; the JSON
   API under `/dev/api/` returns **401**. The bare-`/dev` redirect is gated on
   the same visibility flag, so an unauthenticated probe gets no 307 either.
2. **No nested page route under `/dev`.** `require_dev` redirects to the
   relative string `"login"`, which hard-codes depth 1.
   `tests/dev_static_test.rs:33-34` pins this.
3. **Every URL relative.** Verified at the wire, not by reading code: the raw
   `Location` headers are `login` and `./dev/`.
4. **Pop-out**: the blob URL resolves in the pop-out window, and the
   `{hello:true}` handshake recovers a reloaded pop-out.
5. **Conflict banner on both triggers** — the 412 save and the SSE
   `file-change` — with Reload replacing the buffer and clearing dirty state.
6. **Read-only repos**: editing is impossible (typing inserts nothing, not
   merely a rejected save), Rename and Delete are disabled, Copy and New stay
   enabled.
7. **Binary assets** survive a copy byte-identically and are never opened as
   text.
8. **Empty writable repos are listed** — fixed in `ccc4f47`, and the SPA must
   not reintroduce the assumption by filtering repos client-side.
9. **Theme**: three-way System / Light / Dark, defaulting to System, with the
   pop-out following the main window's choice in both directions.

## 9 · Risks

- **The pop-out under Ingress is unproven.** It opens as a top-level document
  at the HA ingress URL, outside HA's iframe, and whether HA's
  `ingress_session` cookie authorises that is not knowable from the code and
  not reachable without a Supervisor. This is inherited from the current
  branch and settled on the HA VM, not here. If it fails, the pop-out button
  is dead for sidebar users while working perfectly in every local test.
- **Bundle size grows.** Three hand-written files become a framework, and it
  is embedded in the binary. Worth measuring against the current total and
  stating a budget in the plan.
- **Dense plus Tailwind needs discipline.** Utility classes make per-component
  drift easy and a shared scale optional. The theme must carry the scale.
- **Retiring `editor-check` removes a guard.** It was catching a real class of
  error. The replacement is that nothing is committed, so nothing can be
  stale — but that only holds if every build path genuinely runs `make webui`.
  The plan must verify all three: Makefile, CI and Dockerfile.
