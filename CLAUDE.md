# Byonk - Claude Code Guidelines

## Project Overview

Byonk (Bring Your Own Ink) is a self-hosted content server for TRMNL e-ink devices. It uses Lua scripts for data fetching and SVG templates for rendering.

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

## Home Assistant Test Setup

A local QEMU HAOS VM runs byonk + the integration for end-to-end testing — see the
`ha-vm-testing` skill (`.claude/skills/ha-vm-testing/SKILL.md`) for the full workflow.

Never commit `tools/ha-vm/ssh/` (gitignored) or read the admin token — verify through the HA UI.

## Documentation

Note: mermaid `architecture-beta` diagrams don't support hyphens in labels.
