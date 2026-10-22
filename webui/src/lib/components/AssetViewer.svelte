<script>
  import * as api from "../api.js";
  import { editor } from "../state/editor.svelte.js";

  /** The selected binary file: an image preview, or its size and a download link. */
  let { asset } = $props();

  const IMAGE = /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/i;
  const entry = $derived(editor.files.find((f) => f.name === asset.name));
  const size = $derived(entry?.size ?? 0);
  const isImage = $derived(IMAGE.test(asset.name));
  // size + assetRev: the listing carries no etag, and any change of bytes moves one of them.
  const url = $derived(api.rawUrl(asset.ref, asset.name, `${size}-${editor.assetRev}`));

  let dims = $state(null);
  $effect(() => {
    url;
    dims = null;
  });
</script>

<div data-testid="asset-viewer" class="flex min-h-0 flex-1 flex-col gap-2 overflow-auto p-3 text-sm">
  <p class="font-mono text-xs text-muted-foreground">
    <span class="text-foreground">{asset.name}</span>
    {#if isImage && dims} · {dims.w} × {dims.h} px{/if}
    · {size} bytes
  </p>
  {#if isImage}
    <img
      src={url}
      alt={asset.name}
      class="max-h-full max-w-full self-start object-contain"
      onload={(e) => (dims = { w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
    />
  {:else}
    <p>binary file, {size} bytes</p>
  {/if}
  <a class="self-start text-xs underline" href={url} download={asset.name}>Download {asset.name}</a>
</div>
