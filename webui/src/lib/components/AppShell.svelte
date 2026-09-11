<script>
  import IconRail from "./IconRail.svelte";
  import SplitPane from "./SplitPane.svelte";
  import { layout } from "../state/layout.svelte.js";

  // Placeholders until Tasks 6-10 fill them in.
  import FilesPane from "./FilesPane.svelte";
  import EditorPane from "./EditorPane.svelte";
  import TunePane from "./TunePane.svelte";
  import PreviewPane from "./PreviewPane.svelte";
  import SimulateControls from "./SimulateControls.svelte";

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
  </div>
</div>
