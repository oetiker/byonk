# Dev UI as a Svelte 5 Application — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the ~3,300 hand-written lines behind `/dev/` with a Svelte 5 single-page application built from components, without changing one line of server behaviour.

**Architecture:** A new `webui/` pnpm project builds with Vite into `webui/dist/` under three fixed filenames. Rust embeds those three files with `include_str!`, exactly as it embeds the hand-written files today, so the `FROM scratch` release image still needs no filesystem. State lives in four Svelte 5 rune modules; components read and write those modules rather than each other. The old UI keeps serving `/dev/` until one late task swaps the routes and deletes it, so every commit on the way builds and runs.

**Tech Stack:** Svelte 5.57 (runes), Vite 8.2, Tailwind 4.3, shadcn-svelte (vendored source, not a runtime dependency), Vitest 5.0 + @testing-library/svelte 5.4, CodeMirror 6, pnpm.

**Spec:** `docs/superpowers/specs/2026-09-09-dev-ui-svelte-design.md` — read it before Task 1 and keep it open. It is the binding authority; this plan argues from it. Its §8 is the regression checklist and its §1 is the layout contract.

**Second spec, binding for Tasks 9a–9g only:** `docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md` — the colour-tuning amendment, added 2026-09-10 after Task 9 was complete. It restores the colour tuning the rewrite had silently dropped, as a fifth panel with a durable write path, and it amends §1 and §6 of the spec above. Tasks 9a–9g are lettered rather than numbered so that Tasks 10–14 and their pre-generated briefs keep their numbers.

---

## Global Constraints

Every task's requirements implicitly include this section.

- **Never `git add -A` or `git add .`.** Four files are permanently modified in the maintainer's tree and must never be staged: `config.yaml`, `docs/generate-samples.sh`, `docs/src/concepts/content-pipeline.md`, `tools/capture-config.yaml`. Add by explicit path and check `git diff --cached --name-only` before every commit.
- **Verification is five direct commands, never `make check` alone** (it has reported exit 0 while tests failed):
  ```
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  (cd docs && mdbook build)
  make webui                     # replaces `make editor-check` from Task 12 on
  ```
- **Every URL the browser sees must be relative.** Under Home Assistant Ingress the Supervisor strips the path prefix before byonk sees the request, so the server cannot know it. This binds `fetch` targets, the `EventSource` URL, `window.open`, asset references and redirects. Vite is configured `base: ""` for exactly this reason. A URL starting with `/` is a defect, not a style choice.
- **No server changes, except in Tasks 9a–9d.** The colour-tuning amendment adds server work, and it is confined to those four tasks: the config reload, the panel-colour override, tuning sessions and the durable config write. Everywhere else the rule stands unchanged — dev auth, the file API, the render endpoint and Ingress handling stay as they are. Task 12 changes *which files* three handlers embed and deletes three routes; it changes no behaviour. If any task other than 9a–9d seems to need a server change, the design drifted — stop and say so.
- **`tests/dev_auth_test.rs` must stay byte-identical to commit `5d6323c`.** It is the accountability hook for the `/dev` vs `/dev/` routing defect.
- **Do not add a nested page route under `/dev`.** `require_dev` redirects to the relative string `"login"`, which hard-codes depth 1. `tests/dev_static_test.rs:33-34` pins this.
- **The dev UI is a COMPILE-time dependency, not a runtime one.** `src/api/dev.rs` and `src/api/dev_auth.rs` pull it in with `include_str!`. A missing file is a compile error. A missing *directory* is a compile error too — but an **empty** directory is not, which is how a build can silently ship an empty UI. Any change to what a build context copies inherits this trap.
- **Changelog entries are user-facing only.** No build, CI or tooling notes in `CHANGES.md`. A build-only commit gets no entry.
- **Do not rename the "assign"/"assigned" vocabulary** (inherited from `main`).
- **Theme decisions are settled — do not relitigate.** Three-way System / Light / Dark defaulting to System; dark keeps byonk's navy identity; the CodeMirror theme is runtime-reconfigurable through a `Compartment`.
- **Package manager is pnpm.** Never npm or yarn. Expect `pnpm-lock.yaml`.
- **Comments, identifiers and docs in English.**
- **Treat this plan as a proposal, not as fact.** It was written by a controller reading the repo, and the controller has been wrong before while sounding certain. Verify every claim about existing code against the repo before you build on it, and say so when the repo disagrees. Run verification in the FOREGROUND, and allow `timeout: 600000` for cargo commands.

### Pinned versions

Verified published on 2026-09-09 with `pnpm view <pkg> version`. Pin exactly these; do not float.

| Package | Version |
|---|---|
| `svelte` | 5.57.0 |
| `vite` | 8.2.2 |
| `@sveltejs/vite-plugin-svelte` | 7.3.0 |
| `tailwindcss` | 4.3.3 |
| `@tailwindcss/vite` | 4.3.3 |
| `vitest` | 5.0.0 |
| `jsdom` | 30.0.1 |
| `@testing-library/svelte` | 5.4.2 |
| `@testing-library/jest-dom` | 7.0.1 |
| `bits-ui` | 2.19.1 |
| `tailwind-variants` | 3.3.1 |
| `clsx` | 2.1.1 |
| `tailwind-merge` | 3.6.0 |
| `@lucide/svelte` | 1.43.0 |

`@lucide/svelte` is the maintained package; the older `lucide-svelte` is not. CodeMirror packages carry over from `static/dev/editor/package.json` at the same ranges.

---

## The server API this app talks to

All paths are **relative to the app document** (`/dev/`), so they are written without a leading slash. Read `src/api/dev.rs` and `src/api/dev_files.rs` to confirm before use.

| Call | Method | Shape |
|---|---|---|
| `api/whoami` | GET | `{user, via, can_logout}` — `via` is `"ingress"`, `"token"` or `"cookie"` |
| `api/repos` | GET | `[{handle, writable}]` — **lists empty writable repos too** |
| `api/screens` | GET | `{screens:[{name, script, template, default_refresh}], devices, panels, dither_algorithms}` |
| `api/files?ref=<repo/screen>` | GET | `[{name, size, text}]` — `text:false` means binary |
| `api/file?ref=<ref>&name=<file>` | GET | body is the text; **`ETag` response header**; binary returns 415 |
| `api/file?ref=&name=` | PUT | body is the text; **`If-Match` header required** (see below) |
| `api/file?ref=&name=` | DELETE | — |
| `api/screen` | POST | `{handle, name, starter}` (`starter` defaults `"minimal"`) |
| `api/screen` | DELETE | `{ref}` |
| `api/screen/copy` | POST | `{ref, to_handle, to_name}` |
| `api/screen/rename` | POST | `{ref, name}` |
| `api/screen/check` | POST | `{ref, file, text}` → `{ok, issues:[{severity, location, message, line}]}` |
| `api/render?…` | GET | PNG; params below |
| `api/events` | GET | SSE; the `file-change` event drives the conflict banner |
| `api/panel-colors/{panel}` | PUT | `{colors_actual}` — temporary override, keyed by **panel**; 404 for an unknown panel (Task 9b) |
| `api/panel-colors/{panel}` | DELETE | clears the temporary override (Task 9b) |
| `api/panel-colors/{panel}/save` | POST | `{colors_actual}` — writes `config.yaml`, reloads, clears the override (Task 9d) |
| `api/tuning-session/{device}` | PUT | start or refresh a tuning session; idempotent (Task 9c) |
| `api/tuning-session/{device}` | DELETE | end it (Task 9c) |

⚠️ **The last five do not exist yet — Tasks 9b–9d build them.** Every other row
describes the server as it already is. The two rows they replace,
`POST api/panel-colors` with a `{device, colors_actual}` body and
`DELETE api/panel-colors/{panel}` keyed by device, are deleted by Task 9b.

**`If-Match` semantics, which are byonk's own and not RFC 9110's:**

- header missing → **428**, meaning "you forgot the header", deliberately distinct from a mismatch
- `If-Match: *` → **create**; **412** if the file already exists
- `If-Match: <etag>` → write; **412** if the on-disk etag differs

**Render query parameters** (`RenderQuery` in `src/api/dev.rs:29`): `screen`, `mac`, `model`, `width`, `height`, `battery_voltage`, `rssi`, `timestamp`, `params`, `colors`, `panel`, `use_actual`, `dither`, `colors_actual`, `max_error`, `chroma_clamp`, `noise_scale`, `strength`.

---

## File structure

**New — `webui/`, the whole application:**

```
webui/
  package.json            pnpm project, scripts: build, dev, test
  pnpm-lock.yaml
  vite.config.js          base:"", fixed output names, dev proxy
  vitest.config.js        jsdom, setup file
  index.html              Vite entry; becomes webui/dist/index.html
  .gitignore              dist/ and node_modules/
  test/setup.js           @testing-library/jest-dom matchers
  src/
    main.js               mounts App into #app
    App.svelte            composes AppShell; owns nothing else
    app.css               Tailwind entry + the density scale + theme tokens
    lib/
      api.js              every fetch; the only module that knows a URL
      utils.js            cn() for shadcn-svelte
      codemirror.js       CodeMirror 6 setup, moved from static/dev/editor/src
      state/
        layout.svelte.js  panel open/closed + splitter sizes, persisted
        screens.svelte.js repo list, screen list, selection
        editor.svelte.js  open files, buffers, dirty, etags, diagnostics
        simulate.svelte.js device/panel/dither/params and the render URL
        tune.svelte.js    colour working copy, override, save   (Task 9f)
      utils/
        color.js          hex <-> HSL, ported from dev.js       (Task 9g)
      components/
        AppShell.svelte      the grid; mounts panels per layout state
        IconRail.svelte      the panel toggles (five, from Task 9e)
        FilesPane.svelte     repo-grouped screen list + New/Copy/Rename/Delete
        EditorPane.svelte    file tabs + CodeMirror + dirty/error markers
        TunePane.svelte      panel/device pickers, editable swatches,
                             HSL + hex, push / save / discard  (Tasks 9e, 9g)
        PreviewPane.svelte   device frame, zoom, pop-out button
        SimulateControls.svelte
        StatusBar.svelte     validity, cursor position, watch state
        ThemeSwitch.svelte   System / Light / Dark
        ConflictBanner.svelte
        SplitPane.svelte     one component, both axes
      ui/                 shadcn-svelte components, vendored by its CLI
```

**Modified:**

- `src/api/dev.rs` — three handlers change which file they embed; three routes go (Task 12)
- `build.rs` — **new**; fails with a plain message when `webui/dist` is missing
- `Makefile` — gains `webui`; `build`, `release` and `check` depend on it; `editor` and `editor-check` retire
- `Dockerfile` — gains a Node builder stage
- `.github/workflows/ci.yml`, `.github/workflows/release-publisher.yml` — build the UI before cargo
- `.gitignore` — `webui/dist`, `webui/node_modules`
- `static/dev/login.html`, `static/dev/preview.html`, `static/dev/lens.js` — **survive**, restyled by hand (Task 11)

**Deleted (Task 12 only):** `static/dev/index.html`, `static/dev/dev.js`, `static/dev/dev.css`, `static/dev/editor.bundle.js`, `static/dev/editor/`.

---

### Task 1: The `webui` project builds three fixed files

**Files:**
- Create: `webui/package.json`, `webui/vite.config.js`, `webui/vitest.config.js`, `webui/index.html`, `webui/.gitignore`, `webui/test/setup.js`, `webui/src/main.js`, `webui/src/App.svelte`, `webui/src/app.css`, `webui/src/lib/utils.js`
- Test: `webui/test/build-output.test.js`

**Interfaces:**
- Consumes: nothing.
- Produces: `webui/dist/index.html`, `webui/dist/app.js`, `webui/dist/app.css` — **exactly these three names, no content hashes**. Task 12's `include_str!` paths depend on them. `cn(...classes)` from `src/lib/utils.js` for shadcn-svelte.

- [ ] **Step 1: Create the project**

```bash
mkdir -p webui/src/lib/state webui/src/lib/components webui/src/ui webui/test
cd webui
```

`webui/package.json`:

```json
{
  "name": "byonk-webui",
  "version": "1.0.0",
  "private": true,
  "type": "module",
  "description": "The byonk /dev screen editor, a Svelte 5 application",
  "scripts": {
    "build": "vite build",
    "dev": "vite",
    "test": "vitest run"
  },
  "dependencies": {
    "@codemirror/commands": "^6.11.0",
    "@codemirror/lang-xml": "^6.1.0",
    "@codemirror/lang-yaml": "^6.1.3",
    "@codemirror/language": "^6.12.4",
    "@codemirror/legacy-modes": "^6.5.4",
    "@codemirror/lint": "^6.9.7",
    "@codemirror/search": "^6.7.2",
    "@codemirror/state": "^6.7.4",
    "@codemirror/view": "^6.43.11",
    "@lezer/highlight": "^1.2.3",
    "@lucide/svelte": "1.43.0",
    "bits-ui": "2.19.1",
    "clsx": "2.1.1",
    "tailwind-merge": "3.6.0",
    "tailwind-variants": "3.3.1"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "7.3.0",
    "@tailwindcss/vite": "4.3.3",
    "@testing-library/jest-dom": "7.0.1",
    "@testing-library/svelte": "5.4.2",
    "jsdom": "30.0.1",
    "svelte": "5.57.0",
    "tailwindcss": "4.3.3",
    "vite": "8.2.2",
    "vitest": "5.0.0"
  }
}
```

`webui/.gitignore`:

```
dist/
node_modules/
```

`webui/vite.config.js` — the three settings that matter are `base`, the fixed
output names, and the dev proxy:

```js
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// The app is served from /dev/ in production and from / by the dev server, and
// under Home Assistant Ingress from a prefix the server never sees. `base: ""`
// makes every emitted asset reference relative to the document, which is the
// only form that survives all three. Anything root-absolute ejects the user
// out of the HA iframe.
//
// Output names are fixed with no content hash because src/api/dev.rs embeds
// these three paths with include_str! at compile time.
export default defineConfig({
  base: "",
  plugins: [svelte(), tailwindcss()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "app.js",
        assetFileNames: (info) =>
          info.names?.[0]?.endsWith(".css") ? "app.css" : "[name][extname]",
        manualChunks: undefined,
      },
    },
  },
  server: {
    // The dev server talks to a byonk running `serve` on :3000. The app's own
    // fetches are relative, so from a document at / they land on /api/...;
    // rewrite those onto /dev/api/... and authenticate with the admin bearer
    // token, which `admit()` accepts (src/api/dev_auth.rs). Export
    // BYONK_ADMIN_TOKEN before `pnpm dev`.
    proxy: {
      "/api": {
        target: "http://127.0.0.1:3000",
        changeOrigin: false,
        rewrite: (p) => p.replace(/^\/api/, "/dev/api"),
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq) => {
            const t = process.env.BYONK_ADMIN_TOKEN;
            if (t) proxyReq.setHeader("authorization", `Bearer ${t}`);
          });
        },
      },
    },
  },
});
```

`webui/vitest.config.js`:

```js
import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte({ hot: false })],
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.js"],
    include: ["test/**/*.test.js"],
  },
});
```

`webui/test/setup.js`:

```js
import "@testing-library/jest-dom/vitest";
```

`webui/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Byonk</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js"></script>
  </body>
</html>
```

`webui/src/main.js`:

```js
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

export default mount(App, { target: document.getElementById("app") });
```

`webui/src/App.svelte`:

```svelte
<script>
  // Task 4 replaces this with AppShell. It exists now so the build has
  // something real to emit and the output-name test is not vacuous.
</script>

<main class="p-4 font-mono text-sm">byonk</main>
```

`webui/src/app.css` — Tailwind entry plus the density scale and both themes.
Every colour is a variable here; no component hard-codes one:

```css
@import "tailwindcss";

/* The density scale of spec §2, decided once here rather than per component.
   Dense and cramped are neighbours; this is the guard. */
@theme {
  --font-sans: ui-sans-serif, system-ui, sans-serif;
  --font-mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  --text-xs: 0.6875rem;
  --text-sm: 0.8125rem;
  --text-base: 0.875rem;
  --spacing: 0.25rem;
}

/* Light is the default; dark keeps byonk's navy identity (settled, do not
   relitigate). `.dark` is set on <html> by the theme store in Task 10 for an
   explicit choice, and by the prefers-color-scheme block for "system". */
:root {
  --bg-color: #ffffff;
  --panel-bg: #f6f7f9;
  --border-color: #d8dce3;
  --text-color: #1a1d23;
  --text-muted: #5a6472;
  --accent-color: #1e5aa8;
  --selection-bg: #cfe0f7;
  --match-bg: #e3ecfa;
  --danger-color: #b3261e;
  --ok-color: #1a7f45;
}

.dark {
  --bg-color: #0f1622;
  --panel-bg: #172030;
  --border-color: #27354a;
  --text-color: #dbe3ef;
  --text-muted: #8896aa;
  --accent-color: #5b9bf5;
  --selection-bg: #24405f;
  --match-bg: #1d324a;
  --danger-color: #f2837c;
  --ok-color: #56c98a;
}

html, body, #app { height: 100%; }
body {
  margin: 0;
  background: var(--bg-color);
  color: var(--text-color);
}
```

`webui/src/lib/utils.js`:

```js
import { clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** Class-name helper shadcn-svelte components expect. */
export function cn(...inputs) {
  return twMerge(clsx(inputs));
}
```

- [ ] **Step 2: Write the failing test**

`webui/test/build-output.test.js`:

```js
import { describe, it, expect } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// src/api/dev.rs embeds these three paths with include_str! at compile time.
// A content hash in any of them is a compile error in Rust, discovered far
// from its cause, so it is caught here instead.
describe("build output", () => {
  const names = ["dist/index.html", "dist/app.js", "dist/app.css"];

  it.each(names)("emits %s under a fixed name", (n) => {
    expect(existsSync(resolve(root, n))).toBe(true);
  });

  it("references its assets relatively, never from the root", () => {
    const html = readFileSync(resolve(root, "dist/index.html"), "utf8");
    // Under Ingress the Supervisor strips a prefix the server never sees, so
    // a src="/app.js" would 404 inside the HA iframe.
    expect(html).not.toMatch(/(?:src|href)="\//);
    expect(html).toMatch(/app\.js/);
    expect(html).toMatch(/app\.css/);
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

```bash
cd webui && pnpm install && pnpm test
```

Expected: FAIL — `dist/index.html` does not exist yet.

- [ ] **Step 4: Build, then run the test to verify it passes**

```bash
cd webui && pnpm build && pnpm test
```

Expected: PASS, 3 files found and the relative-reference assertion holds. If
Vite emitted `app-<hash>.js`, the `rollupOptions.output` block is wrong — fix
it rather than relaxing the test.

- [ ] **Step 5: Commit**

```bash
git add webui/package.json webui/pnpm-lock.yaml webui/vite.config.js \
        webui/vitest.config.js webui/index.html webui/.gitignore \
        webui/test/setup.js webui/test/build-output.test.js \
        webui/src/main.js webui/src/App.svelte webui/src/app.css \
        webui/src/lib/utils.js
git diff --cached --name-only     # confirm the four permanent local mods are absent
git commit -m "build(webui): Svelte 5 + Vite project emitting three fixed files"
```

---

### Task 2: Every build path builds the UI

**Files:**
- Create: `build.rs`, `tests/webui_build_test.rs`
- Modify: `Cargo.toml` (declare `build = "build.rs"` if absent), `Makefile`, `Dockerfile`, `.github/workflows/ci.yml`, `.github/workflows/release-publisher.yml`, `.gitignore`

**Interfaces:**
- Consumes: `webui/dist/` from Task 1.
- Produces: `make webui`; a `build.rs` that fails a Rust build with a plain message when the UI has not been built. Task 12 relies on both.

Why now, before any component exists: spec §9 names *"retiring `editor-check` removes a guard"* as a risk, and says the plan must verify all three build paths. Wiring them first means every later task builds under the real constraint instead of discovering it at the end.

- [ ] **Step 1: Write the failing test**

`tests/webui_build_test.rs`:

```rust
//! The dev UI is a COMPILE-time dependency: `src/api/dev.rs` embeds
//! `webui/dist/*` with `include_str!`. Three build paths therefore have to
//! produce it, and the failure mode when one does not is a Rust compile
//! error a long way from its cause -- or, worse, an empty directory that
//! compiles clean and ships a blank editor.
//!
//! `make editor-check` used to guard the equivalent for the committed
//! CodeMirror bundle. It retires in Task 12 because nothing is committed any
//! more, so nothing can be stale -- but that only holds while every build
//! path genuinely runs `make webui`. This test is what keeps that true.

use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

#[test]
fn makefile_builds_the_ui_before_the_binary() {
    let mk = read("Makefile");
    assert!(
        mk.contains("\nwebui:"),
        "Makefile must define a `webui` target"
    );
    for target in ["debug:", "release:", "check:"] {
        let line = mk
            .lines()
            .find(|l| l.starts_with(target))
            .unwrap_or_else(|| panic!("Makefile has no `{target}` line"));
        assert!(
            line.contains("webui"),
            "`{target}` must depend on webui, got: {line}"
        );
    }
}

