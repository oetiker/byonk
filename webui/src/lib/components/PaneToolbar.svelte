<script>
  // Optional row 2 of a pane (docs/dev-ui-design.md §2): file tabs, view
  // options. Same shape as PaneHeader at the toolbar height. `flush` drops the
  // gutter for a tab strip whose tabs carry their own padding (rule A4).
  let { children, flush = false, ...rest } = $props();
</script>

<div
  data-slot="pane-toolbar"
  class="flex h-(--pane-toolbar-h) min-w-0 shrink-0 items-center gap-2 border-b border-border text-xs {flush ? '' : 'px-2'}"
  {...rest}
>
  {@render children?.()}
</div>
