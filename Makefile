# Byonk Makefile
# Build software and documentation

.PHONY: all build release debug run clean docs docs-dev docs-preview install-docs help

# Default target
all: release docs

# =============================================================================
# Software Build
# =============================================================================

# Build release binary
release:
	cargo build --release

# Build debug binary
debug:
	cargo build

# Run the server (debug mode)
run:
	cargo run

# Run the server (release mode)
run-release: release
	./target/release/byonk

# Run with auto-reload (requires cargo-watch)
watch:
	cargo watch -x run

# Format code
fmt:
	cargo fmt

# Run clippy linter
lint:
	cargo clippy

# Run tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean
	rm -rf docs/node_modules docs/doc_build docs/.rspress

# =============================================================================
# Documentation
# =============================================================================

# Install documentation dependencies
install-docs:
	cd docs && npm install

# Build documentation (requires install-docs first)
docs: install-docs
	cd docs && npm run build

# Start documentation dev server
docs-dev: install-docs
	cd docs && npm run dev

# Preview built documentation
docs-preview: docs
	cd docs && npm run preview

# Generate API docs from running server (requires Byonk running on :3000)
docs-api:
	cd docs && npm run generate-api

# Generate sample screen images (requires Byonk running on :3000)
docs-samples:
	cd docs/scripts && ./generate-samples.sh

# =============================================================================
# Development Helpers
# =============================================================================

# Start server and docs dev server (requires tmux or run in separate terminals)
dev:
	@echo "Starting Byonk server..."
	@echo "Run 'make run' in one terminal and 'make docs-dev' in another"

# Check everything before commit
check: fmt lint test
	@echo "All checks passed!"

# =============================================================================
# Help
# =============================================================================

help:
	@echo "Byonk Makefile"
	@echo ""
	@echo "Software:"
	@echo "  make release      Build release binary"
	@echo "  make debug        Build debug binary"
	@echo "  make run          Run server (debug)"
	@echo "  make run-release  Run server (release)"
	@echo "  make watch        Run with auto-reload (needs cargo-watch)"
	@echo "  make fmt          Format code"
	@echo "  make lint         Run clippy"
	@echo "  make test         Run tests"
	@echo "  make clean        Clean all build artifacts"
	@echo ""
	@echo "Documentation:"
	@echo "  make install-docs Install npm dependencies"
	@echo "  make docs         Build documentation"
	@echo "  make docs-dev     Start docs dev server"
	@echo "  make docs-preview Preview built docs"
	@echo "  make docs-api     Generate API docs (server must be running)"
	@echo "  make docs-samples Generate sample images (server must be running)"
	@echo ""
	@echo "Development:"
	@echo "  make all          Build release + docs"
	@echo "  make check        Format, lint, and test"
	@echo "  make help         Show this help"