#[test]
fn dockerfile_builds_the_ui_and_copies_it_in() {
    let df = read("Dockerfile");
    assert!(
        df.contains("make webui") || df.contains("pnpm build"),
        "Dockerfile needs a Node stage that builds the UI"
    );
    assert!(
        df.contains("webui/dist"),
        "Dockerfile must copy webui/dist into the Rust build stage"
    );
}

#[test]
fn ci_builds_the_ui_before_cargo() {
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("make webui"),
        "CI must run `make webui` before cargo test"
    );
    let rp = read(".github/workflows/release-publisher.yml");
    assert!(
        rp.contains("make webui"),
        "the release workflow must run `make webui` before cargo build"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test --test webui_build_test
```

Expected: FAIL on all three — no `webui` target, no Node stage, no CI step.

- [ ] **Step 3: Add the Makefile target and dependencies**

Add to `.PHONY` and insert near the existing `editor` target:

```make
webui: ## build the /dev editor UI into webui/dist (needs pnpm)
	cd webui && pnpm install --frozen-lockfile && pnpm run build

webui-test: ## run the UI's component tests
	cd webui && pnpm run test
```

Then make `debug`, `release` and `check` depend on it. Read the current lines
first — `release: fmt lint`, `build: debug`, `check: fmt lint test editor-check`
— and add `webui` to each of `debug`, `release` and `check`. Leave `editor` and
`editor-check` alone; Task 12 retires them.

- [ ] **Step 4: Add `build.rs`**

Rust builds stay pure Rust: this only checks, it never runs pnpm. A hidden
Node invocation inside `cargo build` would make a Rust-only build silently
depend on a Node toolchain.

```rust
//! Fail early, and in words, when the dev UI has not been built.
//!
//! `src/api/dev.rs` embeds `webui/dist/{index.html,app.js,app.css}` with
//! `include_str!`. Without this check the failure is three `include_str!`
//! errors pointing at paths the reader has no reason to expect, which is a
//! poor way to learn that `make webui` exists.
//!
//! This deliberately does NOT run pnpm. A Rust build that silently needed
//! Node would be a worse trade than a clear message.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=webui/dist");

    for f in ["index.html", "app.js", "app.css"] {
        let p = format!("webui/dist/{f}");
        if !Path::new(&p).exists() {
            println!(
                "cargo:warning=missing {p} -- run `make webui` (the dev UI is \
                 embedded at compile time)"
            );
        }
    }
}
```

If `Cargo.toml`'s `[package]` has no `build =` key, cargo picks up `build.rs`
by convention; add `build = "build.rs"` only if the file is not detected.

- [ ] **Step 5: Add the Dockerfile Node stage**

Insert **before** the existing `FROM rust:1.88-alpine AS builder` line:

```dockerfile
# The dev UI is compiled into the binary, so it must exist before cargo runs.
FROM node:24-alpine AS webui
RUN corepack enable
WORKDIR /w
COPY webui/package.json webui/pnpm-lock.yaml ./webui/
RUN cd webui && pnpm install --frozen-lockfile
COPY webui ./webui
RUN cd webui && pnpm run build
```

and in the Rust stage, alongside the other compile-time inputs (after
`COPY static ./static`):

```dockerfile
COPY --from=webui /w/webui/dist ./webui/dist
```

⚠️ Verify by reading: a missing directory here is a compile error, but an
**empty** one is not. Confirm the copy landed with a real `app.js`, not just a
directory.

- [ ] **Step 6: Add the CI and release steps**

In `.github/workflows/ci.yml`, before each job's cargo step (the file has
three jobs; `make editor-check` currently sits at line 47 in the first):

```yaml
      - uses: actions/setup-node@v4
        with:
          node-version: "24"
      - run: corepack enable
      - name: Build the dev UI
        run: make webui
```

Add the same block to `.github/workflows/release-publisher.yml` before both
`cargo build --release` invocations (around lines 190 and 389 — read the file,
line numbers drift).

- [ ] **Step 7: Ignore the build output**

Append to `.gitignore`:

```
webui/dist/
webui/node_modules/
```

- [ ] **Step 8: Run the test to verify it passes**

```bash
cargo test --test webui_build_test
```

Expected: PASS, 3 tests.

- [ ] **Step 9: Prove the Docker path without building a container**

A full musl release build is slow. Copy exactly what the Dockerfile copies
into a scratch dir and run `cargo check` there with `CARGO_TARGET_DIR` pointed
at the real `target/`, so dependencies are reused. This is how `f4feba8` was
verified; it takes seconds.

```bash
SB=$(mktemp -d)
for i in Cargo.toml Cargo.lock crates src static screens fonts docs/src byonk-base default-config.yaml build.rs; do
  mkdir -p "$SB/$(dirname $i)"; cp -R "$i" "$SB/$(dirname $i)/"
done
mkdir -p "$SB/webui" && cp -R webui/dist "$SB/webui/dist"
(cd "$SB" && CARGO_TARGET_DIR=$PWD/../target cargo check 2>&1 | tail -5)
```

Expected: no error about a missing file. Record what you ran and what it said.

- [ ] **Step 10: Commit**

```bash
git add build.rs tests/webui_build_test.rs Makefile Dockerfile .gitignore \
        .github/workflows/ci.yml .github/workflows/release-publisher.yml
git diff --cached --name-only
git commit -m "build: every build path builds the dev UI"
```

No `CHANGES.md` entry — build wiring is not user-facing.

---

### Task 3: The API client

**Files:**
- Create: `webui/src/lib/api.js`
- Test: `webui/test/api.test.js`

**Interfaces:**
- Consumes: nothing.
- Produces: the only module in the app that knows a URL. Every later task calls these and never writes `fetch` itself.

```
whoami()                          -> {user, via, can_logout}
listRepos()                       -> [{handle, writable}]
listScreens()                     -> {screens, devices, panels, dither_algorithms}
listFiles(ref)                    -> [{name, size, text}]
readFile(ref, name)               -> {text, etag}
writeFile(ref, name, text, etag)  -> {etag} | throws ConflictError | throws PreconditionRequiredError
deleteFile(ref, name)             -> void
createScreen(handle, name, starter="minimal") -> void
deleteScreen(ref)                 -> void
copyScreen(ref, toHandle, toName) -> void
renameScreen(ref, name)           -> void
checkFile(ref, file, text)        -> {ok, issues:[{severity, location, message, line}]}
renderUrl(params)                 -> string   (relative)
eventsUrl()                       -> string   (relative)
class ConflictError extends Error              (HTTP 412)
class PreconditionRequiredError extends Error  (HTTP 428)
```

- [ ] **Step 1: Write the failing test**

`webui/test/api.test.js`:

```js
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
    mockFetch(async () => new Response("", { status: 204, headers: { ETag: '"new"' } }));
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
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd webui && pnpm test -- api
```

Expected: FAIL — `src/lib/api.js` does not exist.

- [ ] **Step 3: Write the implementation**

`webui/src/lib/api.js`:

```js
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
  return res.status === 204 ? null : res.json().catch(() => null);
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

/** Params are RenderQuery in src/api/dev.rs:29. Blanks are omitted, not sent. */
export const renderUrl = (params) => `api/render?${qs(params)}`;
export const eventsUrl = () => "api/events";
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd webui && pnpm test -- api
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add webui/src/lib/api.js webui/test/api.test.js
git diff --cached --name-only
git commit -m "feat(webui): API client with relative URLs and If-Match handling"
```

---

### Task 4: The layout store and `SplitPane`

**Files:**
- Create: `webui/src/lib/state/layout.svelte.js`, `webui/src/lib/components/SplitPane.svelte`
- Test: `webui/test/layout.test.js`, `webui/test/splitpane.test.js`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```
  layout.panels            -> {files: bool, editor: bool, preview: bool, simulate: bool}
  layout.toggle(name)      -> void
  layout.isOpen(name)      -> bool
  layout.size(axis)        -> number   ("x" | "y", in px)
  layout.setSize(axis, n)  -> void
  layout.load() / layout.save()
  <SplitPane axis="x" bind:size open={bool} />  slots: `a`, `b`
  ```

Spec §1 rules this implements, verbatim: every combination is legal including
none; a hidden panel returns at the size it had; a splitter exists only while
both neighbours are open; open/closed state persists **with** the sizes.

- [ ] **Step 1: Write the failing tests**

`webui/test/layout.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { layout, PANELS, STORAGE_KEY } from "../src/lib/state/layout.svelte.js";

beforeEach(() => {
  localStorage.clear();
  layout.reset();
});

describe("layout store", () => {
  it("starts with all four panels open", () => {
    expect(PANELS).toEqual(["files", "editor", "preview", "simulate"]);
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(true);
  });

  it("allows every combination, including none open", () => {
    for (const p of PANELS) layout.toggle(p);
    // Spec §1: "Do not add a 'you must keep one open' rule -- it turns a
    // toggle into an argument."
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(false);
  });

  it("gives a reopened panel the size it had", () => {
    layout.setSize("x", 640);
    layout.toggle("preview");
    layout.toggle("preview");
    expect(layout.size("x")).toBe(640);
  });

  it("persists open state and sizes together", () => {
    layout.toggle("simulate");
    layout.setSize("y", 220);
    layout.save();

    layout.reset();
    expect(layout.isOpen("simulate")).toBe(true);

    layout.load();
    expect(layout.isOpen("simulate")).toBe(false);
    expect(layout.size("y")).toBe(220);
  });

  it("survives corrupt stored state rather than throwing", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    expect(() => layout.load()).not.toThrow();
    for (const p of PANELS) expect(layout.isOpen(p)).toBe(true);
  });
});
```

`webui/test/splitpane.test.js`:

```js
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import SplitPane from "../src/lib/components/SplitPane.svelte";
import Harness from "./SplitPaneHarness.svelte";

describe("SplitPane", () => {
  it("shows a handle when both sides are open", () => {
    render(Harness, { props: { open: true } });
    expect(screen.getByRole("separator")).toBeInTheDocument();
  });

  it("retires the handle when a neighbour is hidden", () => {
    // Spec §1: a splitter must not survive as a handle that drags nothing.
    render(Harness, { props: { open: false } });
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });
});
```

`webui/test/SplitPaneHarness.svelte`:

```svelte
<script>
  import SplitPane from "../src/lib/components/SplitPane.svelte";
  let { open } = $props();
  let size = $state(300);
</script>

<SplitPane axis="x" bind:size {open}>
  {#snippet a()}<div>left</div>{/snippet}
  {#snippet b()}<div>right</div>{/snippet}
</SplitPane>
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cd webui && pnpm test -- layout splitpane
```

Expected: FAIL — neither module exists.

- [ ] **Step 3: Write the layout store**

`webui/src/lib/state/layout.svelte.js`:

```js
// Panel visibility and splitter sizes, persisted together.
//
// "Together" is the point: a panel that reopens at a default size instead of
// the size it had reads as a bug, so one record carries both (spec §1).

export const PANELS = ["files", "editor", "preview", "simulate"];
export const STORAGE_KEY = "byonk.layout.v1";

const DEFAULTS = {
  panels: { files: true, editor: true, preview: true, simulate: true },
  sizes: { x: 420, y: 260 },
};

function fresh() {
  return structuredClone(DEFAULTS);
}

class Layout {
  #s = $state(fresh());

  get panels() {
    return this.#s.panels;
  }

  isOpen(name) {
    return this.#s.panels[name] === true;
  }

  toggle(name) {
    this.#s.panels[name] = !this.#s.panels[name];
    this.save();
  }

  size(axis) {
    return this.#s.sizes[axis];
  }

  setSize(axis, px) {
    this.#s.sizes[axis] = px;
    this.save();
  }

  reset() {
    this.#s = fresh();
  }

  save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.#s));
    } catch {
      // A private window, or storage denied. The layout is a convenience;
      // losing it must never break the editor.
    }
  }

  load() {
    let raw;
    try {
      raw = localStorage.getItem(STORAGE_KEY);
    } catch {
      return;
    }
    if (!raw) return;
    try {
      const parsed = JSON.parse(raw);
      const next = fresh();
      for (const p of PANELS) {
        if (typeof parsed?.panels?.[p] === "boolean") next.panels[p] = parsed.panels[p];
      }
      for (const a of ["x", "y"]) {
        if (Number.isFinite(parsed?.sizes?.[a])) next.sizes[a] = parsed.sizes[a];
      }
      this.#s = next;
    } catch {
      // Corrupt stored state is not worth a broken page. Keep the defaults.
    }
  }
}

export const layout = new Layout();
```

- [ ] **Step 4: Write `SplitPane`**

`webui/src/lib/components/SplitPane.svelte`:

```svelte
<script>
  // One component, both axes -- two gestures, one concept (spec §1).
  //
  // `open` is whether the SECOND pane is showing. When it is not, this
  // renders pane `a` alone and no handle: a handle that drags nothing is
  // worse than no handle.
  let { axis = "x", size = $bindable(300), open = true, min = 120, a, b } = $props();

  let host;
  let dragging = $state(false);

  function start(e) {
    dragging = true;
    e.preventDefault();
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }

  function move(e) {
    if (!host) return;
    const r = host.getBoundingClientRect();
    const next = axis === "x" ? r.right - e.clientX : r.bottom - e.clientY;
    size = Math.max(min, Math.round(next));
  }

  function stop() {
    dragging = false;
    window.removeEventListener("pointermove", move);
  }

  // Keyboard: a separator that only responds to a pointer is not reachable.
  function key(e) {
    const step = e.shiftKey ? 48 : 12;
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") size = Math.max(min, size - step);
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") size = size + step;
    else return;
    e.preventDefault();
  }
</script>

<div
  bind:this={host}
  class="flex min-h-0 min-w-0 flex-1 {axis === 'x' ? 'flex-row' : 'flex-col'}"
