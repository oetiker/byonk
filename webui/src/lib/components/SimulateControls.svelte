<script>
  import { simulate } from "../state/simulate.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Input } from "$lib/ui/input/index.js";

  // The default palette the server falls back to for the `og` model when
  // no panel is selected. Mirrors `DEFAULT_COLORS` in src/api/display.rs
  // (and `DEFAULTS.colors` in the old static/dev/dev.js) -- swatches-only,
  // the render query itself omits `colors` and lets the server apply the
  // same fallback (Controller Ruling 26).
  const DEFAULT_COLORS = "#000000,#555555,#AAAAAA,#FFFFFF";

  const selectClass =
    "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm " +
    "text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 " +
    "focus-visible:ring-ring/50 dark:bg-input/30";

  function splitColors(s) {
    return s
      ? s
          .split(",")
          .map((c) => c.trim())
          .filter((c) => c)
      : [];
  }

  const selectedPanel = $derived(
    screens.meta.panels?.find((p) => p.id === simulate.panel || p.name === simulate.panel) ??
      null,
  );

  // The old UI shows the actual (measured) colors only while "Show actual
  // panel colors" is checked and the chosen panel has any -- otherwise the
  // nominal palette, or the model default when no panel is chosen at all.
  const swatchColors = $derived(
    splitColors(
      (selectedPanel && simulate.use_actual && selectedPanel.colors_actual) ||
        selectedPanel?.colors ||
        DEFAULT_COLORS,
    ),
  );

  const ditherDefaults = $derived(
    screens.meta.dither_algorithms?.find((a) => a.name === simulate.dither)?.defaults ?? null,
  );

  // An untouched drawer asks for what the engine would have rendered anyway
  // (dev.js's populateDitherSelect: "the engine's own fallback").
  $effect(() => {
    if (simulate.dither) return;
    if (screens.meta.dither_algorithms?.some((a) => a.name === "atkinson")) {
      simulate.dither = "atkinson";
    }
  });

  function onDitherChange() {
    // Defaults first: a fresh algorithm selection starts on the engine's
    // own numbers for it, same as the old UI's applyDitherDefaults().
    if (ditherDefaults) {
      simulate.max_error = String(ditherDefaults.max_error);
      simulate.noise_scale = String(ditherDefaults.noise_scale);
    }
  }

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
     never overlaps the render. -->
<div class="flex flex-col gap-3 overflow-auto min-h-0 p-3 text-sm">
  <div class="flex flex-col gap-1.5">
    <Label for="sim-device">Device</Label>
    <select id="sim-device" class={selectClass} bind:value={simulate.mac}>
      <option value="">None (edited screen only)</option>
      {#each screens.meta.devices ?? [] as d (d.id)}
        <option value={d.id}>{d.id} &rarr; {d.screen}</option>
      {/each}
    </select>
  </div>

  {#if simulate.mac}
    <p class="text-xs text-[var(--text-muted)]">
      A device is selected: the preview renders <em>that device's</em> own configured screen, not
      the one open in the editor.
    </p>
  {/if}

  <div class="flex flex-col gap-1.5">
    <Label for="sim-panel">Panel Profile</Label>
    <select id="sim-panel" class={selectClass} bind:value={simulate.panel}>
      <option value="">None (use default colors)</option>
      {#each screens.meta.panels ?? [] as p (p.id)}
        <option value={p.id}>{p.name}</option>
      {/each}
    </select>
  </div>

  <div class="flex flex-wrap gap-1">
    {#each swatchColors as c, i (i)}
      <span
        class="h-5 w-5 rounded border border-[var(--border-color)]"
        style:background-color={c}
        title={c}
      ></span>
    {/each}
  </div>

  {#if selectedPanel?.colors_actual}
    <label class="flex items-center gap-2 text-xs">
      <input type="checkbox" bind:checked={simulate.use_actual} />
      Show actual panel colors
      <span class="text-[var(--text-muted)]">(dev preview only)</span>
    </label>
  {/if}

  <div class="flex flex-col gap-1.5">
    <Label for="sim-dither">Dither</Label>
    <select
      id="sim-dither"
      class={selectClass}
      bind:value={simulate.dither}
      onchange={onDitherChange}
    >
      {#each screens.meta.dither_algorithms ?? [] as a (a.name)}
        <option value={a.name}>{a.name}</option>
      {/each}
    </select>
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-max-error">Max error</Label>
    <Input
      id="sim-max-error"
      type="number"
      min="0"
      max="2.0"
      step="0.05"
      bind:value={simulate.max_error}
    />
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-chroma-clamp">Chroma clamp</Label>
    <Input
      id="sim-chroma-clamp"
      type="number"
      min="0.05"
      max="1.0"
      step="0.05"
      placeholder="&infin;"
      bind:value={simulate.chroma_clamp}
    />
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-noise-scale">Noise scale</Label>
    <Input
      id="sim-noise-scale"
      type="number"
      min="0"
      max="16.0"
      step="0.5"
      bind:value={simulate.noise_scale}
    />
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="sim-strength">Strength</Label>
    <Input
      id="sim-strength"
      type="number"
      min="0"
      max="2.0"
      step="0.05"
      bind:value={simulate.strength}
    />
  </div>

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

  <div class="flex flex-col gap-1.5">
    <Label for="sim-params">Custom Parameters (JSON)</Label>
    <textarea
      id="sim-params"
      class={selectClass + " h-20 resize-y py-1.5 font-mono"}
      bind:value={simulate.params}
    ></textarea>
  </div>
</div>
