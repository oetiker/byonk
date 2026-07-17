# Byonk - Claude Code Guidelines

## Project Overview

Byonk (Bring Your Own Ink) is a self-hosted content server for TRMNL e-ink devices. It uses Lua scripts for data fetching and SVG templates for rendering.

## Key Directories

- `src/` - Rust source code
- `screens/` - Lua scripts and SVG templates
- `fonts/` - Custom fonts
- `docs/` - mdBook documentation

## Session Handover Discipline

- **`docs/HANDOVER.md` is the cross-session handover** — the single source of truth for "where are we and what's next". Read it first at the start of any session.
- **When context grows large (roughly >25% used), at the next sensible pause point** (e.g. between reviewed tasks in a multi-task execution, never mid-task with an uncommitted tree), **rewrite `docs/HANDOVER.md` fresh and stop**, telling the user to start a new session. A fresh session with a good handover beats a long, degraded one.
- A good handover states: the active initiative, exact branch + HEAD, what's done vs. next, how to resume (which skill/plan/ledger), key decisions, and how to build/verify. Keep it current — overwrite it, don't append.
- **The SDD ledger `.superpowers/sdd/progress.md`** (git-ignored) records per-task review status and commit ranges during subagent-driven execution; it is the recovery map after a compaction. Trust it + `git log` over memory.

## Workflow Reminders

- **Always `git pull` first** before starting work to avoid conflicts
- **Update documentation** when user-visible features change or new ones are added (docs/src/)
- **All changes must be documented in CHANGES.md** - new features, fixes, and changes go in the Unreleased section
- **Commit in sensible intervals** - don't batch unrelated changes
- **Use Makefile targets** for building:
  - `make build` - build debug (runs fmt + clippy first)
  - `make release` - build release (runs fmt + clippy first)
  - `make check` - run fmt, clippy, and tests
  - `make docs` - build documentation

## Release Process

Releases are triggered via GitHub Actions workflow dispatch. The workflow:
1. Bumps version in Cargo.toml
2. Updates CHANGES.md (moves Unreleased to new version)
3. Builds binaries for all platforms
4. Builds Docker container
5. Creates GitHub release with artifacts

## Home Assistant Test Setup

A local QEMU HAOS VM (`tools/ha-vm/`, see its `README.md`) runs both byonk and the
integration for end-to-end testing. It boots headless via `make ha-vm`; user-mode NAT
forwards host ports **8123** (HA UI), **3000** (byonk), **4445** (Samba), **2222** (SSH).

- **byonk server** runs as a *local add-on* built from source (`addons/byonk/` inside the
  VM, its own `Dockerfile`). The add-on reads screens/fonts from `SCREENS_DIR=/config/screens`
  (the `addon_configs/local_byonk/` Samba share) at runtime, and embeds `default-config.yaml`
  at compile time — so **screen file edits are hot** but **Rust changes need an add-on rebuild**.
- **Integration** (`custom_components/byonk/`) deploys with `SMB_USER=byonk SMB_PASS=byonk make ha-deploy`,
  then reload it in the HA UI (or restart HA).
- **SSH** (one-time: install the Terminal & SSH add-on with `tools/ha-vm/ssh/id_ed25519.pub`):
  - `make ha-ssh` — shell in the VM; `make ha-ssh CMD="ha addons info local_byonk"` — one command.
  - `SMB_USER=byonk SMB_PASS=byonk make ha-rebuild` — sync server source + rebuild the add-on.
- **Samba shares** (creds `byonk`/`byonk`, port 4445): `addons` (add-on source),
  `addon_configs` (running add-on's config + `screens/`), `config` (HA config).

Never commit `tools/ha-vm/ssh/` (gitignored) or read the admin token — verify through the HA UI.

## Documentation

Documentation uses mdBook with mermaid diagrams. Build locally with:
```bash
cd docs && mdbook serve
```

Note: mermaid `architecture-beta` diagrams don't support hyphens in labels.