>
  <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">{@render a()}</div>

  {#if open}
    <div
      role="separator"
      tabindex="0"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label="Resize panel"
      class="shrink-0 bg-[var(--border-color)] hover:bg-[var(--accent-color)]
             {axis === 'x' ? 'w-px cursor-col-resize' : 'h-px cursor-row-resize'}
             {dragging ? 'bg-[var(--accent-color)]' : ''}"
      onpointerdown={start}
      onkeydown={key}
    ></div>
    <div
      class="min-h-0 min-w-0 shrink-0 overflow-hidden"
      style={axis === "x" ? `width:${size}px` : `height:${size}px`}
    >
      {@render b()}
    </div>
  {/if}
</div>
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd webui && pnpm test -- layout splitpane
```

Expected: PASS, 5 + 2 tests.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/state/layout.svelte.js \
        webui/src/lib/components/SplitPane.svelte \
        webui/test/layout.test.js webui/test/splitpane.test.js \
        webui/test/SplitPaneHarness.svelte
git diff --cached --name-only
git commit -m "feat(webui): layout store and a SplitPane that retires its handle"
```

---

### Task 5: The shell — `AppShell`, `IconRail`, the four toggles

**Files:**
- Create: `webui/src/lib/components/AppShell.svelte`, `webui/src/lib/components/IconRail.svelte`
- Modify: `webui/src/App.svelte`
- Test: `webui/test/iconrail.test.js`

**Interfaces:**
- Consumes: `layout` and `SplitPane` from Task 4.
- Produces: `AppShell`, which mounts the four panels according to `layout` and is the only component that knows the grid. Later tasks fill the panels in; each is a placeholder `<div>` until its own task.

- [ ] **Step 1: Vendor the shadcn-svelte components you need**

```bash
cd webui
pnpm dlx shadcn-svelte@latest init
pnpm dlx shadcn-svelte@latest add button select input label tabs separator tooltip
```

Answer the init prompts so components land in `src/ui` and the alias resolves
to `$lib`. Its CLI copies source into the repo — these become byonk's own
files, reviewable and editable, with no version drift to chase (spec §3).
Commit whatever it writes, including its config file.

⚠️ Verify what the CLI actually did rather than trusting this step: read
`components.json` and confirm the import alias matches the paths used below.
Adjust the plan's imports to what the CLI produced, not the reverse.

- [ ] **Step 2: Write the failing test**

`webui/test/iconrail.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { fireEvent } from "@testing-library/dom";
import IconRail from "../src/lib/components/IconRail.svelte";
import { layout, PANELS } from "../src/lib/state/layout.svelte.js";

beforeEach(() => {
  localStorage.clear();
  layout.reset();
});

describe("IconRail", () => {
  it("offers one labelled toggle per panel", () => {
    render(IconRail);
    for (const p of PANELS) {
      expect(screen.getByRole("button", { name: new RegExp(p, "i") })).toBeInTheDocument();
    }
  });

  it("reports each panel's state through aria-pressed", async () => {
    render(IconRail);
    const files = screen.getByRole("button", { name: /files/i });
    expect(files).toHaveAttribute("aria-pressed", "true");

    await fireEvent.click(files);
    expect(layout.isOpen("files")).toBe(false);
    expect(files).toHaveAttribute("aria-pressed", "false");
  });

  it("carries no badge or count -- the status bar reports those", () => {
    render(IconRail);
    // Spec §1: "The toggle state is the only thing the icon carries."
    const rail = screen.getByRole("toolbar", { name: /panels/i });
    expect(rail.textContent.replace(/\s/g, "")).toBe("");
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

```bash
cd webui && pnpm test -- iconrail
```

Expected: FAIL — `IconRail.svelte` does not exist.

- [ ] **Step 4: Write `IconRail`**

`webui/src/lib/components/IconRail.svelte`:

```svelte
<script>
  import { Files, Code, Monitor, SlidersHorizontal } from "@lucide/svelte";
  import { layout, PANELS } from "../state/layout.svelte.js";

  const ICONS = {
    files: Files,
    editor: Code,
    preview: Monitor,
    simulate: SlidersHorizontal,
  };
  const LABELS = {
    files: "Files",
    editor: "Editor",
    preview: "Preview",
    simulate: "Simulate",
  };
</script>

<div
  role="toolbar"
  aria-label="Panels"
  aria-orientation="vertical"
  class="flex w-10 shrink-0 flex-col items-center gap-1 border-r
         border-[var(--border-color)] bg-[var(--panel-bg)] py-2"
>
  {#each PANELS as p (p)}
    {@const Icon = ICONS[p]}
    <button
      type="button"
      aria-pressed={layout.isOpen(p)}
      aria-label={LABELS[p]}
      title={LABELS[p]}
      onclick={() => layout.toggle(p)}
      class="flex h-8 w-8 items-center justify-center rounded
             text-[var(--text-muted)] hover:bg-[var(--bg-color)]
             aria-pressed:bg-[var(--bg-color)]
             aria-pressed:text-[var(--accent-color)]"
    >
      <Icon size={16} aria-hidden="true" />
    </button>
  {/each}
</div>
```

- [ ] **Step 5: Write `AppShell`**

`webui/src/lib/components/AppShell.svelte`. It mounts each panel only while
its toggle is on, and a `SplitPane`'s handle exists only while both of its
neighbours are open — which is why `open` is passed rather than assumed:

```svelte
<script>
  import { onMount } from "svelte";
  import IconRail from "./IconRail.svelte";
  import SplitPane from "./SplitPane.svelte";
  import { layout } from "../state/layout.svelte.js";

  // Placeholders until Tasks 6-10 fill them in.
  import FilesPane from "./FilesPane.svelte";
  import EditorPane from "./EditorPane.svelte";
  import PreviewPane from "./PreviewPane.svelte";
  import SimulateControls from "./SimulateControls.svelte";

  onMount(() => layout.load());

  let xSize = $state(layout.size("x"));
  let ySize = $state(layout.size("y"));
  $effect(() => layout.setSize("x", xSize));
  $effect(() => layout.setSize("y", ySize));
</script>

<div class="flex h-full flex-col bg-[var(--bg-color)] text-[var(--text-color)]">
  <div class="flex min-h-0 flex-1">
    <IconRail />

    {#if layout.isOpen("files")}
      <div class="w-56 shrink-0 overflow-auto border-r border-[var(--border-color)]">
        <FilesPane />
      </div>
    {/if}

    <SplitPane
      axis="x"
      bind:size={xSize}
      open={layout.isOpen("preview") || layout.isOpen("simulate")}
    >
      {#snippet a()}
        {#if layout.isOpen("editor")}
          <EditorPane />
        {/if}
      {/snippet}

      {#snippet b()}
        <SplitPane
          axis="y"
          bind:size={ySize}
          open={layout.isOpen("preview") && layout.isOpen("simulate")}
        >
          {#snippet a()}
            {#if layout.isOpen("preview")}<PreviewPane />{/if}
          {/snippet}
          {#snippet b()}
            {#if layout.isOpen("simulate")}<SimulateControls />{/if}
          {/snippet}
        </SplitPane>
      {/snippet}
    </SplitPane>
  </div>
</div>
```

⚠️ That nested-snippet block is the fiddliest markup in the plan. Write it,
then read it back against Svelte 5's snippet syntax and fix it to compile —
the plan's shape is the intent, not a guarantee of syntax.

Create the four panel components as one-line placeholders now so this
compiles; each later task replaces its own:

```svelte
<!-- webui/src/lib/components/FilesPane.svelte (and the other three) -->
<div class="p-2 text-xs text-[var(--text-muted)]">files</div>
```

- [ ] **Step 6: Point `App.svelte` at the shell**

```svelte
<script>
  import AppShell from "./lib/components/AppShell.svelte";
</script>

<AppShell />
```

- [ ] **Step 7: Run the tests and the build**

```bash
cd webui && pnpm test && pnpm build
```

Expected: PASS, and a build that still emits the three fixed filenames.

- [ ] **Step 8: Commit**

```bash
git add webui/components.json webui/src/ui webui/src/App.svelte \
        webui/src/lib/components/AppShell.svelte \
        webui/src/lib/components/IconRail.svelte \
        webui/src/lib/components/FilesPane.svelte \
        webui/src/lib/components/EditorPane.svelte \
        webui/src/lib/components/PreviewPane.svelte \
        webui/src/lib/components/SimulateControls.svelte \
        webui/test/iconrail.test.js webui/package.json webui/pnpm-lock.yaml
git diff --cached --name-only
git commit -m "feat(webui): app shell with one toggle per panel"
```

---

### Task 6: The screens store and `FilesPane`

**Files:**
- Create: `webui/src/lib/state/screens.svelte.js`
- Modify: `webui/src/lib/components/FilesPane.svelte`
- Test: `webui/test/screens.test.js`, `webui/test/filespane.test.js`

**Interfaces:**
- Consumes: `api` (Task 3).
- Produces:
  ```
  screens.repos      -> [{handle, writable}]
  screens.list       -> [{name, ref, handle, writable}]
  screens.grouped    -> [{handle, writable, screens:[...]}]   -- includes EMPTY writable repos
  screens.selected   -> ref | null
  screens.writable(ref) -> bool
  screens.refresh()  -> Promise<void>
  screens.select(ref) -> void
  ```

Spec §8.8 is the trap here: **empty writable repos must be listed.** They were
a real defect (`ccc4f47`), and the natural client-side implementation —
grouping the screen list by handle — reintroduces it, because a repo with no
screens contributes no rows. Group over `api/repos`, not over `api/screens`.

- [ ] **Step 1: Write the failing tests**

`webui/test/screens.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => vi.restoreAllMocks());

describe("screens store", () => {
  it("lists a writable repo that holds no screens yet", async () => {
    // Regression guard for ccc4f47. `local` on a fresh install is exactly
    // this case, and grouping the screen list instead of the repo list is how
    // the bug comes back.
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "local", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "byonk-builtin/hello" }],
      devices: [],
      panels: [],
      dither_algorithms: [],
    });

    await screens.refresh();

    const handles = screens.grouped.map((g) => g.handle);
    expect(handles).toContain("local");
    expect(screens.grouped.find((g) => g.handle === "local").screens).toEqual([]);
  });

  it("knows which refs are writable", async () => {
    vi.spyOn(api, "listRepos").mockResolvedValue([
      { handle: "local", writable: true },
      { handle: "byonk-builtin", writable: false },
    ]);
    vi.spyOn(api, "listScreens").mockResolvedValue({
      screens: [{ name: "local/a" }, { name: "byonk-builtin/b" }],
      devices: [],
      panels: [],
      dither_algorithms: [],
    });

    await screens.refresh();
    expect(screens.writable("local/a")).toBe(true);
    expect(screens.writable("byonk-builtin/b")).toBe(false);
  });
});
```

`webui/test/filespane.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import FilesPane from "../src/lib/components/FilesPane.svelte";
import { screens } from "../src/lib/state/screens.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(async () => {
  vi.restoreAllMocks();
  vi.spyOn(api, "listRepos").mockResolvedValue([
    { handle: "local", writable: true },
    { handle: "byonk-builtin", writable: false },
  ]);
  vi.spyOn(api, "listScreens").mockResolvedValue({
    screens: [{ name: "local/a" }, { name: "byonk-builtin/b" }],
    devices: [],
    panels: [],
    dither_algorithms: [],
  });
  await screens.refresh();
});

describe("FilesPane", () => {
  it("disables Rename and Delete on a read-only screen, keeps Copy and New", async () => {
    // Spec §8.6.
    screens.select("byonk-builtin/b");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /copy/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /new/i })).toBeEnabled();
  });

  it("enables Rename and Delete on a writable screen", () => {
    screens.select("local/a");
    render(FilesPane);
    expect(screen.getByRole("button", { name: /rename/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /delete/i })).toBeEnabled();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cd webui && pnpm test -- screens filespane
```

Expected: FAIL — the store does not exist and `FilesPane` is a placeholder.

- [ ] **Step 3: Write the screens store**

`webui/src/lib/state/screens.svelte.js`:

```js
import * as api from "../api.js";

/**
 * Repos, screens, and which one is selected.
 *
 * Grouping is done over the REPO list, not over the screen list. A writable
 * repo holding no screens is somewhere the user can create one, and it must
 * appear -- `local` on a fresh install is exactly that. Grouping the screen
 * list instead silently drops it, which was the ccc4f47 defect.
 */
class Screens {
  repos = $state([]);
  list = $state([]);
  selected = $state(null);
  meta = $state({ devices: [], panels: [], dither_algorithms: [] });

  grouped = $derived(
    this.repos.map((r) => ({
      handle: r.handle,
      writable: r.writable,
      screens: this.list.filter((s) => s.handle === r.handle),
    })),
  );

  writable(ref) {
    if (!ref) return false;
    const handle = ref.split("/", 1)[0];
    return this.repos.find((r) => r.handle === handle)?.writable === true;
  }

  select(ref) {
    this.selected = ref;
  }

  async refresh() {
    const [repos, payload] = await Promise.all([api.listRepos(), api.listScreens()]);
    this.repos = repos;
    this.meta = {
      devices: payload.devices ?? [],
      panels: payload.panels ?? [],
      dither_algorithms: payload.dither_algorithms ?? [],
    };
    this.list = (payload.screens ?? []).map((s) => {
      const handle = s.name.split("/", 1)[0];
      return { ...s, ref: s.name, handle, label: s.name.slice(handle.length + 1) };
    });
  }
}

export const screens = new Screens();
```

- [ ] **Step 4: Write `FilesPane`**

Repo-grouped list, four buttons, read-only rules from spec §8.6. Use the
vendored `Button` from `src/ui`. The essentials:

```svelte
<script>
  import { screens } from "../state/screens.svelte.js";
  import * as api from "../api.js";

  const canEdit = $derived(screens.writable(screens.selected));

  async function newScreen() {
    const handle = screens.repos.find((r) => r.writable)?.handle;
    const name = prompt("New screen name");
    if (!handle || !name) return;
    await api.createScreen(handle, name);
    await screens.refresh();
    screens.select(`${handle}/${name}`);
  }

  async function copyScreen() {
    const to = screens.repos.find((r) => r.writable)?.handle;
    const name = prompt("Copy to name");
    if (!to || !name || !screens.selected) return;
    await api.copyScreen(screens.selected, to, name);
    await screens.refresh();
    screens.select(`${to}/${name}`);
  }

  async function renameScreen() {
    const name = prompt("Rename to");
    if (!name || !screens.selected) return;
    await api.renameScreen(screens.selected, name);
    await screens.refresh();
  }

  async function deleteScreen() {
    if (!screens.selected) return;
    if (!confirm(`Delete ${screens.selected}?`)) return;
    await api.deleteScreen(screens.selected);
    screens.select(null);
    await screens.refresh();
  }
</script>

<div class="flex flex-col text-xs">
  <div class="flex gap-1 border-b border-[var(--border-color)] p-1">
    <button type="button" onclick={newScreen}>New</button>
    <button type="button" onclick={copyScreen} disabled={!screens.selected}>Copy</button>
    <button type="button" onclick={renameScreen} disabled={!canEdit}>Rename</button>
    <button type="button" onclick={deleteScreen} disabled={!canEdit}>Delete</button>
  </div>

  {#each screens.grouped as g (g.handle)}
    <div class="px-2 pt-2 font-mono text-[10px] uppercase text-[var(--text-muted)]">
      {g.handle}{#if !g.writable}<span class="ml-1">read-only</span>{/if}
    </div>
    {#each g.screens as s (s.ref)}
      <button
        type="button"
        class="truncate px-2 py-0.5 text-left hover:bg-[var(--panel-bg)]
               {screens.selected === s.ref ? 'bg-[var(--accent-color)] text-white' : ''}"
        onclick={() => screens.select(s.ref)}
      >{s.label}</button>
    {/each}
  {/each}
</div>
```

Restyle with the vendored `Button` and the density scale while you build it —
this markup carries the behaviour, not the finished look.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd webui && pnpm test -- screens filespane
```

Expected: PASS, 2 + 2 tests.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/state/screens.svelte.js \
        webui/src/lib/components/FilesPane.svelte \
        webui/test/screens.test.js webui/test/filespane.test.js
git diff --cached --name-only
git commit -m "feat(webui): screens store listing empty writable repos, and FilesPane"
```

---

### Task 7: The editor store, CodeMirror, and `EditorPane`

**Files:**
- Create: `webui/src/lib/state/editor.svelte.js`, `webui/src/lib/codemirror.js`
- Modify: `webui/src/lib/components/EditorPane.svelte`
- Test: `webui/test/editor.test.js`

**Interfaces:**
- Consumes: `api` (Task 3), `screens` (Task 6).
- Produces:
  ```
  editor.tabs        -> [{ref, name, text, etag, dirty, issues, readOnly}]
  editor.activeIndex -> number
  editor.active      -> tab | null
  editor.open(ref, name)   -> Promise<void>
  editor.close(index)      -> void
  editor.setText(text)     -> void
  editor.save()            -> Promise<void>   throws ConflictError
  editor.check()           -> Promise<void>   fills active.issues
  editor.replaceFromDisk(ref, name) -> Promise<void>  (clears dirty)
  createEditor(parent, {doc, language, readOnly, onChange}) -> {view, setDoc, setDiagnostics, setTheme, destroy}
  ```

Two spec rules bite here. §8.6: on a read-only repo, **typing must insert
nothing** — a rejected save is not enough. §7: the dirty marker and the error
marker are **distinct signals that happen to look alike**, so they are
separate fields and a test says so.

- [ ] **Step 1: Move the CodeMirror setup**

`static/dev/editor/src/editor.js` already does this job well. Copy it to
`webui/src/lib/codemirror.js` and change three things:

1. Export a factory rather than attaching to `window`.
2. Add an `EditorState.readOnly` + `EditorView.editable` compartment, both set
   from a `readOnly` option — `readOnly` alone still allows a caret and some
   commands, so both are needed to make typing genuinely insert nothing.
3. Keep the existing theme `Compartment` exactly as it is (settled; the theme
   must stay runtime-reconfigurable) and keep chrome colours reading the CSS
   custom properties rather than a second palette.

Read the original before copying; it is ~200 lines and its comments explain
why each selector exists.

- [ ] **Step 2: Write the failing tests**

`webui/test/editor.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  editor.reset();
});

describe("editor store", () => {
  it("keeps dirty and error as separate signals", async () => {
    // Spec §7: they look alike in the UI and mean different things. A buffer
    // can be dirty and valid, or clean and broken.
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "ok", etag: '"1"' });
    await editor.open("local/a", "script.lua");

    expect(editor.active.dirty).toBe(false);
    editor.setText("bad");
    expect(editor.active.dirty).toBe(true);
    expect(editor.active.issues).toEqual([]);

    vi.spyOn(api, "checkFile").mockResolvedValue({
      ok: false,
      issues: [{ severity: "error", location: "script.lua", message: "boom", line: 1 }],
    });
    await editor.check();
    expect(editor.active.issues).toHaveLength(1);
    expect(editor.active.dirty).toBe(true);

    vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"2"' });
    await editor.save();
    expect(editor.active.dirty).toBe(false);
    expect(editor.active.issues).toHaveLength(1); // still broken, just saved
  });

  it("handles the tab lifecycle, including closing the active one", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    await editor.open("local/a", "two.lua");
    await editor.open("local/a", "three.lua");
    expect(editor.tabs).toHaveLength(3);

    editor.activeIndex = 1;
    editor.close(1);
    expect(editor.tabs.map((t) => t.name)).toEqual(["one.lua", "three.lua"]);
    // Closing the active tab must land on a real neighbour, not off the end.
    expect(editor.activeIndex).toBeLessThan(editor.tabs.length);
    expect(editor.active).not.toBeNull();

    editor.close(0);
    editor.close(0);
    expect(editor.tabs).toHaveLength(0);
    expect(editor.active).toBeNull();
  });

  it("opens an already-open file by focusing its tab, not duplicating it", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "one.lua");
    await editor.open("local/a", "one.lua");
    expect(editor.tabs).toHaveLength(1);
  });

  it("marks a tab read-only when its repo is not writable", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("byonk-builtin/b", "script.lua", { readOnly: true });
    expect(editor.active.readOnly).toBe(true);
  });

  it("sends the etag on save and surfaces a conflict", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.setText("y");

    const write = vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await expect(editor.save()).rejects.toBeInstanceOf(api.ConflictError);
    expect(write.mock.calls[0][3]).toBe('"1"');
    expect(editor.active.dirty).toBe(true); // a failed save does not clean it
  });

  it("replaceFromDisk swaps the buffer and clears dirty", async () => {
    vi.spyOn(api, "readFile").mockResolvedValue({ text: "x", etag: '"1"' });
    await editor.open("local/a", "script.lua");
    editor.setText("mine");
    expect(editor.active.dirty).toBe(true);

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    await editor.replaceFromDisk("local/a", "script.lua");
    expect(editor.active.text).toBe("theirs");
    expect(editor.active.etag).toBe('"2"');
    expect(editor.active.dirty).toBe(false);
  });
});
```

- [ ] **Step 3: Run them to verify they fail**

```bash
cd webui && pnpm test -- editor
```

Expected: FAIL — the store does not exist.

- [ ] **Step 4: Write the editor store**

`webui/src/lib/state/editor.svelte.js`:

```js
import * as api from "../api.js";

/**
 * Open files and their buffers.
 *
 * This lives outside any component on purpose: IconRail can unmount
 * EditorPane at any moment, and an unsaved buffer must survive that.
 *
 * `dirty` and `issues` are deliberately separate. They render similarly --
 * a marker on the tab -- and mean unrelated things: dirty is "differs from
 * disk", issues is "does not parse". Conflating them was worth a test.
 */
class Editor {
  tabs = $state([]);
  activeIndex = $state(-1);

  get active() {
    return this.tabs[this.activeIndex] ?? null;
  }

  reset() {
    this.tabs = [];
    this.activeIndex = -1;
  }

  async open(ref, name, { readOnly = false } = {}) {
    const found = this.tabs.findIndex((t) => t.ref === ref && t.name === name);
    if (found >= 0) {
      this.activeIndex = found;
      return;
    }
    const { text, etag } = await api.readFile(ref, name);
    this.tabs.push({ ref, name, text, saved: text, etag, issues: [], readOnly });
    this.activeIndex = this.tabs.length - 1;
  }

  close(i) {
    if (i < 0 || i >= this.tabs.length) return;
    this.tabs.splice(i, 1);
    // Land on a real neighbour. Leaving activeIndex past the end is how a
    // closed active tab turns into a blank editor with no way back.
    if (this.tabs.length === 0) this.activeIndex = -1;
    else if (this.activeIndex >= this.tabs.length) this.activeIndex = this.tabs.length - 1;
    else if (this.activeIndex > i) this.activeIndex -= 1;
  }

  setText(text) {
    const t = this.active;
    if (!t || t.readOnly) return;
    t.text = text;
  }

  async check() {
    const t = this.active;
    if (!t) return;
    const report = await api.checkFile(t.ref, t.name, t.text);
    t.issues = report.issues ?? [];
  }

  async save() {
    const t = this.active;
    if (!t) return;
    const { etag } = await api.writeFile(t.ref, t.name, t.text, t.etag);
    t.etag = etag;
    t.saved = t.text;
  }

  async replaceFromDisk(ref, name) {
    const t = this.tabs.find((x) => x.ref === ref && x.name === name);
    if (!t) return;
    const { text, etag } = await api.readFile(ref, name);
    t.text = text;
    t.saved = text;
    t.etag = etag;
  }
}

// `dirty` is derived, not stored: two sources of truth for "differs from
// disk" is one too many.
Object.defineProperty(Editor.prototype, "dirtyOf", {
  value(tab) {
    return tab.text !== tab.saved;
  },
});

