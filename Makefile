# Byonk Makefile
# Build software and documentation

export PATH := $(HOME)/.cargo/bin:$(PATH)

.PHONY: all build release debug run clean docs docs-dev check fmt lint test help

# Default target
all: release

# =============================================================================
# Software Build
# =============================================================================

# Build release binary (runs fmt and clippy first)
release: fmt lint
	cargo build --release

# Build debug binary (runs fmt and clippy first)
debug: fmt lint
	cargo build

# Alias for debug build
build: debug

# Run the server (debug mode)
run: fmt lint
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
	cargo clippy -- -D warnings

# Run tests
test:
	cargo test

# Coverage configuration for Homebrew Rust (set LLVM paths)
# For rustup users, these variables are not needed
LLVM_PREFIX ?= $(shell brew --prefix llvm 2>/dev/null || echo "")
ifneq ($(LLVM_PREFIX),)
  export LLVM_COV := $(LLVM_PREFIX)/bin/llvm-cov
  export LLVM_PROFDATA := $(LLVM_PREFIX)/bin/llvm-profdata
endif

# Run tests with coverage (requires cargo-llvm-cov)
# Install: cargo install cargo-llvm-cov
coverage:
	cargo llvm-cov --html --open

# Generate coverage report for CI (lcov format)
coverage-ci:
	cargo llvm-cov --lcov --output-path lcov.info

# Generate coverage report (text summary)
coverage-text:
	cargo llvm-cov --summary-only

# Clean build artifacts
clean:
	cargo clean
	rm -rf docs/book
	rm -f lcov.info

# =============================================================================
# Documentation (mdBook)
# =============================================================================

# Build documentation
docs:
	cd docs && mdbook build

# Start documentation dev server
docs-dev:
	cd docs && mdbook serve

# Generate sample screen images (auto-starts server if needed)
docs-samples: release
	./docs/generate-samples.sh

# =============================================================================
# Development Helpers
# =============================================================================

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
	@echo "  make build        Build debug binary (runs fmt + clippy)"
	@echo "  make release      Build release binary (runs fmt + clippy)"
	@echo "  make run          Run server in debug mode"
	@echo "  make run-release  Run server in release mode"
	@echo "  make watch        Run with auto-reload (needs cargo-watch)"
	@echo "  make fmt          Format code"
	@echo "  make lint         Run clippy"
	@echo "  make test         Run tests"
	@echo "  make check        Format, lint, and test"
	@echo "  make clean        Clean all build artifacts"
	@echo ""
	@echo "Coverage (requires cargo-llvm-cov):"
	@echo "  make coverage      Generate HTML coverage report and open in browser"
	@echo "  make coverage-ci   Generate lcov.info for CI integration"
	@echo "  make coverage-text Print coverage summary to terminal"
	@echo ""
	@echo "Documentation:"
	@echo "  make docs         Build documentation"
	@echo "  make docs-dev     Start docs dev server"
	@echo "  make docs-samples Generate sample images (auto-starts server)"
	@echo ""
	@echo "  make help         Show this help"
