<script>
  import SplitPane from "../src/lib/components/SplitPane.svelte";
  let { open = true } = $props();
  let size = $state(300);
</script>

<SplitPane axis="x" bind:size {open}>
  {#snippet a()}<div>left</div>{/snippet}
  {#snippet b()}<div>right</div>{/snippet}
</SplitPane>