export const editor = new Editor();
```

⚠️ The tests read `editor.active.dirty`. Implement `dirty` as a per-tab
`$derived` (or a getter on the tab object) rather than the prototype helper
sketched above — write whichever compiles under Svelte 5 runes, keep the
single-source-of-truth property, and make the test pass without changing what
it asserts.

- [ ] **Step 5: Write `EditorPane`**

File tabs across the top, CodeMirror below, markers on the tabs. The
read-only rule is the part to get right: pass `readOnly` into `createEditor`
so the editor is genuinely not editable, rather than filtering keystrokes.

```svelte
<script>
  import { onMount } from "svelte";
  import { editor } from "../state/editor.svelte.js";
  import { createEditor } from "../codemirror.js";

  let host;
  let cm;

  onMount(() => {
    cm = createEditor(host, {
      doc: editor.active?.text ?? "",
      language: languageOf(editor.active?.name),
      readOnly: editor.active?.readOnly ?? true,
      onChange: (text) => editor.setText(text),
    });
    return () => cm?.destroy();
  });

  function languageOf(name = "") {
    if (name.endsWith(".lua")) return "lua";
    if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
    if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
    return null;
  }

  // Swapping tabs replaces the document rather than rebuilding the view.
  $effect(() => {
    const t = editor.active;
    if (cm && t) cm.setDoc(t.text, languageOf(t.name), t.readOnly);
  });

  $effect(() => {
    if (cm && editor.active) cm.setDiagnostics(editor.active.issues);
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div role="tablist" class="flex border-b border-[var(--border-color)] text-xs">
    {#each editor.tabs as t, i (t.ref + t.name)}
      <div
        role="tab"
        aria-selected={i === editor.activeIndex}
        class="flex items-center gap-1 border-r border-[var(--border-color)] px-2 py-1"
      >
        <button type="button" onclick={() => (editor.activeIndex = i)}>{t.name}</button>
        {#if t.dirty}<span aria-label="unsaved changes">●</span>{/if}
        {#if t.issues.length}<span aria-label="has errors" class="text-[var(--danger-color)]">▲</span>{/if}
        <button type="button" aria-label={`Close ${t.name}`} onclick={() => editor.close(i)}>×</button>
      </div>
    {/each}
  </div>
  <div bind:this={host} class="min-h-0 flex-1 overflow-auto"></div>
</div>
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd webui && pnpm test -- editor
```

Expected: PASS, 6 tests.

- [ ] **Step 7: Prove the read-only rule by hand**

Component tests do not exercise CodeMirror's key handling. Run
`pnpm dev`, open a `byonk-builtin/*` file, and type. Expected: **nothing is
inserted** — not a rejected save, not a warning toast. Record what you saw.

- [ ] **Step 8: Commit**

```bash
git add webui/src/lib/state/editor.svelte.js webui/src/lib/codemirror.js \
        webui/src/lib/components/EditorPane.svelte webui/test/editor.test.js
git diff --cached --name-only
git commit -m "feat(webui): editor store, CodeMirror wrapper and EditorPane"
```

---

### Task 8: The simulate store and `SimulateControls`

**Files:**
- Create: `webui/src/lib/state/simulate.svelte.js`
- Modify: `webui/src/lib/components/SimulateControls.svelte`
- Test: `webui/test/simulate.test.js`

**Interfaces:**
- Consumes: `api` (Task 3), `screens.meta` (Task 6).
- Produces:
  ```
  simulate.device / panel / dither / maxError / chromaClamp / noiseScale /
  simulate.strength / battery / rssi / timestamp / params
  simulate.url(ref) -> string   -- the relative render URL, blanks omitted
  simulate.size     -> {width, height}   from the chosen panel profile
  ```

- [ ] **Step 1: Write the failing test**

`webui/test/simulate.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";

beforeEach(() => simulate.reset());

describe("simulate store", () => {
  it("builds a relative render URL and omits unset controls", () => {
    simulate.dither = "atkinson";
    const u = simulate.url("local/hello");
    expect(u.startsWith("/")).toBe(false);
    expect(u).toContain("screen=local%2Fhello");
    expect(u).toContain("dither=atkinson");
    expect(u).not.toContain("panel=");
    expect(u).not.toContain("colors_actual=");
  });

  it("takes width and height from the chosen panel profile", () => {
    screens.meta = {
      devices: [],
      dither_algorithms: [],
      panels: [{ name: "trmnl_x", width: 1872, height: 1404 }],
    };
    simulate.panel = "trmnl_x";
    expect(simulate.size).toEqual({ width: 1872, height: 1404 });
  });

  it("falls back to the default size when no panel is chosen", () => {
    expect(simulate.size).toEqual({ width: 800, height: 480 });
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd webui && pnpm test -- simulate
```

Expected: FAIL — module missing.

- [ ] **Step 3: Write the store**

`webui/src/lib/state/simulate.svelte.js`. Field names map one-to-one onto
`RenderQuery` in `src/api/dev.rs:29`; `api.renderUrl` already drops blanks:

```js
import * as api from "../api.js";
import { screens } from "./screens.svelte.js";

const DEFAULTS = {
  device: "",
  panel: "",
  dither: "",
  max_error: "",
  chroma_clamp: "",
  noise_scale: "",
  strength: "",
  battery_voltage: "",
  rssi: "",
  timestamp: "",
  params: "",
};

class Simulate {
  #f = $state(structuredClone(DEFAULTS));

  reset() {
    this.#f = structuredClone(DEFAULTS);
  }

  get fields() {
    return this.#f;
  }

  get size() {
    const p = screens.meta.panels?.find((x) => x.name === this.#f.panel);
    return p ? { width: p.width, height: p.height } : { width: 800, height: 480 };
  }

  url(ref) {
    return api.renderUrl({ screen: ref, ...this.#f });
  }
}

export const simulate = new Simulate();
```

⚠️ The test writes `simulate.dither` and `simulate.panel` directly. Expose the
fields as accessors on the instance (a getter/setter pair per field, or a
`$state` object with public fields) so both the test and the component read
naturally. Choose one shape and use it in `SimulateControls` too.

- [ ] **Step 4: Write `SimulateControls`**

One labelled control per `RenderQuery` field, in the order the current UI uses
(read `static/dev/dev.js` for that order before it is deleted): Device, Panel
Profile, the palette swatches for the chosen profile, Dither and its four
parameters, Battery, WiFi, Current Time, Custom Parameters. Populate Device,
Panel Profile and Dither from `screens.meta`.

The block **scrolls inside whatever height it is given** and never overlaps
the render (spec §1). Give the root `overflow-auto min-h-0`.

- [ ] **Step 5: Run the test to verify it passes**

```bash
cd webui && pnpm test -- simulate
```

Expected: PASS, 3 tests.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/state/simulate.svelte.js \
        webui/src/lib/components/SimulateControls.svelte webui/test/simulate.test.js
git diff --cached --name-only
git commit -m "feat(webui): simulate store and controls"
```

---

### Task 9: `PreviewPane` and the pop-out

**Files:**
- Modify: `webui/src/lib/components/PreviewPane.svelte`
- Test: `webui/test/preview.test.js`

**Interfaces:**
- Consumes: `simulate` (Task 8), `screens` (Task 6).
- Produces: `PreviewPane`, and the `BroadcastChannel` protocol the pop-out speaks.

**This behaviour is proven on real hardware — keep it, do not redesign it.**
On 2026-09-09 the pop-out was verified under Home Assistant Ingress: it opens
as a top-level document at the ingress URL, HA serves it, and it live-follows
a drawer change (800×480 → 1872×1404, new blob, the pop-out's own size label
updated). Two mechanics carried that result and must survive:

1. The pop-out is opened with a **named window** (`window.open(url, "byonk-preview")`),
   so re-opening returns the existing window instead of stacking new ones.
2. The render is handed over as a **blob**, and the pop-out answers a
   `{hello:true}` handshake so a reloaded pop-out gets the current image back
   (spec §8.4).

The URL passed to `window.open` must be **relative** — built from the app's
own document, never from an origin. That is what makes it work inside Ingress.

- [ ] **Step 1: Write the failing test**

`webui/test/preview.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { fireEvent } from "@testing-library/dom";
import PreviewPane from "../src/lib/components/PreviewPane.svelte";

beforeEach(() => vi.restoreAllMocks());

describe("PreviewPane", () => {
  it("opens the pop-out at a relative URL, in a named window", async () => {
    // Proven under HA Ingress on 2026-09-09. An origin-absolute URL here
    // leaves the HA iframe; an unnamed window stacks a new one per click.
    const open = vi.spyOn(window, "open").mockReturnValue({ closed: false });
    render(PreviewPane);
    await fireEvent.click(screen.getByRole("button", { name: /pop ?out/i }));

    const [url, name] = open.mock.calls[0];
    expect(url.startsWith("/")).toBe(false);
    expect(url.startsWith("http")).toBe(false);
    expect(url).toBe("preview.html");
    expect(name).toBe("byonk-preview");
  });

  it("replies to the pop-out's hello handshake with the current render", async () => {
    // Spec §8.4: a reloaded pop-out must recover, not sit blank.
    const posted = [];
    const fake = { postMessage: (m) => posted.push(m), close() {}, onmessage: null };
    vi.spyOn(globalThis, "BroadcastChannel").mockImplementation(() => fake);

    render(PreviewPane);
    fake.onmessage?.({ data: { hello: true } });
    expect(posted.some((m) => "blob" in m || "url" in m)).toBe(true);
  });

  it("fits the device to its box without distorting the aspect ratio", () => {
    // The rule that keeps an upright E1004 (1200x1600) from being squashed.
    render(PreviewPane);
    const img = screen.getByRole("img", { name: /screen preview/i });
    const fit = getComputedStyle(img).objectFit;
    expect(["contain", ""]).toContain(fit);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd webui && pnpm test -- preview
```

Expected: FAIL — `PreviewPane` is still a placeholder.

- [ ] **Step 3: Write `PreviewPane`**

The essentials; build the device frame and zoom controls around them:

```svelte
<script>
  import { onMount } from "svelte";
  import { simulate } from "../state/simulate.svelte.js";
  import { screens } from "../state/screens.svelte.js";

  let blobUrl = $state(null);
  let channel;
  let lastBlob = null;

  onMount(() => {
    channel = new BroadcastChannel("byonk-preview");
    // A reloaded pop-out says hello; answer with the current render so it
    // recovers instead of sitting blank (spec §8.4).
    channel.onmessage = (e) => {
      if (e.data?.hello && lastBlob) channel.postMessage({ blob: lastBlob });
    };
    return () => channel?.close();
  });

  async function renderNow() {
    const ref = screens.selected;
    if (!ref) return;
    const res = await fetch(simulate.url(ref));
    if (!res.ok) return;
    const blob = await res.blob();
    if (blobUrl) URL.revokeObjectURL(blobUrl);
    blobUrl = URL.createObjectURL(blob);
    lastBlob = blob;
    channel?.postMessage({ blob });
  }

  // Re-render whenever the screen or any simulate control changes. This is
  // what makes the pop-out follow a drawer change.
  $effect(() => {
    screens.selected;
    simulate.fields;
    renderNow();
  });

  function popOut() {
    // Relative, and named. Both were proven under HA Ingress on 2026-09-09.
    window.open("preview.html", "byonk-preview");
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center gap-2 border-b border-[var(--border-color)] px-2 py-1 text-xs">
    <span class="font-mono">{simulate.size.width} × {simulate.size.height}</span>
    <button type="button" class="ml-auto" onclick={popOut}>Pop out</button>
  </div>
  <div class="flex min-h-0 flex-1 items-center justify-center p-4">
    <img
      alt="Screen preview"
      src={blobUrl}
      class="max-h-full max-w-full object-contain"
    />
  </div>
</div>
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd webui && pnpm test -- preview
```

Expected: PASS, 3 tests.

- [ ] **Step 5: Prove the pop-out follows a change, by hand**

Component tests cannot open a real window. Run `pnpm dev`, open the pop-out,
and change the Panel Profile. Expected: the pop-out's image and its size label
both change. This is the check that caught the equivalent bug before.

Reaching the pop-out from an automated browser: it is **not** in the MCP tab
group. From the parent page, `window.open("", "byonk-preview")` returns the
existing window and is same-origin, so its DOM is readable.

- [ ] **Step 6: Commit**

```bash
git add webui/src/lib/components/PreviewPane.svelte webui/test/preview.test.js
git diff --cached --name-only
git commit -m "feat(webui): preview pane with the proven pop-out protocol"
```

---

### Task 9a: `reload_config` re-applies the add-on options overlay

> Tasks 9a–9g were added by the **2026-09-10 colour-tuning amendment**. Their
> binding spec is `docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md`.
> Read it alongside the parent spec. 9a–9d are the only server work in this
> plan; 9e–9g are Svelte.

**Files:**
- Modify: `src/server.rs` (`reload_config`, around line 249), `src/main.rs` (around line 889)
- Test: `tests/addon_reload_test.rs` (create)

**Interfaces:**
- Consumes: `crate::addon_options::{options_path, read_options, apply_to_config}` — all already public.
- Produces: `pub fn load_config_with_addon_overlay(loader: &AssetLoader) -> anyhow::Result<AppConfig>` in `src/server.rs`. Task 9d relies on `reload_config` applying the overlay; nothing else calls the new function directly except `main.rs`.

**Why this is here.** `apply_to_config` folds `/data/options.json` onto the
parsed config and runs **once, at startup** (`src/main.rs:889`).
`reload_config` (`src/server.rs:249`) reparses `config.yaml` and swaps it in
without re-applying it. So under the Home Assistant add-on, any config write
resets `screen_repos`, `auth_mode` and `screen_repo_refresh_interval` to the
file's values and discards what the Options form set, until the next restart.

This is pre-existing — admin device writes already do it, and the HA
integration writes devices. It is fixed **before** Task 9d because 9d adds a
`Save to config` button inside the dev UI that HA users reach from the sidebar.
Spec §6.

`state.admin_token` is unaffected either way: it is resolved once into an
`AppState` field (`src/server.rs:187`) and is not re-read per request. Do not
"fix" that too.

- [ ] **Step 1: Write the failing test**

`tests/addon_reload_test.rs`:

```rust
//! `reload_config` must re-apply the Home Assistant add-on options overlay.
//!
//! Before this fix it reparsed `config.yaml` and swapped it in bare, so any
//! config write silently discarded what the add-on Options form had set.

mod common;

use common::TestApp;

/// `BYONK_OPTIONS_FILE` is process-wide. Cargo runs the tests in a binary on
/// parallel threads, so serialise every test that sets it — the same idiom
/// `tests/addon_options_test.rs` already uses for the dev-credential vars.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn a_config_write_re_applies_the_addon_overlay() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();

    // options.json carries a value that config.yaml does NOT.
    let options = dir.path().join("options.json");
    std::fs::write(
        &options,
        r#"{"admin_token":"secret","auth_mode":"ed25519","screen_repos":[]}"#,
    )
    .unwrap();
    std::env::set_var("BYONK_OPTIONS_FILE", &options);

    // The app starts WITHOUT the overlay (TestApp does not apply it), so the
    // live config here says `api_key` — whatever config.yaml says.
    let (app, config_path) = TestApp::new_admin_with_file("secret", dir.path());

    // Any admin write goes through `persist` -> `reload_config`.
    let r = app
        .post_json(
            "/api/admin/devices",
            &[("Authorization", "Bearer secret")],
            r#"{"key":"AA:BB:CC:DD:EE:FF","screen":"byonk-builtin/default"}"#,
        )
        .await;
    assert!(
        r.status().is_success(),
        "device write failed: {} {}",
        r.status(),
        r.text()
    );

    // After the reload the overlay must be back on.
    let cfg = app
        .get_with_headers("/api/admin/config", &[("Authorization", "Bearer secret")])
        .await;
    let body: serde_json::Value = cfg.json();
    assert_eq!(
        body["auth_mode"], "ed25519",
        "reload_config dropped the add-on overlay; body was {body}"
    );

    // The file itself is untouched by the overlay — it is an in-memory fold.
    let on_disk = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        !on_disk.contains("ed25519"),
        "the overlay must never be written back to config.yaml"
    );

    std::env::remove_var("BYONK_OPTIONS_FILE");
}
```

**Verify two shapes before you run it**, as this plan's global constraints
require: that `TestApp::new_admin_with_file` writes a `config.yaml` whose
`auth_mode` is absent or `api_key`, and that `GET /api/admin/config` returns
`auth_mode` at the top level. If either differs, adjust the assertion to the
real shape and say so in your report — do not adjust the *behaviour* being
asserted.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test addon_reload_test -- --nocapture`
Expected: FAIL on the `auth_mode` assertion, reporting `"api_key"`.

- [ ] **Step 3: Share one config-loading path between startup and reload**

In `src/server.rs`, add the function and use it in `reload_config`:

```rust
/// Load `config.yaml` and fold the Home Assistant add-on options over it.
///
/// Startup and reload must produce the same config from the same inputs, so
/// both go through here. `read_options` returns `Missing` when there is no
/// options file — the normal non-add-on case — and `apply_to_config` is then
/// a no-op, so this is safe to call unconditionally.
pub fn load_config_with_addon_overlay(loader: &AssetLoader) -> anyhow::Result<AppConfig> {
    let mut config = AppConfig::load_from_assets(loader)?;
    let opts = crate::addon_options::read_options(&crate::addon_options::options_path());
    crate::addon_options::apply_to_config(&opts, &mut config);
    Ok(config)
}
```

and in `reload_config`, replace the first line:

```rust
pub fn reload_config(state: &AppState) -> anyhow::Result<()> {
    let fresh = load_config_with_addon_overlay(&state.asset_loader)?;
    state.config.store(Arc::new(fresh));
    state.screen_repo_manager.rebuild_loader();
    state.preview_cache.clear();
    Ok(())
}
```

Use the `AssetLoader` type name and import path the file already uses; do not
add a new import if one is in scope.

- [ ] **Step 4: Use the same path at startup**

In `src/main.rs`, replace the two lines around 888-889:

```rust
    let mut config = byonk::models::AppConfig::load_from_assets(&asset_loader)?;
    byonk::addon_options::apply_to_config(&addon, &mut config);
```

with:

```rust
    // Same loader as `reload_config`, so a reload can never produce a
    // different config from the same files (see `load_config_with_addon_overlay`).
    let config = byonk::server::load_config_with_addon_overlay(&asset_loader)?;
```

`addon` is still needed on the next line for `state.addon_mode`; leave that
read of it alone. If dropping `mut` makes the compiler complain about a later
mutation, keep `mut` rather than restructuring.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --test addon_reload_test -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Run the full verification**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all clean. `tests/addon_options_test.rs` must still pass unchanged —
if it does not, the overlay semantics moved and that is a defect, not a test to
update.

- [ ] **Step 7: Commit**

```bash
git add src/server.rs src/main.rs tests/addon_reload_test.rs
git diff --cached --name-only     # must list exactly those three
git commit -m "fix(config): re-apply the add-on options overlay on reload

reload_config reparsed config.yaml and swapped it in without folding
/data/options.json over it, so any config write discarded what the HA
add-on Options form had set until the next restart. Startup and reload
now share one loader, so they cannot disagree."
```

---

### Task 9b: Re-key the colour override from device to panel

**Files:**
- Modify: `src/server.rs` (`DevOverrides`, around line 37), `src/api/display.rs` (panel resolution around 788-806, override lookup at 813), `src/api/dev.rs` (`PanelColorsRequest` at 352, handlers 351-383, routes 868-869)
- Modify: `tests/common/app.rs` (add a `put_json` helper)
- Test: `tests/dev_panel_colors_test.rs` (create)

**Interfaces:**
- Consumes: nothing from 9a.
- Produces:
  - `DevOverrides.panel_colors: Arc<RwLock<HashMap<String /* panel config key */, String>>>` — same field, new key meaning.
  - `pub fn resolve_panel<'a>(config: &'a AppConfig, device_config_panel: Option<&str>, board_header: Option<&str>, model_str: &str) -> (Option<&'a PanelConfig>, Option<String> /* key */, Option<String> /* source label */)` in `src/api/display.rs`.
  - Routes `PUT /dev/api/panel-colors/{panel}` and `DELETE /dev/api/panel-colors/{panel}`, body `{"colors_actual": "..."}` on the PUT.
  - `TestApp::put_json(path, headers, body)`.

**Why.** Tuning is a property of a panel profile, not of one device: two devices
sharing a panel share its measured colours. The route already spells the path
parameter `panel` while the handler binds it to `device` — this makes the code
agree with the URL. Spec §4. The project's standing guidance permits byonk API
changes, and nothing outside the dev UI calls this endpoint.

The override keeps its place in the measured-colours precedence chain —
`SRC_DEV_OVERRIDE` stays ahead of `panel.colors_actual` and the
`Measured-Colors` header. **Only the key changes.**

- [ ] **Step 1: Add the `put_json` test helper**

In `tests/common/app.rs`, beside `patch_json`:

```rust
    pub async fn put_json(&self, path: &str, headers: &[(&str, &str)], body: &str) -> TestResponse {
        let mut builder = Request::put(path).header("Content-Type", "application/json");
        for (n, v) in headers {
            builder = builder.header(*n, *v);
        }
        self.request(builder.body(Body::from(body.to_string())).unwrap())
            .await
    }
```

- [ ] **Step 2: Write the failing test**

`tests/dev_panel_colors_test.rs`:

```rust
//! The dev colour override is keyed by PANEL, so every device using that
//! panel profile sees it — which is the point: a calibration belongs to the
//! hardware, not to one unit of it.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

/// Two devices, one panel profile. `e1004` carries a `match:` so the
/// Board-header route is exercised too.
fn app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
    colors_actual: '#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D'
devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
  hallway:
    screen: byonk-builtin/default
    panel: e1004
  office:
    screen: byonk-builtin/default
";
    TestApp::new_with_config_yaml(yaml, dir)
}

#[tokio::test]
async fn put_and_delete_an_override_by_panel_id() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/e1004",
            &[AUTH],
            r#"{"colors_actual":"#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00994D"}"#,
        )
        .await;
    assert!(r.status().is_success(), "PUT failed: {}", r.status());

    let r = app.delete("/dev/api/panel-colors/e1004", &[AUTH]).await;
    assert!(r.status().is_success(), "DELETE failed: {}", r.status());
}

#[tokio::test]
async fn an_unknown_panel_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = app(dir.path());

    let r = app
        .put_json(
            "/dev/api/panel-colors/nosuchpanel",
            &[AUTH],
            r#"{"colors_actual":"#000000,#FFFFFF"}"#,
        )
        .await;
    assert_eq!(
        r.status(),
        axum::http::StatusCode::NOT_FOUND,
        "an override for a panel that does not exist can never be read back, \
         so it must be refused rather than stored where nothing will find it"
    );
}
```

And a unit test for the resolver, in `src/api/display.rs`'s test module:

```rust
    #[test]
    fn resolve_panel_reports_the_config_key_in_every_branch() {
        // `AppConfig` derives `Deserialize`, so YAML is the shortest honest
        // way to build one — and it exercises the same parse the server uses.
        let config: AppConfig = serde_yaml::from_str(
            "\
panels:
  e1004:
    name: reTerminal E1004
    match: e1004
    colors: '#000000,#FFFFFF'
",
        )
        .expect("fixture config parses");

        // (a) the device names the panel
        let (p, key, src) = resolve_panel(&config, Some("e1004"), None, "");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("device_config:e1004"));

        // (b) the Board header matches
        let (p, key, src) = resolve_panel(&config, None, Some("e1004"), "");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("board_header:e1004"));

        // (c) the Model header matches
        let (p, key, src) = resolve_panel(&config, None, None, "e1004");
        assert!(p.is_some());
        assert_eq!(key.as_deref(), Some("e1004"));
        assert_eq!(src.as_deref(), Some("model_header:e1004"));

        // (d) nothing matches
        let (p, key, src) = resolve_panel(&config, None, None, "unknown");
        assert!(p.is_none());
        assert_eq!(key, None);
        assert_eq!(src, None);
    }
```

- [ ] **Step 3: Run both to verify they fail**

Run: `cargo test --test dev_panel_colors_test` and
`cargo test --lib display::tests::resolve_panel_reports_the_config_key_in_every_branch`
Expected: FAIL — the route does not exist, and `resolve_panel` is not defined.

- [ ] **Step 4: Extract `resolve_panel`**

`src/api/display.rs:788-806` currently inlines the three branches and builds
only a source label. Lift it out verbatim, adding the key as a third return
value. Do not change which branch wins or what the labels say:

```rust
/// Resolve a device's panel profile, returning the profile, its **config key**
/// and a human-readable source label.
///
/// The key is what dev colour overrides are keyed by (`DevOverrides::panel_colors`),
/// so it must be the `config.panels` map key and not the profile's display name.
pub fn resolve_panel<'a>(
    config: &'a AppConfig,
    device_config_panel: Option<&str>,
    board_header: Option<&str>,
    model_str: &str,
) -> (Option<&'a PanelConfig>, Option<String>, Option<String>) {
    if let Some(panel_name) = device_config_panel {
        return match config.get_panel(panel_name) {
            Some(p) => (
                Some(p),
                Some(panel_name.to_string()),
                Some(format!("device_config:{panel_name}")),
            ),
            None => (None, None, None),
        };
    }
    if let Some((name, p)) = board_header.and_then(|b| config.find_panel_for_board(b)) {
        return (
            Some(p),
            Some(name.to_string()),
            Some(format!("board_header:{name}")),
        );
    }
    // Some devices (e.g. reTerminal E1004) report their panel identity in the
    // `Model` header rather than `Board`; fall back to matching against it.
    if let Some((name, p)) = config.find_panel_for_board(model_str) {
        return (
            Some(p),
            Some(name.to_string()),
            Some(format!("model_header:{name}")),
        );
    }
    (None, None, None)
}
```

Check the real return type of `find_panel_for_board` before you write `name.to_string()`
— if it already hands back an owned `String`, drop the conversion.

Replace the inlined block at the call site with:

```rust
    let (panel, panel_key, panel_source) = resolve_panel(
        &config,
        device_config_panel.as_deref(),
        board_header.as_deref(),
        model_str,
    );
```

- [ ] **Step 5: Key the override lookup by panel**

At `src/api/display.rs:809-815`, replace the device-keyed lookup and correct
the comment above it, which currently states the opposite rule:

```rust
    // Resolve measured colors: dev override > panel.colors_actual >
    // Measured-Colors header > None. Dev overrides are keyed by PANEL config
    // key, because a calibration belongs to the panel profile: every device
    // using that profile shares it. (2026-09-10 colour-tuning amendment.)
    let measured_colors_header = headers.get_str("Measured-Colors").map(|s| s.to_string());
    let override_colors = if let Some(ref key) = panel_key {
        dev_overrides.panel_colors.read().await.get(key).cloned()
    } else {
        None
    };
```

Update `DevOverrides`'s doc comment in `src/server.rs:32-41` too: `panel_colors`
is now keyed by panel config key while `dither` and `tuning` stay keyed by
device config key. The current comment says "Both maps are keyed by device
config key" and it is now wrong in two ways — it says *both* of three, and it
is no longer true of `panel_colors`.

- [ ] **Step 6: Replace the two handlers and their routes**

In `src/api/dev.rs`, replace `PanelColorsRequest`, `handle_set_panel_colors`
and `handle_delete_panel_colors`:

```rust
/// Request body for `PUT /dev/api/panel-colors/{panel}`.
#[derive(Debug, Deserialize)]
pub struct PanelColorsRequest {
    pub colors_actual: String,
}

/// Set a temporary colour override for a panel profile (dev colour tuning).
///
/// Keyed by panel, not device: a calibration belongs to the panel profile, so
/// every device using it picks the override up on its next poll. In memory
/// only — `POST .../save` (below) is what makes a calibration durable.
pub async fn handle_set_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelColorsRequest>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    state
        .dev_overrides
        .panel_colors
        .write()
        .await
        .insert(panel, body.colors_actual);
    Ok(StatusCode::OK)
}

/// Remove a panel's temporary colour override, reverting to `config.yaml`.
pub async fn handle_delete_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
) -> StatusCode {
    state.dev_overrides.panel_colors.write().await.remove(&panel);
    StatusCode::OK
}

/// Guard: an override for a panel that does not exist could never be read
/// back, because the lookup is by resolved panel key. Refuse it rather than
/// storing it where nothing will find it.
fn require_known_panel(state: &AppState, panel: &str) -> Result<(), ApiError> {
    if state.config.load().get_panel(panel).is_some() {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}
```

Use whatever `ApiError` import and `NotFound` spelling `src/api/dev.rs` already
uses for its other 404s; do not introduce a second error idiom in the file.

Routes at `src/api/dev.rs:868-869` become:

```rust
        .route(
            "/panel-colors/{panel}",
            put(handle_set_panel_colors).delete(handle_delete_panel_colors),
        )
```

Add `put` to the `axum::routing` import and drop `post` only if nothing else in
the file still uses it.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --test dev_panel_colors_test` then `cargo test --workspace`
Expected: PASS. If any existing test posted to `/dev/api/panel-colors` with a
`device` field, it is asserting the old contract and must be updated to the new
one — say so in your report rather than reverting the change.

- [ ] **Step 8: Commit**

```bash
git add src/server.rs src/api/display.rs src/api/dev.rs \
        tests/common/app.rs tests/dev_panel_colors_test.rs
git diff --cached --name-only
git commit -m "feat(dev): key the colour override by panel, not by device

A calibration belongs to a panel profile: two devices sharing a panel
share its measured colours. The route already spelled the path parameter
\`panel\` while the handler bound it to \`device\` — the code now agrees
with the URL. Panel resolution moves into resolve_panel(), which returns
the config key the override map needs."
```

---

### Task 9c: Tuning sessions hold one device at a 60-second refresh

**Files:**
- Modify: `src/server.rs` (`DevOverrides`), `src/api/display.rs` (refresh-rate answer), `src/api/dev.rs` (two handlers, one route)
- Test: `tests/dev_tuning_session_test.rs` (create)

**Interfaces:**
- Consumes: `require_known_panel` is *not* used here — sessions are keyed by device.
- Produces:
  - `DevOverrides.tuning_sessions: Arc<RwLock<HashMap<String /* device config key */, std::time::Instant>>>`
  - `pub const TUNING_REFRESH_SECS: u32 = 60;` and `pub const TUNING_SESSION_TTL: Duration = Duration::from_secs(30 * 60);` in `src/server.rs`
  - `pub async fn tuning_session_active(overrides: &DevOverrides, device_key: &str) -> bool` in `src/server.rs` — lazily drops a stale entry and reports whether a fresh one remains.
  - Routes `PUT /dev/api/tuning-session/{device}` and `DELETE /dev/api/tuning-session/{device}`.

**Why.** byonk cannot make a device fetch. The device polls and byonk answers
with a `refresh_rate` resolved by `resolve_refresh_rate`
(`src/services/content_pipeline.rs:27`) as script > device override > screen
default. Left alone, a calibration loop runs at whatever cadence the screen
chose, which is minutes. Sixty seconds — not less — because a six-colour panel
needs most of a minute to repaint and you want time to look at the result
before it goes again. Spec §3.

**This deliberately overrides a rule the pipeline states on purpose:** that the
screen wins "because only it knows when its content next changes". During a
tuning session that is false — the thing changing the content is a person with
a slider. So the override is narrow (one device, bounded time) and it is
**logged at `info` with its reason**, so a surprising cadence is explainable
from the log alone.

**Expiry is lazy on purpose.** There is no background task, so there is nothing
to schedule, leak or shut down. A closed lid or a crashed tab must not leave a
battery-powered device repainting every minute until byonk restarts — that is
what the TTL is for, and it is not optional.

- [ ] **Step 1: Write the failing test**

`tests/dev_tuning_session_test.rs`:

```rust
//! A tuning session holds one device at a 60-second refresh so that a colour
//! calibration iterates in a minute rather than in the screen's own cadence.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");

#[tokio::test]
async fn a_session_shortens_that_device_s_refresh_and_ending_it_restores_the_rate() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = common_app(dir.path());

    let mac = "AA:BB:CC:DD:EE:01";
    let api_key = app.register_device(mac).await;

    let baseline = refresh_rate_of(&app, mac, &api_key).await;
    assert_ne!(
        baseline, 60,
        "the fixture screen must not already answer 60s, or this test proves nothing"
    );

    let r = app.put_json("/dev/api/tuning-session/kitchen", &[AUTH], "").await;
    assert!(r.status().is_success(), "PUT session failed: {}", r.status());

    assert_eq!(
        refresh_rate_of(&app, mac, &api_key).await,
        60,
        "a live tuning session must shorten the refresh rate"
    );

    let r = app.delete("/dev/api/tuning-session/kitchen", &[AUTH]).await;
    assert!(r.status().is_success());

    assert_eq!(
        refresh_rate_of(&app, mac, &api_key).await,
        baseline,
        "ending the session must hand the rate straight back"
    );
}

#[tokio::test]
async fn another_device_is_unaffected() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = common_app(dir.path());

    let other = "AA:BB:CC:DD:EE:02";
    let other_key = app.register_device(other).await;
    let baseline = refresh_rate_of(&app, other, &other_key).await;

    app.put_json("/dev/api/tuning-session/kitchen", &[AUTH], "").await;

    assert_eq!(
        refresh_rate_of(&app, other, &other_key).await,
        baseline,
        "a session on one device must not touch another"
    );
}
```

with these two helpers in the same file:

```rust
/// Two devices, keyed by the MACs the tests register.
fn common_app(dir: &std::path::Path) -> (TestApp, std::path::PathBuf) {
    let yaml = "\
admin:
  token: secret
devices:
  kitchen:
    screen: byonk-builtin/default
    refresh: 900
  hallway:
    screen: byonk-builtin/default
    refresh: 900
";
    TestApp::new_with_config_yaml(yaml, dir)
}

async fn refresh_rate_of(app: &TestApp, mac: &str, api_key: &str) -> u32 {
    let headers = common::fixtures::display_headers(mac, api_key);
    let r = app
        .get_with_headers("/api/display", &common::fixtures::as_str_pairs(&headers))
        .await;
    let body: serde_json::Value = r.json();
    body["refresh_rate"]
        .as_u64()
        .unwrap_or_else(|| panic!("no refresh_rate in {body}")) as u32
}
```

**Two things to check against the repo before running this**, per the plan's
global constraints: that a registered MAC maps to the device key `kitchen`
(`tests/api_display_test.rs` shows how registration and the device key relate —
if a registered device does not land on the config key, map the MAC explicitly
in the fixture), and that `refresh: 900` actually reaches the response rather
than being overridden by the builtin screen's own `refresh_rate`. The
`assert_ne!(baseline, 60)` guard exists to catch exactly that: if it trips, the
fixture is wrong, not the feature.

**The TTL needs its own test and cannot be written against wall-clock time.**
Write it as a unit test in `src/server.rs`'s test module, inserting a stale
`Instant` directly:

```rust
    #[tokio::test]
    async fn a_stale_tuning_session_expires_and_is_dropped() {
        let overrides = DevOverrides::default();
        let stale = std::time::Instant::now() - (TUNING_SESSION_TTL + Duration::from_secs(1));
        overrides
            .tuning_sessions
            .write()
            .await
            .insert("kitchen".to_string(), stale);

        assert!(!tuning_session_active(&overrides, "kitchen").await);
        assert!(
            overrides.tuning_sessions.read().await.is_empty(),
            "a stale entry must be removed, not merely ignored — otherwise the \
             map grows for the life of the process"
        );
    }

    #[tokio::test]
    async fn a_fresh_tuning_session_is_active_and_kept() {
        let overrides = DevOverrides::default();
        overrides
            .tuning_sessions
            .write()
            .await
            .insert("kitchen".to_string(), std::time::Instant::now());

        assert!(tuning_session_active(&overrides, "kitchen").await);
        assert!(!overrides.tuning_sessions.read().await.is_empty());
    }
```

`Instant::now() - duration` panics on some platforms if it would underflow;
use `checked_sub` and `unwrap_or(Instant::now())` if the compiler or a CI
target objects.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test dev_tuning_session_test` and `cargo test --lib server::`
Expected: FAIL — no route, and `tuning_sessions` / `tuning_session_active` do
not exist.

- [ ] **Step 3: Add the state and the predicate**

In `src/server.rs`:

```rust
/// Refresh rate answered to a device with a live tuning session, in seconds.
///
/// Sixty, not less: a six-colour panel needs most of a minute to repaint, and
/// the person calibrating it needs time to look at the result before it goes
/// again.
pub const TUNING_REFRESH_SECS: u32 = 60;

/// How long a tuning session survives without being touched.
///
/// The backstop for a closed lid or a crashed tab. A battery-powered device
/// must not be left repainting every minute until byonk restarts.
pub const TUNING_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
```

on `DevOverrides`:

```rust
    /// device_config_key → when the dev UI last touched this tuning session.
    ///
    /// Present and fresh ⇒ `/api/display` answers `TUNING_REFRESH_SECS` for
    /// that device. Expiry is lazy (see `tuning_session_active`), so there is
    /// no background task to schedule, leak or shut down.
    pub tuning_sessions: Arc<RwLock<HashMap<String, Instant>>>,
```

and the predicate:

```rust
/// Is a tuning session live for `device_key`? Drops the entry if it has expired.
pub async fn tuning_session_active(overrides: &DevOverrides, device_key: &str) -> bool {
    let mut sessions = overrides.tuning_sessions.write().await;
    match sessions.get(device_key) {
        Some(touched) if touched.elapsed() < TUNING_SESSION_TTL => true,
        Some(_) => {
            sessions.remove(device_key);
            false
        }
        None => false,
    }
}
```

Import `std::time::{Duration, Instant}` at the top of the file if they are not
already in scope.

- [ ] **Step 4: Answer the shortened rate**

In `src/api/display.rs`, after `refresh_rate` has been resolved and before it
is put on the response — the existing `let refresh_rate = if recovery_active
{ ... }` around line 1226 is the right neighbourhood, and a tuning session must
lose to recovery, not win over it:

```rust
    // A live tuning session shortens this device's cadence so a colour
    // calibration iterates in a minute. This deliberately overrides
    // `resolve_refresh_rate`'s rule that the screen owns its own cadence:
    // during a calibration the thing changing the content is a person with a
    // slider. Narrow (one device), bounded (TUNING_SESSION_TTL) and logged.
    let refresh_rate = match device_entry_key.as_deref() {
        Some(key) if tuning_session_active(&dev_overrides, key).await => {
            tracing::info!(
                device = key,
                refresh_rate = TUNING_REFRESH_SECS,
                "tuning session active — shortening refresh rate"
            );
            TUNING_REFRESH_SECS
        }
        _ => refresh_rate,
    };
```

Confirm the real name of the device-key binding at that point in the function
before you use `device_entry_key` — the earlier resolution block binds it, but
check the name is still in scope there.

- [ ] **Step 5: Add the two handlers and the route**

In `src/api/dev.rs`:

```rust
/// Start or refresh a tuning session for a device.
///
/// Idempotent: a repeat PUT is how the dev UI keeps the session alive while a
/// calibration is in progress.
pub async fn handle_start_tuning_session(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .insert(device, std::time::Instant::now());
    StatusCode::OK
}

/// End a tuning session, handing the device its normal refresh rate back.
pub async fn handle_end_tuning_session(
    State(state): State<AppState>,
    axum::extract::Path(device): axum::extract::Path<String>,
) -> StatusCode {
    state
        .dev_overrides
        .tuning_sessions
        .write()
        .await
        .remove(&device);
    StatusCode::OK
}
```

route:

```rust
        .route(
            "/tuning-session/{device}",
            put(handle_start_tuning_session).delete(handle_end_tuning_session),
        )
```

The session is **not** validated against `config.devices`. A device may be
registered and not yet mapped, and a session on a key nothing resolves to is
inert rather than wrong — unlike a colour override, which would be silently
unreadable.

- [ ] **Step 6: Refresh the session on every override push**

In `handle_set_panel_colors` (Task 9b), the override PUT must also count as
activity — otherwise a long, attentive calibration expires mid-session. But
that handler is keyed by panel and knows no device. So do **not** wire it
there: the client re-PUTs the session instead (Task 9f, `tune.push()`). Add
this comment above `handle_set_panel_colors` so the next reader does not
"fix" the apparent omission:

```rust
/// Note: this does NOT refresh a tuning session. The override is keyed by
/// panel and a session by device, and one panel may have several. The client
/// re-PUTs its own session alongside each push (spec §3).
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test --test dev_tuning_session_test` then `cargo test --workspace`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/server.rs src/api/display.rs src/api/dev.rs tests/dev_tuning_session_test.rs
git diff --cached --name-only
git commit -m "feat(dev): tuning sessions hold one device at a 60s refresh

byonk cannot push, so a colour calibration iterates at whatever cadence
the screen chose — minutes. A session shortens one device to 60s, which
is about what a six-colour panel needs to repaint. Bounded by an explicit
end and a 30-minute idle TTL, expired lazily on read, and logged with its
reason: it overrides a rule content_pipeline states on purpose."
```

---

### Task 9d: `Save to config` — the durable write

**Files:**
- Create: `src/api/config_persist.rs` (the shared `persist` helper)
- Modify: `src/api/mod.rs` (declare the module), `src/api/admin/write.rs` (delete the private `persist`, import the shared one), `src/api/dev.rs` (the save handler and its route)
- Test: `tests/dev_panel_colors_save_test.rs` (create)

**Interfaces:**
- Consumes: `require_known_panel` (Task 9b); `reload_config` with the overlay fix (Task 9a).
- Produces:
  - `pub fn persist(state: &AppState, path: &Path, new_yaml: String) -> Result<(), ApiError>` in `src/api/config_persist.rs` — moved verbatim from `src/api/admin/write.rs:423`.
  - Route `POST /dev/api/panel-colors/{panel}/save`, body `{"colors_actual": "..."}`.

**Why.** A calibration must survive a restart and become the real
configuration, not a dev-session experiment copied out by hand.
`config_writer::set_scalar` (`src/services/config_writer.rs:64`) is exactly the
right tool: comment-preserving, replaces the key in place when it exists, adds
it to the parent mapping when it does not — the common case, since
`colors_actual` is optional. Spec §5.

**It is not blocked in add-on mode.** `require_writable_global`
(`src/api/admin/write.rs:70`) makes global config read-only under the add-on
because the Options form is its sole editor. That argument does not reach
`panels`: the Options schema (`homeassistant/byonk/config.yaml:42`) carries no
panel key at all. **Do not add that guard here** — the dev UI reached from the
Home Assistant sidebar is the main way this will be used.

- [ ] **Step 1: Write the failing test**

`tests/dev_panel_colors_save_test.rs`:

```rust
//! `Save to config` writes a panel's measured colours into config.yaml
//! durably, preserving every comment around them.

mod common;

use common::TestApp;

const AUTH: (&str, &str) = ("Authorization", "Bearer secret");
const TUNED: &str = "#101010,#E8E8E8,#B03030,#D0C040,#3040A0,#00994D";

fn commented_config() -> &'static str {
    "\
# byonk configuration — hand-written, with comments that must survive.
admin:
  token: secret

panels:
  # The reTerminal E1004. Its greens are hand-tuned; see docs.
  e1004:
    name: reTerminal E1004
    colors: '#000000,#FFFFFF,#FF0000,#FFFF00,#0000FF,#00FF00'
  og:
    name: TRMNL OG
    colors: '#000000,#FFFFFF'
    colors_actual: '#1A1A1A,#D8D8D8'   # measured 2026-01-01

devices:
  kitchen:
    screen: byonk-builtin/default
    panel: e1004
"
}

#[tokio::test]
async fn saving_adds_colors_actual_and_keeps_every_comment() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/e1004/save",
            &[AUTH],
            &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
        )
        .await;
    assert!(r.status().is_success(), "save failed: {} {}", r.status(), r.text());

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(yaml.contains(TUNED), "the tuned colours are not in the file:\n{yaml}");
    assert!(
        yaml.contains("hand-tuned; see docs"),
        "a comment was lost:\n{yaml}"
    );
    assert!(
        yaml.contains("# measured 2026-01-01"),
        "an inline comment on a sibling panel was lost:\n{yaml}"
    );
    assert!(
        yaml.contains("# byonk configuration"),
        "the header comment was lost:\n{yaml}"
    );
}

#[tokio::test]
async fn saving_replaces_an_existing_colors_actual_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let (app, config_path) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/og/save",
            &[AUTH],
            r#"{"colors_actual":"#0A0A0A,#F0F0F0"}"#,
        )
        .await;
    assert!(r.status().is_success());

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    assert!(yaml.contains("#0A0A0A,#F0F0F0"));
    assert!(!yaml.contains("#1A1A1A,#D8D8D8"), "the old value is still there:\n{yaml}");
}

#[tokio::test]
async fn saving_clears_the_temporary_override() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    app.put_json(
        "/dev/api/panel-colors/e1004",
        &[AUTH],
        &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
    )
    .await;
    let r = app
        .post_json(
            "/dev/api/panel-colors/e1004/save",
            &[AUTH],
            &format!(r#"{{"colors_actual":"{TUNED}"}}"#),
        )
        .await;
    assert!(r.status().is_success());

    // The override and the config now say the same thing, so the override is
    // dead weight — and a live override that nobody can see is the trap this
    // whole design is built to avoid. Deleting it again must still succeed.
    let r = app.delete("/dev/api/panel-colors/e1004", &[AUTH]).await;
    assert!(r.status().is_success());
}

#[tokio::test]
async fn saving_an_unknown_panel_is_a_404() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _cfg) = TestApp::new_with_config_yaml(commented_config(), dir.path());

    let r = app
        .post_json(
            "/dev/api/panel-colors/nosuchpanel/save",
            &[AUTH],
            r#"{"colors_actual":"#000000,#FFFFFF"}"#,
        )
        .await;
    assert_eq!(r.status(), axum::http::StatusCode::NOT_FOUND);
}
```

`saving_clears_the_temporary_override` is weak as written — a DELETE succeeds
whether or not the key was there. **Strengthen it** by asserting the effect
instead: after the save, `GET /dev/api/screens` (or whichever dev endpoint
serves `PanelInfo`) must report the panel's `colors_actual` as the tuned value.
Find that endpoint and assert on it; if no dev endpoint exposes `PanelInfo`,
say so in your report and leave the weak version with a comment naming the gap.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test dev_panel_colors_save_test`
Expected: FAIL — the route does not exist.

- [ ] **Step 3: Move `persist` into a shared module**

Create `src/api/config_persist.rs` containing the body of
`src/api/admin/write.rs:423-441` **verbatim** — the atomic temp-file write, the
rename, the reload and the roll-back on a failed reparse. Change nothing about
its behaviour; this is a move, not a rewrite:

```rust
//! Atomically replace `config.yaml` and swap the result into the live config.
//!
//! Shared by the admin write handlers and by the dev UI's colour-tuning save.
//! Kept in one place because the roll-back is the interesting part: a patch
//! that produces YAML byonk cannot reparse must leave the file as it was.

use crate::error::ApiError;
use crate::server::{reload_config, AppState};

pub fn persist(state: &AppState, path: &std::path::Path, new_yaml: String) -> Result<(), ApiError> {
    // ... moved verbatim from api/admin/write.rs
}
```

Declare it in `src/api/mod.rs` (`pub mod config_persist;`), delete the private
copy from `write.rs`, and import the shared one there. `cargo test --workspace`
must stay green with no other edit — if an admin test breaks, the move was not
verbatim.

- [ ] **Step 4: Write the save handler**

In `src/api/dev.rs`:

```rust
/// Write a panel's tuned measured colours into `config.yaml`, durably.
///
/// This is stage 2 of colour tuning: stage 1 is the in-memory override above,
/// which lets a calibration reach real hardware without becoming the
/// configuration. Saving makes it the configuration and drops the override,
/// because an override that merely restates the config is invisible weight.
///
/// Deliberately NOT guarded by `require_writable_global`: `panels` is absent
/// from the Home Assistant add-on Options schema, so nothing else owns it, and
/// the sidebar dev UI is the main way this is used. See spec §5.
pub async fn handle_save_panel_colors(
    State(state): State<AppState>,
    axum::extract::Path(panel): axum::extract::Path<String>,
    Json(body): Json<PanelColorsRequest>,
) -> Result<StatusCode, ApiError> {
    require_known_panel(&state, &panel)?;
    let path = require_file_config(&state)?;
    let _guard = state.write_lock.lock().await;

    let yaml = std::fs::read_to_string(&path)
        .map_err(|e| ApiError::Internal(format!("read config: {e}")))?;
    let new_yaml = crate::services::config_writer::set_scalar(
        &yaml,
        &["panels", &panel, "colors_actual"],
        serde_yaml::Value::from(body.colors_actual.as_str()),
    )
    .map_err(|e| ApiError::Internal(format!("patch config: {e}")))?;

    crate::api::config_persist::persist(&state, &path, new_yaml)?;

    // The config now carries the value, so the override would only restate it.
    state.dev_overrides.panel_colors.write().await.remove(&panel);
    Ok(StatusCode::OK)
}
```

`require_file_config` is private to `src/api/admin/write.rs`. Move it into
`config_persist.rs` beside `persist` and re-import it in `write.rs` — it is the
same guard for the same reason (an embedded, read-only config cannot be
written) and both callers need it.

Route:

```rust
        .route("/panel-colors/{panel}/save", post(handle_save_panel_colors))
```

Register it **before** or after `/panel-colors/{panel}` as axum's matcher
requires; run the tests to confirm the two do not shadow each other.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --test dev_panel_colors_save_test` then `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Full verification**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 7: Commit**

```bash
git add src/api/config_persist.rs src/api/mod.rs src/api/admin/write.rs \
        src/api/dev.rs tests/dev_panel_colors_save_test.rs
git diff --cached --name-only
git commit -m "feat(dev): save tuned panel colours to config.yaml

Stage 2 of colour tuning: the in-memory override lets a calibration
reach real hardware without becoming the configuration; this makes it
the configuration. set_scalar keeps every comment, and persist() — now
shared with the admin writers rather than private to them — rolls the
file back if the result will not reparse.

Not guarded by require_writable_global: panels are absent from the HA
add-on Options schema, so nothing else owns them."
```

---

### Task 9e: The fifth panel — layout, icon rail, shell

**Files:**
- Create: `webui/src/lib/components/TunePane.svelte` (placeholder; Task 9g fills it)
- Modify: `webui/src/lib/state/layout.svelte.js`, `webui/src/lib/components/IconRail.svelte`, `webui/src/lib/components/AppShell.svelte`
- Test: `webui/test/layout.test.js`, `webui/test/iconrail.test.js`, `webui/test/appshell.test.js` (all exist — extend them)

**Interfaces:**
- Consumes: `layout` (Task 4), `AppShell`/`IconRail` (Task 5).
- Produces: `"tune"` as a member of `PANELS`; `TunePane.svelte` mounting in the shell.

**Why this task exists separately, and why it is BEFORE Task 10.** Task 10
modifies `AppShell.svelte` too. Landing the layout change first means
`AppShell`, `IconRail` and `layout` are each touched once by this amendment
rather than twice.

Spec §2. **Every rule in the parent spec's §1 holds unchanged** — every
combination of five is legal including none, a hidden panel gives its width to
its neighbours and gets it back, a splitter exists only while both neighbours
are open, the icon carries only the toggle state. Do not add a rule.

`Tune` docks **between the editor and the preview**, taking width from the
right-hand pane. That is deliberate: the parent spec's §1 argues at length that
height is the scarce dimension, because an upright E1004 is 1200×1600 and a
control block under it squeezes the device to a stamp. A column takes the other
axis. It sits next to the preview because that is what the eye compares — the
swatch being dragged against the render it produces.

- [ ] **Step 1: Write the failing tests**

Add to `webui/test/layout.test.js`:

```js
it("carries tune as a fifth panel, closed by default", () => {
  expect(PANELS).toEqual(["files", "editor", "tune", "preview", "simulate"]);
  layout.reset();
  expect(layout.isOpen("tune")).toBe(false);
  expect(layout.isOpen("files")).toBe(true);
});

it("persists and restores tune alongside the other four", () => {
  layout.reset();
  layout.toggle("tune");
  expect(layout.isOpen("tune")).toBe(true);

  const other = new (Object.getPrototypeOf(layout).constructor)();
  other.load();
  expect(other.isOpen("tune")).toBe(true);
});
```

If `Layout` is not exported, export the class so the second test can build a
second instance, or reload the module — do not weaken the test to only check
`localStorage` contents, because that asserts the serialisation and not the
round trip.

Add to `webui/test/iconrail.test.js`:

```js
it("shows one toggle per panel, including tune", () => {
  render(IconRail);
  expect(screen.getAllByRole("button")).toHaveLength(5);
  expect(screen.getByLabelText("Tune")).toBeTruthy();
});

it("toggles tune", async () => {
  layout.reset();
  render(IconRail);
  const btn = screen.getByLabelText("Tune");
  expect(btn.getAttribute("aria-pressed")).toBe("false");
  await fireEvent.click(btn);
  expect(btn.getAttribute("aria-pressed")).toBe("true");
});
```

Add to `webui/test/appshell.test.js`:

```js
it("mounts TunePane only while the tune panel is open", async () => {
  layout.reset();
  render(AppShell);
  expect(screen.queryByTestId("tune-pane")).toBeNull();

  layout.toggle("tune");
  await tick();
  expect(screen.getByTestId("tune-pane")).toBeTruthy();
});

it("keeps the horizontal splitter alive when only tune is open on the right", async () => {
  layout.reset();
  for (const p of ["preview", "simulate"]) if (layout.isOpen(p)) layout.toggle(p);
  layout.toggle("tune");
  await tick();
  // The splitter separates the editor from everything to its right; tune
  // being the only thing there is still something there.
  expect(screen.getByRole("separator", { name: /vertical|horizontal|resize/i })).toBeTruthy();
});
```

Match the existing files' import lists, `render` helper and the accessible name
`SplitPane` actually gives its handle — read `SplitPane.svelte` and
`splitpane.test.js` first rather than guessing the role or name.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd webui && pnpm vitest run test/layout.test.js test/iconrail.test.js test/appshell.test.js`
Expected: FAIL — `PANELS` has four members and `TunePane` does not exist.

- [ ] **Step 3: Add `tune` to the layout store**

In `webui/src/lib/state/layout.svelte.js`:

```js
export const PANELS = ["files", "editor", "tune", "preview", "simulate"];
```

and in `DEFAULTS`:

```js
const DEFAULTS = {
  panels: { files: true, editor: true, tune: false, preview: true, simulate: true },
  sizes: { x: 420, y: 260 },
};
```

`tune` defaults **closed**: the other four are how you write a screen, while
tuning is an occasional calibration and should not occupy width until it is
asked for.

Nothing else in the file changes. `load()` already iterates `PANELS`, and the
persisted record is keyed by name, so reordering the array does not invalidate
a stored layout. Leave `STORAGE_KEY` at `"byonk.layout.v1"` — a reader of an
old record simply finds no `tune` key and takes the default.

- [ ] **Step 4: Add the icon**

In `webui/src/lib/components/IconRail.svelte`, import `Palette` from
`@lucide/svelte` and add one entry to each map:

```js
  import { Files, Code, Palette, Monitor, SlidersHorizontal } from "@lucide/svelte";

  const ICONS = {
    files: Files,
    editor: Code,
    tune: Palette,
    preview: Monitor,
    simulate: SlidersHorizontal,
  };
  const LABELS = {
    files: "Files",
    editor: "Editor",
    tune: "Tune",
    preview: "Preview",
    simulate: "Simulate",
  };
```

The markup is already generic over `PANELS` and needs no change. `Palette`
exists in the pinned `@lucide/svelte` 1.42.0 — **do not move that pin.** If it
does not, pick another icon from the pinned version and say which; never bump
the dependency.

- [ ] **Step 5: Create the placeholder pane**

`webui/src/lib/components/TunePane.svelte`:

```svelte
<!--
  Panel colour tuning. Task 9g fills this in; Task 9e only proves it docks.
-->
<div data-testid="tune-pane" class="flex h-full flex-col gap-3 p-2 text-xs">
  <h2 class="font-medium">Tune</h2>
</div>
```

- [ ] **Step 6: Dock it in the shell**

In `webui/src/lib/components/AppShell.svelte`, import `TunePane`, widen the
horizontal splitter's `open` test, and wrap the right-hand pane:

```svelte
    <SplitPane
      axis="x"
      bind:size={xSize}
      open={layout.isOpen("preview") || layout.isOpen("simulate") || layout.isOpen("tune")}
    >
      {#snippet a()}
        {#if layout.isOpen("editor")}
          <EditorPane />
        {/if}
      {/snippet}

      {#snippet b()}
        <div class="flex min-h-0 min-w-0 flex-1">
          {#if layout.isOpen("tune")}
            <div
              class="w-64 shrink-0 overflow-auto border-r border-[var(--border-color)]"
            >
              <TunePane />
            </div>
          {/if}

          <SplitPane
            axis="y"
            bind:size={ySize}
            open={layout.isOpen("preview") && layout.isOpen("simulate")}
          >
            {#snippet a()}
              {#if layout.isOpen("preview")}<PreviewPane />{/if}
            {/snippet}
            {#snippet b()}
              {#if layout.isOpen("simulate")}<SimulateControls />{/if}
            {/snippet}
          </SplitPane>
        </div>
      {/snippet}
    </SplitPane>
```

The vertical splitter's `open` test is unchanged: it separates preview from
simulate, and tune is neither.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cd webui && pnpm vitest run`
Expected: PASS, whole suite. A failure in `splitpane.test.js` or
`appshell.test.js` that you did not write means the wrapper `div` changed the
flex geometry — fix the geometry, not the test.

- [ ] **Step 8: Commit**

```bash
git add webui/src/lib/state/layout.svelte.js \
        webui/src/lib/components/IconRail.svelte \
        webui/src/lib/components/AppShell.svelte \
        webui/src/lib/components/TunePane.svelte \
        webui/test/layout.test.js webui/test/iconrail.test.js webui/test/appshell.test.js
git diff --cached --name-only
git commit -m "feat(webui): dock a fifth Tune panel between editor and preview

It takes width from the right-hand pane, never height: an upright E1004
is 1200x1600 and a control block under it squeezes the device to a stamp.
Next to the preview because that is what the eye compares. Closed by
default — tuning is an occasional calibration, not part of writing a
screen. Every layout rule from spec 2's section 1 holds unchanged."
```

---

### Task 9f: The `tune` store, and the preview's live working copy

**Files:**
- Create: `webui/src/lib/state/tune.svelte.js`
- Modify: `webui/src/lib/api.js`, `webui/src/lib/state/simulate.svelte.js`
- Test: `webui/test/tune.test.js` (create), `webui/test/simulate.test.js` (extend)

**Interfaces:**
- Consumes: `screens.meta` (Task 6), `simulate` (Task 8).
- Produces:
  - `api.setPanelOverride(panel, colors_actual)`, `api.clearPanelOverride(panel)`, `api.savePanelColors(panel, colors_actual)`, `api.startTuningSession(device)`, `api.endTuningSession(device)`
  - `simulate.colors_actual_override` — `null` or `{panel, colors}`
  - the `tune` store: `tune.panel`, `tune.device`, `tune.colors` (array), `tune.selected` (index), `tune.baseline` (array), `tune.pushed` (boolean), `tune.error`, and the methods `selectPanel(id)`, `selectDevice(key)`, `setColor(i, hex)`, `push()`, `save()`, `discard()`, `reset()`
- **Task 10 must render `tune.error` and show `tune.pushed` in `StatusBar`.** See the amendment to Task 10, Step 5.

**Why the field lives on `simulate`.** `simulate.url()` already puts
`colors_actual` on the render query whenever a panel is chosen
(`webui/src/lib/state/simulate.svelte.js:203-209`), and the server honours it.
So the preview follows a slider **with no server call at all** — which is the
whole reason stages 1 and 2 can be separate.

The field is parked on `simulate` and not on `tune` because `tune` must read
`simulate` to default its panel selector, and the reverse import would close a
cycle between two rune modules that both evaluate at page load. One direction
only. This follows the precedent already set for `simulate.error` (Ruling 43):
the field lives on the store the render path reads, not on the store that
conceptually owns it.

**Do not** mutate `colors_actual` on the object inside `screens.meta.panels` —
which is what the old UI did to its `<option>` dataset. That edits shared
metadata other components read as the configured truth, so a tuning session in
progress would make the Simulate panel's swatches lie about what is in
`config.yaml`.

- [ ] **Step 1: Write the failing tests**

`webui/test/tune.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { tune } from "../src/lib/state/tune.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import * as api from "../src/lib/api.js";

const PANEL = { id: "e1004", name: "E1004", colors_actual: "#101010,#E8E8E8,#00FF00" };

beforeEach(() => {
  vi.restoreAllMocks();
  tune.reset();
  simulate.reset();
});

describe("tune store", () => {
  it("loads a panel's configured colors as the working copy and the baseline", () => {
    tune.selectPanel(PANEL.id, PANEL);
    expect(tune.colors).toEqual(["#101010", "#E8E8E8", "#00FF00"]);
    expect(tune.baseline).toEqual(["#101010", "#E8E8E8", "#00FF00"]);
    expect(tune.dirty).toBe(false);
  });

  it("drives the render query without any fetch", () => {
    const spy = vi.spyOn(globalThis, "fetch");
    tune.selectPanel(PANEL.id, PANEL);
    tune.setColor(2, "#00994D");

    expect(simulate.colors_actual_override).toEqual({
      panel: "e1004",
      colors: "#101010,#E8E8E8,#00994D",
    });
    expect(tune.dirty).toBe(true);
    expect(spy).not.toHaveBeenCalled();
  });

  it("starts a tuning session when a device is selected and ends it on deselect", async () => {
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue();

    await tune.selectDevice("kitchen");
    expect(start).toHaveBeenCalledWith("kitchen");

    await tune.selectDevice("");
    expect(end).toHaveBeenCalledWith("kitchen");
  });

  it("push sends the override AND re-touches the session", async () => {
    const put = vi.spyOn(api, "setPanelOverride").mockResolvedValue();
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    tune.selectPanel(PANEL.id, PANEL);
    await tune.selectDevice("kitchen");
    start.mockClear();
    tune.setColor(2, "#00994D");

    await tune.push();

    expect(put).toHaveBeenCalledWith("e1004", "#101010,#E8E8E8,#00994D");
    expect(start).toHaveBeenCalledWith("kitchen");
    expect(tune.pushed).toBe(true);
  });

  it("discard clears the override, ends the session and reverts the working copy", async () => {
    vi.spyOn(api, "setPanelOverride").mockResolvedValue();
    vi.spyOn(api, "startTuningSession").mockResolvedValue();
    const del = vi.spyOn(api, "clearPanelOverride").mockResolvedValue();
    const end = vi.spyOn(api, "endTuningSession").mockResolvedValue();

    tune.selectPanel(PANEL.id, PANEL);
    await tune.selectDevice("kitchen");
    tune.setColor(2, "#00994D");
    await tune.push();

    await tune.discard();

    expect(del).toHaveBeenCalledWith("e1004");
    expect(end).toHaveBeenCalledWith("kitchen");
    expect(tune.colors).toEqual(tune.baseline);
    expect(tune.pushed).toBe(false);
    expect(simulate.colors_actual_override).toBeNull();
  });

  it("save writes, refreshes metadata, and stops reading as dirty", async () => {
    const save = vi.spyOn(api, "savePanelColors").mockResolvedValue();
    vi.spyOn(api, "endTuningSession").mockResolvedValue();
    tune.selectPanel(PANEL.id, PANEL);
    tune.setColor(2, "#00994D");

    await tune.save();

    expect(save).toHaveBeenCalledWith("e1004", "#101010,#E8E8E8,#00994D");
    expect(tune.baseline).toEqual(["#101010", "#E8E8E8", "#00994D"]);
    expect(tune.dirty).toBe(false);
    expect(tune.pushed).toBe(false);
  });

  it("records a failure instead of letting it reject unhandled", async () => {
    vi.spyOn(api, "setPanelOverride").mockRejectedValue(new Error("boom"));
    tune.selectPanel(PANEL.id, PANEL);

    await expect(tune.push()).resolves.toBeUndefined();
    expect(tune.error).toMatchObject({ op: "push" });
    expect(tune.pushed).toBe(false);
  });
});
```

Add to `webui/test/simulate.test.js`:

```js
it("prefers a matching colors_actual_override in the render query", () => {
  simulate.panel = "e1004";
  simulate.use_actual = true;
  simulate.colors_actual_override = { panel: "e1004", colors: "#111111,#EEEEEE" };
  expect(simulate.url("local/a")).toContain(encodeURIComponent("#111111,#EEEEEE"));
});

it("ignores an override that names a different panel", () => {
  simulate.panel = "e1004";
  simulate.use_actual = true;
  simulate.colors_actual_override = { panel: "og", colors: "#111111,#EEEEEE" };
  const url = simulate.url("local/a");
  expect(url).not.toContain(encodeURIComponent("#111111,#EEEEEE"));
});

it("falls back to the configured colors once the override is cleared", () => {
  simulate.panel = "e1004";
  simulate.use_actual = true;
  simulate.colors_actual_override = { panel: "e1004", colors: "#111111,#EEEEEE" };
  simulate.colors_actual_override = null;
  const url = simulate.url("local/a");
  expect(url).not.toContain(encodeURIComponent("#111111,#EEEEEE"));
});
```

The simulate tests need `screens.meta.panels` to contain an `e1004` entry with
a `colors_actual`, exactly as `simulate.test.js`'s existing panel tests set it
up — reuse that fixture, do not build a second one.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd webui && pnpm vitest run test/tune.test.js test/simulate.test.js`
Expected: FAIL — the module and the field do not exist.

- [ ] **Step 3: Add the five API calls**

In `webui/src/lib/api.js`, beside the existing helpers. **Every path is
relative** — under Ingress the prefix is unknowable:

```js
export const setPanelOverride = (panel, colors_actual) =>
  put(`api/panel-colors/${encodeURIComponent(panel)}`, { colors_actual });
export const clearPanelOverride = (panel) =>
  del(`api/panel-colors/${encodeURIComponent(panel)}`);
export const savePanelColors = (panel, colors_actual) =>
  post(`api/panel-colors/${encodeURIComponent(panel)}/save`, { colors_actual });
export const startTuningSession = (device) =>
  put(`api/tuning-session/${encodeURIComponent(device)}`);
export const endTuningSession = (device) =>
  del(`api/tuning-session/${encodeURIComponent(device)}`);
```

`post` exists. `put` and `del` may not — read the file first and add whichever
is missing in the same shape as `post`, raising the same error types on a
non-OK status so the store's error handling is uniform.

- [ ] **Step 4: Add the field to `simulate`**

In `webui/src/lib/state/simulate.svelte.js`, add the field beside the other
`$state` declarations:

```js
  /**
   * A live colour-tuning working copy, or `null`.
   *
   * `{panel, colors}` — the panel it belongs to, and the comma-separated
   * `colors_actual` string. Written by the `tune` store, read only by `url()`
   * below. It lives here rather than on `tune` because `tune` imports this
   * module for its default panel, and the reverse import would close a cycle
   * between two rune modules that both evaluate at page load. Same reasoning
   * as `error` (Ruling 43).
   */
  colors_actual_override = $state(null);
```

Add `colors_actual_override: null` to `DEFAULTS` and to `reset()`, so a reset
cannot leave a stale working copy steering the render.

In `url()`, replace the `colors_actual` line:

```js
      const p = this.#selectedPanel();
      // A live tuning working copy wins, but only for the panel it belongs to
      // — otherwise switching panels mid-calibration would render the new
      // panel with the old panel's colours.
      const ov = this.colors_actual_override;
      if (ov && ov.panel === this.panel) {
        query.colors_actual = ov.colors;
      } else if (p && p.colors_actual) {
        query.colors_actual = p.colors_actual;
      }
```

Leave rule (c) — that `colors_actual` travels only when a panel is chosen —
exactly as it is. This code is already inside `if (this.panel)`.

- [ ] **Step 5: Write the `tune` store**

`webui/src/lib/state/tune.svelte.js`:

```js
// Panel colour tuning: a working copy, a temporary override, and a durable save.
//
// Two stages, because e-ink is reflective: a calibration is only valid under
// the light the panel hangs in, so it must be able to reach real hardware
// (stage 1, `push`) before it becomes the configuration (stage 2, `save`).
//
// State lives here and not in `TunePane` so that closing the panel — which
// unmounts the component and clears the server-side override — does not throw
// away a half-finished calibration.

import * as api from "../api.js";
import { simulate } from "./simulate.svelte.js";
import { screens } from "./screens.svelte.js";

const split = (s) => (s ? s.split(",").map((c) => c.trim()).filter(Boolean) : []);

class Tune {
  panel = $state("");
  device = $state("");
  colors = $state([]);
  baseline = $state([]);
  selected = $state(0);
  pushed = $state(false);
  /** `null` or `{op: "push" | "save" | "discard" | "session", message}`. */
  error = $state(null);

  get dirty() {
    return this.colors.join(",") !== this.baseline.join(",");
  }

  /** The comma-separated wire form of the working copy. */
  get wire() {
    return this.colors.join(",");
  }

  selectPanel(id, panelInfo = null) {
    const info = panelInfo ?? (screens.meta.panels ?? []).find((p) => p.id === id) ?? null;
    this.panel = id;
    // Fall back to the official colours when a panel has never been measured:
    // tuning from the nominal palette is the normal first calibration.
    const from = split(info?.colors_actual) .length
      ? split(info.colors_actual)
      : split(info?.colors);
    this.colors = [...from];
    this.baseline = [...from];
    this.selected = 0;
    this.pushed = false;
    this.#mirror();
  }

  async selectDevice(key) {
    const previous = this.device;
    if (previous && previous !== key) await this.#endSession(previous);
    this.device = key;
    if (!key) return;
    try {
      await api.startTuningSession(key);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  setColor(i, hex) {
    if (i < 0 || i >= this.colors.length) return;
    const next = [...this.colors];
    next[i] = hex;
    this.colors = next;
    this.#mirror();
  }

  async push() {
    try {
      await api.setPanelOverride(this.panel, this.wire);
      this.pushed = true;
      this.error = null;
      // The override is keyed by panel and the session by device, so the
      // server cannot refresh the session from the push. The client does it.
      if (this.device) await api.startTuningSession(this.device);
    } catch (e) {
      this.pushed = false;
      this.error = { op: "push", message: e.message };
    }
  }

  async save() {
    try {
      await api.savePanelColors(this.panel, this.wire);
      this.baseline = [...this.colors];
      this.pushed = false;
      this.error = null;
      // The panel list the client holds now understates the truth.
      await screens.loadMeta();
      await this.#endSession(this.device);
      this.device = "";
    } catch (e) {
      this.error = { op: "save", message: e.message };
    }
  }

  async discard() {
    try {
      await api.clearPanelOverride(this.panel);
      this.error = null;
    } catch (e) {
      this.error = { op: "discard", message: e.message };
    }
    this.colors = [...this.baseline];
    this.pushed = false;
    this.#mirror();
    await this.#endSession(this.device);
    this.device = "";
  }

  /**
   * Give up the server-side state without touching the working copy.
   *
   * Called when the Tune panel closes. An override that nobody is watching is
   * a trap: it silently changes what every device using that panel renders,
   * with nothing on screen to say so.
   */
  async release() {
    if (this.pushed) {
      try {
        await api.clearPanelOverride(this.panel);
      } catch {
        // Best effort. A failure here leaves an override live, which the
        // status bar still reports; it must not throw out of an unmount.
      }
      this.pushed = false;
    }
    await this.#endSession(this.device);
    this.device = "";
    simulate.colors_actual_override = null;
  }

  reset() {
    this.panel = "";
    this.device = "";
    this.colors = [];
    this.baseline = [];
    this.selected = 0;
    this.pushed = false;
    this.error = null;
    simulate.colors_actual_override = null;
  }

  async #endSession(key) {
    if (!key) return;
    try {
      await api.endTuningSession(key);
    } catch (e) {
      this.error = { op: "session", message: e.message };
    }
  }

  #mirror() {
    simulate.colors_actual_override = this.colors.length
      ? { panel: this.panel, colors: this.wire }
      : null;
  }
}

export const tune = new Tune();
```

Check `screens`'s real metadata-reload method name before you write
`screens.loadMeta()` — Task 6 named it, and this plan may have it wrong.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd webui && pnpm vitest run`
Expected: PASS, whole suite.

- [ ] **Step 7: Commit**

```bash
git add webui/src/lib/state/tune.svelte.js webui/src/lib/api.js \
        webui/src/lib/state/simulate.svelte.js \
        webui/test/tune.test.js webui/test/simulate.test.js
git diff --cached --name-only
git commit -m "feat(webui): tune store, and a live working copy for the preview

The preview follows a slider with no server call: simulate.url() already
sends colors_actual, so the working copy just feeds it. The field is
parked on simulate because tune imports simulate for its default panel
and the reverse would close a module cycle — the same reasoning as
simulate.error."
```

---

### Task 9g: `TunePane`

**Files:**
- Modify: `webui/src/lib/components/TunePane.svelte` (replace the Task 9e placeholder)
- Create: `webui/src/lib/utils/color.js`
- Test: `webui/test/tunepane.test.js` (create)

**Interfaces:**
- Consumes: `tune` (Task 9f), `screens.meta` (Task 6), `simulate` (Task 8).
- Produces: nothing later tasks read. `Task 10` reads `tune.error` and `tune.pushed` from the store, not from this component.

Spec §8.

- [ ] **Step 1: Write the failing test**

`webui/test/tunepane.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import TunePane from "../src/lib/components/TunePane.svelte";
import { tune } from "../src/lib/state/tune.svelte.js";
import { screens } from "../src/lib/state/screens.svelte.js";
import { simulate } from "../src/lib/state/simulate.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  tune.reset();
  simulate.reset();
  screens.meta.panels = [
    { id: "e1004", name: "E1004", colors: "#000000,#FFFFFF,#00FF00",
      colors_actual: "#101010,#E8E8E8,#00994D" },
    { id: "og", name: "OG", colors: "#000000,#FFFFFF" },
  ];
  screens.meta.devices = [
    { id: "kitchen", screen: "local/a", panel: "e1004" },
    { id: "attic", screen: "local/a", panel: null },
  ];
});

describe("TunePane", () => {
  it("shows one swatch per colour once a panel is chosen", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    expect(screen.getAllByRole("radio")).toHaveLength(3);
  });

  it("selecting a device sets the panel and starts a session", async () => {
    const start = vi.spyOn(api, "startTuningSession").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Device"), { target: { value: "kitchen" } });
    await tick();
    expect(start).toHaveBeenCalledWith("kitchen");
    expect(tune.panel).toBe("e1004");
  });

  it("says so when a device's panel is not known from config", async () => {
    vi.spyOn(api, "startTuningSession").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Device"), { target: { value: "attic" } });
    await tick();
    expect(screen.getByText(/pick the panel/i)).toBeTruthy();
  });

  it("a slider drag changes the selected swatch and the working copy", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    await fireEvent.click(screen.getAllByRole("radio")[2]);
    await fireEvent.input(screen.getByLabelText("Lightness"), { target: { value: "20" } });
    await tick();
    expect(tune.colors[2]).not.toBe("#00994D");
    expect(tune.dirty).toBe(true);
  });

  it("a hex entry that is not a colour changes nothing", async () => {
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });
    await fireEvent.click(screen.getAllByRole("radio")[0]);
    await fireEvent.input(screen.getByLabelText("Hex"), { target: { value: "nope" } });
    await tick();
    expect(tune.colors[0]).toBe("#101010");
  });

  it("Push, Save and Discard call the store", async () => {
    const push = vi.spyOn(tune, "push").mockResolvedValue();
    const save = vi.spyOn(tune, "save").mockResolvedValue();
    const discard = vi.spyOn(tune, "discard").mockResolvedValue();
    render(TunePane);
    await fireEvent.change(screen.getByLabelText("Panel"), { target: { value: "e1004" } });

    await fireEvent.click(screen.getByRole("button", { name: /push to panel/i }));
    await fireEvent.click(screen.getByRole("button", { name: /save to config/i }));
    await fireEvent.click(screen.getByRole("button", { name: /discard/i }));

    expect(push).toHaveBeenCalled();
    expect(save).toHaveBeenCalled();
    expect(discard).toHaveBeenCalled();
  });

  it("releases the server-side override when it unmounts", async () => {
    const release = vi.spyOn(tune, "release").mockResolvedValue();
    const { unmount } = render(TunePane);
    unmount();
    expect(release).toHaveBeenCalled();
  });

  it("round-trips a colour through hex and HSL without drift", () => {
    // Guards the ported maths: the old UI's sliders write HSL and the hex box
    // writes hex, so a value that changes when it merely passes through both
    // makes every swatch creep as the user works.
    for (const hex of ["#000000", "#FFFFFF", "#00994D", "#B03030", "#3040A0"]) {
      expect(hslToHex(...Object.values(hexToHsl(hex))).toUpperCase()).toBe(hex.toUpperCase());
    }
  });
});
```

Import `hexToHsl` / `hslToHex` from the new `color.js` in that last test.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd webui && pnpm vitest run test/tunepane.test.js`
Expected: FAIL — the placeholder has no controls.

