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
