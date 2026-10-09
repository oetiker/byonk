<script>
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import * as api from "../api.js";
  import { devicePanel } from "../render-request.js"; // the one device->panel lookup (ruling PF8)
  import { Button } from "$lib/ui/button/index.js";
  import InlineNameForm from "./InlineNameForm.svelte";
  import InlineConfirm from "./InlineConfirm.svelte";

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
   * let the repo be chosen in its inline create form; the inline name form
   * below asks for a name only, so the repo is still not the user's to pick.
   */
  function defaultWritableHandle() {
    const writable = screens.repos.filter((r) => r.writable);
    return (writable.find((r) => r.handle === "local") ?? writable[0])?.handle;
  }

  /**
   * Every handler below calls the API, which can reject -- a 409 conflict,
   * a dropped connection, a 401 after session expiry. Left uncaught, that
   * is an unhandled promise rejection with no user-visible feedback: the
   * form just closes and nothing happens. `run()` is the one place that
   * catches, so no handler can leak a rejection past its onclick, and it
   * records enough on `screens.error` for something else to act on --
   * rendering that is Task 10's job (StatusBar), not this component's.
   * A fresh operation clears the previous error so a stale failure can't
   * linger after a later success.
   *
   * Resolves to whether the operation succeeded, so a form can close on
   * success and stay open (with the typed name) on failure.
   */
  async function run(op, fn) {
    screens.error = null;
    try {
      await fn();
      return true;
    } catch (err) {
      screens.error = { op, message: err?.message ?? String(err) };
      return false;
    }
  }

  /**
   * The open inline form, or null. The target ref is captured when the form
   * opens and submit acts on that, so a confirmation that names `local/a`
   * can never delete `local/b`; changing the subject closes it.
   *   { kind: "new" }
   *   { kind: "copy" | "rename" | "delete", ref }
   *   { kind: "rename-confirm", ref, name }   unsaved edits, name already given
   */
  let ask = $state(null);

  $effect(() => {
    if (ask?.ref && subject.screenRef !== ask.ref) ask = null;
  });

  const pathOf = (ref) => ref.slice(ref.indexOf("/") + 1);
  const hasUnsavedEdits = (ref) => editor.tabs.some((t) => t.ref === ref && t.dirty);

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
    ask = { kind: "new" };
  }
  function copyScreen() {
    if (subject.screenRef) ask = { kind: "copy", ref: subject.screenRef };
  }
  function renameScreen() {
    if (subject.screenRef) ask = { kind: "rename", ref: subject.screenRef };
  }
  function deleteScreen() {
    if (subject.screenRef) ask = { kind: "delete", ref: subject.screenRef };
  }
  const closeForm = () => (ask = null);

  async function submitNew(name) {
    const handle = defaultWritableHandle();
    if (!handle) return;
    const ok = await run("create", async () => {
      const { ref } = await api.createScreen(handle, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
    if (ok) ask = null;
  }

  async function submitCopy(name) {
    const to = defaultWritableHandle();
    const from = ask.ref;
    if (!to) return;
    const ok = await run("copy", async () => {
      const { ref } = await api.copyScreen(from, to, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
    if (ok) ask = null;
  }

  // Renaming reloads the editor under the new ref, which drops every buffer;
  // unsaved edits get one more question first instead of vanishing.
  function submitRename(name) {
    if (hasUnsavedEdits(ask.ref)) ask = { kind: "rename-confirm", ref: ask.ref, name };
    else return doRename(ask.ref, name);
  }

  async function doRename(from, name) {
    const ok = await run("rename", async () => {
      const { ref } = await api.renameScreen(from, name);
      await screens.refresh();
      subject.select("screen", ref);
    });
    if (ok) ask = null;
    // A failed confirmed rename returns to the name form so the name can be fixed.
    else if (ask?.kind === "rename-confirm") ask = { kind: "rename", ref: from, name };
  }

  async function submitDelete() {
    const ref = ask.ref;
    const ok = await run("delete", async () => {
      await api.deleteScreen(ref);
      // Finding H: the deleted screen's buffers and render must go too. The
      // subject effect never reloads for "no screen", so drop them here.
      subject.clear();
      await editor.loadScreen(null);
      await screens.refresh();
    });
    if (ok) ask = null;
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

  {#if ask?.kind === "new"}
    <InlineNameForm label="New screen name" onsubmit={submitNew} oncancel={closeForm} />
  {:else if ask?.kind === "copy"}
    <InlineNameForm label="Copy to name" value={pathOf(ask.ref)} onsubmit={submitCopy} oncancel={closeForm} />
  {:else if ask?.kind === "rename"}
    <InlineNameForm label="Rename to" value={ask.name ?? pathOf(ask.ref)} onsubmit={submitRename} oncancel={closeForm} />
  {:else if ask?.kind === "rename-confirm"}
    <InlineConfirm
      label="Confirm rename with unsaved edits"
      message={`${ask.ref} has unsaved edits that renaming would discard. Rename anyway?`}
      confirmLabel="Rename anyway"
      onconfirm={() => doRename(ask.ref, ask.name)}
      oncancel={closeForm}
    />
  {:else if ask?.kind === "delete"}
    <InlineConfirm
      label="Confirm screen deletion"
      message={`Delete ${ask.ref}? This cannot be undone.`}
      onconfirm={submitDelete}
      oncancel={closeForm}
    />
  {/if}

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
