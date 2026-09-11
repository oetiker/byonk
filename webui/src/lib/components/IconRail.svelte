<script>
  import { Files, Code, Palette, Monitor, SlidersHorizontal } from "@lucide/svelte";
  import { layout, PANELS } from "../state/layout.svelte.js";
  import ThemeSwitch from "./ThemeSwitch.svelte";

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

  <!-- Ruling 52: grouped with the Shell in spec, mounted here because
       IconRail is the only chrome no panel toggle can unmount -- it must
       stay reachable with every panel, including the editor, closed. -->
  <div class="mt-auto">
    <ThemeSwitch />
  </div>
</div>
