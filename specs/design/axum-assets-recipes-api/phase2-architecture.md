# Phase 2: Solution & Architecture - Assets API and Recipes API

## Overview

The Assets API and Recipes API are implemented as thin HTTP wrappers around existing `AssetManager` and `AsyncRecipeProvider` services, following the established Store API pattern in liquers-axum. Both APIs reuse all `api_core` types (`ApiResponse`, `DataEntry`, `ErrorDetail`, `SerializationFormat`) and follow identical handler patterns (builders, generic Environment, async handlers). The Assets API additionally includes WebSocket support for real-time asset notifications.

## Data Structures

### New Structs

#### AssetsApiBuilder<E: Environment>

```rust
pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: Option<String>,  // Default: "{base_path}/ws"
    _phantom: PhantomData<E>,
}
```

**Ownership rationale:**
- `base_path` is owned (consumed during route building)
- `websocket_path` is optional (default computed from base_path)
- `_phantom` is zero-cost generic marker

**Serialization:** Not serializable (builder is ephemeral)

#### RecipesApiBuilder<E: Environment>

```rust
pub struct RecipesApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}
```

**Ownership rationale:**
- `base_path` is owned (consumed during route building)
- `_phantom` is zero-cost generic marker

**Serialization:** Not serializable (builder is ephemeral)

#### WebSocketState (internal, for Assets API)

```rust
struct WebSocketState {
    subscriptions: Arc<RwLock<HashMap<Query, Vec<(SubscriptionId, mpsc::Sender<NotificationMessage>)>>>>,
}
```

**Ownership rationale:**
- `Arc<RwLock<...>>` for shared mutable state across WebSocket tasks
- `RwLock` (not Mutex) - read-heavy workload (many subscribers, fewer writes)
- `mpsc::Sender` for per-client notification channels

**Serialization:** Not serializable (runtime state only)

### New Enums

