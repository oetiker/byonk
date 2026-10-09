<script>
  // Palette swatches with H/S/L sliders, a hex box and an original/live pair.
  // `value` and `baseline` are comma-separated hex strings; `onchange` gets the
  // new comma string on every valid edit.
  import { hexToHsl, hslToHex } from "../utils/color.js";
  import { Label } from "$lib/ui/label/index.js";
  import { Input } from "$lib/ui/input/index.js";

  let { value = "", baseline = "", onchange } = $props();

  const HEX_RE = /^#?[0-9A-Fa-f]{6}$/;
  const split = (s) => (s ?? "").split(",").map((c) => c.trim()).filter(Boolean);

  let selected = $state(0);
  const colors = $derived(split(value));
  const original = $derived(split(baseline));
  const selectedHex = $derived(colors[selected] ?? "");
  const hsl = $derived(HEX_RE.test(selectedHex) ? hexToHsl(selectedHex) : { h: 0, s: 0, l: 0 });

  function setColor(i, hex) {
    const next = [...colors];
    next[i] = hex;
    onchange?.(next.join(","));
  }
  const onHue = (e) => setColor(selected, hslToHex(Number(e.currentTarget.value), hsl.s, hsl.l));
  const onSat = (e) => setColor(selected, hslToHex(hsl.h, Number(e.currentTarget.value), hsl.l));
  const onLight = (e) => setColor(selected, hslToHex(hsl.h, hsl.s, Number(e.currentTarget.value)));

  // Kept apart from `selectedHex` so a half-typed hex is not clobbered by the
  // canonical value; re-synced when the value or the selection changes.
  let hexDraft = $state("");
  $effect(() => {
    hexDraft = colors[selected] ?? "";
  });
  function onHexInput(e) {
    const raw = e.currentTarget.value.trim();
    if (!HEX_RE.test(raw)) return;
    setColor(selected, (raw.startsWith("#") ? raw : "#" + raw).toUpperCase());
  }
</script>

{#if colors.length}
  <div class="flex flex-col gap-1.5" data-testid="swatch-editor">
    <div role="radiogroup" aria-label="Swatches" class="flex flex-wrap gap-1.5">
      {#each colors as c, i (i)}
        <button
          type="button"
          role="radio"
          aria-checked={selected === i}
          aria-label={`Swatch ${i + 1}`}
          title={c}
          class="h-6 w-6 rounded border-2"
          style:background-color={c}
          style:border-color={selected === i ? "var(--ring)" : "var(--border-color)"}
          onclick={() => (selected = i)}
        ></button>
      {/each}
    </div>
    <Label for="swatch-hue">Hue</Label>
    <input id="swatch-hue" type="range" min="0" max="360" value={hsl.h} oninput={onHue} />
    <Label for="swatch-sat">Saturation</Label>
    <input id="swatch-sat" type="range" min="0" max="100" value={hsl.s} oninput={onSat} />
    <Label for="swatch-light">Lightness</Label>
    <input id="swatch-light" type="range" min="0" max="100" value={hsl.l} oninput={onLight} />
    <Label for="swatch-hex">Hex</Label>
    <Input id="swatch-hex" bind:value={hexDraft} oninput={onHexInput} />
    <div class="flex items-center gap-2 text-xs">
      <span class="text-[var(--text-muted)]">Original</span>
      <span class="h-5 w-5 rounded border border-[var(--border-color)]" style:background-color={original[selected]}></span>
      <span>{original[selected] ?? ""}</span>
      <span class="text-[var(--text-muted)]">Live</span>
      <span class="h-5 w-5 rounded border border-[var(--border-color)]" style:background-color={colors[selected]}></span>
      <span>{colors[selected]}</span>
    </div>
  </div>
{/if}
