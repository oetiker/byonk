use axum::{
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod api;
mod assets;
mod error;
mod models;
mod rendering;
mod services;

use models::AppConfig;
use services::{ContentCache, ContentPipeline, InMemoryRegistry, RenderService};

#[derive(Parser)]
#[command(name = "byonk")]
#[command(about = "Bring Your Own Ink - content server for TRMNL e-ink devices")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP server (default)
    Serve,
    /// Render a screen directly to a PNG file
    Render {
        /// Device MAC address (determines which screen to render)
        #[arg(short, long)]
        mac: String,

        /// Output PNG file path
        #[arg(short, long)]
        output: PathBuf,

        /// Device type: "og" (800x480) or "x" (1872x1404)
        #[arg(short, long, default_value = "og")]
        device: String,

        /// Battery voltage (e.g., 4.12)
        #[arg(short, long)]
        battery: Option<f32>,

        /// WiFi signal strength in dBm (e.g., -67)
        #[arg(short, long)]
        rssi: Option<i32>,

        /// Firmware version string
        #[arg(short, long)]
        firmware: Option<String>,
    },
    /// Extract embedded assets to filesystem for customization
    Init {
        /// Extract screen files (Lua scripts and SVG templates)
        #[arg(long)]
        screens: bool,

        /// Extract font files
        #[arg(long)]
        fonts: bool,

        /// Extract config.yaml
        #[arg(long)]
        config: bool,

        /// Extract all assets
        #[arg(long)]
        all: bool,

        /// Overwrite existing files
        #[arg(long, short)]
        force: bool,

        /// List embedded assets without extracting
        #[arg(long)]
        list: bool,
    },
}

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Byonk API",
        description = "Bring Your Own Ink - content server for TRMNL e-ink devices",
        version = "0.1.0",
        license(name = "MIT")
    ),
    paths(
        api::handle_setup,
        api::handle_display,
        api::handle_image,
        api::handle_log,
    ),
    components(schemas(
        api::SetupResponse,
        api::DisplayJsonResponse,
        api::LogRequest,
        api::LogResponse,
    )),
    tags(
        (name = "Device", description = "Device registration and management"),
        (name = "Display", description = "Display content retrieval"),
        (name = "Logging", description = "Device log submission")
    )
)]
struct ApiDoc;

/// Application state shared across all handlers
#[derive(Clone)]
struct AppState {
    registry: Arc<InMemoryRegistry>,
    renderer: Arc<RenderService>,
    content_pipeline: Arc<ContentPipeline>,
    content_cache: Arc<ContentCache>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Render {
            mac,
            output,
            device,
            battery,
            rssi,
            firmware,
        }) => run_render_command(&mac, &output, &device, battery, rssi, firmware),
        Some(Commands::Init {
            screens,
            fonts,
            config,
            all,
            force,
            list,
        }) => run_init_command(screens, fonts, config, all, force, list),
        Some(Commands::Serve) => run_server().await,
        None => {
            run_status_command();
            Ok(())
        }
    }
}

/// Render a screen directly to a PNG file (no server needed)
fn run_render_command(
    mac: &str,
    output: &PathBuf,
    device_type: &str,
    battery: Option<f32>,
    rssi: Option<i32>,
    firmware: Option<String>,
) -> anyhow::Result<()> {
    use assets::AssetLoader;
    use models::DisplaySpec;
    use services::DeviceContext;

    // Minimal logging for CLI
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "byonk=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    // Create asset loader with optional external paths from env vars
    let screens_dir = std::env::var("SCREENS_DIR").ok().map(PathBuf::from);
    let fonts_dir = std::env::var("FONTS_DIR").ok().map(PathBuf::from);
    let config_file = std::env::var("CONFIG_FILE").ok().map(PathBuf::from);

    let asset_loader = Arc::new(AssetLoader::new(screens_dir, fonts_dir, config_file));

    // Seed if configured paths are empty
    if let Err(e) = asset_loader.seed_if_configured() {
        tracing::warn!(%e, "Failed to seed assets");
    }

    // Load config and initialize services
    let config = Arc::new(AppConfig::load_from_assets(&asset_loader));
    let renderer = Arc::new(RenderService::new(&asset_loader)?);
    let content_pipeline = Arc::new(
        ContentPipeline::new(config.clone(), asset_loader, renderer.clone())
            .expect("Failed to initialize content pipeline"),
    );

    // Parse device type
    let display_spec = match device_type {
        "x" => DisplaySpec::X,
        _ => DisplaySpec::OG,
    };

    // Create device context with all provided fields
    let device_context = DeviceContext {
        mac: mac.to_string(),
        battery_voltage: battery,
        rssi,
        model: Some(device_type.to_string()),
        firmware_version: firmware,
        width: Some(display_spec.width),
        height: Some(display_spec.height),
    };

    // Run the Lua script
    let script_result = content_pipeline
        .run_script_for_device(mac, Some(device_context.clone()))
        .map_err(|e| anyhow::anyhow!("Script error: {e}"))?;

    // Render SVG from script result
    let svg_content = content_pipeline
        .render_svg_from_script(&script_result, Some(&device_context))
        .map_err(|e| anyhow::anyhow!("Template error: {e}"))?;

    // Render to PNG
    let png_bytes = content_pipeline
        .render_png_from_svg(&svg_content, display_spec)
        .map_err(|e| anyhow::anyhow!("Render error: {e}"))?;

    // Write to file
    std::fs::write(output, &png_bytes)?;
    println!("Rendered {} ({} bytes)", output.display(), png_bytes.len());

    Ok(())
}

