//! The dev UI is a COMPILE-time dependency: `src/api/dev.rs` embeds
//! `webui/dist/*` with `include_str!`. Three build paths therefore have to
//! produce it, and the failure mode when one does not is a Rust compile
//! error a long way from its cause -- or, worse, an empty directory that
//! compiles clean and ships a blank editor.
//!
//! `make editor-check` used to guard the equivalent for the committed
//! CodeMirror bundle. It retires in Task 12 because nothing is committed any
//! more, so nothing can be stale -- but that only holds while every build
//! path genuinely runs `make webui`. This test is what keeps that true.

use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

#[test]
fn makefile_builds_the_ui_before_the_binary() {
    let mk = read("Makefile");
    assert!(
        mk.contains("\nwebui:"),
        "Makefile must define a `webui` target"
    );
    for target in ["debug:", "release:", "check:"] {
        let line = mk
            .lines()
            .find(|l| l.starts_with(target))
            .unwrap_or_else(|| panic!("Makefile has no `{target}` line"));
        assert!(
            line.contains("webui"),
            "`{target}` must depend on webui, got: {line}"
        );
    }
}

#[test]
fn dockerfile_builds_the_ui_and_copies_it_in() {
    let df = read("Dockerfile");
    assert!(
        df.contains("make webui") || df.contains("pnpm build"),
        "Dockerfile needs a Node stage that builds the UI"
    );
    assert!(
        df.contains("webui/dist"),
        "Dockerfile must copy webui/dist into the Rust build stage"
    );
}

#[test]
fn ci_builds_the_ui_before_cargo() {
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("make webui"),
        "CI must run `make webui` before cargo test"
    );
    let rp = read(".github/workflows/release-publisher.yml");
    assert!(
        rp.contains("make webui"),
        "the release workflow must run `make webui` before cargo build"
    );
}
