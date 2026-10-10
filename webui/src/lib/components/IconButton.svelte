<script>
  // An icon-only Button with its label as an accessible name and a tooltip
  // (docs/dev-ui-design.md §5: tooltip + aria-label, not title=).
  import { Button } from "$lib/ui/button/index.js";
  import * as Tooltip from "$lib/ui/tooltip/index.js";

  let { icon: Icon, label, tip = label, size = "icon-sm", variant = "ghost", ...rest } = $props();
</script>

<!-- Own provider: panes render standalone (tests, pop-out), so no ancestor is assumed. -->
<Tooltip.Provider>
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <Button {...props} type="button" {size} {variant} aria-label={label} {...rest}>
        <Icon aria-hidden="true" />
      </Button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content>{tip}</Tooltip.Content>
</Tooltip.Root>
</Tooltip.Provider>