/// Extract embedded assets to filesystem
fn run_init_command(
    screens: bool,
    fonts: bool,
    config: bool,
    all: bool,
    force: bool,
    list: bool,
) -> anyhow::Result<()> {
    use assets::{AssetCategory, AssetLoader};

    if list {
        println!("Embedded assets:\n");
        println!("Screens:");
        for f in AssetLoader::list_embedded(AssetCategory::Screens) {
            println!("  {f}");
        }
        println!("\nFonts:");
        for f in AssetLoader::list_embedded(AssetCategory::Fonts) {
            println!("  {f}");
        }
        println!("\nConfig:");
        for f in AssetLoader::list_embedded(AssetCategory::Config) {
            println!("  {f}");
        }
        return Ok(());
    }

    // Determine which categories to extract
    let mut categories = Vec::new();
    if all || screens {
        categories.push(AssetCategory::Screens);
    }
    if all || fonts {
        categories.push(AssetCategory::Fonts);
    }
    if all || config {
        categories.push(AssetCategory::Config);
    }

    if categories.is_empty() {
        eprintln!("No categories specified. Use --all, --screens, --fonts, or --config");
        eprintln!("\nRun 'byonk init --list' to see embedded assets.");
        std::process::exit(1);
    }

    // Create asset loader with paths from env vars (or defaults)
    let screens_dir = std::env::var("SCREENS_DIR").ok().map(PathBuf::from);
    let fonts_dir = std::env::var("FONTS_DIR").ok().map(PathBuf::from);
    let config_file = std::env::var("CONFIG_FILE").ok().map(PathBuf::from);

    let loader = AssetLoader::new(screens_dir, fonts_dir, config_file);

    // Extract assets
    let report = loader.init(&categories, force)?;

    if !report.written.is_empty() {
        println!("Extracted {} files:", report.written.len());
        for f in &report.written {
            println!("  + {f}");
        }
    }
    if !report.skipped.is_empty() {
        println!(
            "\nSkipped {} existing files (use --force to overwrite):",
            report.skipped.len()
        );
        for f in &report.skipped {
            println!("  - {f}");
        }
    }

    if report.written.is_empty() && report.skipped.is_empty() {
        println!("No files to extract.");
    }

    Ok(())
}

/// Display status and configuration information
fn run_status_command() {
    use assets::{AssetCategory, AssetLoader};

    const VERSION: &str = env!("CARGO_PKG_VERSION");

    // Read environment variables
    let bind_addr = std::env::var("BIND_ADDR").ok();
    let config_file = std::env::var("CONFIG_FILE").ok();
    let screens_dir = std::env::var("SCREENS_DIR").ok();
    let fonts_dir = std::env::var("FONTS_DIR").ok();

    // Header
    println!("Byonk v{VERSION} - Bring Your Own Ink");
    println!("Content server for TRMNL e-ink devices\n");

    // Environment variables section
    println!("Environment Variables:");
    println!(
        "  BIND_ADDR   = {}",
        bind_addr.as_deref().unwrap_or("0.0.0.0:3000 (default)")
    );
    println!(
        "  CONFIG_FILE = {}",
        config_file.as_deref().unwrap_or("(not set)")
    );
    println!(
        "  SCREENS_DIR = {}",
        screens_dir.as_deref().unwrap_or("(not set)")
    );
    println!(
        "  FONTS_DIR   = {}",
        fonts_dir.as_deref().unwrap_or("(not set)")
    );

    // Asset sources section
    println!("\nAsset Sources:");

    // Create asset loader to check actual sources
    let loader = AssetLoader::new(
        screens_dir.clone().map(PathBuf::from),
        fonts_dir.clone().map(PathBuf::from),
        config_file.clone().map(PathBuf::from),
    );

    // Config source
    let config_source = if let Some(ref path) = config_file {
        let p = PathBuf::from(path);
        if p.exists() {
            path.to_string()
        } else {
            "embedded (file not found)".to_string()
        }
    } else {
        "embedded".to_string()
    };
    println!("  Config:  {config_source}");

    // Helper for pluralization
    fn plural(n: usize) -> &'static str {
        if n == 1 {
            "file"
        } else {
            "files"
        }
    }

    // Screens source
    let screens_list = loader.list_screens();
    let screens_count = screens_list.len();
    let embedded_screens = AssetLoader::list_embedded(AssetCategory::Screens);
    let embedded_count = embedded_screens.len();

    if let Some(ref path) = screens_dir {
        let p = PathBuf::from(path);
        if p.exists() {
            println!(
                "  Screens: {path} ({screens_count} {}, {embedded_count} embedded)",
                plural(screens_count)
            );
        } else {
            println!(
                "  Screens: embedded ({embedded_count} {})",
                plural(embedded_count)
            );
        }
    } else {
        println!(
            "  Screens: embedded ({embedded_count} {})",
            plural(embedded_count)
        );
    }

    // Fonts source
    let fonts = loader.get_fonts();
    let fonts_count = fonts.len();
    let embedded_fonts = AssetLoader::list_embedded(AssetCategory::Fonts);
    let embedded_fonts_count = embedded_fonts.len();

    if let Some(ref path) = fonts_dir {
        let p = PathBuf::from(path);
        if p.exists() {
            println!(
                "  Fonts:   {path} ({fonts_count} {}, {embedded_fonts_count} embedded)",
                plural(fonts_count)
            );
        } else {
            println!(
                "  Fonts:   embedded ({embedded_fonts_count} {})",
                plural(embedded_fonts_count)
            );
        }
    } else {
        println!(
            "  Fonts:   embedded ({embedded_fonts_count} {})",
            plural(embedded_fonts_count)
        );
    }

    // Commands section
    println!("\nCommands:");
    println!("  byonk serve    Start the HTTP server");
    println!("  byonk render   Render a screen to PNG file");
    println!("  byonk init     Extract embedded assets");
    println!("\nRun 'byonk --help' for more details.");
}

