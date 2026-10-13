<script>
  // The Edit pane for a screen repo (U11): Details (handle, URL, ref, token)
  // and a read-only Status block. Nothing here renders: a repo has no preview.
  import { subject } from "../state/subject.svelte.js";
  import { screenRepos, NEW_REPO } from "../state/screenRepos.svelte.js";
  import { Input } from "$lib/ui/input/index.js";
  import { Button } from "$lib/ui/button/index.js";
  import * as Field from "$lib/ui/field/index.js";
  import { Circle, RefreshCw, Save, Undo2 } from "@lucide/svelte";
  import PaneHeader from "./PaneHeader.svelte";
  import HeaderAction from "./HeaderAction.svelte";

  const key = $derived(subject.id);
  const isNew = $derived(key === NEW_REPO);
  const info = $derived(isNew ? null : screenRepos.find(key));
  const d = $derived(screenRepos.draft(key));
  const readOnly = $derived(screenRepos.readOnly(key));
  const dirty = $derived(screenRepos.dirty(key));
  const missing = $derived(screenRepos.missing(key));
  const canSave = $derived(dirty && missing.length === 0 && !saving);
  const failure = $derived(screenRepos.failure?.key === key ? screenRepos.failure : null);
  const tokenSet = $derived(!!info?.has_token && !d?.clearToken);
  // Update is allowed in add-on mode too (it changes no config); only the
  // repos byonk registers itself cannot be updated here.
  const canUpdate = $derived(!!info && info.managed === false && info.kind === "git");

  let saving = $state(false);
  let updating = $state(false);

  async function save() {
    if (!canSave) return;
    saving = true;
    try {
      await screenRepos.save(key);
    } finally {
      saving = false;
    }
  }
  function discard() {
    if (isNew) {
      screenRepos.discardNew();
      subject.clear();
    } else {
      screenRepos.forget(key);
    }
  }
  async function update() {
    updating = true;
    try {
      await screenRepos.update(key);
    } finally {
      updating = false;
    }
  }
  const on = (field) => (e) => screenRepos.set(key, field, e.currentTarget.value);

  function source(r) {
    if (r?.kind === "builtin") return "built in";
    if (r?.kind === "local") return r.repo ?? "local folder";
    if (r?.kind === "path") return r.path ?? "folder";
    if (r?.kind === "dir") return "folder";
    return r?.repo ?? "—";
  }
  function when(ts) {
    if (!ts) return "never";
    const t = new Date(ts);
    return Number.isNaN(t.getTime()) ? ts : t.toLocaleString();
  }
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col text-sm" data-testid="repo-editor">
  {#snippet subjectName()}
    <span data-testid="edit-subject" class="truncate font-mono text-foreground">screen repo {isNew ? "(new)" : key}</span>
    {#if dirty}
      <span role="img" aria-label="unsaved changes" class="shrink-0 text-muted-foreground"><Circle class="size-2 fill-current" aria-hidden="true" /></span>
    {/if}
  {/snippet}
  {#snippet headerActions()}
    {#if canUpdate}
      <HeaderAction icon={RefreshCw} label="Update now" variant="ghost" disabled={updating} onclick={update} />
    {/if}
    {#if !readOnly}
      <HeaderAction icon={Undo2} label="Discard" variant="ghost" disabled={!dirty} onclick={discard} />
      <HeaderAction icon={Save} label="Save" disabled={!canSave} onclick={save} />
    {/if}
  {/snippet}
  <PaneHeader role="toolbar" aria-label="Actions" title="Edit" subject={subjectName} actions={headerActions} />
  {#if failure}
    <!-- The same fixed message row as the config editor's: a refusal stays in view. -->
    <div data-testid="repo-messages" class="flex shrink-0 flex-col gap-0.5 border-b border-border px-2 py-1 text-xs">
      <Field.Error class="text-xs">{failure.op === "update" ? "Update failed" : "Save failed"}: {failure.message}</Field.Error>
    </div>
  {/if}
  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3">
    {#if !d && screenRepos.error}
      <!-- Saved, but the list could not be reloaded: not "gone", just not known yet. -->
      <p class="text-destructive">Could not reload the screen repos: {screenRepos.error}</p>
    {:else if !d}
      <p class="text-muted-foreground">This screen repo is gone. Pick another one in the Library.</p>
    {:else}
      <!-- Always open, and no "…" in the header: for a screen repo the Details
           are the whole editor (the Status block below is read-only), so there
           is nothing else for them to make room for. -->
      <section class="flex flex-col gap-1 pb-2" aria-label="Details">
        <h3 class="font-mono text-xs">Details</h3>
        {#if readOnly}
          <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
            <dt class="text-muted-foreground">Handle</dt><dd class="truncate font-mono">{d.handle}</dd>
            <dt class="text-muted-foreground">{info?.kind === "path" || info?.kind === "dir" ? "Folder" : "Repo URL"}</dt><dd class="truncate font-mono">{source(info)}</dd>
            {#if info?.pin}<dt class="text-muted-foreground">Ref</dt><dd class="truncate font-mono">{info.pin}</dd>{/if}
            {#if info?.has_token}<dt class="text-muted-foreground">Token</dt><dd>set</dd>{/if}
          </dl>
          {#if info?.managed}
            <p class="text-xs text-muted-foreground">byonk registers this repo itself; it cannot be changed here.</p>
          {:else if info?.kind === "path"}
            <p class="text-xs text-muted-foreground">A folder on disk, not a git repo: change it in config.yaml.</p>
          {:else if info?.kind === "dir"}
            <p class="text-xs text-muted-foreground">A folder from SCREEN_REPOS_DIR, not a git repo; it is not in config.yaml.</p>
          {/if}
        {:else}
          <Field.Group class="gap-3">
            <Field.Field>
              {#if isNew}
                <Field.Label for="repo-handle">Handle</Field.Label>
                <Input id="repo-handle" class="font-mono" value={d.handle} oninput={on("handle")} />
                <Field.Description class="text-xs">The first part of every screen ref in this repo (<code>handle/path</code>).</Field.Description>
              {:else}
                <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 text-xs">
                  <dt class="text-muted-foreground">Handle</dt><dd class="truncate font-mono">{d.handle}</dd>
                </dl>
              {/if}
            </Field.Field>
            <Field.Field>
              <Field.Label for="repo-url">Repo URL</Field.Label>
              <Input id="repo-url" class="font-mono" value={d.repo} oninput={on("repo")} />
              <Field.Description class="text-xs">Needs a scheme, e.g. <code>https://…</code> or <code>git@host:…</code>.</Field.Description>
            </Field.Field>
            <Field.Field>
              <Field.Label for="repo-pin">Ref</Field.Label>
              <Input id="repo-pin" class="font-mono" placeholder="branch, tag or sha" value={d.pin} oninput={on("pin")} />
            </Field.Field>
            <Field.Field>
              <Field.Label for="repo-token">Token</Field.Label>
              <div class="flex items-center gap-2">
                <Input id="repo-token" type="password" autocomplete="off" class="min-w-0 flex-1"
                       placeholder={tokenSet ? "set" : ""} value={d.token} oninput={on("token")} />
                {#if tokenSet}
                  <Button type="button" size="xs" variant="outline" onclick={() => screenRepos.clearToken(key)}>Clear token</Button>
                {/if}
              </div>
              <Field.Description class="text-xs">
                {#if d.clearToken}The token is removed on Save.{:else}Write-only: leave it empty to keep the stored one.{/if}
              </Field.Description>
            </Field.Field>
            {#each missing as m (m)}
              <Field.Error class="text-xs">{m}</Field.Error>
            {/each}
          </Field.Group>
        {/if}
      </section>

      {#if info}
        <section class="flex flex-col gap-1 border-t border-border py-2" aria-label="Status">
          <h3 class="font-mono text-xs">Status</h3>
          <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
            <dt class="text-muted-foreground">State</dt>
            <dd class={info.status === "error" ? "text-destructive" : ""}>{info.status}</dd>
            <dt class="text-muted-foreground">Screens</dt><dd>{info.screen_count}</dd>
            {#if info.kind === "git"}
              <dt class="text-muted-foreground">Resolved sha</dt>
              <dd class="truncate font-mono" title={info.resolved_sha ?? undefined}>{info.resolved_sha ?? "—"}</dd>
              <dt class="text-muted-foreground">Last fetch</dt><dd>{when(info.last_fetched)}</dd>
            {/if}
            {#if info.error}
              <dt class="text-muted-foreground">Last error</dt><dd class="break-words text-destructive">{info.error}</dd>
            {/if}
          </dl>
        </section>
      {/if}
    {/if}
  </div>
</div>