#### NotificationMessage (WebSocket messages)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationMessage {
    Initial {
        asset_id: u64,
        query: String,
        timestamp: String,
        metadata: Option<serde_json::Value>,
    },
    StatusChanged {
        asset_id: u64,
        query: String,
        status: String,
        timestamp: String,
    },
    ValueProduced {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    ErrorOccurred {
        asset_id: u64,
        query: String,
        timestamp: String,
        error: ErrorDetail,
    },
    LogMessage {
        asset_id: u64,
        query: String,
        timestamp: String,
        message: String,
    },
    PrimaryProgressUpdated {
        asset_id: u64,
        query: String,
        timestamp: String,
        progress: ProgressInfo,
    },
    SecondaryProgressUpdated {
        asset_id: u64,
        query: String,
        timestamp: String,
        progress: ProgressInfo,
    },
    JobSubmitted {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    JobStarted {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    JobFinished {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Pong {
        timestamp: String,
    },
    UnsubscribedAll {
        timestamp: String,
        message: String,
    },
}
```

**Variant semantics:**
- All variants map to `liquers_core::assets::AssetNotificationMessage` variants
- `asset_id` field added to all asset-related messages (from `AssetRef::id()`)
- `Pong` and `UnsubscribedAll` are server-only messages (not from AssetManager)

**No default match arm:** All match statements on this enum must be explicit.

**Serialization:** `#[serde(tag = "type")]` for tagged JSON serialization (matches spec)

#### ProgressInfo (nested in NotificationMessage)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressInfo {
    pub message: String,
    pub done: u64,
    pub total: u64,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta: Option<String>,
}
```

**Serialization:** All fields serialized, `eta` skipped if None

### ExtValue Extensions

**Not applicable.** No new ExtValue variants needed.

## Trait Implementations

**None required.** All functionality implemented via free functions (handlers) and builder methods.

## Generic Parameters & Bounds

### Generic Builders

Both builders are generic over `Environment`:

```rust
impl<E: Environment> AssetsApiBuilder<E> {
    // Methods
}

impl<E: Environment> RecipesApiBuilder<E> {
    // Methods
}
```

**Bound justification:**
- `Environment`: Required to access services (AssetManager, AsyncRecipeProvider)
- No additional bounds needed (`Environment` trait already implies `Send + Sync + Clone + 'static`)

**Avoid over-constraining:** Only the single `Environment` bound is necessary.

### Generic Handlers

All handlers are generic over `Environment`:

```rust
async fn handler_name<E: Environment>(
    State(env): State<EnvRef<E>>,
    // other extractors
) -> Response
```

**Bound justification:**
- `E: Environment` bound is inferred from `State<EnvRef<E>>` extractor
- Axum requires `State` types to be `Clone`, which `Arc<E>` satisfies

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| All HTTP handlers | Yes | Handlers call async services (AssetManager, AsyncRecipeProvider) |
| WebSocket handlers | Yes | WebSocket I/O is async, notifications are async streams |
| Builder methods | No | Pure construction, no I/O |
| Helper functions | No | Synchronous transformations (query parsing, response building) |

**Pattern:** All handlers are async (follow axum async handler pattern and call async services).

## Function Signatures

### Module: liquers_axum::assets::builder

```rust
impl<E: Environment> AssetsApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;

    pub fn with_websocket_path(mut self, path: impl Into<String>) -> Self;

    pub fn build(self) -> Router<EnvRef<E>>;
}
```

### Module: liquers_axum::assets::handlers

```rust
// GET /api/assets/data/{*query}
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;

// POST /api/assets/data/{*query}
pub async fn post_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    body: Bytes,
) -> Response;

// DELETE /api/assets/data/{*query}
pub async fn delete_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;

// GET /api/assets/metadata/{*query}
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;

// POST /api/assets/metadata/{*query}
pub async fn post_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    Json(metadata_json): Json<serde_json::Value>,
) -> Response;

// GET /api/assets/entry/{*query}
pub async fn get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    headers: axum::http::HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response;

// POST /api/assets/entry/{*query}
pub async fn post_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    headers: axum::http::HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    body: Bytes,
) -> Response;

// DELETE /api/assets/entry/{*query}
pub async fn delete_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;

// GET /api/assets/listdir/{*query}
pub async fn listdir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;

// POST /api/assets/cancel/{*query}
pub async fn cancel_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response;
```

**Parameter choices:**
- `State(env): State<EnvRef<E>>` - Arc clone (cheap) for environment access
- `Path(query_path): Path<String>` - Owned string from URL path
- `headers: axum::http::HeaderMap` - For format negotiation (Accept, Content-Type)
- `AxumQuery(params)` - Query parameters for format selection
- `body: Bytes` - Raw request body for binary data
- Return `Response` - Unified response type (from `axum::response::IntoResponse`)

### Module: liquers_axum::assets::websocket

```rust
// WebSocket upgrade handler
pub async fn websocket_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query_path): Path<String>,
    State(env): State<EnvRef<E>>,
) -> impl IntoResponse;

// WebSocket connection handler (internal)
async fn handle_socket<E: Environment>(
    socket: WebSocket,
    query: Query,
    env: EnvRef<E>,
);

// Client message handler (internal)
#[derive(Deserialize)]
#[serde(tag = "action")]
enum ClientMessage {
    Subscribe { query: String },
    Unsubscribe { query: String },
    UnsubscribeAll,
    Ping,
}
```

### Module: liquers_axum::recipes::builder

```rust
impl<E: Environment> RecipesApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;

    pub fn build(self) -> Router<EnvRef<E>>;
}
```

### Module: liquers_axum::recipes::handlers

```rust
// GET /api/recipes/listdir
pub async fn listdir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
) -> Response;

// GET /api/recipes/data/{*key}
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response;

// GET /api/recipes/metadata/{*key}
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response;

// GET /api/recipes/entry/{*key}
pub async fn get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: axum::http::HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response;

// GET /api/recipes/resolve/{*key}
pub async fn resolve_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response;
```

## Integration Points

### Crate: liquers-axum

**New modules to create:**

1. **`liquers-axum/src/assets/mod.rs`**
   - Exports: `pub use builder::AssetsApiBuilder;`
   - Re-exports handlers (pub(crate) or private)

2. **`liquers-axum/src/assets/builder.rs`**
   - Contains: `AssetsApiBuilder<E>` implementation
   - Route registration for all Assets API endpoints

3. **`liquers-axum/src/assets/handlers.rs`**
   - Contains: All Assets API handler functions
   - Uses: `api_core` types, `liquers_core::parse::parse_query`

4. **`liquers-axum/src/assets/websocket.rs`**
   - Contains: WebSocket upgrade handler, socket handler, client message handling
   - Uses: `axum::extract::ws`, `tokio::sync::mpsc`, notification conversion

5. **`liquers-axum/src/recipes/mod.rs`**
   - Exports: `pub use builder::RecipesApiBuilder;`
   - Re-exports handlers (pub(crate) or private)

6. **`liquers-axum/src/recipes/builder.rs`**
   - Contains: `RecipesApiBuilder<E>` implementation
   - Route registration for all Recipes API endpoints

7. **`liquers-axum/src/recipes/handlers.rs`**
   - Contains: All Recipes API handler functions
   - Uses: `api_core` types, `liquers_core::parse::parse_key`

**Modify existing file:**

**`liquers-axum/src/lib.rs`:**
```rust
pub mod api_core;
pub mod query;
pub mod store;
pub mod assets;   // NEW
pub mod recipes;  // NEW
pub mod axum_integration;

// Re-exports
pub use api_core::{ApiResponse, ErrorDetail, DataEntry, BinaryResponse, SerializationFormat};
pub use query::QueryApiBuilder;
pub use store::StoreApiBuilder;
pub use assets::AssetsApiBuilder;   // NEW
pub use recipes::RecipesApiBuilder; // NEW
pub use liquers_core::context::EnvRef;
```

**No modifications to `api_core`** - reuse all existing types and functions.

### Dependencies

**No new dependencies needed.** All required crates already in `liquers-axum/Cargo.toml`:
- `axum` (with `macros` feature) - for handlers and WebSocket
- `tokio` (with `sync` feature) - for mpsc channels
- `serde`, `serde_json` - for JSON serialization
- `liquers-core` - for Error, Query, Key, AssetManager, AsyncRecipeProvider

**WebSocket support:** Already available via `axum::extract::ws` (axum 0.8+)

## Relevant Commands

### New Commands

**None.** The Assets API and Recipes API are pure HTTP/WebSocket interface layers. They do not register any new commands.

### Relevant Existing Namespaces

**Not applicable.** These APIs expose services (AssetManager, AsyncRecipeProvider), not command execution.

The only command-related interaction is indirect:
- Assets API triggers command execution when queries are evaluated via `AssetManager::get()`
- Recipes API returns query strings that reference existing commands

**User confirmation:** No new commands needed - this is correct for API layer design.

## Web Endpoints

### Assets API Routes (`/liquer/api/assets/*`)

**Builder configuration:**
```rust
let assets_api = AssetsApiBuilder::<E>::new("/liquer/api/assets")
    .with_websocket_path("/liquer/ws/assets")  // Optional, defaults to "{base}/ws"
    .build();
```

**HTTP Endpoints:**

| Method | Route | Handler | Description |
|--------|-------|---------|-------------|
| GET | `/data/{*query}` | `get_data_handler` | Retrieve asset data (triggers evaluation if needed) |
| POST | `/data/{*query}` | `post_data_handler` | Set asset data directly (status → Source) |
| DELETE | `/data/{*query}` | `delete_data_handler` | Remove cached asset value (preserve recipe) |
| GET | `/metadata/{*query}` | `get_metadata_handler` | Retrieve asset metadata only |
| POST | `/metadata/{*query}` | `post_metadata_handler` | Update asset metadata |
| GET | `/entry/{*query}` | `get_entry_handler` | Get data+metadata (DataEntry, CBOR/bincode/JSON) |
| POST | `/entry/{*query}` | `post_entry_handler` | Set data+metadata (DataEntry, CBOR/bincode/JSON) |
| DELETE | `/entry/{*query}` | `delete_entry_handler` | Remove asset (same as DELETE data) |
| GET | `/listdir/{*query}` | `listdir_handler` | List assets in directory |
| POST | `/cancel/{*query}` | `cancel_handler` | Cancel running asset evaluation |

**WebSocket Endpoint:**

| Method | Route | Handler | Description |
|--------|-------|---------|-------------|
| WS | `/ws/{*query}` | `websocket_handler` | Real-time asset notifications (subscribe to asset lifecycle events) |

**Path parameter:**
- `{*query}` - Full query string (may include commands), parsed via `liquers_core::parse::parse_query()`

**Query parameters** (for entry endpoints):
- `format` - Serialization format: `cbor` (default), `bincode`, or `json`

**Headers** (for entry endpoints):
- `Accept` - Content negotiation (overridden by `?format=` query param)
- `Content-Type` - Request body format (for POST requests)

### Recipes API Routes (`/liquer/api/recipes/*`)

**Builder configuration:**
```rust
let recipes_api = RecipesApiBuilder::<E>::new("/liquer/api/recipes").build();
```

**HTTP Endpoints:**

| Method | Route | Handler | Description |
|--------|-------|---------|-------------|
| GET | `/listdir` | `listdir_handler` | List all recipes (via `AsyncRecipeProvider::assets_with_recipes()`) |
| GET | `/data/{*key}` | `get_data_handler` | Get recipe definition (via `AsyncRecipeProvider::recipe()`) |
| GET | `/metadata/{*key}` | `get_metadata_handler` | Get recipe metadata (title, description, volatile) |
| GET | `/entry/{*key}` | `get_entry_handler` | Get recipe data+metadata (DataEntry format) |
| GET | `/resolve/{*key}` | `resolve_handler` | Resolve recipe to execution plan (via `AsyncRecipeProvider::recipe_plan()`) |

**Path parameter:**
- `{*key}` - Recipe key (simple identifier, not a full query), parsed via `liquers_core::parse::parse_key()`

**Query parameters** (for entry endpoint):
- `format` - Serialization format: `cbor` (default), `bincode`, or `json`

**Headers** (for entry endpoint):
- `Accept` - Content negotiation (overridden by `?format=` query param)

**Note:** Recipes API is read-only (no POST/PUT/DELETE operations) - recipes are managed through the `AsyncRecipeProvider` service, not HTTP endpoints.

## Error Handling

### Error Strategy

**Use `liquers_core::error::Error` exclusively.** Convert to HTTP responses at handler boundary via `api_core::error::error_to_detail()`.

### Error Scenarios

| Scenario | ErrorType | Constructor | HTTP Status |
|----------|-----------|-------------|-------------|
| Query parsing fails | `ParseError` | `Error::parse_error(msg)` | 400 Bad Request |
| Key parsing fails | `ParseError` | `Error::parse_error(msg)` | 400 Bad Request |
| Asset not found | `KeyNotFound` | `Error::key_not_found(&query)` | 404 Not Found |
| Recipe not found | `KeyNotFound` | `Error::key_not_found(&key)` | 404 Not Found |
| Asset evaluation fails | `ExecutionError` | Propagated from AssetManager | 500 Internal Server Error |
| Recipe resolution fails | `RecipeError` | Propagated from AsyncRecipeProvider | 422 Unprocessable Entity |
| Metadata deserialization fails | `TypeError` | `Error::general_error(msg)` | 400 Bad Request |
| WebSocket client parse fails | `ParseError` | `Error::parse_error(msg)` | Close connection |
| AssetManager::set not implemented | `NotImplemented` | `Error::not_implemented(msg)` | 501 Not Implemented |

### Error Propagation

```rust
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    // Parse query
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Get asset manager
    let asset_manager = env.get_asset_manager();

    // Get asset (may trigger evaluation)
    match asset_manager.get(&query).await {
        Ok((data, metadata)) => {
            BinaryResponse { data, metadata }.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve asset");
            response.into_response()
        }
    }
}
```

**Pattern:** Use `match` for error handling (not `?` operator), convert `Error` to `ErrorDetail` via `error_to_detail()`, return `ApiResponse::error()`.

**No unwrap/expect:** All error paths are explicit, no panics in handlers.

## Serialization Strategy

### Reuse api_core Types

**All serialization handled by existing `api_core` types:**

- `ApiResponse<T>` - JSON responses (status, result, message, error)
- `BinaryResponse` - Binary data with metadata in headers
- `DataEntry` - Unified data/metadata (CBOR/bincode/JSON via `serialize_data_entry()`)
- `NotificationMessage` - JSON WebSocket messages (`#[serde(tag = "type")]`)

### Format Selection

**For entry endpoints** (`GET/POST /entry`), use existing `api_core::format` helpers:

```rust
// GET - select format from query param or Accept header
let format = select_format(params.get("format").map(|s| s.as_str()), &headers);

// Serialize DataEntry
let bytes = serialize_data_entry(&entry, format)?;

// POST - determine format from query param or Content-Type header
let format = if let Some(format_str) = params.get("format") {
    // parse format string
} else {
    format_from_content_type(&headers).unwrap_or(SerializationFormat::Cbor)
};

// Deserialize DataEntry
let entry = deserialize_data_entry(&body, format)?;
```

**Default format:** CBOR (most efficient, binary-safe)

### Round-trip Compatibility

**DataEntry serialization is symmetric:**
- Serialize → Deserialize → Serialize produces identical output
- JSON format uses base64 for binary data (handled by `api_core::response::base64_serde`)
- CBOR and bincode handle binary data natively

**No new Serde derives needed** - all types reuse existing serialization.

## Concurrency Considerations

### Thread Safety

**Environment sharing:**
- `EnvRef<E> = Arc<E>` - shared across all handlers and WebSocket tasks
- Each handler/task gets cheap Arc clone
- Environment services (AssetManager, AsyncRecipeProvider) are already thread-safe

**WebSocket subscription state:**
- `Arc<RwLock<HashMap<...>>>` for shared subscription map (optional, if needed)
- `RwLock` chosen over `Mutex` - read-heavy workload (many lookups, fewer modifications)
- Each WebSocket connection runs in its own task (independent)

**Asset notifications:**
- `tokio::sync::mpsc` channels for per-client notifications
- AssetManager provides `tokio::sync::broadcast` for fan-out (reuse existing mechanism)
- No manual synchronization needed (channels handle it)

### WebSocket Concurrency Pattern

```rust
async fn websocket_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query_path): Path<String>,
    State(env): State<EnvRef<E>>,  // Arc clone
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        // env moved into closure (Arc clone is cheap)
        handle_socket(socket, query_path, env).await
    })
}

async fn handle_socket<E: Environment>(
    mut socket: WebSocket,
    query_path: String,
    env: EnvRef<E>,  // Each task owns Arc clone
) {
    // Each WebSocket connection is independent
    // Parse query, subscribe to AssetRef, forward notifications
}
```

**No shared state between connections** (unless explicit subscription tracking is needed).

**Send + Sync bounds:** All types satisfy `Send + Sync` (Environment trait requirement).

## Compilation Validation

### Expected to compile: Yes

**Checklist:**
- [x] All types have concrete implementations (no missing trait impls)
- [x] All function signatures are complete and consistent
- [x] Generic bounds are minimal (`E: Environment`)
- [x] No `unwrap()` or `expect()` in handler signatures
- [x] All imports are documented (axum, liquers-core, api_core)
- [x] Follows existing Store API pattern exactly
- [x] Reuses all api_core types (no duplication)

**Verification:**
```bash
cargo check -p liquers-axum --all-features
```

**Expected result:** Compiles successfully after implementing all modules.

**Potential issues:** None (design aligns with existing patterns, no new dependencies).

## References to liquers-patterns.md

**Alignment check:**

- [x] **Crate dependencies:** One-way flow (liquers-axum depends on liquers-core, not reverse)
- [x] **No ExtValue extensions:** No new value types (correct for API layer)
- [x] **No command registration:** API layer doesn't register commands (correct)
- [x] **Async default:** All handlers and WebSocket code is async
- [x] **Error handling:** Uses `liquers_core::error::Error` exclusively (no thiserror/anyhow)
- [x] **Generic over Environment:** Follows existing pattern from Store API
- [x] **Reuse api_core:** All response types reused (no duplication)
- [x] **Builder pattern:** Matches StoreApiBuilder pattern exactly

**Additional patterns followed:**
- Handler signature pattern: `State<EnvRef<E>>`, `Path(...)`, return `Response`
- Error conversion: `error_to_detail()` at handler boundary
- Format negotiation: `select_format()`, `serialize_data_entry()`
- Route registration: `Router::route()` with method chaining

**No deviations from established patterns.**
