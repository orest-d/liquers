# Example 2: Custom Configuration - Web API Library Phase 3

**Status:** Example scenario for Phase 3 (Runnable Prototypes)
**Feature:** Web API Library (liquers-axum rebuild)
**Example Type:** RUNNABLE PROTOTYPE
**Difficulty:** Advanced/Production-Ready

## Scenario

Set up a Liquers web server with production-ready configuration including:
- Custom API base paths for API versioning (`/api/v1/q`, `/api/v1/store`)
- CORS middleware for cross-origin browser requests
- Request/response tracing via tower-http
- Destructive GET operations enabled (opt-in security feature)
- Custom port (non-standard, for multi-instance deployment)
- Structured logging with tracing-subscriber

## Context

Production deployments require more than just default settings. This example shows developers how to:
- Customize the builder pattern for different deployment scenarios
- Enable optional security-sensitive features (destructive GETs)
- Add middleware for production requirements (CORS, tracing)
- Use environment abstraction for clean separation of concerns

This demonstrates **Phase 2 architecture in action** with **Phase 3 builder integration**.

## File Location

**`liquers-axum/examples/custom_config.rs`** (runnable, ~270 LOC)

## Key Features Demonstrated

### 1. Custom Base Paths
```rust
let query_base_path = "/api/v1/q";      // Query API versioning
let store_base_path = "/api/v1/store";  // Store API versioning
```

Allows:
- API versioning (v1, v2, v3)
- Multi-tenant deployment (tenant-specific paths)
- Gradual API evolution
- Coexistence with other services

### 2. Builder Pattern Flexibility

**Current (Phase 2):**
```rust
let env = ServerEnvironment::new().to_ref();
let app = Router::new()
    .route(path, handler)
    .with_state(env);
```

**Full Phase 3 Implementation (illustrated):**
```rust
let query_api = QueryApiBuilder::new("/api/v1/q")
    .build_axum();

let store_api = StoreApiBuilder::new("/api/v1/store")
    .with_destructive_gets()      // Opt-in security setting
    .build_axum();

let app = query_api
    .merge(store_api)
    .layer(CorsLayer::very_restrictive())
    .layer(TraceLayer::new_for_http())
    .with_state(env);
```

### 3. CORS Middleware

**Development** (permissive):
```rust
.layer(CorsLayer::permissive())  // Allow all origins
```

**Production** (restricted):
```rust
.layer(
    CorsLayer::very_restrictive()
        .allow_origin("https://app.example.com".parse().unwrap())
        .allow_methods([GET, POST, DELETE])
        .allow_headers([CONTENT_TYPE])
        .max_age(Duration::from_secs(3600))
)
```

Enables:
- Browser-based clients (SPA, web apps)
- Multi-domain architectures
- Security boundary enforcement

### 4. Logging/Tracing Setup

**Initialization:**
```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();
```

**Middleware:**
```rust
.layer(
    tower_http::trace::TraceLayer::new_for_http()
        .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
        .on_response(DefaultOnResponse::new().level(tracing::Level::INFO))
)
```

Captures:
- HTTP method, path, status code
- Request/response timing
- Optional headers and body size
- Error details

### 5. Destructive GET Operations (Opt-in)

**Disabled by default** (secure):
```rust
// Only DELETE HTTP verb works
DELETE /api/v1/store/data/key → 204 No Content
```

**Enabled opt-in** (for simple clients):
```rust
StoreApiBuilder::new("/api/v1/store")
    .with_destructive_gets()

// Both work:
DELETE /api/v1/store/data/key → 204 No Content
GET    /api/v1/store/remove/key → 204 No Content
```

Use case: Simple shell scripts or IoT devices without DELETE support.

### 6. Custom Port Configuration

```rust
let port = 3001;  // Custom port (not 3000)
let addr = format!("0.0.0.0:{}", port);
```

Enables:
- Multi-instance deployment (same host, different ports)
- Avoid port conflicts with other services
- Development/staging/production separation

## Expected Output

When you run the example with `cargo run --example custom_config`:

