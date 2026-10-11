<script>
  import { screens } from "../state/screens.svelte.js";
  import { subject } from "../state/subject.svelte.js";
  import { editor } from "../state/editor.svelte.js";
  import { configDraft } from "../state/configDraft.svelte.js";
  import * as api from "../api.js";
  import { devicePanel } from "../render-request.js"; // the one device->panel lookup (ruling PF8)
  import { untrack } from "svelte";
  import { FilePlus, Copy, Pencil, Trash2, Ellipsis, Plus } from "@lucide/svelte";
  import * as DropdownMenu from "$lib/ui/dropdown-menu/index.js";
  import { Button } from "$lib/ui/button/index.js";
  import PaneHeader from "./PaneHeader.svelte";
  import IconButton from "./IconButton.svelte";
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
   *   { kind: "delete-entry", target, id, name } a panel or a device; `target`
   *                                              is its kind
   */
  let ask = $state(null);

  $effect(() => {
    if (ask?.ref && subject.screenRef !== ask.ref) ask = null;
    if (ask?.kind === "delete-entry" && !subject.is(ask.target, ask.id)) ask = null;
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
  // Sidebar tokens, like the rail: the selection is a quiet accent row, not
  // a primary-coloured bar, and hover is a lighter step of the same accent.
  const rowClass = (on) =>
    "block h-(--row-h) w-full shrink-0 truncate px-2 text-left " +
    (on
      ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
      : "hover:bg-sidebar-accent/60 hover:text-sidebar-accent-foreground");

  function newScreen() {
    ask = { kind: "new" };
  }
  function copyScreen() {
    if (subject.screenRef) ask = { kind: "copy", ref: subject.screenRef };
  }
  function renameScreen() {
    if (subject.screenRef) ask = { kind: "rename", ref: subject.screenRef };
  }
  // Delete acts on the subject whatever its kind (U7); Copy and Rename stay
  // screen-only.
  const DELETE_TIP = { screen: "Delete screen", panel: "Delete panel", device: "Delete device" };
  const deleteTip = $derived(DELETE_TIP[subject.kind] ?? "Delete screen");
  const canDelete = $derived(subject.kind === "panel" || subject.kind === "device" || canEdit);
  function deleteSubject() {
    if (subject.screenRef) {
      ask = { kind: "delete", ref: subject.screenRef };
    } else if (subject.kind === "panel" || subject.kind === "device") {
      const list = subject.kind === "panel" ? screens.meta.panels : screens.meta.devices;
      const name = (list ?? []).find((x) => x.id === subject.id)?.name ?? null;
      ask = { kind: "delete-entry", target: subject.kind, id: subject.id, name };
    }
  }
  const closeForm = () => (ask = null);

  // --- "+": a placeholder panel / device (U7) ------------------------------
  // The entry is written to config.yaml at once (ruling R1: the preview, Push
  // and overrides need one that exists) and selected; its identity is then
  // edited in the Edit pane's Details block.
  function expandSection(key) {
    if (!isOpen(key)) toggleSection(key);
  }

  /** `base`, else `base<sep>2`, `base<sep>3`, ... -- the first one not taken. */
  function freeId(base, sep, list) {
    const taken = new Set((list ?? []).map((x) => x.id));
    if (!taken.has(base)) return base;
    for (let n = 2; ; n++) if (!taken.has(`${base}${sep}${n}`)) return `${base}${sep}${n}`;
  }

  const DEFAULT_PANEL_COLORS = "#000000,#FFFFFF";
  async function newPanel() {
    const current = subject.kind === "panel"
      ? (screens.meta.panels ?? []).find((p) => p.id === subject.id)
      : null;
    const body = { id: freeId("new_panel", "_", screens.meta.panels), name: "New panel",
                   colors: current?.colors ?? DEFAULT_PANEL_COLORS };
    if (current?.width) body.width = current.width;
    if (current?.height) body.height = current.height;
    expandSection("panel");
    await run("create panel", async () => {
      const { id } = await api.createPanel(body);
      await screens.refresh();
      subject.select("panel", id ?? body.id);
    });
  }

  async function newDevice() {
    const body = { key: freeId("new-device", "-", screens.meta.devices), name: "New device",
                   screen: subject.screenRef ?? screens.list[0]?.ref ?? "" };
    expandSection("device");
    await run("create device", async () => {
      const { key } = await api.createDevice(body);
      await screens.refresh();
      subject.select("device", key ?? body.key);
    });
  }

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

  async function submitDeleteEntry() {
    const { target, id } = ask;
    const ok = await run(`delete ${target}`, async () => {
      if (target === "panel") await api.deletePanel(id);
      else await api.deleteDevice(id);
      configDraft.forget(target, id);
      subject.clear();
      await screens.refresh();
    });
    if (ok) ask = null;
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col text-xs" role="region" aria-label="Library">
  <PaneHeader title="Library">
    {#snippet actions()}
      <!-- Wide header: four icon buttons. Below 14rem (title + four buttons
           no longer fit; at 1024 px the pane is ~177 px, at its 12% minimum
           in an 800 px window ~136 px) they fold into one "More actions"
           menu. Both forms are in the DOM; the container query only hides
           one, so without CSS (jsdom) the wide buttons stay reachable. -->
      <div data-testid="library-actions" class="flex items-center gap-1 @max-[14rem]:hidden">
        <IconButton icon={FilePlus} label="New" tip="New screen" onclick={newScreen} />
        <IconButton icon={Copy} label="Copy" tip="Copy screen" onclick={copyScreen} disabled={!subject.screenRef} />
        <IconButton icon={Pencil} label="Rename" tip="Rename screen" onclick={renameScreen} disabled={!canEdit} />
        <IconButton icon={Trash2} label="Delete" tip={deleteTip} onclick={deleteSubject} disabled={!canDelete} />
      </div>
      <div data-testid="library-actions-menu" class="hidden @max-[14rem]:flex">
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button {...props} type="button" size="icon-sm" variant="ghost" aria-label="More actions">
                <Ellipsis aria-hidden="true" />
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <!-- An item may open an inline form that focuses its own field or
               button; the menu must not pull focus back to its trigger as
               it closes. -->
          <DropdownMenu.Content align="end" onCloseAutoFocus={(e) => e.preventDefault()}>
            <DropdownMenu.Item aria-label="New" onSelect={newScreen}><FilePlus aria-hidden="true" />New screen</DropdownMenu.Item>
            <DropdownMenu.Item aria-label="Copy" disabled={!subject.screenRef} onSelect={copyScreen}><Copy aria-hidden="true" />Copy screen</DropdownMenu.Item>
            <DropdownMenu.Item aria-label="Rename" disabled={!canEdit} onSelect={renameScreen}><Pencil aria-hidden="true" />Rename screen</DropdownMenu.Item>
            <DropdownMenu.Item aria-label="Delete" variant="destructive" disabled={!canDelete} onSelect={deleteSubject}><Trash2 aria-hidden="true" />{deleteTip}</DropdownMenu.Item>
            <DropdownMenu.Separator />
            <DropdownMenu.Item onSelect={newPanel}><Plus aria-hidden="true" />Add panel</DropdownMenu.Item>
            <DropdownMenu.Item onSelect={newDevice}><Plus aria-hidden="true" />Add device</DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </div>
    {/snippet}
  </PaneHeader>

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
  {:else if ask?.kind === "delete-entry"}
    <InlineConfirm
      label={`Confirm ${ask.target} deletion`}
      message={`Delete ${ask.target} ${ask.name ? `${ask.name} (${ask.id})` : ask.id}? This cannot be undone.`}
      onconfirm={submitDeleteEntry}
      oncancel={closeForm}
    />
  {/if}

  {#snippet section(key, label, body, add)}
    <!-- Expanded sections share the pane height (flex-1 on a zero basis) and
         scroll on their own; a collapsed one shrinks to its header. -->
    <section class="flex flex-col border-t border-border {isOpen(key) ? 'min-h-(--section-h) flex-1 basis-0 overflow-hidden' : 'shrink-0'}"
             aria-label={label}>
      <!-- The header row holds the disclosure toggle and, beside it (never
           inside it: a button may not nest a button), the section's "+". -->
      <div class="flex h-(--section-h) shrink-0 items-center bg-muted">
        <button type="button" aria-expanded={isOpen(key)}
                class="flex h-(--section-h) min-w-0 flex-1 items-center gap-1 px-1.5 text-left text-xs font-bold uppercase tracking-wide hover:bg-accent"
                onclick={() => toggleSection(key)}>
          <svg aria-hidden="true" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round"
               class="size-4 shrink-0 transition-transform {isOpen(key) ? 'rotate-90' : ''}">
            <path d="M6 3.5 10.5 8 6 12.5" />
          </svg><span class="truncate">{label}</span>
        </button>
        {#if add}
          <IconButton icon={Plus} label={add.label} size="icon-xs" class="shrink-0" onclick={add.onclick} />
        {/if}
      </div>
      {#if isOpen(key)}
        <div class="min-h-0 flex-1 overflow-auto pb-1">{@render body()}</div>
      {/if}
    </section>
  {/snippet}

  {#snippet screensBody()}
    {#each screens.grouped as g (g.handle)}
      <div class="mx-2 mt-3 shrink-0 cursor-default select-none border-b border-border pb-0.5 font-mono text-xs uppercase text-muted-foreground first:mt-1">
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
  {@render section("panel", "Panels", panelsBody, { label: "Add panel", onclick: newPanel })}
  {@render section("device", "Devices", devicesBody, { label: "Add device", onclick: newDevice })}
</div>
