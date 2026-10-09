<script>
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import * as api from "../api.js";
  import { devicePanel } from "../render-request.js"; // the one device->panel lookup (ruling PF8)
  import { Button } from "$lib/ui/button/index.js";

  const canEdit = $derived(screens.writable(subject.screenRef));

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

  function panelName(id) {
    return (screens.meta.panels ?? []).find((p) => p.id === id)?.name ?? id;
  }
  function devicePanelLabel(d) {
    const id = devicePanel({ meta: screens.meta, usage: configDraft.usage }, d.id);
    return id ? panelName(id) : "panel unknown";
  }
  const rowClass = (on) =>
    "truncate px-2 py-0.5 text-left hover:bg-[var(--panel-bg)] " +
    (on ? "bg-[var(--accent-color)] text-white" : "");

  function newScreen() {
    return run("create", async () => {
      const handle = defaultWritableHandle();
      const name = prompt("New screen name");
      if (!handle || !name) return;
      const { ref } = await api.createScreen(handle, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
  }

  function copyScreen() {
    return run("copy", async () => {
      const to = defaultWritableHandle();
      const name = prompt("Copy to name");
      if (!to || !name || !subject.screenRef) return;
      const { ref } = await api.copyScreen(subject.screenRef, to, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
  }

  function renameScreen() {
    return run("rename", async () => {
      const name = prompt("Rename to");
      if (!name || !subject.screenRef) return;
      const { ref } = await api.renameScreen(subject.screenRef, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
  }

  function deleteScreen() {
    return run("delete", async () => {
      if (!subject.screenRef) return;
      if (!confirm(`Delete ${subject.screenRef}?`)) return;
      await api.deleteScreen(subject.screenRef);
      // Finding H: the deleted screen's buffers and render must go too. The
      // subject effect never reloads for "no screen", so drop them here.
      subject.clear();
      await editor.loadScreen(null);
      await screens.refresh();
    });
  }
</script>

<div class="flex flex-col text-xs" role="region" aria-label="Library">
  <div class="flex gap-1 border-b border-[var(--border-color)] p-1">
    <Button type="button" size="xs" variant="outline" onclick={newScreen}>New</Button>
    <Button
      type="button"
      size="xs"
      variant="outline"
      onclick={copyScreen}
      disabled={!subject.screenRef}
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

  <h2 class="px-2 pt-2 text-[10px] font-medium uppercase text-[var(--text-muted)]">Screens</h2>
  {#each screens.grouped as g (g.handle)}
    <div class="px-2 pt-1 font-mono text-[10px] uppercase text-[var(--text-muted)]">
      {g.handle}{#if !g.writable}<span class="ml-1">read-only</span>{/if}
    </div>
    {#each g.screens as s (s.ref)}
      <button type="button" class={rowClass(subject.is("screen", s.ref))}
              aria-current={subject.is("screen", s.ref) ? "true" : undefined}
              onclick={() => subject.select("screen", s.ref)}>{s.label}</button>
    {/each}
  {/each}

  <h2 class="px-2 pt-3 text-[10px] font-medium uppercase text-[var(--text-muted)]">Panels</h2>
  {#each screens.meta.panels ?? [] as p (p.id)}
    <button type="button" class={rowClass(subject.is("panel", p.id))}
            aria-current={subject.is("panel", p.id) ? "true" : undefined}
            onclick={() => subject.select("panel", p.id)}>{p.name}</button>
  {/each}

  <h2 class="px-2 pt-3 text-[10px] font-medium uppercase text-[var(--text-muted)]">Devices</h2>
  {#each screens.meta.devices ?? [] as d (d.id)}
    <button type="button" class={rowClass(subject.is("device", d.id))}
            aria-current={subject.is("device", d.id) ? "true" : undefined}
            onclick={() => subject.select("device", d.id)}>{d.name ?? d.id} · {devicePanelLabel(d)}</button>
  {/each}
</div>