- [ ] **Step 3: Port the colour maths**

`webui/src/lib/utils/color.js` — port `hexToHsl` and `hslToHex` from
`static/dev/dev.js:1227-1237` **verbatim in behaviour**, as ES module exports
with JSDoc. That file is deleted by Task 12, so this is the surviving copy.

The round-trip test above will fail if the port rounds differently from the
original. If it does, fix the port, not the test: a colour that drifts each
time it passes through both directions makes every swatch creep as the user
works.

- [ ] **Step 4: Write `TunePane`**

Replace `webui/src/lib/components/TunePane.svelte`. It holds, top to bottom:

- a **Panel** `<select>` over `screens.meta.panels`, defaulting to
  `simulate.panel`, calling `tune.selectPanel(id)`;
- a **Device** `<select>` over `screens.meta.devices`, calling
  `tune.selectDevice(key)`; when the chosen device's `panel` is non-null it
  also calls `tune.selectPanel` with it, and when it is null it shows
  "This device's panel is not set in config — pick the panel above.";
- the **swatch row** for `tune.colors`, as a `radiogroup` so exactly one is
  selected and it is reachable by keyboard;
- **Hue / Saturation / Lightness** `<input type="range">` and a **Hex**
  `<input type="text">`, all driving `tune.setColor(tune.selected, …)`. An
  entry that does not match `/^#?[0-9A-Fa-f]{6}$/` changes nothing;
