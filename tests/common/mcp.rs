//! Minimal MCP-over-HTTP client for tests: JSON-RPC POSTs against the
//! in-process router. byonk runs the transport in stateless + JSON-response
//! mode, so each POST is self-contained and the body is plain JSON — no
//! session header to thread, no SSE frames to unwrap.

use super::app::{TestApp, TestResponse};

pub struct McpTestClient<'a> {
    app: &'a TestApp,
    token: Option<&'a str>,
    next_id: std::cell::Cell<u64>,
}

impl<'a> McpTestClient<'a> {
    pub fn new(app: &'a TestApp, token: Option<&'a str>) -> Self {
        Self {
            app,
            token,
            next_id: std::cell::Cell::new(1),
        }
    }

    /// Raw JSON-RPC POST. rmcp requires BOTH mime types in `Accept` and
    /// `application/json` as `Content-Type`, in every mode.
    pub async fn raw(&self, method: &str, params: serde_json::Value) -> TestResponse {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut headers: Vec<(&str, &str)> = vec![
            ("Accept", "application/json, text/event-stream"),
            // rmcp's DNS-rebinding guard requires a Host header on every
            // request (even with `disable_allowed_hosts()`, which only
            // clears the allow-list check, not the presence check). Real
            // HTTP/1.1 clients always send one; the in-process `oneshot`
            // request built from a relative path in `TestApp` does not, so
            // the test client supplies it explicitly.
            ("Host", "localhost"),
        ];
        let bearer;
        if let Some(t) = self.token {
            bearer = format!("Bearer {t}");
            headers.push(("Authorization", &bearer));
        }
        self.app
            .post_json("/mcp", &headers, &body.to_string())
            .await
    }

    /// Perform the MCP handshake and return the server's `InitializeResult`.
    pub async fn initialize(&self) -> serde_json::Value {
        let resp = self
            .raw(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "byonk-test", "version": "0" }
                }),
            )
            .await;
        assert_eq!(
            resp.status,
            axum::http::StatusCode::OK,
            "initialize failed: {}",
            resp.text()
        );
        let v: serde_json::Value = resp.json();
        v["result"].clone()
    }

    pub async fn list_tools(&self) -> serde_json::Value {
        let resp = self.raw("tools/list", serde_json::json!({})).await;
        let v: serde_json::Value = resp.json();
        v["result"].clone()
    }

    /// Call a tool and return its `result`. Panics with the JSON-RPC error
    /// body when the call fails at the protocol level.
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> serde_json::Value {
        let resp = self
            .raw(
                "tools/call",
                serde_json::json!({ "name": name, "arguments": args }),
            )
            .await;
        let v: serde_json::Value = resp.json();
        assert!(
            v.get("error").is_none(),
            "tool {name} returned a protocol error: {v}"
        );
        v["result"].clone()
    }
}
