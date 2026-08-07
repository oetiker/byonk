# Screen Authoring

This page explains where byonk's screens actually come from, why some are
editable and others aren't, and how to turn a read-only screen into a
starting point for your own.

## Three source layers

Every screen ref (`handle/path`) resolves through a **screen repo** — a
directory tree with a `byonk-screens.yaml` manifest, where every folder
containing a `meta.yaml` is a screen (see
[Screens live in screen repos, not config entries](configuration.md#screens-live-in-screen-repos-not-config-entries)).
byonk ships three of these out of the box, and they play different roles:

| Layer | Handle | Writable? | What it's for |
|-------|--------|-----------|----------------|
| Base include library | `byonk-base-v1` | No (embedded) | Shared SVG layouts and components (`base.svg`, `hinting.svg`, `header.svg`, …) that screens `{% extends %}` or `{% include %}` — see [SVG Templates](../tutorial/svg-templates.md). It's also a sandboxed Lua module namespace: `require("byonk-base-v1/std")` and similar from `script.lua`. Not a screen repo itself; you never reference it as `handle/path` for a *screen*, only inside `{% include "byonk-base-v1/…" %}` or `require("byonk-base-v1/…")`. |
| Built-in screens | `byonk-builtin` | No | A minimal, fixed set: `default` (the fallback screen for un-onboarded/unassigned devices) and `calibration/*` (panel calibration patterns). Always present, never changes shape. |
| Examples | `examples` | Yes | Worked, runnable samples (`hello`, `mandelbrot`, `webscrape`, `gphoto`, `swiss-departure-board`, a font demo) — seeded to disk once so you can read, run, and edit them directly. |

Your own screens live in a fourth place: the `local` repo, described below.
It isn't a *shipped* layer — it starts out empty (or with whatever you put
in it).

## What makes a repo writable

Writability is a property of *where a screen repo's files live*, not of its
name. A screen repo backed by files on disk under a directory byonk manages
(`local`, `examples`, or any `path:`-configured repo — see below) is
writable. A screen repo embedded in the binary (`byonk-builtin`) or checked
out from git (`repo:`-configured, including the git-fetched form of
`screen_repos` entries) is read-only — git checkouts can be silently
replaced by the next fetch, so treating them as writable would risk losing
edits.

This is why the `byonk-builtin` *handle* can never be re-pointed at your own
files: it's always the embedded set, wherever you run byonk.

**However**, watch out for a sharp edge this doesn't protect you from:
`SCREENS_DIR` (your `local` repo) is also checked, file by file, whenever a
`byonk-builtin` screen is *read* — so a `local` screen that happens to reuse
a built-in's exact folder name silently overrides that built-in's files.
(This is per-file only: `byonk-builtin`'s *set of screens* is fixed by what's
embedded in the binary, so your own screens are never listed under it — they
appear once, under `local`.) In
particular, don't create `local/default` or `local/calibration/color` (etc.)
expecting them to be independent of `byonk-builtin/default` and
`byonk-builtin/calibration/color` — they aren't; `byonk-builtin/default` is
the fallback screen shown to un-onboarded and unassigned devices, so
overriding it this way is easy to do by accident. Pick a different name for
your own screens (as in the examples on this page) and this never comes up.

## Where your own screens live: `local`

Set `SCREENS_DIR` (or, for the Home Assistant app, use its `/config/screens`
folder) and byonk auto-registers it as a writable screen repo under the
handle **`local`** — no `screen_repos:` entry required. This is where you
put screens you author yourself: `local/my-clock`, `local/hello` from the
[tutorial](../tutorial/first-screen.md), and so on.

An empty or missing `SCREENS_DIR` is seeded once with just the
`byonk-screens.yaml` manifest that registers it — never with copies of the
built-in screens, which stay embedded-only. See
[Environment Variables](installation.md#environment-variables) for the
seeding details.

## Where examples land: `EXAMPLES_DIR`

The shipped example screens are seeded once to `<SCREENS_DIR>/../examples`
by default, and auto-register as the writable `examples` handle — override
the location with the `EXAMPLES_DIR` environment variable (useful in Docker,
where the derived default may fall outside your mounted volume). See
[Environment Variables](installation.md#environment-variables) for the full
seeding-vs-registration precedence rules.

## The `path:` config variant

Beyond `local` and `examples`, you can register any writable directory as a
named screen repo with `screen_repos.<handle>.path`:

```yaml
screen_repos:
  drafts: { path: /data/drafts }
```

This is the writable counterpart to `repo:` (a git-fetched, read-only screen
repo): `repo` and `path` are mutually exclusive on the same entry. Use
`path:` when you want a second writable repo of your own — organized
separately from `local` — rather than a third layer for everyone.

## Fork-to-edit

Because `byonk-builtin` and any `repo:`-configured screen repo are
read-only, the way to customize one of their screens is to **copy it into a
writable repo** and edit the copy — the original keeps working for anyone
still referencing it, and your edits are safe from the next git fetch.

Today that copy is a manual step: copy the screen's `meta.yaml`,
`script.lua`, and `screen.svg` (and any other files in its folder) from the
read-only repo's directory into `local` or `examples` under a new name, then
point a device's `screen` at the new `handle/path`. For example, to base a
screen on `examples/hello`:

```
cp -r <examples dir>/hello <SCREENS_DIR>/my-hello
```

then set `screen: local/my-hello` on a device.

## Forking from an MCP client

You don't have to copy files by hand. byonk exposes its screen-authoring core
(`ScreenStore`) over MCP, so an LLM client can do the whole loop for you. Point
your client at `/mcp` with the admin token as a bearer credential — see
[MCP](mcp.md) for the endpoint and authentication details — then:

- `copy_screen` forks any screen, including read-only builtins and examples,
  into a writable repo. Pass the destination **repo handle** as `to_handle`
  (e.g. `local`) and the new screen's **path segment** as `to_name`
  (e.g. `my-hello`), yielding `local/my-hello`.
- `read_screen_file` / `write_screen_file` read and edit `meta.yaml`,
  `script.lua` and `screen.svg`. Writes take an optional `if_match` etag so
  concurrent edits don't clobber each other.
- `validate_screen` and `render_screen` check your work; `render_screen`
  returns the actual dithered PNG plus the script's `log`, `data` and `error`,
  which is the fastest way to debug a script.
- `assign_screen` points a device at the result.

`list_screens` and `list_screen_repos` report which handles are writable —
only those can be edited in place, so fork a builtin first.

A web-based screen editor is still to come; this page will gain a section on it
once it lands.

## Next Steps

- [Your First Screen](../tutorial/first-screen.md) — build a screen from scratch in `local`
- [Configuration](configuration.md#screen-repos-section) — the `screen_repos:` section reference
- [Installation](installation.md#environment-variables) — `SCREENS_DIR`, `EXAMPLES_DIR`, and seeding behavior
