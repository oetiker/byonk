<script>
  import { simulate } from "../state/simulate.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Input } from "$lib/ui/input/index.js";
  import { Textarea } from "$lib/ui/textarea/index.js";
  import IconButton from "./IconButton.svelte";
  import { X } from "@lucide/svelte";

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

<!-- Spec §1: shown in full at its natural height; AppShell wraps it in a
     capped, scrolling box for windows too short for both it and the render. -->
<div class="flex w-full min-w-0 flex-col gap-3 p-3 text-sm" data-testid="simulated-inputs">
  {#if simulate.pending}
    <span data-testid="sim-badge" class="text-xs text-destructive">not saved</span>
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
      <span class="text-xs text-muted-foreground">
        {simulate.timeLocked ? "frozen" : "live"}
      </span>
    </Label>
    <div class="flex items-center gap-1">
      <Input id="sim-time" type="datetime-local" bind:value={timeInput} onchange={onTimeChange} />
      {#if simulate.timeLocked}
        <IconButton icon={X} label="Reset to live time" onclick={resetTime} variant="outline" class="shrink-0" />
      {/if}
    </div>
  </div>

  {#if subject.kind !== "device"}
    <div class="flex flex-col gap-1.5">
      <Label for="sim-params">Custom Parameters (JSON)</Label>
      <Textarea id="sim-params" class="min-h-20 resize-y font-mono" bind:value={simulate.params} />
    </div>
  {/if}
</div>
