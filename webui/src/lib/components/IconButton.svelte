<script>
  // An icon-only Button with its label as an accessible name and a tooltip
  // (docs/dev-ui-design.md §5: tooltip + aria-label, not title=).
  import { Button } from "$lib/ui/button/index.js";
  import * as Tooltip from "$lib/ui/tooltip/index.js";
  import { mergeProps } from "bits-ui";

  let { icon: Icon, label, tip = label, size = "icon-sm", variant = "ghost", ...rest } = $props();
</script>

<!-- Own provider: panes render standalone (tests, pop-out), so no ancestor is assumed. -->
<Tooltip.Provider>
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <!-- mergeProps chains handlers: a caller's onclick runs after the trigger's own
           (which closes the tooltip) instead of replacing it. -->
      <Button {...mergeProps(props, { type: "button", size, variant, "aria-label": label }, rest)}>
        <Icon aria-hidden="true" />
      </Button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content>{tip}</Tooltip.Content>
</Tooltip.Root>
</Tooltip.Provider>
