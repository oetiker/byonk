<script>
  // One component, both axes -- two gestures, one concept (spec §1).
  //
  // `open` is whether the SECOND pane is showing. When it is not, this
  // renders pane `a` alone and no handle: a handle that drags nothing is
  // worse than no handle.
  let { axis = "x", size = $bindable(300), open = true, min = 120, a, b } = $props();

  let host;
  let dragging = $state(false);

  function start(e) {
    dragging = true;
    e.preventDefault();
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }

  function move(e) {
    if (!host) return;
    const r = host.getBoundingClientRect();
    const next = axis === "x" ? r.right - e.clientX : r.bottom - e.clientY;
    size = Math.max(min, Math.round(next));
  }

  function stop() {
    dragging = false;
    window.removeEventListener("pointermove", move);
  }

  // Keyboard: a separator that only responds to a pointer is not reachable.
  function key(e) {
    const step = e.shiftKey ? 48 : 12;
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") size = Math.max(min, size - step);
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") size = size + step;
    else return;
    e.preventDefault();
  }
</script>

<div
  bind:this={host}
  class="flex min-h-0 min-w-0 flex-1 {axis === 'x' ? 'flex-row' : 'flex-col'}"
>
  <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">{@render a()}</div>

  {#if open}
    <div
      role="separator"
      tabindex="0"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label="Resize panel"
      class="shrink-0 bg-[var(--border-color)] hover:bg-[var(--accent-color)]
             {axis === 'x' ? 'w-px cursor-col-resize' : 'h-px cursor-row-resize'}
             {dragging ? 'bg-[var(--accent-color)]' : ''}"
      onpointerdown={start}
      onkeydown={key}
    ></div>
    <div
      class="min-h-0 min-w-0 shrink-0 overflow-hidden"
      style={axis === "x" ? `width:${size}px` : `height:${size}px`}
    >
      {@render b()}
    </div>
  {/if}
</div>
