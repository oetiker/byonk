//! Fail the build, in words, when the dev UI has not been built.
//!
//! `src/api/dev.rs` embeds `webui/dist/{index.html,app.js,app.css}` with
//! `include_str!`. Without this check the failure is three `include_str!`
//! errors pointing at paths the reader has no reason to expect, buried under
//! whatever else fails to compile around them -- a poor way to learn that
//! `make webui` exists. So this checks each file individually and aborts the
//! build (`std::process::exit(1)`, not a `cargo:warning`, which does not
//! stop the build and gets lost in the noise) with a message naming the fix.
//!
//! This deliberately does NOT run pnpm. A Rust build that silently needed
//! Node would be a worse trade than a clear message.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=webui/dist");

    let missing: Vec<&str> = ["index.html", "app.js", "app.css"]
        .into_iter()
        .filter(|f| !Path::new(&format!("webui/dist/{f}")).exists())
        .collect();

    if !missing.is_empty() {
        eprintln!("error: the dev UI has not been built.");
        eprintln!();
        for f in &missing {
            eprintln!("  missing webui/dist/{f}");
        }
        eprintln!();
        eprintln!("src/api/dev.rs embeds these files at compile time with include_str!.");
        eprintln!("Run `make webui` (needs pnpm), then rebuild.");
        std::process::exit(1);
    }
}