- an **original / live** pair — `tune.baseline[tune.selected]` beside
  `tune.colors[tune.selected]` — so the change reads as a difference and not
  only as an absolute;
- **`[Push to panel]`**, **`[Save to config]`**, **`[Discard]`**.
  `Push` is disabled while `!tune.panel`; `Save` while `!tune.dirty`;
  `Discard` while `!tune.pushed && !tune.dirty`.

Use the existing `ui/` primitives (`Label`, `Input`, `Button`) and the plain
`<select>` idiom `SimulateControls.svelte` already uses — do not vendor a new
shadcn component, and add no dependency.

`Push` is a **button, never automatic.** The old popup pushed on a 300 ms
debounce (`dev.js:1311-1340`). That is wrong now, because a push has a physical
side effect: it drives a battery-powered device into a 60-second refresh cycle.
Dragging a slider must not do that.

Release on unmount:

```svelte
  import { onDestroy } from "svelte";
  // Closing the panel gives up the server-side override and the session. The
  // working copy survives, because it lives in the store.
  onDestroy(() => { tune.release(); });
```

- [ ] **Step 5: Run the whole suite**

Run: `cd webui && pnpm vitest run`
Expected: PASS.

- [ ] **Step 6: Full verification**

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd docs && mdbook build)
make webui
```

- [ ] **Step 7: Commit**

```bash
git add webui/src/lib/components/TunePane.svelte webui/src/lib/utils/color.js \
        webui/test/tunepane.test.js
