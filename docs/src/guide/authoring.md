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
| Base include library | `byonk-base-v1` | No (embedded) | Shared SVG layouts and components (`base.svg`, `hinting.svg`, `header.svg`, …) that screens `{% extends %}` or `{% include %}` — see [SVG Templates](../tutorial/svg-templates.md). Not a screen repo itself; you never reference it as `handle/path` for a *screen*, only inside `{% include "byonk-base-v1/…" %}`. |
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

This is why `byonk-builtin` can never be edited in place, even though its
files exist on disk in a Docker volume or the Home Assistant app's `/config`
share: the handle is embedded, and edits to a same-named on-disk folder
wouldn't be the thing devices actually resolve.

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

byonk's internal screen-authoring core (`ScreenStore`) already implements
this fork operation — along with validating and rendering a screen's source
files — as a foundation for upcoming interfaces (an MCP tool surface for
LLM-driven authoring, and a web-based screen editor) that will do this copy
for you. Those aren't wired up yet; this page will gain a "how to fork from
the UI/MCP" section once they land.

## Next Steps

- [Your First Screen](../tutorial/first-screen.md) — build a screen from scratch in `local`
- [Configuration](configuration.md#screen-repos-section) — the `screen_repos:` section reference
- [Installation](installation.md#environment-variables) — `SCREENS_DIR`, `EXAMPLES_DIR`, and seeding behavior
