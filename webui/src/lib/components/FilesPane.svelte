<script>
  import { screens } from "../state/screens.svelte.js";
  import * as api from "../api.js";
  import { Button } from "$lib/ui/button/index.js";

  const canEdit = $derived(screens.writable(screens.selected));

  /**
   * Where New and Copy put a screen when the user is not asked.
   *
   * `local` is the reserved handle for the user's own screens
   * (`LOCAL_HANDLE`, src/services/screen_repo_manager.rs:34), so it is the
   * default even though /api/repos does not sort it first: that list is
   * writable-first then handle-ascending (src/api/dev_files.rs:149), and
   * `examples` -- the directory byonk seeds with its own samples -- sorts
   * ahead of `local`. Taking the first writable entry put every new screen
   * in with the shipped examples (Finding G).
   *
   * Falls back to the first writable repo, because `local` is registered
   * from `screens_dir` and a deployment can leave it out or make it
   * read-only; New must degrade rather than stop working.
   *
   * This picks a default, it does not restore the choice. The pre-rewrite UI
   * let the repo be chosen in its inline create form; that choice has nowhere
   * to go while these dialogs are native `prompt()`s, and returning it is
   * part of the parked inline-form question (R5).
   */
  function defaultWritableHandle() {
    const writable = screens.repos.filter((r) => r.writable);
    return (writable.find((r) => r.handle === "local") ?? writable[0])?.handle;
  }

  /**
   * Every handler below calls the API, which can reject -- a 409 conflict,
   * a dropped connection, a 401 after session expiry. Left uncaught, that
   * is an unhandled promise rejection with no user-visible feedback: the
   * dialog just closes and nothing happens. `run()` is the one place that
   * catches, so no handler can leak a rejection past its onclick, and it
   * records enough on `screens.error` for something else to act on --
   * rendering that is Task 10's job (StatusBar), not this component's.
   * A fresh operation clears the previous error so a stale failure can't
   * linger after a later success.
   */
  async function run(op, fn) {
    screens.error = null;
    try {
      await fn();
    } catch (err) {
      screens.error = { op, message: err?.message ?? String(err) };
    }
  }

  function newScreen() {
    return run("create", async () => {
      const handle = defaultWritableHandle();
      const name = prompt("New screen name");
      if (!handle || !name) return;
      const { ref } = await api.createScreen(handle, name);
      await screens.refresh();
      screens.select(ref);
    });
  }

  function copyScreen() {
    return run("copy", async () => {
      const to = defaultWritableHandle();
      const name = prompt("Copy to name");
      if (!to || !name || !screens.selected) return;
      const { ref } = await api.copyScreen(screens.selected, to, name);
      await screens.refresh();
      screens.select(ref);
    });
  }

  function renameScreen() {
    return run("rename", async () => {
      const name = prompt("Rename to");
      if (!name || !screens.selected) return;
      const { ref } = await api.renameScreen(screens.selected, name);
      await screens.refresh();
      screens.select(ref);
    });
  }

  function deleteScreen() {
    return run("delete", async () => {
      if (!screens.selected) return;
      if (!confirm(`Delete ${screens.selected}?`)) return;
      await api.deleteScreen(screens.selected);
      screens.select(null);
      await screens.refresh();
    });
  }
</script>

<div class="flex flex-col text-xs">
  <div class="flex gap-1 border-b border-[var(--border-color)] p-1">
    <Button type="button" size="xs" variant="outline" onclick={newScreen}>New</Button>
    <Button
      type="button"
      size="xs"
      variant="outline"
      onclick={copyScreen}
      disabled={!screens.selected}
    >
      Copy
    </Button>
    <Button type="button" size="xs" variant="outline" onclick={renameScreen} disabled={!canEdit}>
      Rename
    </Button>
    <Button type="button" size="xs" variant="outline" onclick={deleteScreen} disabled={!canEdit}>
      Delete
    </Button>
  </div>

  {#each screens.grouped as g (g.handle)}
    <div class="px-2 pt-2 font-mono text-[10px] uppercase text-[var(--text-muted)]">
      {g.handle}{#if !g.writable}<span class="ml-1">read-only</span>{/if}
    </div>
    {#each g.screens as s (s.ref)}
      <button
        type="button"
        class="truncate px-2 py-0.5 text-left hover:bg-[var(--panel-bg)]
               {screens.selected === s.ref ? 'bg-[var(--accent-color)] text-white' : ''}"
        onclick={() => screens.select(s.ref)}
      >{s.label}</button>
    {/each}
  {/each}
</div>
