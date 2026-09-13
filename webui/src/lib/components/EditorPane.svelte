<script>
  import { onMount, untrack } from "svelte";
  import { editor } from "../state/editor.svelte.js";
  import { screens } from "../state/screens.svelte.js";
  import { createEditor } from "../codemirror.js";
  import StatusBar from "./StatusBar.svelte";

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

  // Ruling 64: nothing ever called `editor.check()`, so the lint gutter
  // (codemirror.js's `lintGutter()`) rendered but stayed permanently empty.
  // A debounce while typing -- not a check on every keystroke (each one is
  // a server round trip, api/screen/check), and not check-only-on-save
  // (Task 14's own walkthrough shows the error marker appearing BEFORE the
  // fix-then-save step, so it has to surface from typing alone).
  //
  // Keyed PER TAB (a Map, not one shared timer): the old hand-written UI's
  // `scheduleCheck` (static/dev/dev.js) kept a single `checkTimer` for
  // every open file, so editing file A and then switching to file B within
  // the debounce window silently cancelled A's pending check via
  // `clearTimeout` -- A's gutter would then sit stale until its next
  // keystroke. That is a real defect in the original, not a design choice
  // worth reproducing here; a Map keyed by the tab's own identity lets two
  // files' pending checks coexist.
  const CHECK_DEBOUNCE_MS = 400;
  const checkTimers = new Map();

  function scheduleCheck(t) {
    const key = t.ref + t.name;
    clearTimeout(checkTimers.get(key));
    checkTimers.set(
      key,
      setTimeout(() => {
        checkTimers.delete(key);
        editor.check(t);
      }, CHECK_DEBOUNCE_MS),
    );
  }

  onMount(() => {
    cm = createEditor(host, {
      doc: editor.active?.text ?? "",
      language: languageOf(editor.active?.name),
      readOnly: editor.active?.readOnly ?? true,
      onChange: (text) => {
        editor.setText(text);
        // The tab being edited, captured NOW -- not re-read from
        // `editor.active` inside the timeout, which could by then be a
        // different tab if the user switched away during the debounce
        // window (see the Map comment above).
        const t = editor.active;
        if (t) scheduleCheck(t);
      },
      // Ruling 38: no brief in this plan wired a save action at all --
      // codemirror.js's Mod-s keymap was calling nothing. trySave() is
      // this store's own conflict-aware save, so wiring it straight through
      // here also gets the keybinding conflict handling for free.
      onSave: () => editor.trySave(),
      onCursor: (line, col) => editor.setCursor(line, col),
    });
    return () => {
      for (const id of checkTimers.values()) clearTimeout(id);
      checkTimers.clear();
      cm?.destroy();
    };
  });

  // Swapping tabs replaces the document rather than rebuilding the view.
  // This must key on tab IDENTITY (ref/name/readOnly) plus `revision`, not on
  // `t.text`: text is read inside `untrack` on purpose. If it were a tracked
  // read, every keystroke (onChange -> editor.setText -> t.text) would
  // re-run this effect and call setDoc again, resetting the document -- and
  // the caret with it -- under the user's fingers.
  //
  // `revision` is what catches replaceFromDisk() swapping the buffer of the
  // ALREADY-active tab (e.g. resolveReload() on a save conflict, Task 10) --
  // ref/name/readOnly are untouched by that call, so without a dedicated
  // signal this effect would never re-run and CodeMirror would keep showing
  // the stale text while the store's text/etag/dirty were already correct
  // underneath (task-7 fix round 1, Ruling 28).
  $effect(() => {
    const t = editor.active;
    if (!cm || !t) return;
    const name = t.name;
    const readOnly = t.readOnly;
    t.revision; // read only to establish the tracked dependency, see above
    untrack(() => cm.setDoc(t.text, languageOf(name), readOnly));
  });

  $effect(() => {
    if (cm && editor.active) cm.setDiagnostics(editor.active.issues);
  });
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center border-b border-[var(--border-color)] text-xs">
    <div role="tablist" class="flex min-w-0 flex-1 overflow-x-auto">
      <!--
        Iterates `editor.files` (the SCREEN's listing), not `editor.tabs`
        (loaded buffers) -- Task 1. One tab strip entry per file that
        exists, whether or not it has ever been opened. A binary file
        (`text: false`, e.g. a rendered PNG) has nothing a text buffer could
        show, so it renders as a disabled, non-interactive entry instead of
        a `role="tab"` button.
      -->
      {#each editor.files as f (f.name)}
        {@const t = editor.tabs.find((x) => x.name === f.name)}
        {#if f.text}
          <div
            role="tab"
            aria-selected={t != null && t === editor.active}
            class="flex items-center gap-1 border-r border-[var(--border-color)] px-2 py-1"
          >
            <button
              type="button"
              onclick={() =>
                editor.open(screens.selected, f.name, {
                  readOnly: !screens.writable(screens.selected),
                })}
            >
              {f.name}
            </button>
            <!-- Dirty/issues follow the LOADED buffer, not the listing entry: a
                 file that has never been opened has neither. -->
            {#if t?.dirty}<span aria-label="unsaved changes">●</span>{/if}
            {#if t?.issues.length}<span aria-label="has errors" class="text-[var(--danger-color)]">▲</span>{/if}
            {#if t}
              <!-- Closes the BUFFER only -- `editor.close()` never touches `files`,
                   so the file stays listed and can be reopened. -->
              <button
                type="button"
                aria-label={`Close ${f.name}`}
                onclick={() => editor.close(editor.tabs.indexOf(t))}
              >
                ×
              </button>
            {/if}
          </div>
        {:else}
          <div
            aria-disabled="true"
            class="flex cursor-not-allowed items-center gap-1 border-r
                   border-[var(--border-color)] px-2 py-1 text-[var(--text-muted)] opacity-60"
          >
            {f.name} ({f.size})
          </div>
        {/if}
      {/each}
    </div>
    <!--
      The keybinding above is invisible unless you already know it's there
      (Ruling 38). Enabled exactly when there is something worth saving --
      same rule the old UI's toolbar Save button used (dev.js: "enabled
      whenever any buffer is dirty").
    -->
    <button
      type="button"
      class="mx-1 shrink-0 rounded border border-[var(--border-color)] px-2 py-0.5
             disabled:opacity-40"
      disabled={!editor.active || editor.active.readOnly || !editor.active.dirty}
      onclick={() => editor.trySave()}
    >
      Save
    </button>
  </div>
  <div bind:this={host} class="min-h-0 flex-1 overflow-auto"></div>
  <StatusBar />
</div>
