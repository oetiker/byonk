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

fn default_true() -> bool {
    true
}

/// Which image(s) `render_screen` should return.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ImageChoice {
    /// The dithered PNG, as the panel would show it.
    #[default]
    Dithered,
    /// The pre-dither, full-colour render.
    Raw,
    /// Dithered then raw, each preceded by a text block naming it.
    Both,
    /// No image at all — diagnostics only.
    None,
}

impl ImageChoice {
    fn wants_dithered(self) -> bool {
        matches!(self, Self::Dithered | Self::Both)
    }
    fn wants_raw(self) -> bool {
        matches!(self, Self::Raw | Self::Both)
    }
}

/// Downscale a PNG to at most `max_width`, preserving aspect ratio.
///
/// Returns the original bytes unchanged when it is already narrow enough, or
/// when decode/re-encode fails: a scaling problem must not turn a successful
/// render into a failed tool call, and handing back the full-size image is
/// always a correct (if larger) answer.
fn downscale_png(png: &[u8], max_width: u32) -> Vec<u8> {
    if max_width == 0 {
        return png.to_vec();
    }
    let Ok(img) = image::load_from_memory(png) else {
        return png.to_vec();
    };
    if img.width() <= max_width {
        return png.to_vec();
    }
    let height = ((img.height() as u64 * max_width as u64) / img.width().max(1) as u64).max(1);
    let scaled = img.resize(
        max_width,
        height as u32,
        image::imageops::FilterType::Triangle,
    );
    let mut out = std::io::Cursor::new(Vec::new());
    match scaled.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => png.to_vec(),
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderArgs {
    /// Screen reference, `handle/path`.
    pub screen_ref: String,
    /// Device model selecting default size and palette. Defaults to `og`
    /// (800x480, 4-grey).
    #[serde(default)]
    pub model: Option<String>,
    /// Override the device model's default render width, in pixels.
    #[serde(default)]
    pub width: Option<u32>,
    /// Override the device model's default render height, in pixels.
    #[serde(default)]
    pub height: Option<u32>,
    /// Panel profile name from the `panels:` config section.
    #[serde(default)]
    pub panel: Option<String>,
    /// Dither algorithm, e.g. `floyd-steinberg`, `atkinson`.
    #[serde(default)]
    pub dither: Option<String>,
    /// Which image(s) to return. `dithered` (default) is what the panel
    /// shows; `raw` is the pre-dither, full-colour render; `both` returns
    /// dithered first then raw, each preceded by a text block naming it;
    /// `none` returns diagnostics only. Use `none` when you only need the
    /// script's `log`/`data`/`error` — images are by far the largest part of
    /// this response. Note `raw` is full-colour and roughly ten times the
    /// size of the dithered image (for an 800x480 screen, ~650 KB against
    /// ~65 KB), so pair `raw`/`both` with `image_max_width` unless you need
    /// its exact pixels. Images are only produced on a *successful* render; a
    /// failure returns diagnostics with no image block regardless.
    #[serde(default)]
    pub image: ImageChoice,
    /// Downscale returned image(s) to at most this width in pixels,
    /// preserving aspect ratio. Never upscales. Use it to spend a fraction of
    /// the context on a layout check — a 800x480 PNG costs roughly six times
    /// what the same image at 200px wide does. Caveat: resampling destroys
    /// the dither pattern, so a scaled `dithered` image is good for judging
    /// layout and tone but useless for judging dithering itself; omit this to
    /// inspect the real pixels.
    #[serde(default)]
    pub image_max_width: Option<u32>,
    /// Return the table the script produced. Defaults to true. Set false when
    /// you only need to see the picture: a script that embeds an image with
    /// `image_process` puts a full base64 data URI in `data`, and the
    /// diagnostics are serialized twice (once as text, once as structured
    /// content), so that URI is carried twice on top of the PNG itself.
    #[serde(default = "default_true")]
    pub include_data: bool,
    /// Unix timestamp to render at, for testing time-dependent screens.
    #[serde(default)]
    pub timestamp: Option<i64>,
    /// Draw the returned PNG in the panel's measured colours — what the
    /// screen will actually look like — instead of the spec colours that are
    /// sent to the panel. Defaults to on whenever measured colours are
    /// available (from `panel` or from `colors_actual`). This changes only
    /// how the returned PNG is drawn; it never changes the dithering.
    #[serde(default)]
    pub use_actual: Option<bool>,
    /// Measured panel colours for this render, comma-separated hex (e.g.
    /// `#0A0A0A,#E8E6E0,#A83A30`), index-parallel to the palette. Use this to
    /// preview a calibration without adding a panel to the config. A
    /// `colors_actual` returned by the screen's own script wins over this,
    /// and a list whose length doesn't match the palette is ignored (with a
    /// warning in `log`) rather than failing the render.
    #[serde(default)]
    pub colors_actual: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RenderDiagnostics {
    /// Captured `log_info`/`log_warn`/`log_error` output from the script.
    pub log: Vec<String>,
    /// The table the Lua script returned. Absent when you passed
    /// `include_data: false` — omitted, not empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    pub refresh_rate: u32,
    /// Present when the render failed. `line` points into script.lua when
    /// the failure was a Lua error.
    pub error: Option<RenderErrorOut>,
    /// Which layer supplied the measured ("actual") panel colours this
    /// render dithered against: `script` (the screen's own
    /// `colors_actual`), `render_opts` (the `colors_actual` argument you
    /// passed), `panel.colors_actual` (the named panel profile), or `none`
    /// (no calibration applied — the render used the spec palette). Use it
    /// to confirm which calibration actually took effect; a candidate whose
    /// length doesn't match the palette is skipped, and `log` says so.
    pub measured_source: String,
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
                          every edit — it is the fastest way to see what a change did. \
                          Control what comes back, because this is the most expensive tool \
                          here: `image` picks dithered (default), raw, both or none, \
                          `image_max_width` downscales it, and `include_data: false` drops \
                          the script's data table — worth doing when a script embeds an \
                          image, since the resulting base64 URI is carried twice. A failed \
                          render includes no image block at all — read the error field \
                          instead. By default the returned PNG shows what the panel will \
                          actually look like when measured colours are available; pass \
                          use_actual=false to see the spec colours that are sent to the \
                          panel instead. The diagnostics' measured_source field names which \
                          layer supplied those measured colours."
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
            include_raw: a.image.wants_raw(),
            use_actual: a.use_actual,
            colors_actual: a.colors_actual,
            ..RenderOpts::default()
        };
        let screen_ref = a.screen_ref.clone();
        let result = blocking(move || store.render(&screen_ref, opts)).await?;

        let diagnostics = RenderDiagnostics {
            log: result.log,
            data: a.include_data.then_some(result.data),
            refresh_rate: result.refresh_rate,
            error: result.error.as_ref().map(|e| RenderErrorOut {
                line: e.line,
                message: e.message.clone(),
            }),
            measured_source: result.measured_source.to_string(),
        };
        let failed = diagnostics.error.is_some();

        let mut content: Vec<ContentBlock> = Vec::new();
        let b64 = base64::engine::general_purpose::STANDARD;
        // Label each image so `both` doesn't rely on block order to say which
        // is which — two anonymous PNGs are indistinguishable to a client that
        // reorders or drops one.
        let labelled = a.image == ImageChoice::Both;
        let push_image = |bytes: &[u8], label: &str, content: &mut Vec<ContentBlock>| {
            let bytes = match a.image_max_width {
                Some(w) => downscale_png(bytes, w),
                None => bytes.to_vec(),
            };
            if labelled {
                content.push(ContentBlock::text(label.to_string()));
            }
            content.push(ContentBlock::image(b64.encode(&bytes), "image/png"));
        };
        // A failed render has an empty `png` by contract — emit only the
        // diagnostics so the agent isn't handed a zero-byte image.
        if a.image.wants_dithered() && !result.png.is_empty() {
            push_image(
                &result.png,
                "dithered (as the panel shows it)",
                &mut content,
            );
        }
        if let Some(raw) = &result.raw_png {
            push_image(raw, "raw (pre-dither, full colour)", &mut content);
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
