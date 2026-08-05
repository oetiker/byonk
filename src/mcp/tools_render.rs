//! Render and validate. `render_screen` returns the PNG as an MCP image
//! block *and* the diagnostics an author needs — captured log output, the
//! data table the script returned, and a line-numbered error — so a failing
//! edit is debuggable in one round trip.

use base64::Engine as _;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};

use super::{blocking, ok_json, ByonkMcp};
use crate::services::screen_store::RenderOpts;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderArgs {
    /// Screen reference, `handle/path`.
    pub screen_ref: String,
    /// Device model selecting default size and palette. Defaults to `og`
    /// (800x480, 4-grey).
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// Panel profile name from the `panels:` config section.
    #[serde(default)]
    pub panel: Option<String>,
    /// Dither algorithm, e.g. `floyd-steinberg`, `atkinson`.
    #[serde(default)]
    pub dither: Option<String>,
    /// Also return the pre-dither, full-colour PNG for comparison.
    #[serde(default)]
    pub include_raw: bool,
    /// Unix timestamp to render at, for testing time-dependent screens.
    #[serde(default)]
    pub timestamp: Option<i64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderDiagnostics {
    /// Captured `log_info`/`log_warn`/`log_error` output from the script.
    pub log: Vec<String>,
    /// The table the Lua script returned.
    pub data: serde_json::Value,
    pub refresh_rate: u32,
    /// Present when the render failed. `line` points into script.lua when
    /// the failure was a Lua error.
    pub error: Option<RenderErrorOut>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderErrorOut {
    pub line: Option<u32>,
    pub message: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateArgs {
    pub screen_ref: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ValidateOutput {
    pub ok: bool,
    pub issues: Vec<IssueOut>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IssueOut {
    /// `error` or `warning`.
    pub severity: String,
    /// The screen-relative file the issue is in.
    pub location: String,
    pub message: String,
}

#[tool_router(router = tools_render_router, vis = "pub")]
impl ByonkMcp {
    #[tool(
        description = "Render a screen and return the dithered PNG plus diagnostics: the \
                          script's captured log output, the data table it returned, the \
                          refresh rate, and any error with its line number. Use this after \
                          every edit — it is the fastest way to see what a change did."
    )]
    pub async fn render_screen(
        &self,
        Parameters(a): Parameters<RenderArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let store = self.state.screen_store.clone();
        let opts = RenderOpts {
            model: a.model.unwrap_or_else(|| "og".to_string()),
            width: a.width,
            height: a.height,
            panel: a.panel,
            dither: a.dither,
            timestamp: a.timestamp,
            include_raw: a.include_raw,
            ..RenderOpts::default()
        };
        let screen_ref = a.screen_ref.clone();
        let result = blocking(move || store.render(&screen_ref, opts)).await?;

        let diagnostics = RenderDiagnostics {
            log: result.log,
            data: result.data,
            refresh_rate: result.refresh_rate,
            error: result.error.as_ref().map(|e| RenderErrorOut {
                line: e.line,
                message: e.message.clone(),
            }),
        };
        let failed = diagnostics.error.is_some();

        let mut content: Vec<ContentBlock> = Vec::new();
        let b64 = base64::engine::general_purpose::STANDARD;
        // A failed render has an empty `png` by contract — emit only the
        // diagnostics so the agent isn't handed a zero-byte image.
        if !result.png.is_empty() {
            content.push(ContentBlock::image(b64.encode(&result.png), "image/png"));
        }
        if let Some(raw) = &result.raw_png {
            content.push(ContentBlock::image(b64.encode(raw), "image/png"));
        }
        content.push(ContentBlock::text(
            serde_json::to_string_pretty(&diagnostics)
                .unwrap_or_else(|e| format!("failed to serialize diagnostics: {e}")),
        ));

        let structured = serde_json::to_value(&diagnostics)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        // `CallToolResult` is `#[non_exhaustive]`, so it has no struct-literal
        // or structured-content-builder constructor — build via `success()`
        // and assign the extra fields. A failed render must still be flagged
        // `is_error` while carrying its diagnostics as visible content.
        let mut result = CallToolResult::success(content);
        result.structured_content = Some(structured);
        result.is_error = Some(failed);
        Ok(result)
    }

    #[tool(
        description = "Statically check a screen without running it: meta.yaml against its \
                          schema, script.lua compiled (not executed), and screen.svg parsed \
                          with its {% extends %} chain resolved. A missing {% include %} \
                          target is NOT caught here — Tera resolves includes only while \
                          rendering, so use render_screen to catch a dangling include."
    )]
    pub async fn validate_screen(
        &self,
        Parameters(a): Parameters<ValidateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use crate::services::screen_store::Severity;
        let store = self.state.screen_store.clone();
        let report = blocking(move || store.validate(&a.screen_ref)).await?;
        // `validate` reports findings rather than failing, so a screen with
        // issues is a *successful* call whose payload says `ok: false`. Do
        // not flag it `is_error` — the agent is meant to read the issues.
        ok_json(ValidateOutput {
            ok: report.ok,
            issues: report
                .issues
                .into_iter()
                .map(|i| IssueOut {
                    severity: match i.severity {
                        Severity::Error => "error".to_string(),
                        Severity::Warning => "warning".to_string(),
                    },
                    location: i.location,
                    message: i.message,
                })
                .collect(),
        })
    }
}
