<script>
  import { onDestroy } from "svelte";

  // One component, both axes -- two gestures, one concept (spec §1).
  //
  // `open` is whether the SECOND pane is showing. When it is not, this
  // renders pane `a` alone and no handle: a handle that drags nothing is
  // worse than no handle.
  let { axis = "x", size = $bindable(300), open = true, min = 120, a, b } = $props();

  let host;
  let dragging = $state(false);
  let capturedPointerId = null;

  // Calculate max size based on container bounds (pane a must keep at least min pixels)
  let maxSize = $derived.by(() => {
    if (!host) return 9999;
    const r = host.getBoundingClientRect();
    const extent = axis === "x" ? r.width : r.height;
    return Math.max(min, extent - min);
  });

  function start(e) {
    dragging = true;
    e.preventDefault();
    // Capture pointer so pointerup fires even outside the viewport
    if (e.target.setPointerCapture) {
      e.target.setPointerCapture(e.pointerId);
      capturedPointerId = e.pointerId;
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }

  function move(e) {
    if (!host) return;
    const r = host.getBoundingClientRect();
    const next = axis === "x" ? r.right - e.clientX : r.bottom - e.clientY;
    const extent = axis === "x" ? r.width : r.height;
    const maxSize = extent - min;
    size = Math.max(min, Math.min(Math.round(next), maxSize));
  }

  function stop() {
    dragging = false;
    // Release pointer capture if it was set
    if (capturedPointerId !== null && host) {
      try {
        host.releasePointerCapture(capturedPointerId);
      } catch {
        // Environment may not support releasePointerCapture
      }
      capturedPointerId = null;
    }
    window.removeEventListener("pointermove", move);
  }

  // Clean up in case component unmounts while dragging
  onDestroy(() => {
    if (dragging) {
      stop();
    }
  });

  // Keyboard: a separator that only responds to a pointer is not reachable.
  function key(e) {
    if (!host) return;
    const step = e.shiftKey ? 48 : 12;
    const r = host.getBoundingClientRect();
    const extent = axis === "x" ? r.width : r.height;
    const maxSize = extent - min;
    let next = size;
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = size - step;
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") next = size + step;
    else return;
    size = Math.max(min, Math.min(next, maxSize));
    e.preventDefault();
  }
</script>

<div
  bind:this={host}
  class="flex min-h-0 min-w-0 flex-1 {axis === 'x' ? 'flex-row' : 'flex-col'}"
>
  <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">{@render a()}</div>

  {#if open}
    <!-- WAI-ARIA Window Splitter pattern: role="separator" requires div (not button) with
         tabindex, aria-value* attributes, and keyboard/pointer handlers.
         svelte-ignore: linter is overly strict; ARIA pattern is correct per spec. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      tabindex="0"
      aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-label="Resize panel"
      aria-valuenow={size}
      aria-valuemin={min}
      aria-valuemax={maxSize}
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
