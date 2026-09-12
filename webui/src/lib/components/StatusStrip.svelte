<script>
  // The error/override surface no panel toggle can hide (review finding 2,
  // "Ruling 49"). StatusBar's own validity/cursor/watch trio stays inside
  // EditorPane per spec §1's table -- unchanged -- but the four captured
  // error fields and `tune.pushed` used to live there too, and StatusBar
  // only exists while the Editor panel is open. That silently hid all five
  // whenever the Editor was closed, which directly contradicts
  // docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md §8: "An
  // active override must be visible from outside the Tune panel... StatusBar
  // shows that an override is live and for which panel." Mounted by
  // AppShell above the panel row, next to ConflictBanner, outside every
  // `layout.isOpen(...)` gate.
  //
  // Renders nothing at all -- not even an empty bar -- when there is
  // nothing to report: height is the scarce dimension in this layout (spec
  // §1), and an always-present empty strip would spend it for nothing.
  import { editor } from "../state/editor.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { simulate } from "../state/simulate.svelte.js";
  import { tune } from "../state/tune.svelte.js";
  import { auth } from "../state/auth.svelte.js";

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
      !!tune.error ||
      tune.pushed ||
      !!auth.error,
  );
</script>

{#if hasAnything}
  <div
    class="flex flex-col gap-0.5 border-b border-[var(--border-color)]
           bg-[var(--panel-bg)] px-2 py-1 font-mono text-xs text-[var(--text-color)]"
  >
    {#if tune.pushed}
      <span role="status" class="text-[var(--danger-color)]">
        ⚠ override live on {tune.panel || "?"}
      </span>
    {/if}
    {#if editor.error}
      <div role="alert" class="text-[var(--danger-color)]">save: {editor.error.message}</div>
    {/if}
    {#if screens.error}
      <div role="alert" class="text-[var(--danger-color)]">
        {screens.error.op}: {screens.error.message}
      </div>
    {/if}
    {#if simulate.error}
      <div role="alert" class="text-[var(--danger-color)]">
        {simulate.error.op}: {simulate.error.message}
      </div>
    {/if}
    {#if tune.error}
      <div role="alert" class="text-[var(--danger-color)]">
        tune {tune.error.op}: {tune.error.message}
      </div>
    {/if}
    {#if auth.error}
      <div role="alert" class="text-[var(--danger-color)]">
        {auth.error.op}: {auth.error.message}
      </div>
    {/if}
  </div>
{/if}
