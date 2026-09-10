<script>
  import { screens } from "../state/screens.svelte.js";
  import * as api from "../api.js";
  import { Button } from "$lib/ui/button/index.js";

  const canEdit = $derived(screens.writable(screens.selected));

  async function newScreen() {
    const handle = screens.repos.find((r) => r.writable)?.handle;
    const name = prompt("New screen name");
    if (!handle || !name) return;
    const { ref } = await api.createScreen(handle, name);
    await screens.refresh();
    screens.select(ref);
  }

  async function copyScreen() {
    const to = screens.repos.find((r) => r.writable)?.handle;
    const name = prompt("Copy to name");
    if (!to || !name || !screens.selected) return;
    const { ref } = await api.copyScreen(screens.selected, to, name);
    await screens.refresh();
    screens.select(ref);
  }

  async function renameScreen() {
    const name = prompt("Rename to");
    if (!name || !screens.selected) return;
    const { ref } = await api.renameScreen(screens.selected, name);
    await screens.refresh();
    screens.select(ref);
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
    <Button type="button" size="xs" variant="outline" onclick={newScreen}>New</Button>
    <Button
      type="button"
      size="xs"
      variant="outline"
      onclick={copyScreen}
      disabled={!screens.selected}
    >
      Copy
    </Button>
    <Button type="button" size="xs" variant="outline" onclick={renameScreen} disabled={!canEdit}>
      Rename
    </Button>
    <Button type="button" size="xs" variant="outline" onclick={deleteScreen} disabled={!canEdit}>
      Delete
    </Button>
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
