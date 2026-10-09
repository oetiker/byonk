<script>
  import { simulate } from "../state/simulate.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Input } from "$lib/ui/input/index.js";

  const selectClass =
    "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm " +
    "text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 " +
    "focus-visible:ring-ring/50 dark:bg-input/30";

  // --- Current Time: a live clock that locks the moment it is edited -----

  let timeInput = $state(formatDatetimeLocal(new Date()));

  function formatDatetimeLocal(d) {
    const pad = (n) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  $effect(() => {
    const id = setInterval(() => {
      if (!simulate.timeLocked) timeInput = formatDatetimeLocal(new Date());
    }, 1000);
    return () => clearInterval(id);
  });

  function onTimeChange() {
    simulate.timestamp = Math.floor(new Date(timeInput).getTime() / 1000);
    simulate.timeLocked = true;
  }

  function resetTime() {
    simulate.timeLocked = false;
    timeInput = formatDatetimeLocal(new Date());
  }
</script>

<!-- Spec §1: this block scrolls inside whatever height it is given, and
     never overlaps the render. `h-full` is what makes that true: the slot
     it sits in (SplitPane's second pane) has a fixed height and clips its
     overflow, so without a height of its own the block was cut off at the
     bottom instead of scrolling. -->
<div class="flex h-full flex-col gap-3 overflow-auto min-h-0 p-3 text-sm" data-testid="simulated-inputs">
  {#if simulate.pending}
    <span data-testid="sim-badge" class="text-xs text-[var(--danger-color)]">not saved</span>
  {/if}
  <div class="flex flex-col gap-1.5">
    <Label for="sim-battery">Battery (V)</Label>
    <Input
      id="sim-battery"
      type="number"
      min="3.0"
      max="4.2"
      step="0.1"
      bind:value={simulate.battery_voltage}
    />
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-rssi">WiFi (dBm)</Label>
    <Input id="sim-rssi" type="number" min="-100" max="0" bind:value={simulate.rssi} />
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-time">
      Current Time
      <span class="text-xs text-[var(--text-muted)]">
        {simulate.timeLocked ? "frozen" : "live"}
      </span>
    </Label>
    <div class="flex items-center gap-1">
      <input
        id="sim-time"
        type="datetime-local"
        class={selectClass}
        bind:value={timeInput}
        onchange={onTimeChange}
      />
      {#if simulate.timeLocked}
        <button
          type="button"
          class="h-8 shrink-0 rounded-lg border border-input px-2 text-sm"
          title="Reset to live time"
          onclick={resetTime}
        >
          &times;
        </button>
      {/if}
    </div>
  </div>

  {#if subject.kind !== "device"}
    <div class="flex flex-col gap-1.5">
      <Label for="sim-params">Custom Parameters (JSON)</Label>
      <textarea
        id="sim-params"
        class={selectClass + " h-20 resize-y py-1.5 font-mono"}
        bind:value={simulate.params}
      ></textarea>
    </div>
  {/if}
</div>
