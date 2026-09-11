<script>
  import { onDestroy } from "svelte";
  import { tune } from "../state/tune.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { hexToHsl, hslToHex } from "../utils/color.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Input } from "$lib/ui/input/index.js";
  import { Button } from "$lib/ui/button/index.js";

  const HEX_RE = /^#?[0-9A-Fa-f]{6}$/;

  const selectClass =
    "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm " +
    "text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 " +
    "focus-visible:ring-ring/50 dark:bg-input/30";

  // The panel selector defaults to the panel Simulate has chosen (spec §8),
  // but only once, on mount: once `tune.panel` is set (by this, by the
  // device selector below, or by the user), it is never overwritten just
  // because `simulate.panel` changes underneath it -- that would silently
  // swap a calibration in progress out from under the user.
  $effect(() => {
    if (!tune.panel && simulate.panel) {
      tune.selectPanel(simulate.panel);
    }
  });

  const selectedDevice = $derived(screens.meta.devices?.find((d) => d.id === tune.device) ?? null);
  const deviceMissingPanel = $derived(!!tune.device && selectedDevice && !selectedDevice.panel);

  async function onDeviceChange(e) {
    const key = e.currentTarget.value;
    await tune.selectDevice(key);
    if (!key) return;
    const dev = screens.meta.devices?.find((d) => d.id === key);
    if (dev?.panel) {
      tune.selectPanel(dev.panel);
    }
  }

  function onPanelChange(e) {
    tune.selectPanel(e.currentTarget.value);
  }

  // The selected swatch, decomposed into H/S/L for the sliders. Falls back
  // to black when nothing is selected yet, or the working copy holds
  // something that isn't a clean 6-digit hex (shouldn't happen -- every
  // write goes through the same HEX_RE guard the hex box uses below -- but
  // a slider must not throw on a malformed value it didn't create).
  const selectedHex = $derived(tune.colors[tune.selected] ?? "");
  const hsl = $derived(HEX_RE.test(selectedHex) ? hexToHsl(selectedHex) : { h: 0, s: 0, l: 0 });

  function onHueInput(e) {
    tune.setColor(tune.selected, hslToHex(Number(e.currentTarget.value), hsl.s, hsl.l));
  }
  function onSatInput(e) {
    tune.setColor(tune.selected, hslToHex(hsl.h, Number(e.currentTarget.value), hsl.l));
  }
  function onLightInput(e) {
    tune.setColor(tune.selected, hslToHex(hsl.h, hsl.s, Number(e.currentTarget.value)));
  }

  // The hex box's own draft text. Kept separate from `selectedHex` so that
  // an invalid in-progress edit (a partial hex, a typo) is not clobbered by
  // the canonical value on every keystroke -- it is re-synced only when the
  // working copy actually changes (a valid edit, a different swatch picked,
  // a push/save/discard) or the selection moves.
  let hexDraft = $state("");
  $effect(() => {
    hexDraft = tune.colors[tune.selected] ?? "";
  });

  function onHexInput(e) {
    const raw = e.currentTarget.value.trim();
    if (!HEX_RE.test(raw)) return;
    const hex = (raw.startsWith("#") ? raw : "#" + raw).toUpperCase();
    tune.setColor(tune.selected, hex);
  }

  // Closing the panel gives up the server-side override and the session. The
  // working copy survives, because it lives in the store.
  onDestroy(() => {
    tune.release();
  });
</script>

<div data-testid="tune-pane" class="flex h-full flex-col gap-3 overflow-auto min-h-0 p-3 text-sm">
  <h2 class="font-medium">Tune</h2>

  <div class="flex flex-col gap-1.5">
    <Label for="tune-panel">Panel</Label>
    <select id="tune-panel" class={selectClass} value={tune.panel} onchange={onPanelChange}>
      <option value="">Select a panel</option>
      {#each screens.meta.panels ?? [] as p (p.id)}
        <option value={p.id}>{p.name}</option>
      {/each}
    </select>
  </div>

  <div class="flex flex-col gap-1.5">
    <Label for="tune-device">Device</Label>
    <select id="tune-device" class={selectClass} value={tune.device} onchange={onDeviceChange}>
      <option value="">None</option>
      {#each screens.meta.devices ?? [] as d (d.id)}
        <option value={d.id}>{d.id}</option>
      {/each}
    </select>
  </div>

  {#if deviceMissingPanel}
    <p class="text-xs text-[var(--text-muted)]">
      This device's panel is not set in config — pick the panel above.
    </p>
  {/if}

  {#if tune.colors.length}
    <div role="radiogroup" aria-label="Swatches" class="flex flex-wrap gap-1.5">
      {#each tune.colors as c, i (i)}
        <button
          type="button"
          role="radio"
          aria-checked={tune.selected === i}
          aria-label={`Swatch ${i + 1}`}
          title={c}
          tabindex="0"
          class="h-6 w-6 rounded border-2"
          style:background-color={c}
          style:border-color={tune.selected === i ? "var(--ring)" : "var(--border-color)"}
          onclick={() => (tune.selected = i)}
        ></button>
      {/each}
    </div>

    <div class="flex flex-col gap-1.5">
      <Label for="tune-hue">Hue</Label>
      <input
        id="tune-hue"
        type="range"
        min="0"
        max="360"
        value={hsl.h}
        oninput={onHueInput}
      />
    </div>

    <div class="flex flex-col gap-1.5">
      <Label for="tune-sat">Saturation</Label>
      <input
        id="tune-sat"
        type="range"
        min="0"
        max="100"
        value={hsl.s}
        oninput={onSatInput}
      />
    </div>

    <div class="flex flex-col gap-1.5">
      <Label for="tune-light">Lightness</Label>
      <input
        id="tune-light"
        type="range"
        min="0"
        max="100"
        value={hsl.l}
        oninput={onLightInput}
      />
    </div>

    <div class="flex flex-col gap-1.5">
      <Label for="tune-hex">Hex</Label>
      <Input id="tune-hex" bind:value={hexDraft} oninput={onHexInput} />
    </div>

    <div class="flex items-center gap-2 text-xs">
      <span class="text-[var(--text-muted)]">Original</span>
      <span
        class="h-5 w-5 rounded border border-[var(--border-color)]"
        style:background-color={tune.baseline[tune.selected]}
      ></span>
      <span>{tune.baseline[tune.selected]}</span>
      <span class="text-[var(--text-muted)]">Live</span>
      <span
        class="h-5 w-5 rounded border border-[var(--border-color)]"
        style:background-color={tune.colors[tune.selected]}
      ></span>
      <span>{tune.colors[tune.selected]}</span>
    </div>

    <div class="flex flex-wrap gap-2">
      <Button type="button" disabled={!tune.panel} onclick={() => tune.push()}>
        Push to panel
      </Button>
      <Button
        type="button"
        variant="outline"
        disabled={!tune.dirty}
        onclick={() => tune.save()}
      >
        Save to config
      </Button>
      <Button
        type="button"
        variant="ghost"
        disabled={!tune.pushed && !tune.dirty}
        onclick={() => tune.discard()}
      >
        Discard
      </Button>
    </div>
  {/if}
</div>
