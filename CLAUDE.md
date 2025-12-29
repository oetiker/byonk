# Byonk - Claude Code Guidelines

## Project Overview

Byonk (Bring Your Own Ink) is a self-hosted content server for TRMNL e-ink devices. It uses Lua scripts for data fetching and SVG templates for rendering.

## Key Directories

- `src/` - Rust source code
- `screens/` - Lua scripts and SVG templates
- `fonts/` - Custom fonts
- `docs/` - mdBook documentation

## Workflow Reminders

- **Update documentation** when user-visible features change or new ones are added (docs/src/)
- **All changes must be documented in CHANGES.md** - new features, fixes, and changes go in the Unreleased section
- **Commit in sensible intervals** - don't batch unrelated changes
- **Run `cargo fmt`** before committing Rust code
- **Run `cargo check`** to verify code compiles

## Release Process

Releases are triggered via GitHub Actions workflow dispatch. The workflow:
1. Bumps version in Cargo.toml
2. Updates CHANGES.md (moves Unreleased to new version)
3. Builds binaries for all platforms
4. Builds Docker container
5. Creates GitHub release with artifacts

## Documentation

Documentation uses mdBook with mermaid diagrams. Build locally with:
```bash
cd docs && mdbook serve
```

Note: mermaid `architecture-beta` diagrams don't support hyphens in labels.
