<script>
  import { onDestroy } from "svelte";

  // One component, both axes -- two gestures, one concept (spec §1).
  //
  // `open` is whether the SECOND pane is showing. When it is not, this
  // renders pane `a` alone and no handle: a handle that drags nothing is
  // worse than no handle.
  let { axis = "x", size = $bindable(300), open = true, min = 120, a, b } = $props();

  // `host` must be reactive: a plain `let host` assigned via `bind:this`
  // does NOT invalidate anything that reads it, so a `$derived` built on top
  // of it would run once before mount (host undefined) and never again.
  let host = $state();
  let dragging = $state(false);
  let capturedPointerId = null;
  let capturedTarget = null;

  // Upper bound on `size`, kept in `$state` and refreshed explicitly rather
  // than computed as a `$derived` from `host.getBoundingClientRect()`:
  // `getBoundingClientRect()` reads live layout, which is not itself
  // reactive, so a `$derived` would still go stale on a window/container
  // resize the user didn't cause by touching the splitter.
  //
  // Tradeoff (chosen deliberately, not the only valid answer): we refresh
  // this from a fresh rect on mount and on every move()/key() call -- both
  // already read a fresh rect for their own math, so this piggybacks on
  // that for free. A ResizeObserver would also catch resizes that happen
  // without the user touching the splitter (browser window resize, a
  // sibling panel collapsing), keeping aria-valuemax always current, at the
  // cost of an observer instance per SplitPane and a mock in every test
  // that touches this component. We accepted the gap instead: aria-valuemax
  // can go stale between an external resize and the next drag/keypress on
  // THIS separator. That only matters to a screen reader user who queries
  // valuemax mid-resize without interacting with the control -- a narrow
  // window we judged not worth the added machinery here.
  let maxSize = $state(undefined);

  function refreshMaxSize() {
    if (!host) return;
    const r = host.getBoundingClientRect();
    const extent = axis === "x" ? r.width : r.height;
    // A zero (or negative) extent -- an unmounted/unmeasured element, a
    // genuinely zero-size container, or jsdom's default all-zero
    // getBoundingClientRect in tests -- cannot be bounded against: `extent
    // - min` would be negative, and clamping against a negative max snaps
    // `size` down to `min` on every move/key, including a *grow* press.
    // Skip the upper clamp entirely rather than compute a bound that lies.
    maxSize = extent > 0 ? Math.max(min, extent - min) : undefined;
  }

  // Single source of truth for the [min, maxSize] clamp, used by move() and
  // key() alike so the rule is defined once, not duplicated per gesture.
  function clamp(value) {
    const bounded = maxSize === undefined ? value : Math.min(value, maxSize);
    return Math.max(min, Math.round(bounded));
  }

  // Establish a real maxSize as soon as the container is mounted and
  // measurable, so aria-valuemax is never served the pre-mount fallback.
  $effect(() => {
    if (host) refreshMaxSize();
  });

  function start(e) {
    dragging = true;
    e.preventDefault();
    // Capture pointer so pointerup fires even outside the viewport. Capture
    // is requested on e.target (the separator itself) -- release must
    // happen on that same element (see stop()).
    if (e.target.setPointerCapture) {
      e.target.setPointerCapture(e.pointerId);
      capturedPointerId = e.pointerId;
      capturedTarget = e.target;
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }

  function move(e) {
    if (!host) return;
    const r = host.getBoundingClientRect();
    const next = axis === "x" ? r.right - e.clientX : r.bottom - e.clientY;
    refreshMaxSize();
    size = clamp(next);
  }

  function stop() {
    dragging = false;
    // Release capture on the element that received it (the separator),
    // not on `host`. Per the Pointer Events spec, releasing on an element
    // that does not hold the capture is a no-op -- releasing on `host`
    // here would silently do nothing while claiming cleanup.
    if (capturedTarget && capturedPointerId !== null) {
      try {
        capturedTarget.releasePointerCapture(capturedPointerId);
      } catch {
        // Already released implicitly (pointerup/pointercancel/DOM removal
        // all release capture automatically per spec) -- nothing to do.
      }
    }
    capturedPointerId = null;
    capturedTarget = null;
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
    const step = e.shiftKey ? 48 : 12;
    let next = size;
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = size - step;
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") next = size + step;
    else return;
    refreshMaxSize();
    size = clamp(next);
    e.preventDefault();
  }
</script>

<div
  bind:this={host}
  class="flex min-h-0 min-w-0 flex-1 {axis === 'x' ? 'flex-row' : 'flex-col'}"
>
  <div class="flex min-h-0 min-w-0 flex-1 overflow-hidden">{@render a()}</div>

  {#if open}
    <!-- WAI-ARIA Window Splitter pattern: role="separator" requires a div
         (not button) carrying tabindex, aria-value* attributes, and
         keyboard/pointer handlers -- the correct pattern per spec, but
         Svelte's static a11y table doesn't model this conditional
         interactivity, so it warns on both.
         Each warning code needs its OWN svelte-ignore comment line here.
         A single svelte-ignore comment combining both codes on one line
         fails to suppress a11y_no_noninteractive_element_interactions once
         this element sits inside a conditional block nested under an
         ancestor with bind:this (this file's own host div) -- verified
         directly with svelte.compile(): the combined-comment form left
         that one warning firing while a11y_no_noninteractive_tabindex was
         suppressed; splitting into two single-code comments suppresses
         both. Reproduced and confirmed on svelte 5.57.0. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
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
    <!-- `flex`, like pane `a`: a flex-1 child (a nested SplitPane root)
         only fills this pane if the pane is a flex container; as a block it
         sat at content height and left dead space below. -->
    <div
      class="flex min-h-0 min-w-0 shrink-0 overflow-hidden"
      style={axis === "x" ? `width:${size}px` : `height:${size}px`}
    >
      {@render b()}
    </div>
  {/if}
</div>
