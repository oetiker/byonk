<script>
  import { onMount } from "svelte";
  import { simulate } from "../state/simulate.svelte.js";
  import { screens } from "../state/screens.svelte.js";

  let blobUrl = $state(null);
  let channel;
  let lastBlob = null;

  onMount(() => {
    channel = new BroadcastChannel("byonk-preview");
    // A reloaded pop-out says hello; answer with the current render so it
    // recovers instead of sitting blank (spec §8.4). There is nothing to
    // answer with until a render has actually completed -- `lastBlob` stays
    // null until then, which is deliberate (Controller Ruling 29): a
    // pop-out opened before any screen is selected has no "current image"
    // to recover.
    channel.onmessage = (e) => {
      if (e.data?.hello && lastBlob) channel.postMessage({ blob: lastBlob });
    };
    return () => channel?.close();
  });

  /**
   * Render the current screen with the current simulate controls.
   *
   * Two failures are caught here instead of becoming unhandled rejections
   * (Controller Ruling 41):
   *
   *  - `simulate.url(ref)` THROWS on invalid params JSON (Task 8 rule (e),
   *    simulate.svelte.js) -- deliberately, so the user is never shown a
   *    render made from parameters they didn't write.
   *  - the server can answer a real render request with a JSON
   *    `RenderErrorResponse { error, details }` (src/api/dev.rs:157-161)
   *    instead of an image; `!res.ok` must not be a silent no-op.
   *
   * Both land on `simulate.error`, in the `{ op, message }` shape
   * `screens.error` uses (simulate.svelte.js, screens.svelte.js) --
   * rendering it is Task 10's job, the same split `screens.error` has with
   * `FilesPane`. A fresh attempt clears the previous failure first, same
   * reasoning as `FilesPane`'s `run()`, so a stale failure can't outlive a
   * later success.
   */
  async function renderNow() {
    const ref = screens.selected;
    if (!ref) return;
    simulate.error = null;

    try {
      const url = simulate.url(ref);
      const res = await fetch(url);
      if (!res.ok) {
        const body = await res.json().catch(() => null);
        const message = body?.details
          ? `${body.error}: ${body.details}`
          : (body?.error ?? `${res.status} ${res.statusText}`);
        throw new Error(message);
      }
      const blob = await res.blob();
      if (blobUrl) URL.revokeObjectURL(blobUrl);
      blobUrl = URL.createObjectURL(blob);
      lastBlob = blob;
      channel?.postMessage({ blob });
    } catch (e) {
      simulate.error = { op: "render", message: e.message };
    }
  }

  // Re-render whenever the screen or any simulate control changes. This is
  // what makes the pop-out follow a drawer change.
  $effect(() => {
    screens.selected;
    simulate.fields;
    renderNow();
  });

  function popOut() {
    // Relative, and named. Both were proven under HA Ingress on 2026-09-09:
    // a root-absolute or origin-absolute URL leaves the HA iframe, and an
    // unnamed window stacks a new one per click instead of reusing it.
    window.open("preview.html", "byonk-preview");
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center gap-2 border-b border-[var(--border-color)] px-2 py-1 text-xs">
    <span class="font-mono">{simulate.size.width} × {simulate.size.height}</span>
    <button type="button" class="ml-auto" onclick={popOut}>Pop out</button>
  </div>
  <div class="flex min-h-0 flex-1 items-center justify-center p-4">
    <img alt="Screen preview" src={blobUrl} class="max-h-full max-w-full object-contain" />
  </div>
</div>
