<script>
  // The foot of the editor (spec §1's mockup: "✓ valid  lua 3:22 / watch:
  // connected"). Validity, cursor position and watch connection state ONLY
  // -- spec §1's table names exactly these three for the Editor's status
  // bar, and this component stays literally scoped to it.
  //
  // The four captured-error fields (editor.error, screens.error,
  // simulate.error, tune.error) and tune.pushed used to render here too,
  // but StatusBar only exists while the Editor panel is open (it is mounted
  // inside EditorPane, which AppShell unmounts on `{#if
  // layout.isOpen("editor")}`) -- so all five went dark together whenever
  // the Editor was closed. That contradicts
  // docs/superpowers/specs/2026-09-10-dev-ui-color-tuning-design.md §8's
  // "An active override must be visible from outside the Tune panel", so
  // review finding 2 (fix round 2) moved them to `StatusStrip`, mounted by
  // `AppShell` above the panel row where no panel toggle can remove it.
  import { editor } from "../state/editor.svelte.js";
  import { Check, X } from "@lucide/svelte";

  function languageOf(name = "") {
    if (name.endsWith(".lua")) return "lua";
    if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
    if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
    return "text";
  }

  // An asset shown in front of a text tab is what the bar describes (U24):
  // its name, and no validity or cursor of the hidden buffer.
  const asset = $derived(editor.asset);
  const t = $derived(asset ? null : editor.active);
  const issues = $derived(t?.issues ?? []);
</script>

<div
  class="flex h-(--status-h) shrink-0 items-center gap-3 overflow-hidden whitespace-nowrap border-t border-border
         bg-muted px-2 font-mono text-xs text-foreground"
>
  {#if asset}
    <span class="text-muted-foreground">{asset.name} (asset)</span>
  {:else if !t}
    <span class="text-muted-foreground">no file open</span>
  {:else if issues.length === 0}
    <span class="inline-flex items-center gap-1 text-success"><Check class="size-3.5" aria-hidden="true" />valid</span>
  {:else}
    <span class="inline-flex items-center gap-1 text-destructive">
      <X class="size-3.5" aria-hidden="true" />{issues.length} issue{issues.length === 1 ? "" : "s"}
    </span>
  {/if}

  {#if t}
    <span>{languageOf(t.name)} {editor.cursorLine}:{editor.cursorCol}</span>
  {/if}

  <span class={editor.watchConnected ? "text-success" : "text-muted-foreground"}>
    watch: {editor.watchConnected ? "connected" : "disconnected"}
  </span>
</div>