git diff --cached --name-only
git commit -m "feat(webui): TunePane — swatches, HSL and hex, push and save

Push is a button and not a debounce: it drives a battery-powered device
into a 60s refresh, so dragging a slider must not trigger it. Closing the
panel releases the server-side override, because an override nobody is
watching silently changes what every device on that panel renders."
```

---
### Task 10: Conflict handling, `ConflictBanner`, `StatusBar`

**Files:**
- Create: `webui/src/lib/components/ConflictBanner.svelte`, `webui/src/lib/components/StatusBar.svelte`
- Modify: `webui/src/lib/components/AppShell.svelte`, `webui/src/lib/state/editor.svelte.js`
- Test: `webui/test/conflict.test.js`

**Interfaces:**
- Consumes: `editor` (Task 7), `api.eventsUrl` (Task 3).
- Produces: `editor.conflict` — `null` or `{ref, name, source: "save" | "watch"}`; `editor.resolveReload()`; `editor.resolveOverwrite()`; the `EventSource` subscription.

Spec §8.5: the banner must appear on **both** triggers — the 412 from a save
and the SSE `file-change` — and Reload must replace the buffer **and clear
dirty**.

Two facts from the previous session's testing:

- The SSE conflict path is only reachable through `SCREENS_DIR`. The watcher
  watches nothing else, so an external edit inside `examples` fires no event.
  Put a screen in `SCREENS_DIR` to exercise it by hand.
- **Overwrite has never been exercised.** Reload is verified both ways;
  Overwrite is not. Test it here, and try it by hand in Task 14.

- [ ] **Step 1: Write the failing test**

`webui/test/conflict.test.js`:

```js
import { describe, it, expect, vi, beforeEach } from "vitest";
import { editor } from "../src/lib/state/editor.svelte.js";
import * as api from "../src/lib/api.js";

beforeEach(() => {
  vi.restoreAllMocks();
  editor.reset();
});

async function openDirty() {
  vi.spyOn(api, "readFile").mockResolvedValue({ text: "disk", etag: '"1"' });
  await editor.open("local/a", "script.lua");
  editor.setText("mine");
}

