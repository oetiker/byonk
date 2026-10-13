<script>
  // The on-disk-changed banner (spec §6, §8.5). It must appear on BOTH
  // triggers editor.conflict can carry -- a 412 from trySave() and the SSE
  // watcher's file-change (via editor.onWatchEvent) -- and it is not one of
  // the four toggleable panels, so AppShell mounts it in the non-shifting status overlay
  // rather than gating it on any layout state.
  import { editor } from "../state/editor.svelte.js";
  import { Alert } from "$lib/ui/alert/index.js";
  import { Button } from "$lib/ui/button/index.js";

  const c = $derived(editor.conflict);
</script>

{#if c}
  <Alert
    variant="destructive"
    class="flex h-(--pane-toolbar-h) shrink-0 items-center gap-2 rounded-none border-0 border-b border-destructive bg-muted px-2 py-0 text-xs"
  >
    <span class="text-foreground">
      <span class="font-mono">{c.name}</span>
      changed on disk{c.source === "save" ? " while you were editing" : ""}.
    </span>
    <Button type="button" size="xs" variant="outline" onclick={() => editor.resolveReload()}>Reload</Button>
    <Button type="button" size="xs" variant="outline" onclick={() => editor.resolveOverwrite()}>Overwrite</Button>
  </Alert>
{/if}
