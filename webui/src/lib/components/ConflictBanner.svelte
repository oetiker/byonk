<script>
  // The on-disk-changed banner (spec §6, §8.5). It must appear on BOTH
  // triggers editor.conflict can carry -- a 412 from trySave() and the SSE
  // watcher's file-change (via editor.onWatchEvent) -- and it is not one of
  // the four toggleable panels, so AppShell mounts it above the panel row
  // rather than gating it on any layout state.
  import { editor } from "../state/editor.svelte.js";

  const c = $derived(editor.conflict);
</script>

{#if c}
  <div
    role="alert"
    class="flex items-center gap-2 border-b border-destructive
           bg-muted px-2 py-1 text-xs"
  >
    <span>
      <span class="font-mono">{c.name}</span>
      changed on disk{c.source === "save" ? " while you were editing" : ""}.
    </span>
    <button type="button" onclick={() => editor.resolveReload()}>Reload</button>
    <button type="button" onclick={() => editor.resolveOverwrite()}>Overwrite</button>
  </div>
{/if}