describe("conflict handling", () => {
  it("raises the banner when a save returns 412", async () => {
    await openDirty();
    vi.spyOn(api, "writeFile").mockRejectedValue(new api.ConflictError());
    await editor.trySave();
    expect(editor.conflict).toMatchObject({
      ref: "local/a",
      name: "script.lua",
      source: "save",
    });
  });

  it("raises the banner when the watcher reports a file-change", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });
    expect(editor.conflict).toMatchObject({ source: "watch" });
  });

  it("ignores a file-change for a file that is not open", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/other", name: "script.lua" });
    expect(editor.conflict).toBeNull();
  });

  it("Reload replaces the buffer and clears dirty", async () => {
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    await editor.resolveReload();

    expect(editor.active.text).toBe("theirs");
    expect(editor.active.dirty).toBe(false);
    expect(editor.conflict).toBeNull();
  });

  it("Overwrite keeps my text and wins with a fresh etag", async () => {
    // Never exercised before this test. Overwrite must re-read only the etag,
    // not the text -- re-reading the text would discard the edit it exists to
    // preserve.
    await openDirty();
    editor.onFileChange({ ref: "local/a", name: "script.lua" });

    vi.spyOn(api, "readFile").mockResolvedValue({ text: "theirs", etag: '"2"' });
    const write = vi.spyOn(api, "writeFile").mockResolvedValue({ etag: '"3"' });
    await editor.resolveOverwrite();

    expect(write.mock.calls[0][2]).toBe("mine");
    expect(write.mock.calls[0][3]).toBe('"2"');
    expect(editor.active.text).toBe("mine");
    expect(editor.active.dirty).toBe(false);
    expect(editor.conflict).toBeNull();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd webui && pnpm test -- conflict
```

Expected: FAIL — `trySave`, `onFileChange`, `conflict`, `resolveReload` and
`resolveOverwrite` do not exist.

- [ ] **Step 3: Extend the editor store**

Add to `webui/src/lib/state/editor.svelte.js`:

```js
  conflict = $state(null);

  /** save() that turns a 412 into the banner instead of an exception. */
  async trySave() {
    const t = this.active;
    if (!t) return;
    try {
      await this.save();
    } catch (e) {
      if (e instanceof api.ConflictError) {
        this.conflict = { ref: t.ref, name: t.name, source: "save" };
        return;
      }
      throw e;
    }
  }

  /**
   * The watcher reported a change. Only files we have open matter -- the
   * watcher reports everything under SCREENS_DIR.
   */
  onFileChange({ ref, name }) {
    const open = this.tabs.some((t) => t.ref === ref && t.name === name);
    if (open) this.conflict = { ref, name, source: "watch" };
  }

  async resolveReload() {
    const c = this.conflict;
    if (!c) return;
    await this.replaceFromDisk(c.ref, c.name);
    this.conflict = null;
  }

  /**
   * Keep my text and win.
   *
   * Re-read the file for its ETAG ONLY. Taking its text as well would throw
   * away the edit this button exists to preserve.
   */
  async resolveOverwrite() {
    const c = this.conflict;
    if (!c) return;
    const t = this.tabs.find((x) => x.ref === c.ref && x.name === c.name);
    if (!t) {
      this.conflict = null;
      return;
    }
    const { etag } = await api.readFile(c.ref, c.name);
    const res = await api.writeFile(c.ref, c.name, t.text, etag);
    t.etag = res.etag;
    t.saved = t.text;
    this.conflict = null;
  }
```

- [ ] **Step 4: Write `ConflictBanner` and subscribe to the events stream**

`webui/src/lib/components/ConflictBanner.svelte`:

```svelte
<script>
  import { editor } from "../state/editor.svelte.js";
  const c = $derived(editor.conflict);
</script>

{#if c}
  <div role="alert" class="flex items-center gap-2 border-b border-[var(--danger-color)]
                            bg-[var(--panel-bg)] px-2 py-1 text-xs">
    <span>
      <span class="font-mono">{c.name}</span>
      changed on disk{c.source === "save" ? " while you were editing" : ""}.
    </span>
    <button type="button" onclick={() => editor.resolveReload()}>Reload</button>
    <button type="button" onclick={() => editor.resolveOverwrite()}>Overwrite</button>
  </div>
{/if}
```

In `AppShell.svelte`, mount `<ConflictBanner />` above the panel row — it is
not one of the four panels and must not be hidden by a toggle — and subscribe:

```js
  import * as api from "../api.js";
  onMount(() => {
    const es = new EventSource(api.eventsUrl());
    es.addEventListener("file-change", (e) => {
      try {
        editor.onFileChange(JSON.parse(e.data));
      } catch {
        // A malformed event is not worth breaking the page over.
      }
    });
    return () => es.close();
  });
```

⚠️ Check the actual `file-change` payload shape against `handle_events` in
`src/api/dev.rs` before trusting `{ref, name}`. If it differs, adapt the
parsing here — not the store's interface, and not the test's assertions about
behaviour.

- [ ] **Step 5: Write `StatusBar`**

Validity, cursor position and watch state, in monospace, at the foot of the
editor (spec §1). It reads `editor.active.issues`, the CodeMirror cursor
position, and whether the `EventSource` is open.

**Amended 2026-09-10 — it must also report colour tuning.** `StatusBar` shows,
beside the other indicators:

- **that a colour override is live, and for which panel**, from `tune.pushed`
  and `tune.panel`. This is not decoration. An override silently changes what
  every real device using that panel renders, and the Tune panel it was set
  from may be closed. An override nobody can see is the trap the whole
  two-stage design exists to avoid (colour-tuning spec §1, §8).
- **`tune.error`** — `{op, message}`, set by a failed push, save, discard or
  session call (Task 9f). Task 9f captures it and nothing renders it until
  here, so a failure would otherwise look like success.

Add the two cases to `webui/test/conflict.test.js` or a sibling file: an
override live with the Tune panel unmounted still shows in the status bar, and
a `tune.error` is displayed rather than swallowed.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd webui && pnpm test -- conflict
```

Expected: PASS, 5 tests.

- [ ] **Step 7: Commit**

```bash
git add webui/src/lib/state/editor.svelte.js \
        webui/src/lib/components/ConflictBanner.svelte \
        webui/src/lib/components/StatusBar.svelte \
        webui/src/lib/components/AppShell.svelte webui/test/conflict.test.js
git diff --cached --name-only
git commit -m "feat(webui): conflict banner on both triggers, with Overwrite tested"
```

---

### Task 11: Theme, and restyling the two survivors

**Files:**
- Create: `webui/src/lib/components/ThemeSwitch.svelte`, `webui/src/lib/state/theme.svelte.js`
- Modify: `static/dev/login.html`, `static/dev/preview.html`, `static/dev/lens.js`
- Test: `webui/test/theme.test.js`

**Interfaces:**
- Consumes: `layout`'s storage conventions.
- Produces: `theme.mode` (`"system" | "light" | "dark"`), `theme.set(mode)`, and the `BroadcastChannel` message that carries the choice to the pop-out.

Settled and not open for relitigation: three-way **System / Light / Dark**,
defaulting to System; dark keeps byonk's navy identity; the CodeMirror theme
is swapped at runtime through its `Compartment`. Spec §8.9 adds that the
pop-out follows the main window's choice **in both directions**.

`static/dev/login.html`, `static/dev/preview.html` and `static/dev/lens.js`
**survive** the rewrite as hand-written files served exactly as they are today
(spec §4). Restyle them to the dense visual language; do not rebuild them in
Svelte. The login page keeps no framework: loading the whole bundle before
anyone can authenticate is a cost with no benefit, and the 404-when-
unconfigured behaviour is easier to keep true when the page is a constant.

- [ ] **Step 1: Write the failing test**

`webui/test/theme.test.js`:

```js
import { describe, it, expect, beforeEach } from "vitest";
import { theme } from "../src/lib/state/theme.svelte.js";

beforeEach(() => {
  localStorage.clear();
  document.documentElement.className = "";
  theme.reset();
});

describe("theme", () => {
  it("defaults to system", () => {
    expect(theme.mode).toBe("system");
  });

  it("puts .dark on the root for an explicit dark choice", () => {
    theme.set("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    theme.set("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("remembers the choice across a reload", () => {
    theme.set("dark");
    theme.reset();
    expect(theme.mode).toBe("system");
    theme.load();
    expect(theme.mode).toBe("dark");
  });

  it("announces the choice so the pop-out can follow", () => {
    const seen = [];
    theme.onBroadcast((m) => seen.push(m));
    theme.set("dark");
    expect(seen).toContain("dark");
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd webui && pnpm test -- theme
```

Expected: FAIL — module missing.

- [ ] **Step 3: Write the theme store and switch**

Persist under `byonk.theme.v1`. On `"system"`, follow
`matchMedia("(prefers-color-scheme: dark)")` and keep listening, so a change
to the OS setting is picked up without a reload. Broadcast every change on the
`byonk-preview` channel as `{theme: mode}` — the pop-out already listens on
that channel for renders, and one channel is simpler than two.

`ThemeSwitch.svelte` is a three-option select built from the vendored
`Select`, labelled so it is reachable by keyboard.

- [ ] **Step 4: Make the pop-out follow**

In `static/dev/preview.html`, handle `{theme}` messages on the same channel
and set `.dark` on its own root. Verify **both directions**: a change made
while the pop-out is open, and a pop-out opened after the change.

- [ ] **Step 5: Restyle the survivors**

Apply the density scale by hand to `static/dev/login.html` and
`static/dev/preview.html`, using the same CSS custom-property names as
`webui/src/app.css` so the two look like one product. These files have no
Tailwind — write the CSS inline, as they do today.

⚠️ `static/dev/preview.html` has a live defect worth fixing while you are
here: `setupLens()` runs before the `BroadcastChannel` is created in the same
IIFE, so a failure in `lens.js` takes the mirror down with it. Create the
channel first.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd webui && pnpm test -- theme
```

Expected: PASS, 4 tests.

- [ ] **Step 7: Commit**

```bash
git add webui/src/lib/state/theme.svelte.js \
        webui/src/lib/components/ThemeSwitch.svelte webui/test/theme.test.js \
        static/dev/login.html static/dev/preview.html static/dev/lens.js
git diff --cached --name-only
git commit -m "feat(webui): three-way theme the pop-out follows; restyle login and pop-out"
```

---

### Task 12: The swap — serve the SPA, delete the old UI

**Files:**
- Modify: `src/api/dev.rs`, `Makefile`, `tests/dev_static_test.rs` (only if it names a deleted route)
- Delete: `static/dev/index.html`, `static/dev/dev.js`, `static/dev/dev.css`, `static/dev/editor.bundle.js`, `static/dev/editor/`
- Test: `tests/dev_static_test.rs`, `webui/test/bundle-size.test.js`

**Interfaces:**
- Consumes: `webui/dist/{index.html,app.js,app.css}` (Task 1), the build wiring (Task 2), and every component task.
- Produces: `/dev/` serving the SPA.

This is the one task that must be atomic. Everything before it left the old UI
serving `/dev/`, so every commit built and ran. Do not split this one.

- [ ] **Step 1: Write the failing test**

Add to `tests/dev_static_test.rs` (read the file first — it already pins the
routing depth at lines 33-34, and **that assertion must not change**):

```rust
/// The SPA is served under three fixed names, and the routes the vanilla UI
/// needed are gone. `dev.js`, `dev.css` and `editor.bundle.js` were the
/// hand-written UI; serving them after the rewrite would mean a stale file
/// was left embedded.
#[tokio::test]
async fn serves_the_spa_and_not_the_retired_routes() {
    let app = test_app_with_dev_enabled().await;

    for (path, needle) in [
        ("/dev/", "<div id=\"app\">"),
        ("/dev/app.js", ""),
        ("/dev/app.css", ""),
    ] {
        let res = get(&app, path).await;
        assert_eq!(res.status(), 200, "{path} must be served");
        if !needle.is_empty() {
            assert!(body_string(res).await.contains(needle), "{path} body");
        }
    }

    for path in ["/dev/dev.js", "/dev/dev.css", "/dev/editor.bundle.js"] {
        assert_eq!(
            get(&app, path).await.status(),
            404,
            "{path} belonged to the deleted UI"
        );
    }
}
```

⚠️ Match the helper names this file already uses (`test_app_with_dev_enabled`,
`get`, `body_string` are placeholders). Read the file and reuse its own
helpers rather than adding new ones.

`webui/test/bundle-size.test.js` — spec §9 asks for a stated budget:

```js
import { describe, it, expect } from "vitest";
import { statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// The UI is embedded in the binary, so its size is the binary's size. The
// vanilla UI it replaces was 3,305 lines across five files, about 400 KB
// once the CodeMirror bundle is counted. This budget is deliberately close
// to that: a framework is affordable here, an unbounded one is not.
// Raising it is a decision, not a formality -- say why in the commit.
const BUDGET_BYTES = 900 * 1024;

describe("bundle size", () => {
  it("fits the stated budget", () => {
    const js = statSync(resolve(root, "dist/app.js")).size;
    const css = statSync(resolve(root, "dist/app.css")).size;
    expect(js + css).toBeLessThan(BUDGET_BYTES);
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

```bash
cargo test --test dev_static_test
cd webui && pnpm build && pnpm test -- bundle-size
```

Expected: the Rust test FAILS (no `/dev/app.js` route); the size test tells
you the real number. If it is already over budget, do not raise the budget
reflexively — check for a duplicated CodeMirror or an un-tree-shaken icon
import first.

- [ ] **Step 3: Point the handlers at the built files**

In `src/api/dev.rs`, change the three surviving handlers and add one. The
`include_str!` paths are relative to the source file, so `webui/dist` is
`../../webui/dist`:

```rust
/// Serve the dev UI's HTML shell, built by `make webui`.
pub async fn handle_dev_page() -> Html<&'static str> {
    Html(include_str!("../../webui/dist/index.html"))
}

/// Serve the dev UI application bundle.
pub async fn handle_app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("../../webui/dist/app.js"),
    )
}

/// Serve the dev UI stylesheet.
pub async fn handle_app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("../../webui/dist/app.css"),
    )
}
```

Delete `handle_dev_js`, `handle_dev_css` and `handle_editor_bundle`. Keep
`handle_lens_js` and `handle_preview_page` — those files survive.

- [ ] **Step 4: Rewire the routes**

In `dev_router` (`src/api/dev.rs:861`), inside the `protected` router:

```rust
    let protected = Router::new()
        .route("/", get(handle_dev_page))
        .route("/app.js", get(handle_app_js))
        .route("/app.css", get(handle_app_css))
        .route("/lens.js", get(handle_lens_js))
        // Flat, not nested: `require_dev` redirects an unauthenticated
        // visitor to the relative string "login", which is only correct one
        // level below `/dev/`.
        .route("/preview.html", get(handle_preview_page))
        .nest("/api", api)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::dev_auth::require_dev,
        ));
```

⚠️ `/dev.css` is currently registered **outside** the protected router, so the
login page can style itself. `app.css` belongs to the app and must stay
**inside** the protected router. Do not move it out by copying the old line.
The login page carries its own CSS (Task 11); if it still needs a stylesheet
route, keep the old open route serving the login page's own file — never the
app's.

- [ ] **Step 5: Delete the old UI**

```bash
git rm static/dev/index.html static/dev/dev.js static/dev/dev.css \
       static/dev/editor.bundle.js
git rm -r static/dev/editor
```

Confirm what remains is exactly `login.html`, `preview.html` and `lens.js`:

```bash
ls static/dev/
```

- [ ] **Step 6: Retire `editor` and `editor-check`**

Remove both targets from the `Makefile`, their `.PHONY` entries, their help
lines (around lines 199-201) and the explanatory comment above them (lines
137-151). Remove `editor-check` from the `check` target and from
`.github/workflows/ci.yml` (line 47).

The guard they provided is replaced by the fact that nothing is committed, so
nothing can be stale — which only holds because `tests/webui_build_test.rs`
(Task 2) keeps all three build paths building the UI. Say that in the commit.

- [ ] **Step 7: Run everything**

```bash
cargo test --workspace
cd webui && pnpm test
```

Expected: PASS throughout, including `tests/dev_auth_test.rs` **unchanged**
and `tests/dev_static_test.rs:33-34` unchanged.

- [ ] **Step 8: Commit**

```bash
git add src/api/dev.rs Makefile .github/workflows/ci.yml \
        tests/dev_static_test.rs webui/test/bundle-size.test.js
git add -u static/dev
git diff --cached --name-only
git commit -m "feat(dev): serve the Svelte UI; delete the hand-written one"
```

---

### Task 13: Documentation and changelog

**Files:**
- Modify: `CHANGES.md`, `docs/src/guide/dev-mode.md`, `CLAUDE.md` (build commands), `README.md` if it names `make editor`

**Interfaces:** none.

- [ ] **Step 1: Find every stale reference**

```bash
grep -rn 'editor-check\|make editor\|dev\.js\|dev\.css\|editor\.bundle' \
  --include='*.md' --include='Makefile' --include='*.yml' . | grep -v '^./target'
```

Fix each hit. A reference to a file that no longer exists is a defect in the
docs, not a cosmetic issue.

- [ ] **Step 2: Update the dev-mode guide**

`docs/src/guide/dev-mode.md` describes the editor's layout. Rewrite the layout
section for the **five** toggleable panels: what each icon shows and hides, that
every combination is allowed, that hidden panels return at the size they had,
and that splitter positions persist. Screenshots, if it has any, are now wrong.

**Amended 2026-09-10 — colour tuning needs its own section**, because it is the
one panel whose behaviour a user cannot guess from looking at it. Write it as
the two-stage loop it is, and say plainly:

- tuning is **per panel profile**, so every device using that profile shares it;
- **Push to panel** sets a temporary override that real devices pick up on
  their next poll, and is **lost on restart**;
- while a device is selected in Tune, **that device refreshes every 60 seconds**
  so a calibration iterates quickly — and that this costs battery, ends when
  you save, discard, deselect or close the panel, and expires by itself after
  30 minutes;
- **Save to config** writes `colors_actual` into `config.yaml` under that panel,
  keeping the file's comments, and is what makes a calibration permanent;
- the browser preview alone cannot confirm a calibration, because e-ink is
  reflective and the panel shows the light in the room it hangs in.

Also fix the two known docs falsehoods if they are still present, since this
task is already in the neighbourhood: `homeassistant/byonk/DOCS.md:64-65` and
`docs/src/guide/home-assistant.md:204` both claim `config.yaml` edits are picked
up without a restart. They are not — `reload_config` runs only from a write
handler. Say so in your report if you fix them, since they are outside this
plan's stated scope.

- [ ] **Step 3: Add the changelog entry**

Under `## Unreleased`. **User-facing only** — no build, CI or tooling notes.
The user-visible change is the interface, not the framework:

```markdown
- **The screen editor has been rebuilt.** Same capabilities, new layout: an
  icon rail on the left with one toggle per panel — Files, Editor, Preview and
  Simulate — so you can hide anything you are not using and give the space to
  what you are. Drag the splitters to size the rest. Your layout, including
  which panels are open, is remembered between visits.

  The simulate controls sit under the preview rather than over it, so you can
  watch a render change while you change it. On an upright panel — a
  reTerminal E1004 is 1200 × 1600 — hide or collapse the controls and the
  device takes the whole column.
```

- [ ] **Step 4: Build the docs**

```bash
(cd docs && mdbook build)
```

Expected: exit 0, no broken links.

- [ ] **Step 5: Commit**

```bash
git add CHANGES.md docs/src/guide/dev-mode.md CLAUDE.md README.md
git diff --cached --name-only
git commit -m "docs: the rebuilt screen editor and its panel toggles"
```

---

### Task 14: Full verification

**Files:** none — this task changes nothing. It proves.

- [ ] **Step 1: The five commands**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
(cd docs && mdbook build)
make webui
```

All must exit 0. Fix anything that fails **in its own commit**. Do not run
`make check` in place of these — it has reported exit 0 while tests failed.

- [ ] **Step 1b: The colour-tuning checks (added 2026-09-10)**

These are server-side and run with the five commands above; they exist because
three of the amendment's behaviours are invisible to a unit test.

```bash
cargo test --test dev_panel_colors_test \
           --test dev_panel_colors_save_test \
           --test dev_tuning_session_test \
           --test addon_reload_test
```

Then confirm by hand, in the sandbox of Step 2:

- **the two-stage split holds** — drag a slider and watch the preview change
  while `config.yaml` on disk does not; then press Save and watch the file gain
  a `colors_actual` line **with every surrounding comment intact**;
- **an override really is temporary** — push one, restart byonk, and confirm the
  panel renders from `config.yaml` again;
- **closing the Tune panel releases the override**, and the status bar stops
  reporting one;
- **the working copy survives that close** — reopen Tune and the colours you had
  are still there;
- **a session ends** — select a device, confirm `/api/display` answers
  `refresh_rate: 60`, then close the panel and confirm it answers the normal
  rate again. `curl` the endpoint directly; do not infer it from the UI.

- [ ] **Step 2: Browser walk-through in a scratch sandbox**

Never point this at the maintainer's checkout. Copy `config.yaml` plus empty
directories into a scratch dir and let byonk seed `examples` and
`byonk-builtin` itself:

```bash
SB=<scratchpad>/sandbox
BIND_ADDR=127.0.0.1:3000 CONFIG_FILE=$SB/config.yaml SCREENS_DIR=$SB/screens \
EXAMPLES_DIR=$SB/examples SCREEN_REPOS_DIR=$SB/repos \
SCREEN_REPOS_CACHE_DIR=$SB/cache \
BYONK_DEV_USERNAME=u BYONK_DEV_PASSWORD=p ./target/debug/byonk serve
```

Run `git status` in the checkout afterwards and confirm nothing was written.

Walk spec §8 end to end, and record the result of each:

1. Unconfigured: `/dev/`, `/dev` and `/dev/login` → **404**; `/dev/api/*` → **401**
2. Configured: login → create → error marker → fix → save → preview
3. All four panel toggles, including all four off and back on
4. Splitter drag on both axes, then reload: positions and open state restored
5. Pop-out follows a Panel Profile change
6. Copy an `examples/…` screen into `local`, then delete it
7. **Read-only**: type into a `byonk-builtin` file — nothing may be inserted
8. Conflict banner from a save (412) — **and Overwrite**, which has never been exercised
9. Conflict banner from the watcher: edit a file in `SCREENS_DIR` externally
10. Theme: System / Light / Dark, with the pop-out following both directions
11. Logout: verify the session is actually dead, not merely returned to the form

Record a GIF at `<scratchpad>/svelte-ui-walkthrough.gif`.

- [ ] **Step 3: Home Assistant VM**

The Ingress properties were proven for the vanilla UI on 2026-09-09 and must
be re-proven for the SPA, because every one of them is a property of the
*client* as much as the server. Follow the `ha-vm-testing` skill.

What that session learned, so you do not rediscover it:

- The VM's add-on manifest is **not** synced by `make ha-rebuild`. Copy
  `homeassistant/byonk/config.yaml` over the VM's `addons/byonk/config.yaml`,
  **minus its `image:` line** (that line switches the add-on from
  build-from-source to prebuilt), and bump the version to bust the cache.
- Then `ha store reload`, then `ha apps update local_byonk`. Reloading the
  store *before* building avoids a second Rust compile.
- The VM's own `Dockerfile` does `COPY . .`, so it picks up whatever the sync
  copied. **It must now copy `webui/dist` too** — add `webui/dist` to the sync
  list in `tools/ha-vm/rebuild.sh`, or the build fails on `include_str!`.
- `ha` CLI in HA 2026.7 prints a deprecation line to stdout that breaks JSON
  parsing; strip it before piping to `jq` or `python3 -m json.tool`.
- Do not print the add-on's `options` wholesale — it contains the admin token.
  Select the fields you need with `jq`.

Checklist:

1. The sidebar entry loads the SPA under `/api/hassio_ingress/<token>/dev/`
2. The status bar shows the HA user; `api/whoami` reports `via: "ingress"`
3. No Logout button is rendered
4. Edit and save a file; confirm it on disk over Samba
5. **The pop-out opens and follows a change** — the property proven on
   2026-09-09; a root-absolute URL anywhere in the SPA breaks it
6. From the Mac host, `http://localhost:3000/dev/` still demands the password
7. From a foreign container, the ingress port still answers **403**

- [ ] **Step 4: The final whole-branch review**

One review covering both halves of the branch — the vanilla-JS work and this
rewrite — as spec §Sequencing requires:

```bash
scripts/review-package PLAN 133c528 HEAD
```

Use the most capable model. Point it at the ledger's deferred-minor and parked
lines. Note that `docs/superpowers/specs/2026-09-07-dev-mode-editor-design.md`
carries two knowingly stale lines — `preserve-exact` at :348 and
`ingress_entry: /dev/` at :137 — left as written under the standing ruling not
to amend a spec to match its implementation.

- [ ] **Step 5: Ask before pushing**

Ask the maintainer before `git push` and before opening the PR. Then:

```bash
git push -u origin feat/dev-editor
gh pr create --title "feat: always-on dev UI with login, screen editor and HA ingress" \
             --body-file <scratchpad>/pr-body.md
```

PR body: the summary, the breaking change, the test list, screenshots, and
"🤖 Generated with [Claude Code](https://claude.com/claude-code)".

---

## Self-review

**Colour-tuning amendment coverage (2026-09-10).** Spec
`2026-09-10-dev-ui-color-tuning-design.md`: §1 two stages → Tasks 9b, 9d, 9f,
9g. §2 layout → Task 9e. §3 tuning session → Task 9c. §4 re-key → Task 9b.
§5 durable write → Task 9d. §6 add-on reload fix → Task 9a. §7 HTTP surface →
Tasks 9b, 9c, 9d, and the API table above. §8 `TunePane` and the `tune` store →
Tasks 9f, 9g, plus Task 10 Step 5 for the two things `StatusBar` must render.
§9 must-not-regress → the Global Constraints plus Task 14 Step 1b. §10 testing →
each task's own tests. §11 risks → no task; they are judgement calls for the
final review.

**Amendment gaps, stated rather than hidden.**

- **`resolve_panel`'s extraction is the riskiest edit in Tasks 9a–9d.** It moves
  live code out of a long function in `src/api/display.rs`. Diff it against the
  original before and after; a branch that changes precedence changes what real
  devices render.
- **Task 9c's fixture may not produce a non-60 baseline.** Its `assert_ne!`
  guard is there to trip loudly rather than let the test pass vacuously.
- **`TunePane` is untested against a real pointer.** Its sliders are driven by
  `fireEvent.input` in jsdom, like every other control in this plan. Task 14's
  browser list is where a real drag is exercised.
- **The 60-second refresh is not exercised against real hardware by any task.**
  Task 14 Step 1b confirms the number byonk answers; whether a physical panel
  actually keeps up with it is a question only the maintainer's own device can
  settle.

**Spec coverage.** §1 layout → Tasks 4, 5. §2 visual language → Tasks 1 (scale), 11 (theme). §3 toolchain → Task 1. §4 build and embedding → Tasks 1, 2, 12. §5 serving → Task 12. §6 component structure → Tasks 4-11. §7 testing → every task's tests; the five named cases are Tasks 4 (splitter persistence), 7 (dirty vs error, tab lifecycle), 9 (aspect ratio), 10 (conflict, both paths). §8 regressions 1-9 → Task 14 Step 2, with 6 also unit-tested in Task 6 and by hand in Task 7, and 8 unit-tested in Task 6. §9 risks → bundle budget in Task 12, three build paths in Task 2, density scale in Task 1; the pop-out risk is closed and re-proved in Task 14 Step 3.

**Known gaps, stated rather than hidden.**

- **`SimulateControls` has no full-field test.** Task 8 tests the URL builder and the size rule, not one assertion per control. The controls are a wide, shallow surface; the render URL is the part that can be wrong in an interesting way.
- **`StatusBar` has no test.** It displays state three other stores already test.
- **shadcn-svelte's CLI output is unknown until it runs.** Task 5 Step 1 says to verify the alias and adapt the imports. Every later import path in this plan assumes `src/lib/...` and `src/ui/...`.
- **`AppShell`'s nested snippets are the plan's least certain markup.** Task 5 Step 5 flags it.
- ~~**The `file-change` payload shape is assumed to be `{ref, name}`.**~~ **Checked, and the assumption is wrong:** the payload is a JSON array of bare basenames (`src/api/dev.rs:294-303`), so a subscriber that destructures `{ref, name}` off it gets `undefined` and matches nothing. Match by basename, as `dev.js:1622-1631` does. Task 10 must carry this; its own tests cannot catch it, because they call `onFileChange` directly.
