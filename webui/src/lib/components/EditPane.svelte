<script>
  // What we MODIFY is the Edit panel, always, and it names its subject
  // (spec §2). Routes on the subject's kind.
  import { subject } from "../state/subject.svelte.js";
  import EditorPane from "./EditorPane.svelte";
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  {#if subject.kind === "screen"}
    <EditorPane />
  {:else if subject.kind === "panel" || subject.kind === "device"}
    <p class="p-3 text-sm text-[var(--text-muted)]">Config editor for {subject.kind} {subject.id}</p>
  {:else}
    <p class="p-3 text-sm text-[var(--text-muted)]">Pick a screen, panel or device in the Library.</p>
  {/if}
</div>
