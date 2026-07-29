//! MCP (Model Context Protocol) server, mounted at `/mcp`.
//!
//! Lets an LLM author screens against a byonk running anywhere on the LAN —
//! including inside Home Assistant — with no filesystem access. Every tool
//! delegates to `ScreenStore` or the existing admin paths; this module adds
//! no screen logic of its own.
//!
//! Auth is the same Bearer admin token that gates `/api/admin/*`: no token
//! configured ⇒ 404 (invisible), wrong token ⇒ 401.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    Router,
};
use rmcp::{
    handler::server::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
    tool_handler,
    transport::streamable_http_server::{
        session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServerHandler,
};

use crate::api::admin::require_admin;
use crate::error::ApiError;
use crate::server::AppState;

/// The MCP server handler. One instance per request in stateless mode; it
/// only holds `AppState`, which is cheap to clone (all `Arc`s).
#[derive(Clone)]
pub struct ByonkMcp {
    /// `pub`, not `pub(crate)`: no tool reads it until Task 6, and a
    /// crate-private never-read field trips `dead_code` under the
    /// project's `clippy -- -D warnings` gate. A handler struct's state
    /// is legitimately part of this module's public surface.
    pub state: AppState,
    tool_router: ToolRouter<Self>,
}

impl ByonkMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            // Combined from the per-module routers; each is added as its
            // task lands. See `#[tool_router(router = …, vis = "pub")]`.
            tool_router: ToolRouter::new(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ByonkMcp {
    fn get_info(&self) -> ServerInfo {
        // NOT `Implementation::from_build_env()` — that macro expands inside
        // rmcp, so it would report rmcp's crate name and version.
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        );
        // `Implementation` is `#[non_exhaustive]`, so it must be built via
        // its constructor + builder methods, not a struct literal.
        info.server_info = rmcp::model::Implementation::new("byonk", env!("CARGO_PKG_VERSION"))
            .with_title("Byonk screen authoring")
            .with_description(
                "Author, validate and render TRMNL e-ink screens on this byonk server.",
            );
        info.with_instructions(
            "Screens are directories addressed as `handle/path` (e.g. `local/clock`), \
             each containing meta.yaml, script.lua and screen.svg. Only repos reported \
             as writable by list_screens can be edited; fork a read-only screen with \
             copy_screen first. After editing, call render_screen to see the result and \
             read its log/data/error fields. Read the byonk://reference/* resources for \
             the Lua and SVG contracts.",
        )
    }
}

/// Mount `/mcp` on the main router, behind the admin-token gate.
pub fn mount(router: Router<AppState>, state: &AppState) -> Router<AppState> {
    let owned = state.clone();
    // `StreamableHttpServerConfig` is `#[non_exhaustive]`, so it must be
    // built via `Default::default()` + builder methods, not a struct
    // literal with `..Default::default()` (rustc E0639).
    let config = StreamableHttpServerConfig::default()
        // Stateless: byonk's tools are pure request/response, so there is
        // no session state worth keeping. `json_response` then returns
        // plain application/json instead of SSE framing.
        .with_stateful_mode(false)
        .with_json_response(true)
        // rmcp defaults to loopback-only Host validation (DNS-rebinding
        // defence). byonk's whole purpose here is being driven over the LAN
        // at an arbitrary hostname, and the Bearer token already defeats
        // rebinding — a rebound browser request carries no token and 401s.
        .disable_allowed_hosts();
    let service = StreamableHttpService::new(
        move || Ok(ByonkMcp::new(owned.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );

    let mcp = Router::new()
        .route_service("/", service)
        .layer(middleware::from_fn_with_state(state.clone(), gate));

    router.nest("/mcp", mcp)
}

/// Same semantics as `/api/admin/*`: 404 when admin is disabled, 401 on a
/// missing/wrong token.
async fn gate(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    require_admin(&state, request.headers())?;
    Ok(next.run(request).await)
}
