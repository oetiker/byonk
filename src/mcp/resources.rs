//! MCP resources: the authoring contracts, served from the server the agent
//! is editing. No local scaffolding, no filesystem access needed.

use rmcp::model::{Resource, ResourceContents};

use crate::assets::EmbeddedDocs;
use crate::server::AppState;
use crate::services::screen_repo_manager::EXAMPLES_HANDLE;

/// (uri, embedded path, name, description)
const DOCS: &[(&str, &str, &str, &str)] = &[
    (
        "byonk://reference/lua-api",
        "api/lua-api.md",
        "lua-api",
        "Every global and function byonk injects into a screen's script.lua, \
         and the contract for the table it returns.",
    ),
    (
        "byonk://reference/svg-templates",
        "tutorial/svg-templates.md",
        "svg-templates",
        "How screen.svg works: Tera syntax, the byonk-base-v1 layout library, \
         the blocks it exposes, and the extends/include conventions.",
    ),
    (
        "byonk://reference/authoring",
        "guide/authoring.md",
        "authoring",
        "How screens, screen repos and writability fit together on this server.",
    ),
];

const META_SCHEMA_URI: &str = "byonk://schema/meta.yaml";
/// Worked examples are addressed `byonk://examples/<screen path>` — one
/// resource per shipped example screen.
const EXAMPLES_PREFIX: &str = "byonk://examples/";

pub fn list(state: &AppState) -> Vec<Resource> {
    let mut out: Vec<Resource> = DOCS
        .iter()
        .map(|(uri, _, name, description)| {
            Resource::new(*uri, *name)
                .with_description(*description)
                .with_mime_type("text/markdown")
        })
        .collect();
    out.push(
        Resource::new(META_SCHEMA_URI, "meta-yaml-schema")
            .with_description(
                "JSON Schema for a screen's meta.yaml, generated from the type that \
                 parses it — including the params descriptor sub-language.",
            )
            .with_mime_type("application/json"),
    );

    // One resource per shipped example. These are real, working screens on
    // this very server, so an agent can read a complete meta+lua+svg triple
    // that is known to render here — far better grounding than prose.
    for screen in state.screen_store.list_screens() {
        if screen.handle != EXAMPLES_HANDLE {
            continue;
        }
        out.push(
            Resource::new(
                format!("{EXAMPLES_PREFIX}{}", screen.path),
                format!("example-{}", screen.path.replace('/', "-")),
            )
            .with_title(screen.title.clone())
            .with_description(format!(
                "Worked example — {}. Full source of meta.yaml, script.lua and screen.svg.",
                screen.description
            ))
            .with_mime_type("text/markdown"),
        );
    }
    out
}

pub fn read(uri: &str, state: &AppState) -> Option<Vec<ResourceContents>> {
    if uri == META_SCHEMA_URI {
        let schema = crate::models::screen_meta::meta_json_schema();
        return Some(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text: serde_json::to_string_pretty(&schema).ok()?,
            meta: None,
        }]);
    }

    if let Some(path) = uri.strip_prefix(EXAMPLES_PREFIX) {
        let screen_ref = format!("{EXAMPLES_HANDLE}/{path}");
        // Refuse anything that isn't actually a listed example, so this
        // cannot be turned into a general read primitive for other repos.
        // `screen_ref` is built from the raw suffix (including any `..`
        // segments) and checked for *exact* membership in `list_screens()` —
        // it is never joined onto a filesystem path itself, so there is no
        // separate traversal path to close: a request like
        // `byonk://examples/../byonk-builtin/default` produces
        // `screen_ref == "examples/../byonk-builtin/default"`, which cannot
        // equal any real `screen_ref` and is rejected right here.
        if !state
            .screen_store
            .list_screens()
            .iter()
            .any(|s| s.screen_ref == screen_ref)
        {
            return None;
        }
        let mut text = format!("# Example: {screen_ref}\n");
        for file in ["meta.yaml", "script.lua", "screen.svg"] {
            let body = state
                .screen_store
                .read_file(&screen_ref, file)
                .ok()
                .map(|c| String::from_utf8_lossy(&c.bytes).into_owned())
                .unwrap_or_else(|| "(unreadable)".to_string());
            let lang = match file {
                "meta.yaml" => "yaml",
                "script.lua" => "lua",
                _ => "xml",
            };
            text.push_str(&format!("\n## {file}\n\n```{lang}\n{body}\n```\n"));
        }
        return Some(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("text/markdown".to_string()),
            text,
            meta: None,
        }]);
    }

    let (_, path, _, _) = DOCS.iter().find(|(u, _, _, _)| *u == uri)?;
    let file = EmbeddedDocs::get(path)?;
    Some(vec![ResourceContents::TextResourceContents {
        uri: uri.to_string(),
        mime_type: Some("text/markdown".to_string()),
        text: String::from_utf8(file.data.to_vec()).ok()?,
        meta: None,
    }])
}
