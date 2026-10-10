# Dev UI layout process (shadcn-svelte, Tailwind v4)

Follow this **before writing markup** for any dev-UI screen, pane or region.
Applies to everything under `webui/`. Every UI task brief points here.

Stack facts this relies on: shadcn-svelte `nova` style, base colour `neutral`,
bits-ui 2.19, Tailwind 4.3, lucide icons, `webui/src/app.css` holds all tokens.

---

## 0. The process (in order)

| Step | Output | Rule |
|---|---|---|
| 1. Region sketch | ASCII box sketch of the screen: rail, panes, rows | Name every **row** (header, toolbar, body, footer) in every pane. Rows that sit side by side across panes must share a token height (§3). |
| 2. Row inventory | Table: row → height token → contents (left / right) | Every row has a height **token**, not "whatever the content makes it". No row may `flex-wrap`. |
| 3. Component pick | Each control mapped to a shadcn-svelte component (§5) | Hand-rolling needs a written reason (e.g. "Tabs can't host a nested close button"). |
| 4. Scroll owner | One scroller per region, named in the sketch | §4 flex chain. |
| 5. Token check | Only semantic colour utilities, only scale sizes | No `[var(--x)]`, no `text-[11px]`, no hex. |
| 6. Build | Use `PaneHeader` / `PaneToolbar` (shared components) | Alignment comes from the shared component, never from per-pane discipline. |
| 7. Measure | Browser measurement script (§8), light + dark, 3 widths | jsdom has no layout: a vitest pass proves nothing about alignment. |

---

## 1. Tokens (decided once, in `app.css`)

### 1.1 Spacing

Tailwind `--spacing: 0.25rem` (4px). Allowed steps in chrome:

| Use | Class | px |
|---|---|---|
| Gap inside a control group / icon-to-label | `gap-1` | 4 |
| Gap between header items | `gap-2` | 8 |
| Horizontal gutter of every header, toolbar, list row, status bar | `px-2` | 8 |
| Form body padding | `p-3` | 12 |
| Gap between form fields | `gap-3` | 12 |
| Label → control | `gap-1.5` | 6 |