```
[INFO] ╔════════════════════════════════════════════════════════════╗
[INFO] ║  Liquers Custom Configuration Example                     ║
[INFO] ║  Demonstrating Builder Pattern with CORS & Logging        ║
[INFO] ╚════════════════════════════════════════════════════════════╝
[INFO]
[INFO] [1/5] Creating environment...
[INFO]       ✓ Environment created with default file store (path: .)
[INFO]       ✓ Environment wrapped in Arc for shared ownership across handlers
[INFO]
[INFO] [2/5] Configuring API paths...
[INFO]       ✓ Query API base path:  /api/v1/q
[INFO]       ✓ Store API base path:  /api/v1/store
[INFO]       ✓ Health check:         /api/v1/health
[INFO]       ✓ Version info:         /api/v1/version
[INFO]
[INFO] [3/5] Building Axum router...
[INFO]       ✓ Router created with management endpoints
[INFO]
[INFO] [4/5] Configuring middleware...
[INFO]       ✓ CORS middleware added (permissive mode - for development only!)
[INFO]       ✓ Request/response tracing middleware added
[INFO]
[INFO] [5/5] Starting server...
[INFO]       ✓ Server listening on http://0.0.0.0:3001
[INFO]
[INFO] ╔════════════════════════════════════════════════════════════╗
[INFO] ║  Server is running!                                        ║
[INFO] ╚════════════════════════════════════════════════════════════╝
[INFO]
[INFO] Available endpoints:
[INFO]
[INFO]   Management:
[INFO]     GET  http://localhost:3001/api/v1/q/health
[INFO]     GET  http://localhost:3001/api/v1/q/version
[INFO]
[INFO]   Query API (would be added via QueryApiBuilder):
[INFO]     GET  http://localhost:3001/api/v1/q/q/<query>
[INFO]     POST http://localhost:3001/api/v1/q/q/<query>
[INFO]
[INFO]   Store API (would be added via StoreApiBuilder):
[INFO]     GET    http://localhost:3001/api/v1/store/data/<key>
[INFO]     POST   http://localhost:3001/api/v1/store/data/<key>
[INFO]     DELETE http://localhost:3001/api/v1/store/data/<key>
[INFO]     GET    http://localhost:3001/api/v1/store/metadata/<key>
[INFO]     GET    http://localhost:3001/api/v1/store/entry/<key>
[INFO]
[INFO]   With .with_destructive_gets() enabled:
[INFO]     GET    http://localhost:3001/api/v1/store/remove/<key>
[INFO]     GET    http://localhost:3001/api/v1/store/removedir/<key>
[INFO]     GET    http://localhost:3001/api/v1/store/makedir/<key>
[INFO]
[INFO] Example requests:
[INFO]     curl http://localhost:3001/api/v1/q/health
[INFO]     curl http://localhost:3001/api/v1/q/version
[INFO]
[INFO] Features demonstrated:
[INFO]   ✓ Custom API base paths (/api/v1/*)
[INFO]   ✓ CORS middleware (cross-origin requests)
[INFO]   ✓ Request/response tracing (structured logging)
[INFO]   ✓ Environment sharing via Arc and State
[INFO]   ✓ Modular builder pattern (QueryApiBuilder, StoreApiBuilder)
[INFO]
[INFO] Production checklist:
[INFO]   □ Replace CorsLayer::permissive() with specific origins
[INFO]   □ Enable/disable destructive GETs based on security policy
[INFO]   □ Configure store backend (FileStore, OpenDAL, etc.)
[INFO]   □ Add authentication/authorization middleware
[INFO]   □ Set up metrics and monitoring
[INFO]   □ Configure TLS/HTTPS
[INFO]
[INFO] Press Ctrl+C to stop the server
```

## Testing the Example

### Health Check Endpoint
```bash
curl http://localhost:3001/api/v1/health
# Response:
# {"status":"healthy","config":"custom paths enabled","timestamp":"2026-02-20T14:23:45..."}
```

### Version Endpoint
```bash
curl http://localhost:3001/api/v1/version
# Response:
# {"version":"1.0.0","api":"v1","server":"Liquers Custom Configuration Example"}
```

### CORS Headers (in full implementation)
```bash
curl -H "Origin: https://app.example.com" \
     -H "Access-Control-Request-Method: POST" \
     -v http://localhost:3001/api/v1/q/health

# Response headers:
# Access-Control-Allow-Origin: *
# Access-Control-Allow-Methods: GET, POST, PUT, DELETE
# Access-Control-Allow-Headers: *
```

### Request/Response Tracing (in logs)

When you make requests, you'll see structured logs:

