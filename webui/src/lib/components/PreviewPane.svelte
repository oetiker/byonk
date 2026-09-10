<script>
  import { onMount } from "svelte";
  import { simulate } from "../state/simulate.svelte.js";
  import { screens } from "../state/screens.svelte.js";

  let blobUrl = $state(null);
  let channel;
  let lastBlob = null;

  // --- device frame -------------------------------------------------------
  //
  // Spec §"work areas" (docs/superpowers/specs/2026-09-09-dev-ui-svelte-
  // design.md:290) lists "device frame" and "zoom" as PreviewPane
  // deliverables alongside aspect-ratio handling and the pop-out button --
  // this task shipped only the last two in round 1 (Controller Ruling 42).
  //
  // The frame class rule is ported verbatim from the old UI's
  // `updateDeviceFrame()` (static/dev/dev.js:262): a request for more than
  // 1000px on either axis gets the "x" bezel, everything else gets "og".
  //
  // Unlike the old page, this reads `simulate.size` -- what the page ASKS
  // for -- and nothing else. The old page had a SECOND path
  // (`updateDisplaySizeFromImage`, dev.js:270) that corrected the class
  // from the decoded PNG's actual `naturalWidth`/`naturalHeight`, because
  // its render request could omit width/height and let the server fall
  // back to a model default the page didn't know ahead of time. That case
  // doesn't exist here: `simulate.url()` always sends explicit `width` and
  // `height` computed from this same `simulate.size` (simulate.svelte.js),
  // and `resolve_preview_dimensions` (src/api/display.rs) honors an
  // explicit width/height whenever the query carries one -- so the
  // request and the response are always the same size, and a second,
  // decode-time correction would be dead code here.
  let frameClass = $derived(
    simulate.size.width > 1000 || simulate.size.height > 1000 ? "x" : "og",
  );

  // --- 1:1 pixel lens -------------------------------------------------------
  //
  // A native reimplementation, not a reuse of `static/dev/lens.js`. That
  // file is a classic script with top-level mutable globals and a single
  // hard-coded `document.getElementById("display-image")` -- built for
  // exactly one `<script src>` inclusion per static HTML document (its own
  // header comment says as much), which is how Task 11 keeps using it
  // for `static/dev/preview.html` and login's sibling. Importing it here
  // would mean either turning it into an ES module (changing its shape
  // right before Task 11 restyles it as the hand-written file it is
  // today -- explicitly out of scope this round) or loading it a second
  // time as an extra runtime asset outside the three-file build contract
  // (index.html/app.js/app.css, vite.config.js) this SPA is held to. Doing
  // the same magnification in a few component-scoped lines avoids both,
  // and fixes the single-instance assumption as a side effect.
  //
  // Gated on `blobUrl` rather than `img.naturalWidth`/`.complete` (what
  // `lens.js` checks): jsdom never populates either for a `blob:` URL --
  // confirmed directly, they stay `0`/`false` indefinitely -- so `blobUrl`
  // (this component's own "a render actually completed" signal, already
  // used for the hello handshake above) is the only readiness check a
  // test here can actually drive. It is also the more correct one: it
  // reflects OUR state, not a decode race with the browser.
  let lensVisible = $state(false);
  let lensX = $state(0);
  let lensY = $state(0);
  let imgEl;
  // `$state`, unlike `imgEl` above: this binding sits inside the
  // `{#if lensVisible}` block, so Svelte needs it reactive to notice the
  // element mounting/unmounting there (confirmed by running the suite
  // without `$state()` here first -- it warned "lensCanvasEl is updated,
  // but is not declared with $state(...)" at svelte.dev/e/non_reactive_update).
  let lensCanvasEl = $state(null);

  function showLens(e) {
    if (!blobUrl) return; // nothing rendered yet -- nothing to magnify
    lensVisible = true;
    moveLens(e);
  }

  function hideLens() {
    lensVisible = false;
  }

  function moveLens(e) {
    if (!blobUrl) return;
    const lensSize = 200;
    lensX = e.clientX + 20;
    lensY = e.clientY - lensSize - 10;

    // Canvas 2D is not implemented in this project's jsdom test
    // environment ("Not implemented: HTMLCanvasElement's getContext()" --
    // confirmed directly, and installing the `canvas` npm package to
    // paper over it would add a dependency this branch's constraints
    // forbid). `ctx` is null there, and real magnification is untestable
    // here for the same reason -- deferred to Task 14's browser list, same
    // as the aspect-ratio check (Ruling 30).
    const ctx = lensCanvasEl?.getContext?.("2d");
    if (!ctx || !imgEl) return;
    const rect = imgEl.getBoundingClientRect();
    if (!imgEl.naturalWidth || !imgEl.naturalHeight || !rect.width || !rect.height) return;

    const lensRadius = lensSize / 2;
    const magnification = 2;
    const sourceSize = lensSize / magnification;

    const imgX = e.clientX - rect.left;
    const imgY = e.clientY - rect.top;
    const scaleX = imgEl.naturalWidth / rect.width;
    const scaleY = imgEl.naturalHeight / rect.height;
    const srcW = sourceSize * scaleX;
    const srcH = sourceSize * scaleY;
    const srcX = imgX * scaleX - srcW / 2;
    const srcY = imgY * scaleY - srcH / 2;

    ctx.clearRect(0, 0, lensSize, lensSize);
    ctx.save();
    ctx.beginPath();
    ctx.arc(lensRadius, lensRadius, lensRadius - 2, 0, Math.PI * 2);
    ctx.clip();
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(imgEl, srcX, srcY, srcW, srcH, 0, 0, lensSize, lensSize);
    ctx.restore();
  }

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
    <div class={`device-frame ${frameClass} flex items-center justify-center rounded-xl bg-[var(--panel-bg)] p-6`}>
      <div class="device-bezel rounded-lg bg-[var(--border-color)] p-3">
        <div class="screen-well overflow-hidden rounded-sm bg-[var(--bg-color)]">
          <img
            bind:this={imgEl}
            alt="Screen preview"
            src={blobUrl}
            class={`max-h-full max-w-full object-contain ${frameClass === "og" ? "[image-rendering:pixelated]" : ""}`}
            onmouseenter={showLens}
            onmouseleave={hideLens}
            onmousemove={moveLens}
          />
        </div>
      </div>
    </div>
  </div>
</div>

{#if lensVisible}
  <div
    class="lens pointer-events-none fixed z-50 h-[200px] w-[200px] overflow-hidden rounded-full border-2 border-[var(--accent-color)] bg-[var(--bg-color)] shadow-lg"
    style={`left:${lensX}px; top:${lensY}px;`}
  >
    <canvas bind:this={lensCanvasEl} width="200" height="200" class="block"></canvas>
  </div>
{/if}
