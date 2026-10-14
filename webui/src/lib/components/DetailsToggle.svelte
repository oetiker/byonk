<script>
  // The "…" in an Edit pane header that shows or hides the Details block.
  // `forced` means the block must stay open (unsaved edits, a failed Save):
  // the button then shows it expanded and does nothing, so edits never hide.
  import { Ellipsis } from "@lucide/svelte";
  import IconButton from "./IconButton.svelte";

  let { expanded, forced = false, controls, onToggle } = $props();
</script>

<IconButton
  icon={Ellipsis}
  label="Details"
  tip={forced ? "Details stay open while they hold unsaved changes" : "Show or hide the Details"}
  variant={expanded ? "secondary" : "outline"}
  aria-expanded={expanded}
  aria-controls={controls}
  aria-disabled={forced ? "true" : undefined}
  onclick={() => !forced && onToggle(!expanded)}
/>
