<script>
  // What we MODIFY is the Edit panel, always, and it names its subject
  // (spec §2). Routes on the subject's kind.
  import { subject } from "../state/subject.svelte.js";
  import EditorPane from "./EditorPane.svelte";
  import ConfigEditor from "./ConfigEditor.svelte";
  import ScreenRepoEditor from "./ScreenRepoEditor.svelte";
  import PaneHeader from "./PaneHeader.svelte";
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  {#if subject.kind === "screen"}
    <EditorPane />
  {:else if subject.kind === "panel" || subject.kind === "device"}
    {#key subject.key}<ConfigEditor />{/key}
  {:else if subject.kind === "screen-repo"}
    {#key subject.key}<ScreenRepoEditor />{/key}
  {:else}
    <PaneHeader title="Edit" />
    <p class="p-3 text-sm text-muted-foreground">Pick a screen, panel, device or screen repo in the Library.</p>
  {/if}
</div>