/// Run the HTTP server
async fn run_server() -> anyhow::Result<()> {
    use assets::AssetLoader;

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "byonk=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create asset loader with optional external paths from env vars
    let screens_dir = std::env::var("SCREENS_DIR").ok().map(PathBuf::from);
    let fonts_dir = std::env::var("FONTS_DIR").ok().map(PathBuf::from);
    let config_file = std::env::var("CONFIG_FILE").ok().map(PathBuf::from);
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());

    let asset_loader = Arc::new(AssetLoader::new(
        screens_dir.clone(),
        fonts_dir.clone(),
        config_file.clone(),
    ));

    // Log asset sources
    tracing::info!(
        screens = ?screens_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "embedded".to_string()),
        fonts = ?fonts_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "embedded".to_string()),
        config = ?config_file.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "embedded".to_string()),
        "Asset sources configured"
    );

    // Seed if configured paths are empty
    match asset_loader.seed_if_configured() {
        Ok(report) if !report.is_empty() => {
            tracing::info!(
                screens = report.screens_seeded.len(),
                fonts = report.fonts_seeded.len(),
                config = report.config_seeded,
                "Seeded empty directories with embedded assets"
            );
        }
        Err(e) => {
            tracing::warn!(%e, "Failed to seed assets");
        }
        _ => {}
    }

    // Load application config
    let config = Arc::new(AppConfig::load_from_assets(&asset_loader));

    // Initialize services
    let registry = Arc::new(InMemoryRegistry::new());
    let renderer = Arc::new(RenderService::new(&asset_loader)?);

    // Content pipeline for scripted content
    let content_pipeline = Arc::new(
        ContentPipeline::new(config.clone(), asset_loader, renderer.clone())
            .expect("Failed to initialize content pipeline"),
    );

    let content_cache = Arc::new(ContentCache::new());

    let state = AppState {
        registry: registry.clone(),
        renderer: renderer.clone(),
        content_pipeline: content_pipeline.clone(),
        content_cache: content_cache.clone(),
    };

    // Build router
    let app = Router::new()
        // TRMNL API endpoints
        .route("/api/setup", get(handle_setup))
        .route("/api/display", get(handle_display))
        .route("/api/image/:hash", get(handle_image))
        .route("/api/log", post(api::handle_log))
        // OpenAPI documentation
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Static file serving
        .nest_service("/static", ServeDir::new("./static"))
        // Health check
        .route("/health", get(|| async { "OK" }))
        // Add state and tracing
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Bind server
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Byonk server listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Wrapper for setup handler with correct state extraction
async fn handle_setup(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, error::ApiError> {
    api::handle_setup(axum::extract::State(state.registry), headers).await
}

/// Wrapper for display handler with correct state extraction
async fn handle_display(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, error::ApiError> {
    api::handle_display(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_pipeline),
        axum::extract::State(state.content_cache),
        headers,
    )
    .await
}

/// Wrapper for image handler with correct state extraction
async fn handle_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    path: axum::extract::Path<String>,
    query: axum::extract::Query<api::display::ImageQuery>,
) -> Result<axum::response::Response, error::ApiError> {
    api::handle_image(
        axum::extract::State(state.registry),
        axum::extract::State(state.renderer),
        axum::extract::State(state.content_cache),
        axum::extract::State(state.content_pipeline),
        path,
        query,
    )
    .await
}
