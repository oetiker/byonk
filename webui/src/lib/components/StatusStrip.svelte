<script>
  // The app status bar (U12): ONE always-present, fixed-height, single-line
  // bar at the foot of the shell, in flow below the pane row. Panes never
  // move, whether or not a message shows. It carries every message no panel
  // toggle may hide: the on-disk conflict (with Reload/Overwrite), the five
  // captured error fields (editor, screens, simulate, auth, config) and the
  // live-override notices. Most important first (conflict, errors, live);
  // the line shows that one plus a "+N" count, truncated with the full text
  // as tooltip. Clicking opens a panel UPWARD over the pane foot listing
  // everything in full; it closes by itself when the set of messages changes,
  // on Escape and on a press outside it.
  //
  // editor.error is raised by four operations (save, reload, overwrite,
  // watch-reload), so each message names its own `op`, never a fixed "save:".
  // auth.error (Ruling 66) needs a surface too: a failed logout with no
  // visible consequence reads as a session that ended.
  import { editor } from "../state/editor.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { auth } from "../state/auth.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import { Button } from "$lib/ui/button/index.js";
  import { Alert } from "$lib/ui/alert/index.js";


  const items = $derived.by(() => {
    const out = [];
    const c = editor.conflict;
    if (c) {
      out.push({
        id: `conflict:${c.name}:${c.source}`,
        kind: "conflict",
        text: `${c.name} changed on disk${c.source === "save" ? " while you were editing" : ""}.`,
        c,
      });
    }
    for (const [k, e, label] of [
      ["editor", editor.error, ""],
      ["screens", screens.error, ""],
      ["simulate", simulate.error, ""],
      ["auth", auth.error, ""],
      ["config", configDraft.error, "config "],
    ]) {
      if (e) out.push({ id: `${k}:${e.op}:${e.message}`, kind: "error", text: `${label}${e.op}: ${e.message}` });
    }
    for (const o of configDraft.live.panels)
      out.push({ id: `live-panel:${o.key}`, kind: "live", text: `◉ Live on ${o.key}`, type: "panel", key: o.key });
    for (const o of configDraft.live.devices)
      out.push({ id: `live-device:${o.key}`, kind: "live", text: `◉ Live on ${o.key}`, type: "device", key: o.key });
    return out;
  });

  // The panel is open only for the set of messages it was opened on: a
  // writable derived, so ANY change to the set resets it to collapsed -- and
  // a set that comes back later does not reopen it (U24).
  const sig = $derived(items.map((i) => i.id).join("\n"));
  let open = $derived.by(() => {
    void sig; // the dependency: a changed set collapses the panel
    return false;
  });
  const setOpen = (v) => (open = v);

  // Escape, or a press anywhere outside the bar and its panel, closes it.
  let bar = $state();
  function onKey(e) {
    if (open && e.key === "Escape") setOpen(false);
  }
  function onPointer(e) {
    if (open && bar && !bar.contains(e.target)) setOpen(false);
  }
</script>

<svelte:document onkeydown={onKey} onpointerdown={onPointer} />

{#snippet actions(it)}
  {#if it.kind === "conflict"}
    <Button type="button" size="xs" variant="outline" onclick={() => editor.resolveReload()}>Reload</Button>
    <Button type="button" size="xs" variant="outline" onclick={() => editor.resolveOverwrite()}>Overwrite</Button>
  {:else if it.kind === "live"}
    <Button type="button" size="xs" variant="ghost" onclick={() => configDraft.discardLive(it.type, it.key)}>Discard</Button>
  {/if}
{/snippet}

{#snippet row(it, wrap)}
  <Alert
    variant="destructive"
    role={it.kind === "live" ? "status" : "alert"}
    class="flex min-h-6 min-w-0 items-center gap-2 rounded-none border-0 bg-transparent p-0 text-xs"
  >
    {#if wrap}
      <span class="min-w-0 flex-1 whitespace-pre-wrap break-words">{it.text}</span>
    {:else}
      <button
        type="button"
        class="min-w-0 flex-1 cursor-pointer truncate text-left"
        title={it.text}
        aria-expanded={open}
        aria-controls="status-panel"
        onclick={() => setOpen(!open)}>{it.text}</button
      >
    {/if}
    {@render actions(it)}
  </Alert>
{/snippet}

<div
  bind:this={bar}
  data-testid="status-bar"
  class="relative flex h-(--status-h) shrink-0 items-center gap-2 border-t border-border bg-muted px-2 font-mono text-xs text-foreground"
>
  {#if items.length > 0}
    {#if open}
      <div
        id="status-panel"
        data-testid="status-panel"
        class="absolute inset-x-0 bottom-full z-40 flex max-h-60 flex-col gap-1 overflow-auto border-t border-border bg-muted px-2 py-1 shadow-md"
      >
        {#each items as it (it.id)}
          {@render row(it, true)}
        {/each}
      </div>
      <button type="button" class="cursor-pointer text-left" aria-expanded="true" aria-controls="status-panel"
              onclick={() => setOpen(false)}>
        Hide {items.length} message{items.length === 1 ? "" : "s"}
      </button>
    {:else}
      <div class="flex min-w-0 flex-1 items-center">{@render row(items[0], false)}</div>
      {#if items.length > 1}
        <button
          type="button"
          class="shrink-0 cursor-pointer text-destructive"
          title="Show all {items.length} messages"
          aria-expanded="false"
          aria-controls="status-panel"
          onclick={() => setOpen(true)}>+{items.length - 1}</button
        >
      {/if}
    {/if}
  {/if}
</div>
