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
    // One resource per H2 section, so a client can read one function's
    // contract instead of the whole document.
    for (uri, path, name, _) in DOCS {
        let Some(text) = doc_text(path) else { continue };
        for section in split_doc(&text).sections {
            out.push(
                Resource::new(
                    format!("{uri}/{}", section.slug),
                    format!("{name}/{}", section.slug),
                )
                .with_description(section.title.clone())
                .with_mime_type("text/markdown"),
            );
        }
    }
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

    // A doc index, or one of its sections.
    for (doc_uri, path, _, _) in DOCS {
        let text = if uri == *doc_uri {
            let doc = split_doc(&doc_text(path)?);
            render_index(doc_uri, &doc)
        } else if let Some(slug) = uri
            .strip_prefix(doc_uri)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            let doc = split_doc(&doc_text(path)?);
            doc.sections.into_iter().find(|s| s.slug == slug)?.body
        } else {
            continue;
        };
        return Some(vec![ResourceContents::TextResourceContents {
            uri: uri.to_string(),
            mime_type: Some("text/markdown".to_string()),
            text,
            meta: None,
        }]);
    }
    None
}

fn doc_text(path: &str) -> Option<String> {
    String::from_utf8(EmbeddedDocs::get(path)?.data.to_vec()).ok()
}

/// One `## ` section of a markdown doc.
struct Section {
    /// Heading text without the `## `.
    title: String,
    /// Lowercase, non-alphanumerics -> `-`, collapsed and trimmed.
    slug: String,
    /// The section verbatim, heading line included.
    body: String,
    /// The `### ` headings inside it.
    subheadings: Vec<String>,
}

struct SplitDoc {
    /// Everything before the first H2.
    preamble: String,
    sections: Vec<Section>,
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// Split at `## ` boundaries, ignoring lines inside fenced code blocks.
fn split_doc(text: &str) -> SplitDoc {
    let mut preamble = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|c| *c == '`' || *c == '~')
            .map(|c| (c, trimmed.chars().take_while(|x| *x == c).count()))
            .filter(|(_, n)| *n >= 3);
        if let Some((c, n)) = marker {
            match fence {
                None => fence = Some((c, n)),
                Some((fc, fnn)) if fc == c && n >= fnn && trimmed[n..].trim().is_empty() => {
                    fence = None
                }
                _ => {}
            }
        }
        let in_fence = fence.is_some() || marker.is_some();
        if !in_fence {
            if let Some(title) = line.strip_prefix("## ") {
                let title = title.trim().to_string();
                sections.push(Section {
                    slug: slugify(&title),
                    title,
                    body: String::new(),
                    subheadings: Vec::new(),
                });
            } else if let (Some(sub), Some(cur)) = (line.strip_prefix("### "), sections.last_mut())
            {
                cur.subheadings.push(sub.trim().to_string());
            }
        }
        match sections.last_mut() {
            Some(cur) => cur.body.push_str(line),
            None => preamble.push_str(line),
        }
    }
    SplitDoc { preamble, sections }
}

/// The text before the first H2, then every section with its URI and the
/// `### ` headings inside it.
fn render_index(doc_uri: &str, doc: &SplitDoc) -> String {
    let mut out = doc.preamble.trim_end().to_string();
    out.push_str("\n\n## Sections\n\nRead one section at a time, by its URI");
    match doc.sections.first() {
        Some(first) => out.push_str(&format!(", e.g. `{doc_uri}/{}`.\n", first.slug)),
        None => out.push_str(".\n"),
    }
    for s in &doc.sections {
        out.push_str(&format!("\n- **{}** — `{doc_uri}/{}`\n", s.title, s.slug));
        for sub in &s.subheadings {
            out.push_str(&format!("  - {sub}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_names_a_real_section_uri_not_a_placeholder() {
        let doc = split_doc("# Doc\n\nIntro.\n\n## First Part\n\nA.\n\n## Second\n\nB.\n");
        let index = render_index("byonk://reference/lua-api", &doc);
        assert!(!index.contains("<uri>"), "{index}");
        assert!(
            index.contains("e.g. `byonk://reference/lua-api/first-part`"),
            "{index}"
        );
    }

    #[test]
    fn slug_is_lowercase_dashed_and_trimmed() {
        assert_eq!(slugify("Script Return Value"), "script-return-value");
        assert_eq!(slugify("  HTTP & JSON (v2)! "), "http-json-v2");
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_not_a_split_point() {
        let doc = "intro\n\n## One\n\n```md\n## not a heading\n### nor this\n```\n\n### Real sub\n\n## Two\n\n~~~\n## also not\n~~~\n";
        let split = split_doc(doc);
        assert_eq!(split.preamble, "intro\n\n");
        let titles: Vec<_> = split.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["One", "Two"]);
        assert!(split.sections[0].body.contains("## not a heading"));
        assert_eq!(split.sections[0].subheadings, ["Real sub"]);
        assert!(split.sections[1].body.contains("## also not"));
    }

    #[test]
    fn sections_reassemble_into_the_original_doc() {
        for (_, path, _, _) in DOCS {
            let text = doc_text(path).unwrap();
            let split = split_doc(&text);
            let joined: String = split.preamble.clone()
                + &split
                    .sections
                    .iter()
                    .map(|s| s.body.as_str())
                    .collect::<String>();
            assert_eq!(joined, text, "{path}");
            let mut slugs: Vec<_> = split.sections.iter().map(|s| &s.slug).collect();
            slugs.sort();
            let n = slugs.len();
            slugs.dedup();
            assert_eq!(slugs.len(), n, "{path}: duplicate section slugs");
        }
    }
}