Anything else needs a reason. (Refactoring UI: pick from a constrained scale, don't
nudge per element — https://www.refactoringui.com/ ; Material density works the same
way, in fixed 4dp steps — https://m2.material.io/design/layout/applying-density.html )

### 1.2 Heights (one row height per row *kind*)

Define as CSS variables on `:root`, consume with Tailwind v4's `h-(--x)` syntax — exactly
the shadcn-svelte block convention (`dashboard-01` sets `--header-height:
calc(var(--spacing) * 12)` once on the provider; `SiteHeader` uses `h-(--header-height)`;
`sidebar-16` does the same with `--spacing(14)`).

| Token | Value | Holds | Used by |
|---|---|---|---|
| `--pane-header-h` | `calc(var(--spacing) * 9)` = 36px | controls up to 28px (`size="sm"`, `icon-sm`, `h-7` select) | row 1 of **every** pane: Library, Edit, Preview |
| `--pane-toolbar-h` | `calc(var(--spacing) * 8)` = 32px | controls up to 24px (`size="xs"`, `icon-xs`) | optional row 2: file tabs, view options |
| `--section-h` | `calc(var(--spacing) * 6)` = 24px | collapsible section headers | Library sections, Simulated inputs |
| `--row-h` | `calc(var(--spacing) * 6)` = 24px | list rows | Library items |
| `--status-h` | `calc(var(--spacing) * 6)` = 24px | status text | Edit StatusBar |

Heights are **set**, not derived from padding + content. Use `h-(--pane-header-h)` with
`items-center`, never `py-1` on a header.

### 1.3 Type

Fix the line heights too — Tailwind's defaults are ratios (`--text-xs--line-height:
calc(1 / 0.75)`), so an 11px size gives a 14.67px line and sub-pixel row heights.

```css
@theme {
  --text-xs: 0.6875rem;  --text-xs--line-height: 1rem;     /* 11/16 */
  --text-sm: 0.8125rem;  --text-sm--line-height: 1.25rem;  /* 13/20 */
  --text-base: 0.875rem; --text-base--line-height: 1.25rem;/* 14/20 */
}
```

| Role | Classes |
|---|---|
| Pane title (row 1, left) | `text-xs font-semibold uppercase tracking-wide text-muted-foreground` |
| Subject name in a header | `truncate font-mono text-xs text-foreground` |
| Chrome text (toolbars, rows, status) | `text-xs` |
| Form body (labels, inputs) | `text-sm` |
| Section header | `text-xs font-semibold uppercase tracking-wide` |
| Technical values (refs, sizes, cursor, hex) | add `font-mono` (spec 2026-09-09 §2) |
| Group label inside a list | `text-xs text-muted-foreground` (never `text-[10px]`) |

### 1.4 Colour (light/dark through CSS variables only)

shadcn-svelte theming: every colour is a `--name` / `--name-foreground` pair on `:root`,
overridden under `.dark`, and exposed to Tailwind through `@theme inline`
(https://www.shadcn-svelte.com/docs/theming). New colours are added the same way
(`--success`, `--warning` → `@theme inline { --color-success: var(--success); }`).

Rules:
- **One token system.** byonk's identity colours (navy dark, `#1e5aa8` accent) are written
  *into* the shadcn variables (`--background`, `--card`, `--muted`, `--border`, `--primary`,
  `--accent`, `--ring`, `--sidebar-*`, `--destructive`), not kept beside them.
- Components use only semantic utilities: `bg-background`, `bg-sidebar`, `bg-muted`,
  `border-border`, `text-muted-foreground`, `text-destructive`, `text-success`.
- Panes (Edit, Preview) = `bg-background`; navigation chrome (IconRail, Library) =
  `bg-sidebar text-sidebar-foreground`; selected row = `bg-sidebar-accent
  text-sidebar-accent-foreground` (+ a 2px `--sidebar-primary` inset bar if it must be loud).
- `.dark` on `<html>`; the existing theme store already does what `mode-watcher` does.

### 1.5 Surfaces

| Thing | Treatment |
|---|---|
| Pane-to-pane, row-to-row separation | 1px `border-border` (bottom of header, top of footer); **no shadows** |
| Floating things (popover, tooltip, render overlay, lens) | shadow allowed (`shadow-md`) |
| Panes / rows | square corners |
| Controls | component radius (from `--radius`) |

---

## 2. Pane anatomy template

```
┌ pane (flex flex-col min-h-0 min-w-0) ─────────────────────────┐
│ 1 HEADER   h-(--pane-header-h)  border-b  px-2  gap-2         │  required
│   [TITLE] [subject, font-mono, truncate]  ······  [actions ▸] │
├───────────────────────────────────────────────────────────────┤
│ 2 TOOLBAR  h-(--pane-toolbar-h) border-b  px-2 (or tabs flush) │  optional
├───────────────────────────────────────────────────────────────┤
│ 3 BODY     min-h-0 flex-1 overflow-auto   (the ONE scroller)   │  required
├───────────────────────────────────────────────────────────────┤
│ 4 FOOTER   h-(--status-h) border-t px-2 text-xs                │  optional
└───────────────────────────────────────────────────────────────┘
```

Build rows 1 and 2 only through two shared components:

```svelte
<!-- PaneHeader.svelte -->
<header data-slot="pane-header"
  class="flex h-(--pane-header-h) min-w-0 shrink-0 items-center gap-2 border-b px-2 text-xs">
  <span class="shrink-0 font-semibold uppercase tracking-wide text-muted-foreground">{title}</span>
  <div class="flex min-w-0 flex-1 items-center gap-2">{@render subject?.()}</div>
  <div class="flex shrink-0 items-center gap-1">{@render actions?.()}</div>
</header>
```

`PaneToolbar.svelte` is the same with `h-(--pane-toolbar-h)` and `data-slot="pane-toolbar"`.

---

## 3. Alignment rules across panes

| # | Rule | Source |
|---|---|---|
| A1 | Every pane has row 1, and row 1 is `PaneHeader` — same height token, same bottom border, so the line runs straight across Library → Edit → Preview. | shadcn/ui *mail* example: all three resizable panels open with a 52px row followed by a full-width `<Separator />` (`apps/www/app/(app)/examples/mail/components/mail.tsx`, tag `shadcn@2.1.0`) |
| A2 | If any pane needs row 2, it is `PaneToolbar`. Row-2 lines need not align (panes differ), but each must be the token height. | — |
| A3 | No `flex-wrap` in rows 1/2. Overflow order: truncate subject → hide secondary text → move overflow actions into a `DropdownMenu` (⋯). | — |
| A4 | Same horizontal gutter `px-2` everywhere, so the first glyph of every header sits 8px in. Flush tab strips are the only exception (tabs carry their own `px-2`). | — |
| A5 | Banners that span all panes (ConflictBanner, StatusStrip) sit **above** the pane row so they shift all panes equally. Never put a banner inside one pane above its header. | — |
| A6 | A pane's subject is named in **its header** (spec 2026-10-04 §2: "What we MODIFY is the Edit panel … and it names its subject in its own header"), never as an `h2` inside the scrolling body. | spec |

---

## 4. Scroll ownership (the min-h-0 chain)

| Rule | Why |
|---|---|
| Each pane root: `flex min-h-0 min-w-0 flex-1 flex-col` | flex items default to `min-height/min-width: auto` = content size, so they refuse to shrink and the scroller never engages (byonk already lost a toolbar to this — EditorPane "Finding M"). |
| Every ancestor between `AppShell`'s `h-full` root and the scroller carries `min-h-0` (column) / `min-w-0` (row) | one missing link breaks the chain |
| Header/toolbar/footer rows: `shrink-0` | they keep their token height |
| Exactly one scroller per region: `min-h-0 flex-1 overflow-auto` | nested scrollers trap the wheel |
| CodeMirror owns its own scroller; its host is `min-h-0 flex-1 overflow-hidden` | |
| Native `overflow-auto` is the default. Use shadcn `ScrollArea` only where a styled, always-thin scrollbar matters (e.g. Library); it needs a sized parent (`h-full`) like every example in https://www.shadcn-svelte.com/docs/components/scroll-area | |
| Stacked collapsible sections share height with `flex-1 basis-0 min-h-(--section-h)` when open, `shrink-0` when closed | |

---

## 5. Which shadcn-svelte component for what

All available in the registry (`pnpm dlx shadcn-svelte@latest add <name>`).

| Need | Component | Notes |
|---|---|---|
| Resizable pane split | `resizable` (PaneForge) | `Resizable.PaneGroup direction="horizontal" autoSaveId=…` replaces hand-written persistence; sizes are **percent** (`defaultSize`, `minSize`, `collapsible`, `collapsedSize`) — https://paneforge.com/docs/components/pane |
| Navigation column / icon rail | `sidebar` (`collapsible="icon"`), or keep rail + `Button variant="ghost" size="icon-sm"` + `Tooltip` | Sidebar width vars: `--sidebar-width`, `--sidebar-width-icon` (3rem) |
| Collapsible Library section | `collapsible` (bits-ui) | `Accordion` if only-one-open is wanted; byonk wants independent sections → Collapsible |
| Item lists in Library | `sidebar` `Menu`/`MenuButton isActive`, or plain buttons with the row tokens | |
| File tabs | `tabs` `variant="line"` if no nested close button; else hand-rolled strip on `PaneToolbar` with `Button size="icon-xs"` close | a button can't nest in a tab trigger |
| Pane / toolbar actions | `button` `size="xs"`/`"sm"`, `icon-xs`/`icon-sm`; `button-group` for adjacent related actions | one `variant="default"` (primary) per header at most; others `ghost`/`outline` |
| Icon-only button label | `tooltip` + `aria-label` | not `title=` |
| Vertical divider in a header | `separator orientation="vertical"` with `data-vertical:h-4 data-vertical:self-auto` | as in `sidebar-03/07` blocks |
| Form fields | `field` (`Field.Set/Group/Field/Label/Description/Error`), `input`, `textarea`, `native-select`, `checkbox`, `switch`, `slider` | `native-select` keeps native `<select>` semantics (and existing tests) with themed styling |
| Rich dropdown (icons, search) | `select` / `command` | |
| Inline confirm / banners | `alert` (`variant="destructive"`), `button` | |
| Busy indicator | `spinner` | |
| Empty pane | `empty` | VS Code: welcome views short, links over buttons — https://code.visualstudio.com/api/ux-guidelines/views |
| Keyboard hints | `kbd` | e.g. ⌘S beside Save |
| Icons | lucide (`@lucide/svelte`), `size-3.5` in xs controls, `size-4` default | never text glyphs (`+ − × ● ▲ ✓`) |

---

## 6. Action placement

From VS Code's container model (https://code.visualstudio.com/api/ux-guidelines/overview):
view-toolbar actions belong to the view, editor actions to the editor, status-bar left =
workspace scope, right = active-file scope.

| Scope | Place | Rule |
|---|---|---|
| Whole pane (New screen, Save, Push, Pop out) | Row 1 right (`PaneHeader` actions) | ≤4 visible, rest in ⋯ menu; primary rightmost |
| One view option of the pane's body (target, measured inks, lens) | Row 2 (`PaneToolbar`) or row 1 if it fits | |
| One list item (rename/delete a screen) | Row-hover actions, ≤3 (VS Code tree guideline), or the pane header acting on the selection | |
| Global (theme, logout) | IconRail bottom | always reachable |
| State, not action (valid, cursor, watch) | Footer: left = screen/workspace, right = file | |
| Destructive | Inline confirm in an `Alert` row directly under the header | never `confirm()` |

---

## 7. Density guard

Dense (spec 2026-09-09 §2) means *small fixed rows*, not *removed padding*: keep the 8px
gutter and the 4px internal gaps; get density from 24/32/36px rows and 11/13px type.
Hit targets ≥ 24×24 (WCAG 2.2 SC 2.5.8 Target Size Minimum —
https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

---

## 8. Verification checklist (every layout change)

jsdom computes no layout — **alignment can only be verified in a real browser.** Rebuild
byonk first (RustEmbed bakes `webui/dist` in at compile time; an old process serves the old
UI).

- [ ] Paste in DevTools (or `javascript_tool`) with Library, Edit and Preview open:
  ```js
  [...document.querySelectorAll('[data-slot=pane-header]')]
    .map(e => { const r = e.getBoundingClientRect(); return { h: r.height, bottom: r.bottom }; })
  ```
  All `h` equal to 36 and all `bottom` equal — at window widths 1440, 1024 and 800.
- [ ] Same with a long subject (`examples/swiss-departure-board`), a panel subject, a device
  subject: header heights unchanged (no wrap), subject truncates.
- [ ] Edit pane: subject named in row 1 for screen, panel and device.
- [ ] Every scroll region scrolls at a 600px-tall window; header and footer stay put.
- [ ] Light and dark: no neutral-grey shadcn surface on a navy pane (outline buttons, inputs,
  selects all sit on the same `--background`).
- [ ] `grep -rn '\[var(--\|text-\[[0-9]' webui/src/lib/components` → empty.
- [ ] Icon-only buttons: tooltip on hover/focus, `aria-label` present.
- [ ] Screenshot of the three panes at 1440 attached to the task report.
