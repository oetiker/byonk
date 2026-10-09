<script>
  import { onDestroy, onMount, untrack } from "svelte";
  import * as api from "../api.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import { resolution, parseResolutionHeader } from "../state/resolution.svelte.js";
  import { buildRenderRequest, previewSize, devicePanel } from "../render-request.js";
  import { Button } from "$lib/ui/button/index.js";

  // The render effect must not track the draft: a draft edit is debounced
  // (P2-7) and driven by `configDraft.revision`, while a subject, target or
  // simulated-input change renders at once. Reading the draft untracked keeps
  // the two paths apart; `usage` stays tracked, a new sighting changes the size.
  const draft = {
    panelWire: (id) => untrack(() => configDraft.panelWire(id)),
    deviceWire: (id) => untrack(() => configDraft.deviceWire(id)),
  };

  const size = $derived(previewSize({ subject, meta: screens.meta, usage: configDraft.usage }));

  const selectClass =
    "h-7 min-w-0 rounded-lg border border-input bg-transparent px-2 text-xs text-foreground";
  const targetValue = $derived(subject.target ? `${subject.target.kind}:${subject.target.id}` : "");
  function onTarget(e) {
    const v = e.currentTarget.value;
    if (!v) {
      subject.target = null;
      return;
    }
    const i = v.indexOf(":");
    subject.target = { kind: v.slice(0, i), id: v.slice(i + 1) };
  }
  const asLabel = $derived(
    subject.kind === "screen" ? (subject.target ? subject.target.id : "default") : subject.id,
  );
  const shownScreen = $derived(
    subject.kind === "screen"
      ? subject.id
      : subject.kind === "panel"
        ? subject.borrowedScreen
        : (screens.meta.devices ?? []).find((d) => d.id === subject.id)?.screen,
  );
  const title = $derived(subject.kind ? `${shownScreen ?? "?"} as ${asLabel}` : "nothing selected");
  // Spec §4: a screen can win colors outright, so switching hardware may change only the size.
  // The payload belongs to the subject it was rendered for; ignore a stale one.
  const screenInCharge = $derived(
    resolution.subjectKey === subject.key &&
      (resolution.setting("colors")?.from === "screen" ||
        resolution.setting("colors_actual")?.from === "screen"),
  );

  // The server draws in measured colours only when some resolve for this
  // render (`resolve_use_actual`); otherwise the checkbox cannot change a
  // single pixel. Say so instead of leaving a toggle that appears dead.
  // Only a payload that belongs to the current subject can say it.
  const noMeasured = $derived(
    resolution.subjectKey === subject.key &&
      resolution.payload != null &&
      !resolution.setting("colors_actual")?.effective,
  );

  let blobUrl = $state(null);
  let channel;

  /**
   * Which render is the current one.
   *
   * Renders overlap: `screens.refresh()` rewrites `screens.meta`, which this
   * component's `$effect` reads through `simulate.request()`, so a refresh starts
   * a render of whatever is selected AT THAT MOMENT -- and `FilesPane`'s
   * create/copy/rename all refresh first and select afterwards, across an
   * `await`. Two requests are then in flight for two different screens.
   *
   * Without this counter the one that WINS is the one the server answers
   * last, not the one the user is looking at. A screen that fetches real data
   * is slower than a freshly created starter, so after New the old screen's
   * render routinely landed second and painted over the new one, with nothing
   * to correct it afterwards (Finding J). `AbortController` would cancel the
   * fetch instead, but the request is already served by then in every case
   * that matters here; ignoring the answer is what makes the preview follow
   * the SELECTION rather than the network.
   */
  let renderSeq = 0;

  // --- render-in-progress indicator ---------------------------------------
  //
  // A slow render (a screen that fetches real data can take seconds) would
  // otherwise leave the previous picture under the NEW title with no sign
  // that anything is happening. The label appears only after a short delay
  // so fast renders do not flicker, and only the LATEST request may clear it
  // (same `renderSeq` ordering as Finding J): a superseded response resolving
  // late must not hide the indicator of the render still in flight.
  const BUSY_DELAY_MS = 250;
  let busy = $state(false);
  let busyTimer = null;
  function startBusy() {
    if (busy || busyTimer) return; // already showing, or already counting down
    busyTimer = setTimeout(() => {
      busyTimer = null;
      busy = true;
    }, BUSY_DELAY_MS);
  }
  function endBusy() {
    clearTimeout(busyTimer);
    busyTimer = null;
    busy = false;
  }

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
  // doesn't exist here: `simulate.request()` always sends explicit `width` and
  // `height` computed from this same `simulate.size` (simulate.svelte.js),
  // and `resolve_preview_dimensions` (src/api/display.rs) honors an
  // explicit width/height whenever the query carries one -- so the
  // request and the response are always the same size, and a second,
  // decode-time correction would be dead code here.
  let frameClass = $derived(
    size.width > 1000 || size.height > 1000 ? "x" : "og",
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

  // Ruling 57: the wire contract to static/dev/preview.html's pop-out is
  // `{url, width, height}` -- the one thing that page's `channel.onmessage`
  // has ever understood (it was ported from vanilla dev.js, unchanged, in
  // Task 11). `postFrame()` is the single place that shape gets built, so
  // the render broadcast below and the hello reply here can never drift
  // apart from each other again the way they did when this component
  // shipped posting `{blob}` instead.
  function postFrame() {
    channel?.postMessage({ url: blobUrl, width: size.width, height: size.height });
  }

  onDestroy(() => clearTimeout(busyTimer));

  onMount(() => {
    channel = new BroadcastChannel("byonk-preview");
    // A reloaded pop-out says hello; answer with the current render so it
    // recovers instead of sitting blank (spec §8.4). There is nothing to
    // answer with until a render has actually completed -- `blobUrl` stays
    // null until then, which is deliberate (Controller Ruling 29): a
    // pop-out opened before any screen is selected has no "current image"
    // to recover.
    channel.onmessage = (e) => {
      if (e.data?.hello && blobUrl) postFrame();
    };
    return () => channel?.close();
  });

  /**
   * Render the current screen with the current simulate controls.
   *
   * Two failures are caught here instead of becoming unhandled rejections
   * (Controller Ruling 41):
   *
   *  - `simulate.request(ref)` THROWS on invalid params JSON (Task 8 rule (e),
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
    const seq = ++renderSeq;
    try {
      await renderCurrent(seq);
    } finally {
      // Only the latest request decides; a stale one leaves the indicator alone.
      if (seq === renderSeq) endBusy();
    }
  }

  async function renderCurrent(seq) {
    let req;
    try {
      req = buildRenderRequest({ subject, draft, simulate, meta: screens.meta, usage: configDraft.usage });
    } catch (e) {
      simulate.error = { op: "render", message: e.message };
      resolution.fail(e.message, subject.key);
      return;
    }
    if (!req) {
      // Nothing selected -- typically because the selected screen was just
      // deleted. Drop its last render here and in the pop-out, rather than
      // keep painting a screen that no longer exists (Finding H). Bumping
      // `renderSeq` above also drops a render still in flight for it.
      // Untracked: the re-render effect below calls this synchronously, and
      // must depend on the selection and the controls, not on `blobUrl`.
      untrack(() => {
        resolution.reset();
        if (!blobUrl) return;
        URL.revokeObjectURL(blobUrl);
        blobUrl = null;
        channel?.postMessage({ clear: true });
      });
      return;
    }
    simulate.error = null;
    startBusy();

    try {
      const res = await api.renderPreview(req.body);
      if (!res.ok) {
        const body = await res.json().catch(() => null);
        const message = body?.details
          ? `${body.error}: ${body.details}`
          : (body?.error ?? `${res.status} ${res.statusText}`);
        throw new Error(message);
      }
      const blob = await res.blob();
      if (seq !== renderSeq) return; // superseded -- see `renderSeq`
      let payload;
      try {
        payload = parseResolutionHeader(res.headers?.get?.("X-Byonk-Resolution") ?? null);
      } catch {
        throw new Error("the render's X-Byonk-Resolution header is not valid JSON");
      }
      resolution.record(payload, subject.key);
      if (blobUrl) URL.revokeObjectURL(blobUrl);
      blobUrl = URL.createObjectURL(blob);
      postFrame();
    } catch (e) {
      if (seq !== renderSeq) return; // a superseded render's failure is not news
      resolution.fail(e.message, subject.key);
      simulate.error = { op: "render", message: e.message };
    }
  }

  // Re-render whenever the screen or any simulate control changes. This is
  // what makes the pop-out follow a simulated-input change.
  $effect(() => {
    subject.key;
    subject.target;
    subject.borrowedScreen;
    subject.watchDevice;
    simulate.fields;
    screens.meta;
    renderNow();
  });

  // P2-7: draft edits render after the user pauses; nothing else waits.
  const DRAFT_RENDER_DEBOUNCE_MS = 300;
  let lastRevision = configDraft.revision;
  $effect(() => {
    const rev = configDraft.revision;
    if (rev === lastRevision) return;
    const id = setTimeout(() => {
      lastRevision = rev;
      renderNow();
    }, DRAFT_RENDER_DEBOUNCE_MS);
    return () => clearTimeout(id);
  });

  // "Watch on" offers the devices that use this panel, as the server resolves
  // them (explicit `panel:` or sighting); a watched device that does not is dropped.
  const watchable = $derived(
    subject.kind === "panel"
      ? (screens.meta.devices ?? []).filter(
          (d) => devicePanel({ meta: screens.meta, usage: configDraft.usage }, d.id) === subject.id,
        )
      : [],
  );
  $effect(() => {
    if (subject.kind === "panel" && subject.watchDevice && !watchable.some((d) => d.id === subject.watchDevice)) {
      subject.watchDevice = "";
    }
  });

  function popOut() {
    // Relative, and named. Both were proven under HA Ingress on 2026-09-09:
    // a root-absolute or origin-absolute URL leaves the HA iframe, and an
    // unnamed window stacks a new one per click instead of reusing it.
    window.open("preview.html", "byonk-preview");
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex min-w-0 flex-wrap items-center gap-2 border-b border-[var(--border-color)] px-2 py-1 text-xs">
    <span data-testid="preview-title" class="truncate font-mono">{title}</span>
    <span data-testid="preview-size" class="font-mono">{size.width} × {size.height}</span>
    {#if subject.kind === "screen"}
      <label for="preview-target" class="sr-only">Render as</label>
      <select id="preview-target" class={selectClass} value={targetValue} onchange={onTarget}>
        <option value="">default 800×480</option>
        <optgroup label="Panels">
          {#each screens.meta.panels ?? [] as p (p.id)}<option value={`panel:${p.id}`}>{p.name}</option>{/each}
        </optgroup>
        <optgroup label="Devices">
          {#each screens.meta.devices ?? [] as d (d.id)}<option value={`device:${d.id}`}>{d.name ?? d.id}</option>{/each}
        </optgroup>
      </select>
    {:else if subject.kind === "panel"}
      <label for="preview-screen" class="sr-only">Screen</label>
      <select id="preview-screen" class={selectClass} bind:value={subject.borrowedScreen}>
        {#each screens.list as s (s.ref)}<option value={s.ref}>{s.ref}</option>{/each}
      </select>
      <label for="preview-watch">watch on</label>
      <select id="preview-watch" class={selectClass} bind:value={subject.watchDevice}>
        <option value="">no device</option>
        {#each watchable as d (d.id)}<option value={d.id}>{d.name ?? d.id}</option>{/each}
      </select>
    {/if}
    {#if screenInCharge}
      <span data-testid="screen-in-charge" class="text-[var(--text-muted)]">the screen sets its own colours</span>
    {/if}
    <label class="ml-auto flex items-center gap-1 {noMeasured ? 'opacity-50' : ''}"
           title={noMeasured ? "No measured colours resolve for this render, so there is nothing to show. Pick a panel or device with colors_actual." : undefined}>
      <input type="checkbox" bind:checked={simulate.use_actual} disabled={noMeasured} /> show measured inks
    </label>
    <Button type="button" size="xs" variant="outline" onclick={popOut}>Pop out</Button>
  </div>
  <div data-testid="preview-region" aria-busy={busy ? "true" : "false"}
       class="relative flex min-h-0 flex-1 items-center justify-center p-4">
    <div class={`device-frame ${frameClass} flex items-center justify-center rounded-xl bg-[var(--panel-bg)] p-6`}>
      <div class="device-bezel rounded-lg bg-[var(--border-color)] p-3">
        <div class="screen-well overflow-hidden rounded-sm bg-[var(--bg-color)]">
          <img
            bind:this={imgEl}
            alt="Screen preview"
            src={blobUrl}
            class={`max-h-full max-w-full object-contain transition-opacity ${busy ? "opacity-50" : ""} ${frameClass === "og" ? "[image-rendering:pixelated]" : ""}`}
            onmouseenter={showLens}
            onmouseleave={hideLens}
            onmousemove={moveLens}
          />
        </div>
      </div>
    </div>
    {#if busy}
      <div role="status"
           class="pointer-events-none absolute left-1/2 top-1/2 z-10 flex -translate-x-1/2 -translate-y-1/2 items-center gap-2 rounded-md border border-[var(--border-color)] bg-[var(--panel-bg)] px-3 py-1.5 text-xs text-[var(--text-color)] shadow">
        <span aria-hidden="true" class="render-spinner inline-block h-3.5 w-3.5 rounded-full border-2 border-[var(--border-color)] border-t-[var(--accent-color)]"></span>
        Rendering…
      </div>
    {/if}
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

<style>
  .render-spinner {
    animation: render-spin 0.8s linear infinite;
  }
  @keyframes render-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .render-spinner {
      animation: none;
    }
  }
</style>
