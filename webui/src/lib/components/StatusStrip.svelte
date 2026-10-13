<script>
  // The error surface no panel toggle can hide (review finding 2, "Ruling
  // 49"). StatusBar's own validity/cursor/watch trio stays inside EditorPane
  // per spec §1's table, but the captured error fields cannot live there:
  // StatusBar only exists while the Edit panel is open, so they would go dark
  // whenever it was closed. Mounted by AppShell in an absolutely positioned overlay
  // over the foot of the pane row (U12: showing it never moves a pane), next to
  // ConflictBanner, outside every `layout.isOpen(...)` gate.
  //
  // Renders nothing at all -- not even an empty bar -- when there is
  // nothing to report: height is the scarce dimension in this layout (spec
  // §1), and an always-present empty strip would spend it for nothing.
  import { editor } from "../state/editor.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { auth } from "../state/auth.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import { Button } from "$lib/ui/button/index.js";
  import { Alert } from "$lib/ui/alert/index.js";

  // One 24px line per message (tokens, not py-*); Alert supplies role and colour.
  // Long messages truncate; click toggles the full text (the overlay grows,
  // the panes do not move). The full text is also the tooltip.
  let expanded = $state({});
  const toggle = (k) => (expanded[k] = !expanded[k]);

  const line = "flex min-h-6 items-center gap-2 rounded-none border-0 bg-transparent p-0 text-xs";

  // Ruling 66: restoring Logout (Task 13a) created a FIFTH error owner
  // (auth.error, from a failed whoami or a failed logout) and left it with
  // no surface at all -- worse than merely hidden, since a failed logout
  // with no visible consequence reads to the user as a session that ended
  // when it did not. Same rule as the other four: this is the one place no
  // panel toggle can hide, so it belongs here, not somewhere a click could
  // unmount it.
  const hasAnything = $derived(
    !!editor.error ||
      !!screens.error ||
      !!simulate.error ||
      !!auth.error ||
      !!configDraft.error ||
      configDraft.live.panels.length > 0 ||
      configDraft.live.devices.length > 0,
  );
</script>

{#snippet msg(k, text)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span
    class="min-w-0 flex-1 cursor-pointer {expanded[k] ? 'whitespace-pre-wrap break-words' : 'truncate'}"
    title={text}
    onclick={() => toggle(k)}>{text}</span
  >
{/snippet}

{#if hasAnything}
  <div
    class="flex flex-col gap-0.5 border-t border-border
           bg-muted px-2 py-1 font-mono text-xs text-foreground"
  >
    {#if editor.error}
      <!-- `{op}:`, not a hard-coded "save:". `editor.error` is raised by four
           different operations (save, reload, overwrite, watch-reload --
           editor.svelte.js), and labelling every one of them "save" told the
           user a save had failed when none had been attempted. The other four
           surfaces below already render their own `op`. -->
      <Alert variant="destructive" class={line}>{@render msg("editor", `${editor.error.op}: ${editor.error.message}`)}</Alert>
    {/if}
    {#if screens.error}
      <Alert variant="destructive" class={line}>{@render msg("screens", `${screens.error.op}: ${screens.error.message}`)}</Alert>
    {/if}
    {#if simulate.error}
      <Alert variant="destructive" class={line}>{@render msg("simulate", `${simulate.error.op}: ${simulate.error.message}`)}</Alert>
    {/if}
    {#if auth.error}
      <Alert variant="destructive" class={line}>{@render msg("auth", `${auth.error.op}: ${auth.error.message}`)}</Alert>
    {/if}
    {#each configDraft.live.panels as o (o.key)}
      <Alert variant="destructive" role="status" class={line}>
        ◉ Live on {o.key}
        <Button type="button" size="xs" variant="ghost" onclick={() => configDraft.discardLive("panel", o.key)}>Discard</Button>
      </Alert>
    {/each}
    {#each configDraft.live.devices as o (o.key)}
      <Alert variant="destructive" role="status" class={line}>
        ◉ Live on {o.key}
        <Button type="button" size="xs" variant="ghost" onclick={() => configDraft.discardLive("device", o.key)}>Discard</Button>
      </Alert>
    {/each}
    {#if configDraft.error}
      <Alert variant="destructive" class={line}>{@render msg("config", `config ${configDraft.error.op}: ${configDraft.error.message}`)}</Alert>
    {/if}
  </div>
{/if}
