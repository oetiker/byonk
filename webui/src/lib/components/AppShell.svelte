<script>
  import { onMount, untrack } from "svelte";
  import IconRail from "./IconRail.svelte";
  import * as Resizable from "$lib/ui/resizable/index.js";
  import StatusStrip from "./StatusStrip.svelte";
  import { layout } from "../state/layout.svelte.js";
  import { theme } from "../state/theme.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { auth } from "../state/auth.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import { resolution } from "../state/resolution.svelte.js";
  import { screenRepos, NEW_REPO } from "../state/screenRepos.svelte.js";
  import * as api from "../api.js";

  // Placeholders until Tasks 6-10 fill them in.
  import LibraryPane from "./LibraryPane.svelte";
  import EditPane from "./EditPane.svelte";
  import PreviewPane from "./PreviewPane.svelte";
  import SimulatedInputs from "./SimulatedInputs.svelte";

  // Restore which panels are open synchronously here, not from onMount.
  // This app is never server-rendered (main.js mounts straight into the
  // DOM), so localStorage is available at setup time, and loading before
  // the first render means a closed panel's content is never mounted for a
  // frame only to be torn down again. (Pane sizes are PaneForge's; see the
  // Resizable section below.)
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
  screenRepos.load();

  // The new-repo draft is unsaved and not in any list (ruling U11-R1):
  // leaving it discards it.
  $effect(() => {
    const key = subject.key;
    untrack(() => {
      if (key !== `screen-repo:${NEW_REPO}`) screenRepos.discardNew();
    });
  });

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

  // A new subject's ladder starts at "no render yet", never at the old
  // subject's rungs (spec §5).
  $effect(() => {
    subject.key;
    untrack(() => resolution.reset());
  });

  // --- Resizable panes (Library | Edit | Preview) ----------------------
  // PaneForge sizes the panes in PERCENT and persists them itself under
  // `paneforge:<autoSaveId>` (debounced). Percent keeps every pane above its
  // minSize at any window width; the px preview column this replaced
  // squeezed the Edit pane to 16 px at an 800 px window.
  //
  // Which panes are open is `layout`'s alone (the rail and ConfigEditor read
  // it): a closed pane is not rendered at all, so it can never sit in the
  // group at 0 width while its rail button says open. Each pane keeps a
  // stable `id` and `order`, and PaneForge stores one layout per set of
  // rendered panes, so a panel reopens at the size it had in that
  // arrangement (or the defaults, rescaled, the first time). Panes are not
  // `collapsible`: only the rail opens and closes them; a drag stops at the
  // pane's minimum.
  const SIZES = {
    library: { defaultSize: 18, minSize: 12, order: 1 },
    editor: { defaultSize: 42, minSize: 25, order: 2 },
    preview: { defaultSize: 40, minSize: 25, order: 3 },
  };

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

<div class="flex h-full flex-col bg-background text-foreground">
  <div class="flex min-h-0 flex-1">
    <IconRail />

    <Resizable.PaneGroup
      direction="horizontal"
      autoSaveId="byonk.panes.v2"
      class="min-h-0 min-w-0 flex-1"
    >
      {#if layout.isOpen("library")}
        <Resizable.Pane
          id="library"
          {...SIZES.library}
          class="flex min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground"
        >
          <LibraryPane />
        </Resizable.Pane>
        {#if layout.isOpen("editor") || layout.isOpen("preview")}
          <Resizable.Handle withHandle aria-label="Resize Library" class="hover:bg-primary data-active:bg-primary" />
        {/if}
      {/if}
      {#if layout.isOpen("editor")}
        <Resizable.Pane id="editor" {...SIZES.editor} class="flex min-h-0 min-w-0 overflow-hidden">
          <EditPane />
        </Resizable.Pane>
        {#if layout.isOpen("preview")}
          <Resizable.Handle withHandle aria-label="Resize Edit and Preview" class="hover:bg-primary data-active:bg-primary" />
        {/if}
      {/if}
      {#if layout.isOpen("preview")}
        <Resizable.Pane id="preview" {...SIZES.preview} class="flex min-h-0 min-w-0 flex-col overflow-hidden">
          <!-- The preview takes the height the inputs leave; the inputs take
               their natural height, capped at half the column so a short
               window scrolls them rather than squeezing the render away. -->
          <PreviewPane />
          <div class="flex max-h-[50%] shrink-0 flex-col border-t border-border">
            <SimulatedInputs />
          </div>
        </Resizable.Pane>
      {/if}
    </Resizable.PaneGroup>
  </div>
  <!-- The app status bar: fixed height, always present, below the pane row
       (U12) -- a message never moves a pane. -->
  <StatusStrip />
</div>
