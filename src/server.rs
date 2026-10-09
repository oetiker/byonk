//! HTTP server setup and configuration.
//!
//! This module provides the router and application state used by both
//! the production server and integration tests.

use arc_swap::ArcSwap;
use axum::{
    http::{header::CONNECTION, Request},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower_http::{
    set_header::SetResponseHeaderLayer,
    trace::{MakeSpan, TraceLayer},
};
use tracing::{Level, Span};

use crate::api;
use crate::assets::AssetLoader;
use crate::error::ApiError;
use crate::models::{config::ScreenRepoRef, AppConfig, CascadeValues};
use crate::services::screen_repo_cache::ScreenRepoCache;
use crate::services::screen_repo_manager::ScreenRepoManager;
use crate::services::{
    ContentCache, ContentPipeline, InMemoryRegistry, PreviewCache, RenderService, ScreenStore,
};

/// Refresh rate answered to a device with a live tuning session, in seconds.
///
/// Sixty, not less: a six-colour panel needs most of a minute to repaint, and
/// the person calibrating it needs time to look at the result before it goes
/// again.
pub const TUNING_REFRESH_SECS: u32 = 60;

/// How long a tuning session survives without being touched.
///
/// The backstop for a closed lid or a crashed tab. A battery-powered device
/// must not be left repainting every minute until byonk restarts.
pub const TUNING_SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// How long a dev override survives without being re-PUT. The same backstop
/// as a tuning session: a closed tab must not leave a physical panel showing
/// an uncommitted draft until byonk restarts (cascade spec section 4.7).
pub const OVERRIDE_TTL: Duration = TUNING_SESSION_TTL;

/// Time left on something last touched at `touched`, or `None` once it has
/// expired. The ONE TTL rule: `TimedMap`, `tuning_session_active` and
/// `live_tuning_sessions` all decide expiry through it (ruling PF8).
pub fn ttl_left(touched: Instant) -> Option<Duration> {
    TUNING_SESSION_TTL
        .checked_sub(touched.elapsed())
        .filter(|left| !left.is_zero())
}

/// Is a tuning session live for `device_key`? Drops the entry if it has
/// expired.
pub async fn tuning_session_active(overrides: &DevOverrides, device_key: &str) -> bool {
    let mut sessions = overrides.tuning_sessions.write().await;
    match sessions.get(device_key).map(|touched| ttl_left(*touched)) {
        Some(Some(_)) => true,
        Some(None) => {
            sessions.remove(device_key);
            false
        }
        None => false,
    }
}

/// Every live tuning session with its remaining lifetime, sorted by key;
/// expired ones are dropped. Same rule as [`tuning_session_active`]
/// (ruling PF8): the listing never re-implements expiry.
pub async fn live_tuning_sessions(overrides: &DevOverrides) -> Vec<(String, Duration)> {
    let mut sessions = overrides.tuning_sessions.write().await;
    sessions.retain(|_, touched| ttl_left(*touched).is_some());
    let mut out: Vec<_> = sessions
        .iter()
        .filter_map(|(k, touched)| ttl_left(*touched).map(|left| (k.clone(), left)))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A string-keyed map whose entries expire [`OVERRIDE_TTL`] after their last
/// insert. Expiry is lazy: every read drops what it finds stale, so there
/// is no background task to schedule, leak or shut down.
#[derive(Clone)]
pub struct TimedMap<V> {
    inner: Arc<RwLock<HashMap<String, (V, Instant)>>>,
}

impl<V> Default for TimedMap<V> {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl<V: Clone> TimedMap<V> {
    /// Insert or replace, and (re)start the clock.
    pub async fn insert(&self, key: String, value: V) {
        self.inner
            .write()
            .await
            .insert(key, (value, Instant::now()));
    }

    #[cfg(test)]
    async fn insert_at(&self, key: String, value: V, touched: Instant) {
        self.inner.write().await.insert(key, (value, touched));
    }

    pub async fn remove(&self, key: &str) -> bool {
        self.inner.write().await.remove(key).is_some()
    }

    pub async fn get(&self, key: &str) -> Option<V> {
        let mut map = self.inner.write().await;
        match map.get(key).map(|(v, t)| (v.clone(), ttl_left(*t))) {
            Some((v, Some(_))) => Some(v),
            Some((_, None)) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }

    /// Every live entry with its remaining lifetime, sorted by key.
    pub async fn live(&self) -> Vec<(String, V, Duration)> {
        let mut map = self.inner.write().await;
        map.retain(|_, (_, t)| ttl_left(*t).is_some());
        let mut out: Vec<_> = map
            .iter()
            .filter_map(|(k, (v, t))| ttl_left(*t).map(|left| (k.clone(), v.clone(), left)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// What a configured device last told byonk about its hardware. Kept so the
/// dev UI can be told which devices resolve to a panel through `match:` --
/// the headers are only known at poll time (controller ruling R-F). In
/// memory only: a restart forgets it, and the UI says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub board: Option<String>,
    pub model: String,
}

/// Runtime overrides set by the dev UI and consumed by production handlers.
///
/// `panels` is keyed by **panel config key**: a calibration belongs to the
/// panel profile, so every device sharing it picks it up. `devices` is keyed
/// by **device config key**. Both feed the cascade's `override` rung, the
/// device's values over the panel's (plan ruling P1-3).
#[derive(Clone, Default)]
pub struct DevOverrides {
    /// device_config_key -> the Board/Model headers of its last poll. Not an
    /// override: dev-UI bookkeeping `/api/display` feeds.
    pub sightings: Arc<RwLock<HashMap<String, Sighting>>>,
    pub panels: TimedMap<CascadeValues>,
    pub devices: TimedMap<CascadeValues>,
    /// device_config_key -> when the dev UI last touched this tuning session.
    ///
    /// Present and fresh means `/api/display` answers `TUNING_REFRESH_SECS` for
    /// that device. Expiry is lazy (see `tuning_session_active`).
    pub tuning_sessions: Arc<RwLock<HashMap<String, Instant>>>,
}

impl DevOverrides {
    /// The override rung for one device on one panel: the device's live
    /// override field by field over its panel's. Never carries `colors` or
    /// `refresh`; the PUT routes refuse both, and this guarantees it.
    pub async fn live_override(
        &self,
        device_key: Option<&str>,
        panel_key: Option<&str>,
    ) -> CascadeValues {
        let device = match device_key {
            Some(k) => self.devices.get(k).await,
            None => None,
        }
        .unwrap_or_default();
        let panel = match panel_key {
            Some(k) => self.panels.get(k).await,
            None => None,
        }
        .unwrap_or_default();
        let mut merged = device.or(&panel);
        merged.colors = None;
        merged.refresh = None;
        merged
    }
}

/// Custom span maker that adds a unique request ID to each request's tracing span.
#[derive(Clone, Copy)]
struct RequestIdSpan;

impl<B> MakeSpan<B> for RequestIdSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        // Generate a short unique request ID (first 8 chars of a random hex string)
        let request_id = format!("{:08x}", rand::random::<u32>());

        tracing::span!(
            Level::INFO,
            "request",
            request_id = %request_id,
            method = %request.method(),
            uri = %request.uri(),
        )
    }
}

/// Hot-swappable application config shared by the server and the content pipeline.
pub type SharedConfig = std::sync::Arc<ArcSwap<AppConfig>>;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: SharedConfig,
    pub asset_loader: Arc<AssetLoader>,
    pub admin_token: Option<String>,
    /// Dev UI auth (credentials + session key). See `api::dev_auth`.
    pub dev_auth: Arc<crate::api::dev_auth::DevAuth>,
    /// Serializes admin config writes so concurrent requests can't interleave file patches.
    pub write_lock: Arc<tokio::sync::Mutex<()>>,
    pub registry: Arc<InMemoryRegistry>,
    pub renderer: Arc<RenderService>,
    pub content_pipeline: Arc<ContentPipeline>,
    pub content_cache: Arc<ContentCache>,
    pub dev_overrides: DevOverrides,
    pub screen_repo_manager: Arc<ScreenRepoManager>,
    /// True when byonk started as an HA Supervisor add-on (i.e. `/data/options.json`
    /// was present). In add-on mode, global-config admin writes are read-only.
    pub addon_mode: bool,
    /// Screen-authoring core (read/write/validate/render a screen's source
    /// files). Not yet consumed by any route — the MCP plan (spec 1
    /// Component 5) and the web-UI plan (spec 2) are its consumers.
    pub screen_store: Arc<ScreenStore>,
    /// Panel-recovery sessions in progress, keyed by device. In memory only:
    /// a restart cancels every run, which fails safe.
    pub recovery: Arc<crate::services::RecoveryRegistry>,
    /// Rendered device previews for `/api/admin/devices/{key}/preview`.
    /// Keeps a Home Assistant camera's frame pulls from re-rendering.
    pub preview_cache: Arc<PreviewCache>,
    /// Live-reload source for `/dev/api/events`. Inactive (never fires) when
    /// there is no external `SCREENS_DIR`.
    pub file_watcher: Arc<crate::services::FileWatcher>,
}

/// Create application state from an asset loader.
pub fn create_app_state(asset_loader: Arc<AssetLoader>) -> anyhow::Result<AppState> {
    let config = AppConfig::load_from_assets(&asset_loader)?;
    create_app_state_with_config(asset_loader, config)
}

/// Create application state with a custom config.
pub fn create_app_state_with_config(
    asset_loader: Arc<AssetLoader>,
    config: AppConfig,
) -> anyhow::Result<AppState> {
    create_app_state_with_overrides(asset_loader, Arc::new(config), DevOverrides::default())
}

/// Build the set of on-disk screen repos from `SCREEN_REPOS_DIR`.
///
/// Only handles that appear in `registry` (i.e. `config.screen_repos`) AND whose
/// directory `dir/<handle>` exists on disk are included. Non-builtin screen repos
/// are placed manually under `SCREEN_REPOS_DIR` in this phase; git fetching is a
/// later plan. `byonk-builtin` needs no disk entry — it registers embedded
/// inside `ScreenRepoLoader`.
fn collect_disk_packages(
    dir: &std::path::Path,
    registry: &HashMap<String, ScreenRepoRef>,
) -> HashMap<String, std::path::PathBuf> {
    registry
        .keys()
        .filter_map(|handle| {
            let path = dir.join(handle);
            if path.is_dir() {
                Some((handle.clone(), path))
            } else {
                None
            }
        })
        .collect()
}

/// Build the [`ScreenRepoManager`] from the embedded builtin screen repo, any
/// on-disk screen repos under `SCREEN_REPOS_DIR` declared in `config.screen_repos`, and
/// any previously fetched checkouts under the cache root (`SCREEN_REPOS_CACHE_DIR`
/// env, falling back to a temp dir). Rebuilds the loader once before
/// returning so the manager is immediately usable.
pub fn build_screen_repo_manager(
    asset_loader: Arc<AssetLoader>,
    config: SharedConfig,
) -> Arc<ScreenRepoManager> {
    let cache_root = std::env::var("SCREEN_REPOS_CACHE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("byonk-packages"));

    let extra_disk = std::env::var("SCREEN_REPOS_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|dir| collect_disk_packages(std::path::Path::new(&dir), &config.load().screen_repos))
        .unwrap_or_default();

    // `SCREENS_DIR` auto-registers as the writable `local` screen repo handle,
    // and the seeded examples dir (`<SCREENS_DIR>/../examples`) auto-registers
    // as the writable `examples` handle — unless `config.screen_repos` already
    // has an explicit entry for that handle — see
    // `ScreenRepoManager::build_disk_sources`.
    let screens_dir = asset_loader.screens_dir().map(|p| p.to_path_buf());
    let examples_dir = asset_loader.examples_dir().map(|p| p.to_path_buf());

    let manager = ScreenRepoManager::new(
        asset_loader,
        config,
        ScreenRepoCache::new(cache_root),
        extra_disk,
        screens_dir,
        examples_dir,
    );
    manager.rebuild_loader();
    manager
}

/// Create application state with shared overrides (for dev mode).
pub fn create_app_state_with_overrides(
    asset_loader: Arc<AssetLoader>,
    config: Arc<AppConfig>,
    dev_overrides: DevOverrides,
) -> anyhow::Result<AppState> {
    let admin_token = std::env::var("BYONK_ADMIN_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config.admin.token.clone());

    let dev_auth = Arc::new(crate::api::dev_auth::DevAuth::resolve(
        config.admin.dev_username.as_deref(),
        config.admin.dev_password.as_deref(),
    ));

    let shared_config: SharedConfig = Arc::new(ArcSwap::from(config.clone()));

    let screen_repo_manager =
        build_screen_repo_manager(asset_loader.clone(), shared_config.clone());

    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(
            shared_config.clone(),
            asset_loader.clone(),
            renderer.clone(),
            screen_repo_manager.clone(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to create content pipeline: {e}"))?,
    );
    let content_cache = Arc::new(ContentCache::new());

    // `screen_repo_manager` is the SAME `Arc` handed to `content_pipeline`
    // above — `ScreenStore::render` resolves through the pipeline's
    // manager, everything else through this one directly; see
    // `ScreenStore::new`'s doc comment for why they must match.
    let screen_store = Arc::new(ScreenStore::new(
        screen_repo_manager.clone(),
        content_pipeline.clone(),
    ));

    let file_watcher = Arc::new(crate::services::FileWatcher::new(
        asset_loader.screens_dir().map(|p| p.to_path_buf()),
    ));

    Ok(AppState {
        config: shared_config,
        asset_loader,
        admin_token,
        dev_auth,
        write_lock: Arc::new(tokio::sync::Mutex::new(())),
        registry,
        renderer,
        content_pipeline,
        content_cache,
        dev_overrides,
        screen_repo_manager,
        addon_mode: false,
        screen_store,
        preview_cache: Arc::new(PreviewCache::new()),
        recovery: Arc::new(crate::services::RecoveryRegistry::new()),
        file_watcher,
    })
}

/// Load `config.yaml` and fold the Home Assistant add-on options over it.
///
/// Startup and reload must produce the same config from the same inputs, so
/// both go through here. `read_options` returns `Missing` when there is no
/// options file — the normal non-add-on case — and `apply_to_config` is then
/// a no-op, so this is safe to call unconditionally.
pub fn load_config_with_addon_overlay(loader: &AssetLoader) -> anyhow::Result<AppConfig> {
    let mut config = AppConfig::load_from_assets(loader)?;
    let opts = crate::addon_options::read_options(&crate::addon_options::options_path());
    crate::addon_options::apply_to_config(&opts, &mut config);
    Ok(config)
}

/// Reparse the config file via the asset loader, re-fold the add-on options
/// overlay, and atomically swap it in.
pub fn reload_config(state: &AppState) -> anyhow::Result<()> {
    let fresh = load_config_with_addon_overlay(&state.asset_loader)?;
    state.config.store(Arc::new(fresh));
    state.screen_repo_manager.rebuild_loader();
    // A device preview's fingerprint only covers that device's own config, so
    // a change to something it merely references — a panel profile's colours
    // or dither tuning — would otherwise keep serving a stale render until the
    // TTL elapsed. See `PreviewCache::clear`.
    state.preview_cache.clear();
    Ok(())
}

/// Build the API router with all endpoints and middleware.
///
/// This is the core router used by both production and tests.
/// It includes the `Connection: close` header to prevent connection
/// accumulation from ESP32 clients.
pub fn build_router(state: AppState) -> Router {
    let router = Router::new()
        // TRMNL API endpoints (with and without trailing slashes for firmware compatibility)
        // TRMNL firmware 1.6.9+ sends requests with trailing slashes
        .route("/api/setup", get(handle_setup))
        .route("/api/setup/", get(handle_setup))
        .route("/api/display", get(handle_display))
        .route("/api/display/", get(handle_display))
        .route("/api/image/{hash}", get(handle_image))
        .route("/api/log", post(api::handle_log))
        .route("/api/log/", post(api::handle_log))
        .route("/api/time", get(api::handle_time))
        .route("/api/time/", get(api::handle_time))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Admin API
        .nest("/api/admin", crate::api::admin::admin_router());

    // MCP endpoint — gated by the same admin token as /api/admin/*.
    let router = crate::mcp::mount(router, &state);

    // `/dev` — the dev UI, login-protected. axum's `Router::nest("/dev", …)`
    // routes `/dev` itself but never `/dev/`: the inner router's `/` route
    // maps only onto the bare nest prefix, and nothing gets registered at
    // the trailing-slash form (see `path_for_nested_route` in axum's
    // `path_router.rs`). So `/dev/` is registered explicitly here, alongside
    // the nest, wearing the exact same `require_dev` protection as the
    // nested page route. `/dev` (bare) still resolves via the nest and its
    // own `require_dev`-driven trailing-slash redirect to `./dev/`.
    let dev_slash_route = Router::new()
        .route("/dev/", get(crate::api::dev::handle_dev_page))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::dev_auth::require_dev,
        ));
    let router = router
        .nest("/dev", crate::api::dev::dev_router(&state))
        .merge(dev_slash_route);

    router
        // Add state and tracing with request IDs
        .with_state(state)
        .layer(TraceLayer::new_for_http().make_span_with(RequestIdSpan))
        // Disable keep-alive: ESP32 HTTPClient defaults to keep-alive but never
        // reuses connections, causing orphaned connections to accumulate.
        // See: https://github.com/usetrmnl/trmnl-firmware/pull/274
        .layer(SetResponseHeaderLayer::overriding(
            CONNECTION,
            axum::http::HeaderValue::from_static("close"),
        ))
}

/// Tag every request that arrives through this router with `ViaIngress`, so
/// `require_dev`'s `admit()` trusts the Home Assistant Supervisor's own peer
/// address instead of requiring a login.
///
/// This is the entire boundary between "logged in" and "trusted because the
/// Supervisor said so": `run_server` must call this only on the router bound
/// to the separate `BYONK_INGRESS_ADDR` listener, never on the router bound
/// to the public address. Pulled out of `main.rs` into a small, directly
/// testable `pub fn` — see `ingress_layer_is_present_only_on_the_ingress_router`
/// below — because `main.rs` (a binary crate) has no test harness, and this
/// line is the whole security boundary the ingress admission path rests on.
pub fn ingress_app(app: Router) -> Router {
    app.layer(axum::Extension(crate::api::dev_auth::ViaIngress))
}

// Wrapper handlers to extract state components for the underlying API handlers

async fn handle_setup(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    api::handle_setup(
        axum::extract::State(state.config.load_full()),
        axum::extract::State(state.registry),
        headers,
    )
    .await
}

async fn handle_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    api::handle_display(
        axum::extract::State(state.config.load_full()),
        axum::extract::State(state.registry),
        axum::extract::State(state.content_pipeline),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.dev_overrides),
        axum::extract::State(state.recovery),
        headers,
    )
    .await
}

async fn handle_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
) -> Result<axum::response::Response, ApiError> {
    api::handle_image(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.content_pipeline),
        path,
    )
    .await
}

#[cfg(test)]
mod reload_tests {
    use super::*;

    #[test]
    fn build_router_accepts_brace_path_syntax() {
        // axum 0.8 panics at router-build time on old `:param` syntax; this
        // exercises build_router (and the nested admin_router) to confirm
        // all routes use `{param}` brace syntax and construct cleanly.
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state(loader).unwrap();
        let _ = build_router(state);
    }

    /// `ingress_app` is the entire boundary between "logged in" and "trusted
    /// because the request arrived on the Supervisor-only listener": whoever
    /// calls it in `run_server` must apply it to the ingress-bound router
    /// only, never to the publicly-bound one. Drives a probe request through
    /// each router directly (no socket needed) and reads back whether
    /// `require_dev`'s `admit()` would see the `ViaIngress` marker.
    ///
    /// Non-vacuous in both directions: an `ingress_app` that forgot the
    /// `.layer(...)` call would fail the first assertion; a `run_server` that
    /// (mis)applied `ingress_app` to the public router as well as the
    /// ingress one would still pass this test (it only exercises
    /// `ingress_app` itself) but is exactly the refactor risk this test
    /// exists to make a future reviewer's job of comparing "which router got
    /// `ingress_app`" easy to check by reading `main.rs`.
    #[tokio::test]
    async fn ingress_layer_is_present_only_on_the_ingress_router() {
        use axum::{extract::Request, routing::get};
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        async fn probe(
            marker: Option<axum::Extension<crate::api::dev_auth::ViaIngress>>,
        ) -> &'static str {
            if marker.is_some() {
                "yes"
            } else {
                "no"
            }
        }

        let base = Router::new().route("/probe", get(probe));

        let via_ingress_response = ingress_app(base.clone())
            .oneshot(
                Request::get("/probe")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let via_ingress_body = via_ingress_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(
            &via_ingress_body[..],
            b"yes",
            "a request through ingress_app's router must carry ViaIngress"
        );

        let public_response = base
            .oneshot(
                Request::get("/probe")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let public_body = public_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(
            &public_body[..],
            b"no",
            "the same router, untouched by ingress_app, must never carry ViaIngress"
        );
    }

    /// Basic sanity check that `AppState.screen_store` is wired to a live
    /// writable `local` repo and can create + render a screen through it.
    /// The specific "does `screen_store` share `content_pipeline`'s
    /// `ScreenRepoManager` Arc" invariant has a dedicated regression test
    /// in `tests/screen_store_wiring_test.rs` (it needs a config-reload
    /// scenario to actually discriminate a wiring bug from correct
    /// behavior, which doesn't fit this unit-test module).
    #[tokio::test]
    async fn screen_store_can_create_and_render_a_local_screen() {
        use crate::services::screen_store::{RenderOpts, StarterKind};

        let dir = tempfile::tempdir().unwrap();
        let screens_dir = dir.path().join("screens");
        let loader = Arc::new(AssetLoader::new(Some(screens_dir), None, None));
        loader.seed_if_configured().expect("seed");

        let state = create_app_state(loader).unwrap();

        state
            .screen_store
            .create_screen("local", "wiretest", StarterKind::Minimal)
            .expect("create_screen via screen_store");

        let result = state
            .screen_store
            .render("local/wiretest", RenderOpts::default());
        assert!(
            result.error.is_none(),
            "render must see the screen create_screen just wrote: {:?}",
            result.error
        );
        assert!(!result.png.is_empty());
    }

    #[test]
    fn test_appstate_has_package_manager_resolving_builtin() {
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let cfg = AppConfig::default();
        let state = create_app_state_with_config(loader, cfg).unwrap();
        assert!(state
            .screen_repo_manager
            .loader()
            .resolve("byonk-builtin/default")
            .is_some());
    }

    /// End-to-end (Task 11): seeding an empty `SCREENS_DIR` through the real
    /// production wiring (`AssetLoader::new` -> `seed_if_configured` ->
    /// `create_app_state` -> `build_screen_repo_manager`) must (a) register
    /// the seeded `examples` directory so `examples/hello` actually resolves,
    /// and (b) never copy built-in screens into `SCREENS_DIR` itself.
    #[tokio::test]
    async fn seeded_examples_dir_resolves_end_to_end_and_screens_dir_gets_no_builtin_copies() {
        let dir = tempfile::tempdir().unwrap();
        let screens_dir = dir.path().join("screens");
        let loader = Arc::new(AssetLoader::new(Some(screens_dir.clone()), None, None));
        loader.seed_if_configured().expect("seed");

        let state = create_app_state(loader).unwrap();
        assert!(
            state
                .screen_repo_manager
                .loader()
                .resolve("examples/hello")
                .is_some(),
            "seeded examples dir must resolve examples/hello end to end"
        );

        assert!(screens_dir.join("byonk-screens.yaml").exists());
        assert!(!screens_dir.join("default").exists());
        assert!(!screens_dir.join("calibration").exists());
    }

    #[test]
    fn test_config_swap_is_visible() {
        use crate::models::config::{DeviceConfig, RESERVED_DEFAULT_KEY};
        let loader = Arc::new(AssetLoader::new(None, None, None));
        let state = create_app_state(loader).unwrap();
        // Embedded config ships the reserved DEFAULT device (HA owns all
        // other device registration).
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("byonk-builtin/default")
        );

        // Swap in a config whose DEFAULT device screen is a sentinel.
        let mut cfg = (**state.config.load()).clone();
        cfg.devices.insert(
            RESERVED_DEFAULT_KEY.to_string(),
            DeviceConfig {
                screen: "sentinel".to_string(),
                ..Default::default()
            },
        );
        state.config.store(Arc::new(cfg));
        assert_eq!(
            state.config.load().default_device_screen(),
            Some("sentinel")
        );
    }
}

#[cfg(test)]
mod tuning_session_tests {
    use super::*;

    #[tokio::test]
    async fn a_stale_tuning_session_expires_and_is_dropped() {
        let overrides = DevOverrides::default();
        let stale = Instant::now()
            .checked_sub(TUNING_SESSION_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        overrides
            .tuning_sessions
            .write()
            .await
            .insert("kitchen".to_string(), stale);

        assert!(!tuning_session_active(&overrides, "kitchen").await);
        assert!(
            overrides.tuning_sessions.read().await.is_empty(),
            "a stale entry must be removed, not merely ignored — otherwise the \
             map grows for the life of the process"
        );
    }

    #[tokio::test]
    async fn a_fresh_tuning_session_is_active_and_kept() {
        let overrides = DevOverrides::default();
        overrides
            .tuning_sessions
            .write()
            .await
            .insert("kitchen".to_string(), Instant::now());

        assert!(tuning_session_active(&overrides, "kitchen").await);
        assert!(!overrides.tuning_sessions.read().await.is_empty());
    }

    #[tokio::test]
    async fn the_session_listing_is_sorted_and_drops_expired_sessions() {
        let overrides = DevOverrides::default();
        let stale = Instant::now()
            .checked_sub(TUNING_SESSION_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        {
            let mut sessions = overrides.tuning_sessions.write().await;
            sessions.insert("porch".to_string(), Instant::now());
            sessions.insert("attic".to_string(), Instant::now());
            sessions.insert("old".to_string(), stale);
        }
        let live = live_tuning_sessions(&overrides).await;
        let keys: Vec<&str> = live.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["attic", "porch"]);
        assert!(live.iter().all(|(_, left)| *left <= TUNING_SESSION_TTL));
        assert!(
            !overrides.tuning_sessions.read().await.contains_key("old"),
            "the listing must drop what it finds stale"
        );
    }
}

#[cfg(test)]
mod timed_map_tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_entry_is_returned_with_its_remaining_time() {
        let m: TimedMap<u32> = TimedMap::default();
        m.insert("a".into(), 7).await;
        assert_eq!(m.get("a").await, Some(7));
        let live = m.live().await;
        assert_eq!(live.len(), 1);
        assert!(live[0].2 > OVERRIDE_TTL - Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_stale_entry_is_dropped_on_read_not_merely_hidden() {
        let m: TimedMap<u32> = TimedMap::default();
        let stale = Instant::now()
            .checked_sub(OVERRIDE_TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        m.insert_at("a".into(), 7, stale).await;
        assert_eq!(m.get("a").await, None);
        assert!(m.live().await.is_empty());
        assert!(
            m.inner.read().await.is_empty(),
            "an expired override must be removed, or the map grows for the life of the process"
        );
    }

    #[tokio::test]
    async fn a_re_put_touches_the_entry() {
        let m: TimedMap<u32> = TimedMap::default();
        let old = Instant::now()
            .checked_sub(OVERRIDE_TTL - Duration::from_secs(2))
            .unwrap_or_else(Instant::now);
        m.insert_at("a".into(), 1, old).await;
        m.insert("a".into(), 2).await;
        let live = m.live().await;
        assert_eq!(live[0].1, 2);
        assert!(live[0].2 > OVERRIDE_TTL - Duration::from_secs(5));
    }

    #[tokio::test]
    async fn the_device_override_wins_field_by_field_over_the_panel_one() {
        let o = DevOverrides::default();
        o.panels
            .insert(
                "p".into(),
                CascadeValues {
                    strength: Some(0.1),
                    max_error: Some(0.5),
                    ..Default::default()
                },
            )
            .await;
        o.devices
            .insert(
                "d".into(),
                CascadeValues {
                    strength: Some(0.9),
                    ..Default::default()
                },
            )
            .await;
        let v = o.live_override(Some("d"), Some("p")).await;
        assert_eq!(v.strength, Some(0.9));
        assert_eq!(v.max_error, Some(0.5));
        assert_eq!(o.live_override(None, Some("p")).await.strength, Some(0.1));
        assert_eq!(o.live_override(None, None).await, CascadeValues::default());
    }
}
