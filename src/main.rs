use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use byonk::api;
use byonk::models::AppConfig;
use byonk::server;
use byonk::services::{ContentPipeline, RenderService};

#[derive(Parser)]
#[command(name = "byonk")]
#[command(about = "Bring Your Own Ink - content server for TRMNL e-ink devices")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP server
    Serve,
    /// Start the HTTP server with dev mode (live reload, device simulator)
    Dev,
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

        /// Registration code to display (for testing registration screen)
        #[arg(long)]
        registration_code: Option<String>,
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
            registration_code,
        }) => run_render_command(
            &mac,
            &output,
            &device,
            battery,
            rssi,
            firmware,
            registration_code,
        ),
        Some(Commands::Init {
            screens,
            fonts,
            config,
            all,
            force,
            list,
        }) => run_init_command(screens, fonts, config, all, force, list),
        Some(Commands::Serve) => run_server().await,
        Some(Commands::Dev) => run_dev_server().await,
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
    registration_code: Option<String>,
) -> anyhow::Result<()> {
    use byonk::assets::AssetLoader;
    use byonk::models::DisplaySpec;
    use byonk::services::DeviceContext;

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

    // Build default palette based on device type
    let palette: Vec<(u8, u8, u8)> = if device_type == "x" {
        (0..16)
            .map(|i| {
                let v = (i * 255 / 15) as u8;
                (v, v, v)
            })
            .collect()
    } else {
        vec![(0, 0, 0), (85, 85, 85), (170, 170, 170), (255, 255, 255)]
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
        registration_code,
        colors: Some(byonk::api::display::colors_to_hex_strings(&palette)),
        ..Default::default()
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
        .render_png_from_svg(&svg_content, display_spec, &palette)
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
    use byonk::assets::{AssetCategory, AssetLoader};

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
    use byonk::assets::{AssetCategory, AssetLoader};

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
    println!("  byonk dev      Start server with dev mode (live reload)");
    println!("  byonk render   Render a screen to PNG file");
    println!("  byonk init     Extract embedded assets");
    println!("\nRun 'byonk --help' for more details.");
}

/// Run the HTTP server
async fn run_server() -> anyhow::Result<()> {
    use byonk::assets::AssetLoader;

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

    // Create application state using shared server module
    let state = server::create_app_state(asset_loader)?;

    // Build router: start with shared API routes, add production-only routes
    let app = server::build_router(state)
        // OpenAPI documentation (production only)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Static file serving (production only)
        .nest_service("/static", ServeDir::new("./static"));

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Byonk server listening");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Run the HTTP server with dev mode (file watching, live reload)
async fn run_dev_server() -> anyhow::Result<()> {
    use axum::{routing::get, Router};
    use byonk::api::dev::{
        handle_dev_css, handle_dev_js, handle_dev_page, handle_events, handle_render,
        handle_resolve_mac, handle_screens, DevState,
    };
    use byonk::assets::AssetLoader;
    use byonk::services::FileWatcher;

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

    // Create file watcher for screens directory
    let file_watcher = Arc::new(FileWatcher::new(screens_dir.clone()));
    if file_watcher.is_active() {
        tracing::info!("File watcher active for live reload");
    } else {
        tracing::warn!("File watcher not active - set SCREENS_DIR to enable live reload");
    }

    // Create application state using shared server module
    let state = server::create_app_state(asset_loader.clone())?;

    // Create dev state
    let config = Arc::new(AppConfig::load_from_assets(&asset_loader));
    let dev_state = DevState {
        config,
        content_pipeline: state.content_pipeline.clone(),
        content_cache: state.content_cache.clone(),
        renderer: state.renderer.clone(),
        file_watcher,
    };

    // Build dev routes as a nested router with DevState
    let dev_router = Router::new()
        .route("/", get(handle_dev_page))
        .route("/dev.css", get(handle_dev_css))
        .route("/dev.js", get(handle_dev_js))
        .route("/screens", get(handle_screens))
        .route("/events", get(handle_events))
        .route("/render", get(handle_render))
        .route("/resolve-mac", get(handle_resolve_mac))
        .with_state(dev_state);

    // Build router: start with shared API routes, add dev routes
    let app = server::build_router(state)
        // OpenAPI documentation
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Static file serving
        .nest_service("/static", ServeDir::new("./static"))
        // Dev mode routes (nested with their own state)
        .nest("/dev", dev_router);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "Byonk dev server listening");
    tracing::info!("Dev mode UI available at http://{}/dev", bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
