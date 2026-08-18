//! Test application factory for integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

use byonk::assets::AssetLoader;
use byonk::models::AppConfig;
use byonk::server::{build_router, create_app_state, create_app_state_with_config, AppState};
use byonk::services::{ContentCache, DeviceRegistry, InMemoryRegistry};

/// Test application with router and direct access to services
pub struct TestApp {
    router: axum::Router,
    pub registry: Arc<InMemoryRegistry>,
    pub content_cache: Arc<ContentCache>,
}

impl TestApp {
    /// Create a new test application using embedded assets
    pub fn new() -> Self {
        // Create asset loader with embedded assets only (no external paths)
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));

        // Create application state using shared server module
        let state = create_app_state(asset_loader).expect("Failed to create app state");

        // Keep references for test assertions
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();

        // Build router using shared server module (same as production)
        let router = build_router(state);

        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Create a new test application with registration disabled
    pub fn new_without_registration() -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));

        // Load config and disable registration
        let mut config = AppConfig::load_from_assets(&asset_loader).expect("Failed to load config");
        config.registration.enabled = false;

        let state =
            create_app_state_with_config(asset_loader, config).expect("Failed to create app state");

        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);

        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Create a test app whose reserved DEFAULT device renders `screen`.
    /// Pass a static screen (e.g. `byonk-builtin/calibration/grey`) when a test
    /// needs deterministic, time-independent content — the embedded default
    /// screen renders the wall-clock time and is therefore not reproducible.
    pub fn new_with_default_screen(screen: &str) -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let mut config = AppConfig::load_from_assets(&asset_loader).expect("Failed to load config");
        config
            .devices
            .get_mut("DEFAULT")
            .expect("embedded config has a reserved DEFAULT device")
            .screen = screen.to_string();

        let state =
            create_app_state_with_config(asset_loader, config).expect("Failed to create app state");

        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);

        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Create a test app and return the state for custom router configuration
    pub fn create_state() -> AppState {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        create_app_state(asset_loader).expect("Failed to create app state")
    }

    /// Make a GET request to the given path
    pub async fn get(&self, path: &str) -> TestResponse {
        self.request(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    /// Make a GET request with custom headers
    pub async fn get_with_headers(&self, path: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = Request::get(path);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.request(builder.body(Body::empty()).unwrap()).await
    }

    /// Make a POST request with JSON body
    pub async fn post_json(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> TestResponse {
        let mut builder = Request::post(path).header("Content-Type", "application/json");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.request(builder.body(Body::from(body.to_string())).unwrap())
            .await
    }

    /// Send a request to the router
    async fn request(&self, request: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("Request failed");

        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("Failed to collect body")
            .to_bytes()
            .to_vec();

        TestResponse {
            status,
            headers,
            body,
        }
    }

    /// Admin app whose config is EMBEDDED only (writes will return 409),
    /// with the given admin token enabled.
    pub fn new_admin(token: &str) -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let mut config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        config.admin.token = Some(token.to_string());
        let state = create_app_state_with_config(asset_loader, config).expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Build a test app from an arbitrary config (embedded assets, in-memory).
    pub fn from_config(config: AppConfig) -> Self {
        let asset_loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state_with_config(asset_loader, config).expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Build a test app backed by an arbitrary config YAML string written to a file.
    /// Returns `(app, config_path)`. `dir` must outlive the app.
    pub fn new_with_config_yaml(yaml: &str, dir: &std::path::Path) -> (Self, std::path::PathBuf) {
        let config_path = dir.join("config.yaml");
        std::fs::write(&config_path, yaml).expect("write config file");
        let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path.clone())));
        let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        let state = create_app_state_with_config(asset_loader, config).expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        (
            Self {
                router,
                registry,
                content_cache,
            },
            config_path,
        )
    }

    /// Admin app backed by a real config FILE seeded from the embedded default
    /// (writes succeed). Returns (app, config_path). `dir` must outlive the app.
    pub fn new_admin_with_file(token: &str, dir: &std::path::Path) -> (Self, std::path::PathBuf) {
        let config_path = dir.join("config.yaml");
        let embedded = AssetLoader::new(None, None, None);
        let yaml = embedded.read_config_string().expect("read embedded config");
        std::fs::write(&config_path, format!("admin:\n  token: {token}\n{yaml}"))
            .expect("write config file");

        let asset_loader = Arc::new(AssetLoader::new(None, None, Some(config_path.clone())));
        let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        let state = create_app_state_with_config(asset_loader, config).expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        (
            Self {
                router,
                registry,
                content_cache,
            },
            config_path,
        )
    }

    /// Admin app with a custom screens directory containing a `broken.lua` with an
    /// invalid `@params` block, wired up via a fresh `config.yaml` on disk.
    /// Use this to exercise the warn-not-fatal `schema_error` path.
    ///
    /// Also seeds `dir/examples` (`<SCREENS_DIR>/../examples`, i.e. a sibling
    /// of `screens_dir`) with the real `gphoto` and `swiss-departure-board`
    /// example screens plus a manifest, copied from the embedded `examples`
    /// set — so it auto-registers as the writable `examples` screen repo
    /// handle (Task 11) and `examples/gphoto` / `examples/swiss-departure-board`
    /// resolve for real, params schema and all. `config.yaml` declares no
    /// `screen_repos.examples` entry, so nothing suppresses the auto-register.
    ///
    /// `dir` must outlive the app.
    pub fn new_admin_with_screens(token: &str, dir: &std::path::Path) -> Self {
        // Create screens/ subdirectory with broken.lua and broken.svg
        let screens_dir = dir.join("screens");
        std::fs::create_dir_all(&screens_dir).expect("create screens dir");

        std::fs::write(
            screens_dir.join("broken.lua"),
            "--[[ @params\nk:\n  type: banana\n]]\n",
        )
        .expect("write broken.lua");

        std::fs::write(
            screens_dir.join("broken.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        )
        .expect("write broken.svg");

        // A valid screen present on disk but NOT declared in config.yaml.
        // Exercises filesystem auto-discovery in the /api/admin/screens list.
        std::fs::write(
            screens_dir.join("extra.lua"),
            "--[[ @params\ncolor:\n  type: string\n  label: \"Color\"\n]]\nreturn { data = {} }\n",
        )
        .expect("write extra.lua");
        std::fs::write(
            screens_dir.join("extra.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        )
        .expect("write extra.svg");

        Self::seed_examples_fixture(dir);

        // Write a minimal config.yaml pointing at the broken screen
        let config_path = dir.join("config.yaml");
        let yaml = format!(
            "admin:\n  token: {token}\ndevices:\n  DEFAULT:\n    screen: broken\nscreens:\n  broken:\n    script: broken.lua\n    template: broken.svg\n"
        );
        std::fs::write(&config_path, yaml).expect("write config.yaml");

        let asset_loader = Arc::new(AssetLoader::new(Some(screens_dir), None, Some(config_path)));
        // Mirrors production wiring (`AssetLoader::new` -> `seed_if_configured`
        // -> `create_app_state`, see `main.rs`): without this, `screens_dir`
        // never gets a `byonk-screens.yaml` manifest, so the writable `local`
        // handle never registers in the loader and every `local/*` screen_ref
        // resolves as if the repo didn't exist. The `examples` dir this
        // fixture seeded above is already non-empty, so `seed_examples` is a
        // no-op here; `config.yaml` already exists, so `seed_config` is too.
        asset_loader
            .seed_if_configured()
            .expect("seed local manifest");
        let config = AppConfig::load_from_assets(&asset_loader).expect("load config");
        let state = create_app_state_with_config(asset_loader, config).expect("create state");
        let registry = state.registry.clone();
        let content_cache = state.content_cache.clone();
        let router = build_router(state);
        Self {
            router,
            registry,
            content_cache,
        }
    }

    /// Copy `gphoto` and `swiss-departure-board` from the embedded `examples`
    /// set (Task 10) into `dir/examples`, plus a `byonk-screens.yaml`
    /// manifest, so `dir/examples` auto-registers as the writable `examples`
    /// screen repo handle (Task 11) once `screens_dir` is set to a sibling of
    /// `dir`. Shared by `new_admin_with_screens` and any other fixture that
    /// needs a real, params-bearing `examples/*` ref to resolve.
    fn seed_examples_fixture(dir: &std::path::Path) {
        let examples_dir = dir.join("examples");
        let embedded = AssetLoader::new(None, None, None);
        for (screen, file) in [
            ("gphoto", "meta.yaml"),
            ("gphoto", "script.lua"),
            ("gphoto", "screen.svg"),
            ("swiss-departure-board", "meta.yaml"),
            ("swiss-departure-board", "script.lua"),
            ("swiss-departure-board", "screen.svg"),
        ] {
            let rel = format!("{screen}/{file}");
            let data = embedded
                .read_example(std::path::Path::new(&rel))
                .unwrap_or_else(|_| panic!("read embedded example {rel}"));
            let dest = examples_dir.join(&rel);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::write(&dest, &*data).unwrap();
        }
        std::fs::write(
            examples_dir.join("byonk-screens.yaml"),
            "name: examples\ndescription: Example screens shipped with byonk.\nauthor: Byonk\nlicense: MIT\n",
        )
        .expect("write examples manifest");
    }

    pub async fn patch_json(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> TestResponse {
        let mut builder = Request::patch(path).header("Content-Type", "application/json");
        for (n, v) in headers {
            builder = builder.header(*n, *v);
        }
        self.request(builder.body(Body::from(body.to_string())).unwrap())
            .await
    }

    pub async fn delete(&self, path: &str, headers: &[(&str, &str)]) -> TestResponse {
        let mut builder = Request::delete(path);
        for (n, v) in headers {
            builder = builder.header(*n, *v);
        }
        self.request(builder.body(Body::empty()).unwrap()).await
    }

    /// Register a device and return the api_key
    pub async fn register_device(&self, mac: &str) -> String {
        let headers = [("ID", mac), ("FW-Version", "1.0.0"), ("Model", "og")];
        let response = self.get_with_headers("/api/setup", &headers).await;
        assert_eq!(response.status, StatusCode::OK);

        let json: serde_json::Value = response.json();
        json["api_key"].as_str().unwrap().to_string()
    }
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Test response with convenience methods
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl TestResponse {
    /// Parse body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).expect("Failed to parse JSON response")
    }

    /// Get body as string
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Get raw body bytes
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Check if response is a PNG image
    pub fn is_png(&self) -> bool {
        self.body.len() >= 8 && &self.body[0..8] == b"\x89PNG\r\n\x1a\n"
    }
}
