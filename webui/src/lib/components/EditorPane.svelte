<script>
  import { onMount, untrack } from "svelte";
  import { editor } from "../state/editor.svelte.js";
  import { createEditor } from "../codemirror.js";

  // $state, not a plain `let`: `bind:this` and the assignment inside onMount
  // are both plain writes, and an $effect reading a plain `let` captures its
  // value at first run only -- it is never re-scheduled when that `let`
  // later changes. Making these reactive is what lets the two effects below
  // pick up `cm` once onMount assigns it, regardless of run order.
  let host = $state();
  let cm = $state();

  function languageOf(name = "") {
    if (name.endsWith(".lua")) return "lua";
    if (name.endsWith(".svg") || name.endsWith(".xml")) return "xml";
    if (name.endsWith(".yaml") || name.endsWith(".yml")) return "yaml";
    return null;
  }

  onMount(() => {
    cm = createEditor(host, {
      doc: editor.active?.text ?? "",
      language: languageOf(editor.active?.name),
      readOnly: editor.active?.readOnly ?? true,
      onChange: (text) => editor.setText(text),
    });
    return () => cm?.destroy();
  });

  // Swapping tabs replaces the document rather than rebuilding the view.
  // This must key on tab IDENTITY (ref/name/readOnly), not on `t.text`: text
  // is read inside `untrack` on purpose. If it were a tracked read, every
  // keystroke (onChange -> editor.setText -> t.text) would re-run this
  // effect and call setDoc again, resetting the document -- and the caret
  // with it -- under the user's fingers.
  $effect(() => {
    const t = editor.active;
    if (!cm || !t) return;
    const name = t.name;
    const readOnly = t.readOnly;
    untrack(() => cm.setDoc(t.text, languageOf(name), readOnly));
  });

  $effect(() => {
    if (cm && editor.active) cm.setDiagnostics(editor.active.issues);
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div role="tablist" class="flex border-b border-[var(--border-color)] text-xs">
    {#each editor.tabs as t, i (t.ref + t.name)}
      <div
        role="tab"
        aria-selected={i === editor.activeIndex}
        class="flex items-center gap-1 border-r border-[var(--border-color)] px-2 py-1"
      >
        <button type="button" onclick={() => (editor.activeIndex = i)}>{t.name}</button>
        {#if t.dirty}<span aria-label="unsaved changes">●</span>{/if}
        {#if t.issues.length}<span aria-label="has errors" class="text-[var(--danger-color)]">▲</span>{/if}
        <button type="button" aria-label={`Close ${t.name}`} onclick={() => editor.close(i)}>×</button>
      </div>
    {/each}
  </div>
  <div bind:this={host} class="min-h-0 flex-1 overflow-auto"></div>
</div>
