<script>
  // A text action in a PaneHeader that turns compact when the header (a
  // size container, see PaneHeader.svelte) is narrower than 22rem, so the
  // subject keeps room (fix round 1, finding 3):
  //   - with `short`, the label shortens ("Push to device" -> "Push");
  //   - otherwise the button goes icon-only (`icon`).
  // The full label is always the accessible name and the tooltip, so the
  // compact form loses nothing for a screen reader or a hovering pointer.
  import { Button } from "$lib/ui/button/index.js";
  import * as Tooltip from "$lib/ui/tooltip/index.js";

  let { icon: Icon = null, label, short = null, tip = label, variant = "outline", class: className = "", ...rest } = $props();
  const iconOnly = $derived(!short && Icon);
</script>

<Tooltip.Provider>
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        type="button"
        size="sm"
        {variant}
        aria-label={label}
        class="{iconOnly ? '@max-[22rem]:size-7 @max-[22rem]:px-0' : ''} {className}"
        {...rest}
      >
        {#if iconOnly}
          <Icon aria-hidden="true" class="hidden @max-[22rem]:block" />
          <span class="@max-[22rem]:hidden">{label}</span>
        {:else if short}
          <span class="@max-[22rem]:hidden">{label}</span>
          <span class="hidden @max-[22rem]:inline">{short}</span>
        {:else}
          {label}
        {/if}
      </Button>
    {/snippet}
  </Tooltip.Trigger>
  <Tooltip.Content>{tip}</Tooltip.Content>
</Tooltip.Root>
</Tooltip.Provider>
