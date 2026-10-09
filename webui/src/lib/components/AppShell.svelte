<script>
  import { onMount, untrack } from "svelte";
  import IconRail from "./IconRail.svelte";
  import SplitPane from "./SplitPane.svelte";
  import ConflictBanner from "./ConflictBanner.svelte";
  import StatusStrip from "./StatusStrip.svelte";
  import { layout } from "../state/layout.svelte.js";
  import { theme } from "../state/theme.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { auth } from "../state/auth.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import * as api from "../api.js";

  // Placeholders until Tasks 6-10 fill them in.
  import LibraryPane from "./LibraryPane.svelte";
  import EditPane from "./EditPane.svelte";
  import PreviewPane from "./PreviewPane.svelte";
  import SimulatedInputs from "./SimulatedInputs.svelte";

  // Restore persisted panel visibility/sizes synchronously here, not from
  // onMount as the brief's code block has it. This app is never
  // server-rendered (main.js mounts straight into the DOM), so
  // localStorage is available at setup time, and calling load() here --
  // before `xSize`/`ySize` below read layout.size() -- matters: onMount
  // fires only after the component's first render has already captured
  // those $state initial values from layout's pre-load defaults. Since
  // layout.load() replaces layout's internal state object outright rather
  // than mutating the one xSize/ySize snapshotted from, deferring it to
  // onMount would mean a persisted split size is silently ignored on every
  // page load -- exactly the "reopens at a default size" bug spec §1 (and
  // layout.svelte.js's own header comment) calls out. Panel open/closed
  // state doesn't have this problem below because `layout.isOpen(...)` is
  // read live from the template, never snapshotted into local state.
  layout.load();
  // Same reasoning as layout.load() above: read synchronously here, before
  // ThemeSwitch (mounted inside IconRail) reads `theme.mode` for its first
  // render, so a persisted explicit choice from a prior session is never
  // shown as "System" for one frame before flipping.
  theme.load();

  // Who is looking, and whether IconRail should offer a way to log out
  // (Ruling 63). Not awaited and not needed before first paint like
  // layout/theme above -- `whoami` requires the caller to already be
  // authenticated (it lives behind `require_dev`, which is what got this
  // page served in the first place), so this is purely "fill in the
  // control once the answer comes back", not a gate on rendering anything.
  auth.load();

  // The repo list, the screen list and the simulate metadata (devices,
  // panels, dither algorithms). Same shape as auth.load() above and for the
  // same reason: not a gate on first paint, just "fill the lists in once
  // the answer comes back". Without it nothing ever calls `screens.refresh()`
  // on a fresh load, and since `screens.grouped` derives from `repos`, an
  // empty repo list hides every screen -- there is then no way to open one
  // at all. Task 14's browser walk found exactly that.
  screens.load();
  configDraft.load();

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

  let xSize = $state(layout.size("x"));
  let ySize = $state(layout.size("y"));

  // Ruling 12: SplitPane's `bind:size` updates xSize/ySize live on every
  // pointermove during a drag -- that binding lives inside SplitPane
  // itself and must stay synchronous, or the drag looks laggy. Wiring that
  // straight into `layout.setSize()`, which calls `save()` (a synchronous
  // `localStorage.setItem`), would serialise the whole layout and hit disk
  // once per animation frame for the length of the drag.
  //
  // Debounce only the persisted write, trailing-edge: each time xSize/
  // ySize changes, schedule a write of the CURRENT value after a short
  // delay. Svelte re-runs an $effect by first calling the cleanup returned
  // by its previous run, so a value that changes again before the timer
  // fires cancels the stale write for free -- only the last value in a
  // burst of changes ever reaches localStorage, once movement settles.
  // Trade-off: a drag that ends by unmounting the component within the
  // debounce window (e.g. mid-drag reload) loses that last write, same as
  // any other unsaved-changes-on-unmount gap; acceptable here since
  // AppShell is the application root and is never unmounted in normal use.
  //
  // Skip the write entirely when the bound value already matches what
  // `layout` holds (true at mount, since xSize/ySize above were just read
  // from it) -- otherwise every page load re-persists the exact value it
  // just loaded, 200ms later, for no visible effect. `layout.size(axis)`
  // is itself reactive ($state under the hood), so once the scheduled
  // write below does fire and updates it, this effect re-runs, finds the
  // two values equal again, and quiesces without scheduling a next one.
  const PERSIST_DEBOUNCE_MS = 200;

  $effect(() => {
    const value = xSize;
    if (value === layout.size("x")) return;
    const id = setTimeout(() => layout.setSize("x", value), PERSIST_DEBOUNCE_MS);
    return () => clearTimeout(id);
  });

  $effect(() => {
    const value = ySize;
    if (value === layout.size("y")) return;
    const id = setTimeout(() => layout.setSize("y", value), PERSIST_DEBOUNCE_MS);
    return () => clearTimeout(id);
  });

  // Live-reload watcher (spec §8.5, Task 10). `handle_events` (src/api/
  // dev.rs) emits a JSON array of bare basenames on a `file-change` event
  // -- `editor.onWatchEvent` does the real matching (Ruling 36); this is
  // only the transport and the connection-state bookkeeping (Ruling 40),
  // which is WHY the state it writes (`editor.watchConnected`) lives in a
  // shared rune module rather than a local `const es` no other component
  // could read.
  //
  // Guarded on `typeof EventSource` rather than assumed: this project's
  // jsdom+vitest test environment does not define it (confirmed directly --
  // unlike BroadcastChannel/URL.createObjectURL, which DO survive from
  // Node's globals), so every existing AppShell test that renders this
  // component would otherwise throw in onMount. Every real browser this
  // ships to has it; a test that needs the SSE path installs its own
  // EventSource before rendering (see conflict.test.js).
  onMount(() => {
    if (typeof EventSource === "undefined") return;

    let es;
    let reconnectTimer;

    function connect() {
      es = new EventSource(api.eventsUrl());
      es.onopen = () => {
        editor.watchConnected = true;
      };
      es.onerror = () => {
        editor.watchConnected = false;
        es.close();
        reconnectTimer = setTimeout(connect, 3000);
      };
      es.addEventListener("file-change", (e) => {
        try {
          editor.onWatchEvent(JSON.parse(e.data));
        } catch {
          // A malformed event is not worth breaking the page over.
        }
      });
    }
    connect();

    return () => {
      clearTimeout(reconnectTimer);
      es?.close();
      editor.watchConnected = false;
    };
  });
</script>

<div class="flex h-full flex-col bg-[var(--bg-color)] text-[var(--text-color)]">
  <ConflictBanner />
  <StatusStrip />
  <div class="flex min-h-0 flex-1">
    <IconRail />

    {#if layout.isOpen("library")}
      <div class="w-56 shrink-0 overflow-auto border-r border-[var(--border-color)]">
        <LibraryPane />
      </div>
    {/if}

    <SplitPane axis="x" bind:size={xSize} open={layout.isOpen("preview")}>
      {#snippet a()}
        {#if layout.isOpen("editor")}<EditPane />{/if}
      {/snippet}
      {#snippet b()}
        {#if layout.isOpen("preview")}
          <SplitPane axis="y" bind:size={ySize} open={simulate.expanded}>
            {#snippet a()}<PreviewPane />{/snippet}
            {#snippet b()}<SimulatedInputs />{/snippet}
          </SplitPane>
        {/if}
      {/snippet}
    </SplitPane>
  </div>
</div>
