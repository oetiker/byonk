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
//!
//! These are text-matching tests against Makefile/Dockerfile/CI YAML, not a
//! real build -- they catch the wiring being *removed*, not every way it
//! could be subtly wrong (an empty webui/dist that still satisfies
//! `contains("webui/dist")`, for instance). Within that limit, each
//! assertion below is written to fail on the specific regression it names,
//! not merely on the wiring vanishing from the file entirely -- see the
//! per-job/per-target checks and their comments.

use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Split a GitHub Actions workflow file into `(job_name, block_text)` pairs,
/// one per top-level job under `jobs:`. Deliberately no YAML parser: job
/// keys are the only lines indented by exactly two spaces that appear after
/// the top-level `jobs:` line (restricting to after `jobs:` matters --
/// `on: push:` also has a two-space-indented, colon-terminated `push:` line
/// before it, which would otherwise be misread as a job).
fn workflow_job_blocks(yaml: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = yaml.lines().collect();
    let jobs_start = lines
        .iter()
        .position(|l| *l == "jobs:")
        .unwrap_or_else(|| panic!("no top-level `jobs:` key found in workflow"));

    fn is_job_header(line: &str) -> bool {
        line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':')
    }

    let mut blocks = Vec::new();
    let mut i = jobs_start + 1;
    while i < lines.len() {
        if is_job_header(lines[i]) {
            let name = lines[i].trim().trim_end_matches(':').to_string();
            let start = i;
            i += 1;
            while i < lines.len() && !is_job_header(lines[i]) {
                i += 1;
            }
            blocks.push((name, lines[start..i].join("\n")));
        } else {
            i += 1;
        }
    }
    blocks
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

        // Ordering, not just presence: GNU Make runs a rule's prerequisites
        // left to right in serial mode (no `-j`), and `lint` (cargo clippy)
        // / `test` (cargo test) both compile the crate -- which now fails
        // via build.rs when webui/dist doesn't exist yet. `webui` landing
        // after either one (as it briefly did, before being fixed in
        // 40e902a) breaks every one of these targets on a genuinely clean
        // checkout while still satisfying the plain `contains` check above
        // -- so check the actual position, not just membership.
        let prereqs: Vec<&str> = line.split_whitespace().skip(1).collect();
        let webui_pos = prereqs
            .iter()
            .position(|&p| p == "webui")
            .unwrap_or_else(|| {
                panic!(
                    "`{target}` line has \"webui\" as a substring but not as its own \
                     prerequisite token, got: {line}"
                )
            });
        for later in ["lint", "test"] {
            if let Some(later_pos) = prereqs.iter().position(|&p| p == later) {
                assert!(
                    webui_pos < later_pos,
                    "`{target}` must build webui BEFORE `{later}` -- `{later}` compiles \
                     the crate, and build.rs now fails the build when webui/dist is \
                     missing, got: {line}"
                );
            }
        }
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
fn ci_builds_the_ui_before_every_cargo_job() {
    let ci = read(".github/workflows/ci.yml");
    let blocks = workflow_job_blocks(&ci);
    assert!(
        !blocks.is_empty(),
        "found no job blocks in ci.yml -- job-header parsing is broken, so this test \
         would otherwise pass vacuously"
    );

    // A single file-wide `contains("make webui")` check (the previous form
    // of this test) is satisfied by ONE job running it, even if the other
    // two lost their step -- the docstring above says "Three build paths
    // therefore have to produce it", so check each job that actually needs
    // it, individually.
    //
    // A real cargo invocation, not a bare "cargo " substring: steps named
    // "Cache cargo registry" and paths like `~/.cargo/registry` also match
    // "cargo ", which would false-positive every job (including ones that
    // never compile anything) into "invokes cargo". Match actual
    // subcommands instead.
    const CARGO_INVOCATIONS: &[&str] = &[
        "cargo fmt",
        "cargo clippy",
        "cargo test",
        "cargo build",
        "cargo install",
    ];
    let mut cargo_jobs_checked = 0;
    for (name, block) in &blocks {
        let Some(cargo_pos) = CARGO_INVOCATIONS
            .iter()
            .filter_map(|inv| block.find(inv))
            .min()
        else {
            continue;
        };
        cargo_jobs_checked += 1;
        let webui_pos = block.find("make webui").unwrap_or_else(|| {
            panic!(
                "ci.yml job `{name}` runs cargo but has no `make webui` step -- the \
                 dev UI would not exist when this job's cargo invocation needs it"
            )
        });
        assert!(
            webui_pos < cargo_pos,
            "ci.yml job `{name}` runs `make webui` AFTER its first cargo invocation -- \
             it must come before, or that cargo invocation fails via build.rs"
        );
    }
    assert!(
        cargo_jobs_checked >= 3,
        "expected at least 3 cargo-invoking jobs in ci.yml (check, test, build), only \
         found {cargo_jobs_checked} -- either a job lost its cargo step, or job-block \
         parsing missed one"
    );
}

#[test]
fn release_publisher_builds_the_ui_before_every_cargo_build() {
    let rp = read(".github/workflows/release-publisher.yml");
    let blocks = workflow_job_blocks(&rp);
    assert!(
        !blocks.is_empty(),
        "found no job blocks in release-publisher.yml -- job-header parsing is broken, \
         so this test would otherwise pass vacuously"
    );

    // Per-job, like the ci.yml check above: a file-wide count comparison
    // (make-webui-count >= cargo-build-count) would already have caught the
    // two-sites-currently-two-occurrences regression this exists to guard,
    // but would NOT name which job broke, and would not catch a job that
    // runs `cargo build` twice while another job's single `make webui` gets
    // deleted (2 == 2 with the wrong pairing). A per-job split does not
    // have that blind spot, so it's used here in preference to the count.
    let mut cargo_build_jobs_checked = 0;
    for (name, block) in &blocks {
        if !block.contains("cargo build") {
            continue;
        }
        cargo_build_jobs_checked += 1;
        let webui_pos = block.find("make webui").unwrap_or_else(|| {
            panic!(
                "release-publisher.yml job `{name}` runs `cargo build` but has no \
                 `make webui` step -- the dev UI would not exist when this job's \
                 cargo build needs it"
            )
        });
        let cargo_pos = block.find("cargo build").unwrap();
        assert!(
            webui_pos < cargo_pos,
            "release-publisher.yml job `{name}` runs `make webui` AFTER `cargo build` \
             -- it must come before, or that build fails via build.rs"
        );
    }
    assert!(
        cargo_build_jobs_checked >= 2,
        "expected at least 2 cargo-build jobs in release-publisher.yml \
         (build-binaries, build-docs), only found {cargo_build_jobs_checked} -- either \
         a job lost its cargo build step, or job-block parsing missed one"
    );
}
