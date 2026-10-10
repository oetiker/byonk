<script>
  import { Library, Code, Monitor, LogOut } from "@lucide/svelte";
  import { layout, PANELS } from "../state/layout.svelte.js";
  import { auth } from "../state/auth.svelte.js";
  import ThemeSwitch from "./ThemeSwitch.svelte";

  const ICONS = { library: Library, editor: Code, preview: Monitor };
  const LABELS = { library: "Library", editor: "Edit", preview: "Preview" };
</script>

<div
  role="toolbar"
  aria-label="Panels"
  aria-orientation="vertical"
  class="flex w-10 shrink-0 flex-col items-center gap-1 border-r
         border-border bg-muted py-2"
>
  {#each PANELS as p (p)}
    {@const Icon = ICONS[p]}
    <button
      type="button"
      aria-pressed={layout.isOpen(p)}
      aria-label={LABELS[p]}
      title={LABELS[p]}
      onclick={() => layout.toggle(p)}
      class="flex h-8 w-8 items-center justify-center rounded
             text-muted-foreground hover:bg-background
             aria-pressed:bg-background
             aria-pressed:text-primary"
    >
      <Icon size={16} aria-hidden="true" />
    </button>
  {/each}

  <!-- Ruling 52 (theme switch) / Ruling 63 (logout): both grouped with the
       Shell in spec, mounted here because IconRail is the only chrome no
       panel toggle can unmount -- either must stay reachable with every
       panel, including the editor, closed, and in every legal panel
       combination (Ruling 49). -->
  <div class="mt-auto flex flex-col items-center gap-1">
    {#if auth.canLogout}
      <!-- Hidden outright, not disabled, when `can_logout` is false (e.g.
           under Home Assistant Ingress or the admin bearer token) --
           spec docs/superpowers/specs/2026-09-07-dev-mode-editor-design.md:360
           and Task 14 Step 3 item 3 both require that no Logout control
           renders at all in that case, not merely an inert one. -->
      <button
        type="button"
        aria-label="Logout"
        title={auth.user ? `Logout (${auth.user})` : "Logout"}
        onclick={() => auth.logout()}
        class="flex h-8 w-8 items-center justify-center rounded
               text-muted-foreground hover:bg-background
               hover:text-destructive"
      >
        <LogOut size={16} aria-hidden="true" />
      </button>
    {/if}
    <ThemeSwitch />
  </div>
</div>
