<script>
  // The foot of the editor (spec §1's mockup: "✓ valid  lua 3:22 / watch:
  // connected"). Dense, monospace, and -- per Controller Rulings 20, 39,
  // 41/43 and the tune amendment -- the ONE place that renders every error
  // surface earlier tasks were told to capture but never told to display:
  //
  //   - editor.error      (Ruling 39) -- a save failure that isn't a conflict
  //   - screens.error     (Ruling 20) -- a failed new/copy/rename/delete
  //   - simulate.error    (Rulings 41, 43) -- a failed render
  //   - tune.error        (tune amendment) -- a failed push/save/discard/session
  //   - tune.pushed       (tune amendment) -- a live server-side override
  //     nobody may be watching
  //
  // A captured-but-never-rendered error is worse than the unhandled
  // rejection it replaced, because it looks handled.
  import { editor } from "../state/editor.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { tune } from "../state/tune.svelte.js";

  function languageOf(name = "") {
    if (name.endsWith(".lua")) return "lua";
    if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
    if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
    return "text";
  }

  const t = $derived(editor.active);
  const issues = $derived(t?.issues ?? []);
</script>

<div
  class="flex flex-col gap-0.5 border-t border-[var(--border-color)]
         bg-[var(--panel-bg)] px-2 py-1 font-mono text-xs text-[var(--text-color)]"
>
  <div class="flex flex-wrap items-center gap-3">
    {#if !t}
      <span class="text-[var(--text-muted)]">no file open</span>
    {:else if issues.length === 0}
      <span class="text-[var(--ok-color)]">✓ valid</span>
    {:else}
      <span class="text-[var(--danger-color)]">
        ✖ {issues.length} issue{issues.length === 1 ? "" : "s"}
      </span>
    {/if}

    {#if t}
      <span>{languageOf(t.name)} {editor.cursorLine}:{editor.cursorCol}</span>
    {/if}

    <span
      class={editor.watchConnected ? "text-[var(--ok-color)]" : "text-[var(--text-muted)]"}
    >
      watch: {editor.watchConnected ? "connected" : "disconnected"}
    </span>

    {#if tune.pushed}
      <span role="status" class="text-[var(--danger-color)]">
        ⚠ override live on {tune.panel || "?"}
      </span>
    {/if}
  </div>

  {#if editor.error}
    <div role="alert" class="text-[var(--danger-color)]">save: {editor.error.message}</div>
  {/if}
  {#if screens.error}
    <div role="alert" class="text-[var(--danger-color)]">
      {screens.error.op}: {screens.error.message}
    </div>
  {/if}
  {#if simulate.error}
    <div role="alert" class="text-[var(--danger-color)]">render: {simulate.error.message}</div>
  {/if}
  {#if tune.error}
    <div role="alert" class="text-[var(--danger-color)]">
      tune {tune.error.op}: {tune.error.message}
    </div>
  {/if}
</div>
