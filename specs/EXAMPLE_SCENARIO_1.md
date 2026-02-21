# Example Scenario 1: Basic Server

**Version:** 1.0
**Date:** 2025-02-20
**Status:** Phase 3 (Example Implementation)

## Overview

This is the PRIMARY use case for the Liquers Web API Library: creating a standalone HTTP server with Query and Store APIs using SimpleEnvironment and file-based storage.

### Target Users
- Developers wanting to quickly prototype a Liquers web server
- Applications needing embedded Query and Store APIs
- Users following the getting-started documentation

### What This Example Demonstrates
1. **SimpleEnvironment setup** - Creating a generic environment with core Value type
2. **File-based storage configuration** - Using FileStore for persistent data
3. **Router composition** - Combining Query API and Store API builders
4. **Axum integration** - Binding to port 3000 and handling requests
5. **Graceful shutdown** - Clean server termination on Ctrl+C or SIGTERM

## Scenario Description

A developer wants to launch a Liquers server that:
- Listens on `http://localhost:3000`
- Exposes Query API at `/liquer/q/*` for executing queries
- Exposes Store API at `/liquer/api/store/*` for data storage operations
- Persists data to disk in a `./store` directory
- Supports both GET and POST requests
- Gracefully handles shutdown signals

### Real-World Context
This is what most Liquers deployments need:
- A simple, self-contained HTTP server
- Support for custom data storage
- Easy to add custom commands via registration
- Suitable for containerization and cloud deployment

## Implementation

### File
**Location:** `liquers-axum/examples/basic_server.rs`

**Size:** ~350 lines with comprehensive comments and tests

### Code Structure

#### 1. Environment Initialization
```rust
let mut env: SimpleEnvironment<liquers_core::value::Value> = SimpleEnvironment::new();

let store = Box::new(AsyncStoreWrapper(FileStore::new(
    "./store",  // Data directory
    &Key::new(),
)));
env.with_async_store(store);
let env_ref = env.to_ref();
```

**What it does:**
- Creates a generic environment with core `Value` type
- Configures file-based storage with `./store` directory
- Wraps store in `AsyncStoreWrapper` for async support
- Creates `EnvRef<E>` for sharing across Axum handlers

**Key design decisions:**
- Uses `SimpleEnvironment<Value>` (core-only) - extensible in production apps
- File store for local development - easily replaceable with cloud storage
- `EnvRef` (Arc wrapper) for cheap cloning per request

#### 2. Router Composition
```rust
let query_router = build_query_router::<SimpleEnvironment<liquers_core::value::Value>>();
let store_router = build_store_router::<SimpleEnvironment<liquers_core::value::Value>>();

let app = Router::new()
    .route("/health", get(health_check))
    .merge(query_router)
    .merge(store_router)
    .with_state(env_ref);
```

**What it does:**
- Builds separate routers for Query and Store APIs
- Merges them into a single application
- Attaches shared environment state
- Adds health check endpoint

**Design pattern:**
- **Builder pattern** - `QueryApiBuilder` and `StoreApiBuilder` create reusable routers
- **Generic over Environment** - Works with any `E: Environment + Send + Sync + 'static`
- **Composable** - APIs can be enabled/disabled independently

#### 3. Server Binding and Startup
```rust
let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
let listener = tokio::net::TcpListener::bind(&addr).await?;

axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

**What it does:**
- Binds to localhost on port 3000
- Runs Axum server with async runtime
- Installs signal handlers for graceful shutdown

**Error handling:**
- Falls back to `0.0.0.0` if localhost binding fails
- Logs errors via tracing for visibility

#### 4. Graceful Shutdown
```rust
async fn shutdown_signal() {
    tokio::select! {
        _ = signal::ctrl_c() => { /* SIGINT */ }
        _ = signal::unix::signal(SignalKind::terminate()) => { /* SIGTERM */ }
    }
}
```

**What it does:**
- Catches `Ctrl+C` (SIGINT) and SIGTERM signals
- Allows in-flight requests to complete
- Logs shutdown event
- Cleans up server resources

## Expected Output

### Terminal Output (Running)
```
2025-02-20T10:30:45.123Z  INFO basic_server: Liquers Basic Server starting up...
2025-02-20T10:30:45.124Z  INFO basic_server: Environment initialized with file-based storage
2025-02-20T10:30:45.125Z  INFO basic_server: Application routers configured
2025-02-20T10:30:45.126Z  INFO basic_server: Server listening on http://127.0.0.1:3000
2025-02-20T10:30:45.127Z  INFO basic_server: APIs available at:
2025-02-20T10:30:45.128Z  INFO basic_server:   Query API:  GET/POST  /liquer/q/{query}
2025-02-20T10:30:45.129Z  INFO basic_server:   Store API:  GET/POST/DELETE  /liquer/api/store/{endpoint}/{key}
2025-02-20T10:30:45.130Z  INFO basic_server:
2025-02-20T10:30:45.131Z  INFO basic_server: Press Ctrl+C to shutdown
```

### API Test Responses (Placeholders - Phase 3)
```bash
# Health check
$ curl http://localhost:3000/health
OK

# Query API test
$ curl http://localhost:3000/liquer/q/text-hello
Query API placeholder - implement in Phase 3

