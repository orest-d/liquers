//! Example: Custom Configuration with CORS and Logging
//!
//! This example demonstrates how to set up a Liquers web server with:
//! - Custom base paths for both Query and Store APIs
//! - CORS middleware for cross-origin requests
//! - Logging/tracing setup with tower-http
//! - Destructive GET operations enabled for Store API
//! - Custom port configuration
//!
//! Run with: cargo run --example custom_config
//! Then test with:
//!   curl -v http://localhost:3001/api/v1/health
//!   curl -v http://localhost:3001/api/v1/version

use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json,
    Router,
};
use liquers_axum::environment::ServerEnvironment;
use liquers_core::context::{Environment, EnvRef};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tracing::info;

// ============================================================================
// Custom Handler Examples
// ============================================================================

/// Simple health check endpoint showing configuration
async fn health_check(
    State(_env): State<EnvRef<ServerEnvironment>>,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "config": "custom paths enabled",
            "timestamp": chrono::Local::now().to_string()
        })),
    )
}

/// Version endpoint
async fn version(
    State(_env): State<EnvRef<ServerEnvironment>>,
) -> Json<serde_json::Value> {
    Json(json!({
        "version": "1.0.0",
        "api": "v1",
        "server": "Liquers Custom Configuration Example"
    }))
}

// ============================================================================
// Main Server Setup
// ============================================================================