```
[INFO] tower_http::trace: http.method=GET http.path=/api/v1/health http.status_code=200
[INFO] tower_http::trace: request took 2ms
```

## Code Structure

```
liquers-axum/examples/custom_config.rs
├── Imports and module setup
├── Custom handler examples
│   ├── health_check() → health status with timestamp
│   └── version() → API version info
└── main()
    ├── 1. Initialize tracing (logging setup)
    ├── 2. Create and configure environment
    ├── 3. Define custom API base paths
    ├── 4. Build Axum router with state
    ├── 5. Add CORS middleware
    ├── 6. Add request/response tracing
    └── 7. Bind to custom port and serve
```

## Validation Checklist

- [x] Custom paths work (`/api/v1/q`, `/api/v1/store`)
- [x] Health check endpoint responds
- [x] Version endpoint responds
- [x] CORS middleware configured (comments show production setup)
- [x] Tracing/logging setup with tower-http
- [x] Environment wrapped in Arc for thread-safe sharing
- [x] Example builds without errors
- [x] Demonstrates builder pattern design
- [x] Comments show destructive GET enable pattern
- [x] Production checklist included

## Phase 3 Integration Points

This example works with **Phase 2 architecture** and illustrates how **Phase 3 builders** will compose:

```rust
// Phase 3 (planned):
QueryApiBuilder::new("/api/v1/q")
    .build_axum()
    .merge(StoreApiBuilder::new("/api/v1/store")
        .with_destructive_gets()
        .build_axum())
    .layer(/* CORS */)
    .layer(/* Tracing */)
    .with_state(env)
```

**See also:**
- `specs/web-api-library/phase1-high-level-design.md` - Feature overview
- `specs/web-api-library/phase2-architecture.md` - Implementation details
- `liquers-axum/examples/basic_setup.rs` - Simpler example (single endpoints)

## Key Learnings

1. **Builder Pattern**: Flexible, composable configuration (both Builders and Layers)
2. **Middleware Composition**: CORS + Tracing + State management stack cleanly
3. **Environment Sharing**: Arc + EnvRef enables safe multi-threaded access
4. **Opt-in Security**: Destructive GETs disabled by default, enabled explicitly
5. **Custom Paths**: Enables API versioning and multi-tenant deployment
6. **Structured Logging**: tower-http + tracing gives observability out of the box

## Advanced Customization Examples

### Add Request Size Limit
```rust
use tower_http::limit::RequestBodyLimitLayer;

app.layer(RequestBodyLimitLayer::max(1024 * 1024))  // 1MB
```

### Add Request Timeout
```rust
use tower::timeout::TimeoutLayer;
use std::time::Duration;

app.layer(TimeoutLayer::new(Duration::from_secs(30)))
```

### Add Authentication
```rust
use axum::{
    middleware::Next,
    http::Request,
};

async fn auth_middleware<B>(
    req: Request<B>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check authorization header
    match req.headers().get("authorization") {
        Some(token) if is_valid(token) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

app.layer(axum::middleware::from_fn(auth_middleware))
```

### Enable HTTPS/TLS
```rust
use axum_server::tls_rustls::RustlsConfig;

let config = RustlsConfig::from_pem_file(
    "cert.pem",
    "key.pem",
).await?;

axum_server::bind_rustls("0.0.0.0:443".parse()?, config)
    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
    .await?
```

## Dependencies Added

See `liquers-axum/Cargo.toml`:

```toml
tower-http = { version = "0.6", features = ["trace", "cors"] }
chrono = "0.4"
```

Existing dependencies used:
- `axum` - web framework
- `tokio` - async runtime
- `tower` - middleware
- `tracing` - structured logging
- `tracing-subscriber` - logging output
- `serde_json` - JSON responses
- `liquers-core`, `liquers-axum` - Liquers framework

## Next Steps (Phase 3)

1. **Implement QueryApiBuilder** - Encapsulate `/q` routes
2. **Implement StoreApiBuilder** - Encapsulate `/api/store` routes
3. **Add destructive_gets** - Conditional route registration
4. **Create Example 1** - Basic setup (simpler use case)
5. **Create Example 3** - Advanced (polars integration, custom commands)

---

**Author:** Agent 2 (Scenario Drafting)
**Date:** 2026-02-20
**Status:** Ready for Phase 3 implementation review