# Store API test
$ curl http://localhost:3000/liquer/api/store/data/test/file.txt
Store API placeholder - implement in Phase 3
```

### Shutdown Output
```
2025-02-20T10:30:50.456Z  INFO basic_server: Received Ctrl+C signal, initiating shutdown...
2025-02-20T10:30:50.457Z  INFO basic_server: Server shutdown complete
```

## How to Run

### Prerequisites
- Rust 1.70+ (for tokio async/await)
- 100 MB disk space for cargo build

### Build and Run
```bash
# Check it compiles
cargo check --example basic_server

# Run the server
cargo run --example basic_server

# Test with curl (in another terminal)
curl http://localhost:3000/health
```

### Docker Deployment (Conceptual)
```dockerfile
FROM rust:1.75
WORKDIR /app
COPY . .
RUN cargo build --release --example basic_server
EXPOSE 3000
CMD ["./target/release/examples/basic_server"]
```

## Architecture Integration

### Component Flow
```
SimpleEnvironment<Value>
    ├── Command Registry (default empty)
    ├── Async Store (FileStore wrapped)
    └── Asset Manager (default)
        ↓
EnvRef<E> (Arc wrapper for sharing)
    ↓
Axum State (passed to all handlers)
    ├── GET /health → health_check()
    ├── GET/POST /liquer/q/* → query handlers (Phase 3)
    └── GET/POST/DELETE /liquer/api/store/* → store handlers (Phase 3)
```

### Layer Integration
| Layer | Component | Status |
|-------|-----------|--------|
| **HTTP** | Axum framework | ✅ Ready |
| **Environment** | SimpleEnvironment + FileStore | ✅ Ready |
| **Query API** | QueryApiBuilder (Phase 3) | 🚧 Placeholder |
| **Store API** | StoreApiBuilder (Phase 3) | 🚧 Placeholder |
| **Core** | Value, Key, Query parsing | ✅ Ready |

## Design Decisions

### 1. Core Value Type (Not ExtValue)
**Decision:** Use `SimpleEnvironment<Value>` not `SimpleEnvironment<ExtValue>`

**Rationale:**
- Keeps example minimal and focused on API layer
- Production apps can extend with `ExtValue` if needed
- Avoids liquers-lib dependency for pure HTTP server
- Matches Phase 1 design: web API works with core Value only

### 2. File-Based Storage
**Decision:** Use `FileStore` with `./store` directory

**Rationale:**
- Zero configuration needed
- Works in all environments (local, cloud, container)
- Easy to replace with S3/OpenDAL backend
- Demonstrates full persistence workflow

### 3. Generic Environment Type
**Decision:** Builder functions generic over `E: Environment`

**Rationale:**
- Enables custom Environment implementations
- Supports dependency injection patterns
- Follows Rust zero-cost abstraction principles
- Allows testing with mock environments

### 4. Graceful Shutdown
**Decision:** Use `tokio::signal` with `tokio::select!`

**Rationale:**
- Handles both SIGINT (Ctrl+C) and SIGTERM (container stop)
- Allows in-flight requests to complete
- Cross-platform compatible
- Standard Tokio pattern

## Testing

### Unit Tests Included
```rust
#[test]
fn test_environment_creation() { }

#[test]
fn test_env_ref_creation() { }

#[tokio::test]
async fn test_tcp_listener_binding() { }
```

### Run Tests
```bash
cargo test --example basic_server
```

### Integration Tests (Phase 3)
Future work: Add full integration tests
```rust
#[tokio::test]
async fn test_query_api_endpoint() { }

#[tokio::test]
async fn test_store_api_endpoint() { }
```

## Validation Checklist

- [x] Code compiles without errors
- [x] Code follows CLAUDE.md conventions
  - [x] No `unwrap()`/`expect()` in library code (only tests)
  - [x] Uses typed error constructors (not `Error::new`)
  - [x] All match statements explicit (no `_ =>`)
  - [x] Async by default
  - [x] `E: Environment + Send + Sync + 'static` bounds
- [x] Imports organized (std, external, crate-local)
- [x] Comprehensive comments explaining each step
- [x] Realistic parameters (port 3000, file store, localhost)
- [x] Clean shutdown handling
- [x] Expected output documented
- [x] Ready for user-facing documentation

## Future Work (Phase 3+)

### Immediate
1. Implement `QueryApiBuilder::build_axum()` with actual handlers
2. Implement `StoreApiBuilder::build_axum()` with actual handlers
3. Replace placeholder handlers with real query/store logic
4. Add integration tests for all endpoints

### Short-term
1. Add command registration example (showing extensibility)
2. Add custom middleware (logging, CORS, auth)
3. Add configuration file support (YAML/TOML)
4. Add OpenAPI/Swagger documentation

### Long-term
1. Docker/container deployment guide
2. Cloud storage backend examples (S3, GCS)
3. Benchmarks and performance tuning
4. Security hardening guide

## References

- **Phase 1 Design:** `specs/web-api-library/phase1-high-level-design.md`
- **Phase 2 Architecture:** `specs/web-api-library/phase2-architecture.md`
- **Development Guide:** `CLAUDE.md` (project conventions)
- **API Specification:** `specs/WEB_API_SPECIFICATION.md`
- **Axum Documentation:** https://docs.rs/axum/
- **Tokio Documentation:** https://tokio.rs/

## Related Examples

(Future)
- **Example 2:** Custom commands with registration
- **Example 3:** Cloud storage backend (S3)
- **Example 4:** Middleware and request logging
- **Example 5:** Embedded in existing application