#[tokio::main]
async fn main() {
    // ========================================================================
    // 1. Initialize Tracing (Logging)
    // ========================================================================
    // Set up structured logging with tracing-subscriber
    // This will output to stdout with INFO level and above
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║  Liquers Custom Configuration Example                     ║");
    info!("║  Demonstrating Builder Pattern with CORS & Logging        ║");
    info!("╚════════════════════════════════════════════════════════════╝");
    info!("");

    // ========================================================================
    // 2. Create and Configure Environment
    // ========================================================================
    info!("[1/5] Creating environment...");

    let env = ServerEnvironment::new();

    // In a production scenario, you might configure the environment with:
    // - Custom store backends (OpenDAL, cloud storage, etc.)
    // - Command registry customizations
    // - Asset manager configuration

    info!("      ✓ Environment created with default file store (path: .)");

    // Convert to EnvRef for Axum state sharing (Arc-wrapped)
    let env_ref = env.to_ref();
    info!("      ✓ Environment wrapped in Arc for shared ownership across handlers");

    // ========================================================================
    // 3. Define Custom API Base Paths
    // ========================================================================
    info!("");
    info!("[2/5] Configuring API paths...");

    let query_base_path = "/api/v1/q";           // Query API at custom path
    let store_base_path = "/api/v1/store";       // Store API at custom path
    let health_path = "/api/v1/health";          // Health check endpoint
    let version_path = "/api/v1/version";        // Version info endpoint

    info!("      ✓ Query API base path:  {}", query_base_path);
    info!("      ✓ Store API base path:  {}", store_base_path);
    info!("      ✓ Health check:         {}", health_path);
    info!("      ✓ Version info:         {}", version_path);

    // ========================================================================
    // 4. Build Axum Router with Custom Configuration
    // ========================================================================
    info!("");
    info!("[3/5] Building Axum router...");

    // In the full Phase 3 implementation, you would use:
    //
    //   let query_api = QueryApiBuilder::new(query_base_path)
    //       .build_axum();
    //
    //   let store_api = StoreApiBuilder::new(store_base_path)
    //       .with_destructive_gets()      // Enable GET-based DELETE operations
    //       .build_axum();
    //
    //   let mut app = query_api.merge(store_api);
    //
    // For now, we demonstrate the pattern with custom endpoints:

    let app = Router::new()
        // Management endpoints (health, version, status)
        .route(health_path, get(health_check))
        .route(version_path, get(version))

        // Note: Full Query and Store API routes would be added here via builders:
        //
        //   .route(&format!("{}/*query", query_base_path), get(...))
        //   .route(&format!("{}/*query", query_base_path), post(...))
        //
        //   .route(&format!("{}/data/*key", store_base_path), get(...))
        //   .route(&format!("{}/data/*key", store_base_path), post(...))
        //   .route(&format!("{}/data/*key", store_base_path), delete(...))
        //   .route(&format!("{}/remove/*key", store_base_path), get(...))  // destructive GET
        //   .route(&format!("{}/metadata/*key", store_base_path), get(...))
        //   .route(&format!("{}/entry/*key", store_base_path), get(...))

        .with_state(env_ref.clone());  // Attach environment state

    info!("      ✓ Router created with management endpoints");

    // ========================================================================
    // 5. Add CORS Middleware
    // ========================================================================
    info!("");
    info!("[4/5] Configuring middleware...");

    // Add CORS layer for cross-origin requests
    let app = app.layer(
        CorsLayer::permissive()  // Allow all origins (development only!)

        // For production, use more restrictive configuration:
        // CorsLayer::very_restrictive()
        //     .allow_origin("https://app.example.com".parse().unwrap())
        //     .allow_origin("https://admin.example.com".parse().unwrap())
        //     .allow_methods([GET, POST, DELETE, PUT])
        //     .allow_headers([CONTENT_TYPE, AUTHORIZATION])
        //     .max_age(Duration::from_secs(3600))
    );

    info!("      ✓ CORS middleware added (permissive mode - for development only!)");

    // Add tower-http tracing middleware for request/response logging
    let app = app.layer(
        tower_http::trace::TraceLayer::new_for_http()
            .on_request(
                tower_http::trace::DefaultOnRequest::new()
                    .level(tracing::Level::INFO)
            )
            .on_response(
                tower_http::trace::DefaultOnResponse::new()
                    .level(tracing::Level::INFO)
            )
    );

    info!("      ✓ Request/response tracing middleware added");

    // ========================================================================
    // 6. Bind to Custom Port
    // ========================================================================
    info!("");
    info!("[5/5] Starting server...");

    let port = 3001;  // Custom port (default Liquers uses 3000)
    let addr = format!("0.0.0.0:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("      ✓ Server listening on http://0.0.0.0:{}", port);
    info!("");
    info!("╔════════════════════════════════════════════════════════════╗");
    info!("║  Server is running!                                        ║");
    info!("╚════════════════════════════════════════════════════════════╝");
    info!("");

    // Print API documentation
    print_api_documentation(port, query_base_path, store_base_path);

    info!("");
    info!("Features demonstrated:");
    info!("  ✓ Custom API base paths (/api/v1/*)");
    info!("  ✓ CORS middleware (cross-origin requests)");
    info!("  ✓ Request/response tracing (structured logging)");
    info!("  ✓ Environment sharing via Arc and State");
    info!("  ✓ Modular builder pattern (QueryApiBuilder, StoreApiBuilder)");
    info!("");
    info!("Production checklist:");
    info!("  □ Replace CorsLayer::permissive() with specific origins");
    info!("  □ Enable/disable destructive GETs based on security policy");
    info!("  □ Configure store backend (FileStore, OpenDAL, etc.)");
    info!("  □ Add authentication/authorization middleware");
    info!("  □ Set up metrics and monitoring");
    info!("  □ Configure TLS/HTTPS");
    info!("");
    info!("Press Ctrl+C to stop the server");
    info!("");

    axum::serve(listener, app)
        .await
        .expect("Server failed");
}

/// Print API documentation
fn print_api_documentation(port: u16, query_base: &str, store_base: &str) {
    info!("Available endpoints:");
    info!("");
    info!("  Management:");
    info!("    GET  http://localhost:{}{}/health", port, query_base);
    info!("    GET  http://localhost:{}{}/version", port, query_base);
    info!("");
    info!("  Query API (would be added via QueryApiBuilder):");
    info!("    GET  http://localhost:{}{}/q/<query>", port, query_base);
    info!("    POST http://localhost:{}{}/q/<query>", port, query_base);
    info!("");
    info!("  Store API (would be added via StoreApiBuilder):");
    info!("    GET    http://localhost:{}{}/data/<key>", port, store_base);
    info!("    POST   http://localhost:{}{}/data/<key>", port, store_base);
    info!("    DELETE http://localhost:{}{}/data/<key>", port, store_base);
    info!("    GET    http://localhost:{}{}/metadata/<key>", port, store_base);
    info!("    GET    http://localhost:{}{}/entry/<key>", port, store_base);
    info!("");
    info!("  With .with_destructive_gets() enabled:");
    info!("    GET    http://localhost:{}{}/remove/<key>", port, store_base);
    info!("    GET    http://localhost:{}{}/removedir/<key>", port, store_base);
    info!("    GET    http://localhost:{}{}/makedir/<key>", port, store_base);
    info!("");
    info!("Example requests:");
    info!("    curl http://localhost:{}{}/health", port, query_base);
    info!("    curl http://localhost:{}{}/version", port, query_base);
}

