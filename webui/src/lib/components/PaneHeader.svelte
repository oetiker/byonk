<script>
  // Row 1 of every pane (docs/dev-ui-design.md §2). Height, gutter and bottom
  // border come from here, never from the pane: that is what keeps the line
  // straight across Library, Edit and Preview. No wrapping; the subject
  // truncates.
  //
  // The header is a size container (`@container`): a pane whose header gets
  // too narrow for its actions switches them to a compact form with
  // `@max-[…]:` variants on the actions themselves -- CSS, not JS
  // measurement (fix round 1, findings 2 and 3).
  let { title, subject, actions, ...rest } = $props();
</script>

<header
  data-slot="pane-header"
  class="@container flex h-(--pane-header-h) min-w-0 shrink-0 items-center gap-2 border-b border-border px-2 text-xs"
  {...rest}
>
  <span class="shrink-0 font-semibold uppercase tracking-wide text-muted-foreground">{title}</span>
  <div data-slot="pane-header-subject" class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">{@render subject?.()}</div>
  <div class="flex shrink-0 items-center gap-1">{@render actions?.()}</div>
</header>
