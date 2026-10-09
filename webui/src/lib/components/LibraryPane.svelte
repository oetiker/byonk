<script>
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import * as api from "../api.js";
  import { devicePanel } from "../render-request.js"; // the one device->panel lookup (ruling PF8)
  import { untrack } from "svelte";
  import { Button } from "$lib/ui/button/index.js";
  import InlineNameForm from "./InlineNameForm.svelte";
  import InlineConfirm from "./InlineConfirm.svelte";

  // --- accordion sections (VS Code explorer style) -----------------------
  // Which sections are collapsed is a per-viewer convenience kept in
  // localStorage; every access is guarded because storage can be blocked
  // (private window, cleared site data) and the pane must work without it.
  const COLLAPSED_KEY = "byonk.library.collapsed.v1";
  const SECTIONS = ["screen", "panel", "device"];

  function loadCollapsed() {
    try {
      const raw = JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? "[]");
      return Array.isArray(raw) ? raw.filter((k) => SECTIONS.includes(k)) : [];
    } catch {
      return [];
    }
  }
  let collapsed = $state(loadCollapsed());

  function persistCollapsed() {
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify($state.snapshot(collapsed)));
    } catch {
      /* storage blocked: the choice lasts for this page only */
    }
  }
  const isOpen = (key) => !collapsed.includes(key);
  function toggleSection(key) {
    collapsed = isOpen(key) ? [...collapsed, key] : collapsed.filter((k) => k !== key);
    persistCollapsed();
  }

  // A newly selected subject must not sit hidden in a collapsed section the
  // user did not just close: expand the section that holds it. Reads only
  // the subject; the write is untracked so a manual toggle is never undone.
  $effect(() => {
    const kind = subject.kind;
    const id = subject.id;
    if (!id) return;
    untrack(() => {
      if (!isOpen(kind) && SECTIONS.includes(kind)) {
        collapsed = collapsed.filter((k) => k !== kind);
        persistCollapsed();
      }
    });
  });

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
    "block w-full shrink-0 truncate px-2 py-0.5 text-left hover:bg-[var(--panel-bg)] " +
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

  // Without a writable repo there is nowhere to put a screen; say so rather
  // than leave the form open with nothing happening.
  const NO_WRITABLE = "no writable screen repo to put the screen in";

  async function submitNew(name) {
    const handle = defaultWritableHandle();
    if (!handle) {
      screens.error = { op: "create", message: NO_WRITABLE };
      return;
    }
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
    if (!to) {
      screens.error = { op: "copy", message: NO_WRITABLE };
      return;
    }
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

<div class="flex min-h-0 flex-1 flex-col text-xs" role="region" aria-label="Library">
  <div class="flex shrink-0 gap-1 border-b border-[var(--border-color)] p-1">
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

  {#snippet section(key, label, body)}
    <!-- Expanded sections share the pane height (flex-1 on a zero basis) and
         scroll on their own; a collapsed one shrinks to its header. -->
    <section class="flex flex-col border-t border-[var(--border-color)] {isOpen(key) ? 'min-h-6 flex-1 basis-0 overflow-hidden' : 'shrink-0'}"
             aria-label={label}>
      <button type="button" aria-expanded={isOpen(key)}
              class="flex w-full shrink-0 items-center gap-1 bg-[var(--panel-bg)] px-1.5 py-1 text-left text-[11px] font-bold uppercase tracking-wide hover:bg-[var(--border-color)]"
              onclick={() => toggleSection(key)}>
        <svg aria-hidden="true" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round"
             class="size-4 shrink-0 transition-transform {isOpen(key) ? 'rotate-90' : ''}">
          <path d="M6 3.5 10.5 8 6 12.5" />
        </svg>{label}
      </button>
      {#if isOpen(key)}
        <div class="min-h-0 flex-1 overflow-auto pb-1">{@render body()}</div>
      {/if}
    </section>
  {/snippet}

  {#snippet screensBody()}
    {#each screens.grouped as g (g.handle)}
      <div class="shrink-0 px-2 pt-1 font-mono text-[10px] uppercase text-[var(--text-muted)]">
        {g.handle}{#if !g.writable}<span class="ml-1">read-only</span>{/if}
      </div>
      {#each g.screens as s (s.ref)}
        <button type="button" class={rowClass(subject.is("screen", s.ref))}
                aria-current={subject.is("screen", s.ref) ? "true" : undefined}
                onclick={() => subject.select("screen", s.ref)}>{s.label}</button>
      {/each}
    {/each}
  {/snippet}

  {#snippet panelsBody()}
    {#each screens.meta.panels ?? [] as p (p.id)}
      <button type="button" class={rowClass(subject.is("panel", p.id))}
              aria-current={subject.is("panel", p.id) ? "true" : undefined}
              onclick={() => subject.select("panel", p.id)}>{p.name}</button>
    {/each}
  {/snippet}

  {#snippet devicesBody()}
    {#each screens.meta.devices ?? [] as d (d.id)}
      <button type="button" class={rowClass(subject.is("device", d.id))}
              aria-current={subject.is("device", d.id) ? "true" : undefined}
              onclick={() => subject.select("device", d.id)}>{d.name ?? d.id} · {devicePanelLabel(d)}</button>
    {/each}
  {/snippet}

  {@render section("screen", "Screens", screensBody)}
  {@render section("panel", "Panels", panelsBody)}
  {@render section("device", "Devices", devicesBody)}
</div>
