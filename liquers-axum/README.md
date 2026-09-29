# liquers-axum

Production-ready HTTP REST API library for Liquers, built with Axum.

## Features

- **Composable API Builders**: Generic over `Environment<E>` for maximum flexibility
- **Query Execution API**: GET/POST `/q/{*query}` endpoints with 30-second timeout
- **Complete Store API**: Data, metadata, directory operations, unified entries, and upload
- **Multi-Format Support**: CBOR, bincode, and JSON serialization for atomic data+metadata operations
- **Builder Pattern**: Easy configuration and composition
- **Type-Safe**: Leverages Rust's type system for compile-time safety
- **No Dependencies on liquers-lib**: Uses only `liquers-core` and `liquers-store`

## Quick Start

```rust
use liquers_axum::{QueryApiBuilder, StoreApiBuilder};
use liquers_core::{
    context::{Environment, SimpleEnvironment},
    query::Key,
    store::AsyncFileStore,
    value::Value,
};

#[tokio::main]
async fn main() {
    // Create environment with file-based storage
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncFileStore::new(".", &Key::new())));
    let env_ref = env.to_ref();

    // Build API routers
    let query_router = QueryApiBuilder::new("/liquer/q").build();
    let store_router = StoreApiBuilder::new("/liquer/api/store").build();

    // Compose and serve
    let app = axum::Router::new()
        .merge(query_router)
        .merge(store_router)
        .with_state(env_ref);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

## API Endpoints

The full specification, with result shapes and status codes, is
[`specs/reference/WEB_API_SPECIFICATION.md`](../specs/reference/WEB_API_SPECIFICATION.md).

### Query Execution API

- **GET `/q/{*query}`** - Execute query, return binary result
- **POST `/q/{*query}`** - Execute query with optional JSON body
- `QueryApiBuilder::with_timeout(Duration)` bounds the wait (default 30 s)

### Assets API (`AssetsApiBuilder`)

Two families: `q/…` takes any query (`parse_query`), `key/…` takes a bare key (`parse_key`, no
`-R/`). Reads (`data`, `entry`) evaluate and wait; `submit` starts an evaluation and returns its
status at once; observe routes (`info`, `metadata`, `version`, `contains`, `recover`, `listdir`)
never evaluate.

- **GET `q/data|entry/{*query}`**, **POST|GET `q/submit/{*query}`**, **GET `q/info|metadata|version/{*query}`**, **POST `q/cancel/{*query}`**
- **GET `key/data|entry/{*key}`**, **POST|GET `key/submit/{*key}`**, **GET `key/info|metadata|version|contains|recover/{*key}`**, **GET `key/listdir[/{*key}][?deep=true]`**
- **POST `key/data|entry/{*key}`** (201) - write a `Source` / `Override`; only `type_identifier`, `data_format`, `media_type`, `title`, `description` are client-settable
- **DELETE `key/data|entry/{*key}`** - status-aware remove; **DELETE `key/removedir/{*key}`**; **PUT `key/makedir/{*key}`** (201)
- **POST `key/description|expire|override|cancel/{*key}`**
- **POST `admin/audit[/{*key}]`**, **POST `admin/refresh_command_versions`**
- **WebSocket `ws/q[/{*query}]`, `ws/key[/{*key}]`** - notifications with an `AssetInfo` snapshot; see `examples/WEBSOCKET_EXAMPLE.md`

Builder options:

| Option | Default | Effect |
|---|---|---|
| `read_only()` | off | omit every mutation route and the admin routes (cancel stays) |
| `with_admin(bool)` | `true` | include or omit `admin/…` |
| `with_destructive_gets()` | off | GET forms of the body-less operations (`key/remove`, `key/expire`, …) |
| `with_websocket_path(p)` / `without_websocket()` | `{base}/ws` | move or remove the WebSocket endpoints |
| `with_websocket_limits(WebSocketLimits)` | 64 KiB, 256 | message size and subscriptions per connection |

These switches are a stop-gap: there is no access control yet (`CORE-SESSION-AND-KEY-ACL`).

### Recipes API (`RecipesApiBuilder`, read-only)

- **GET `listdir`**, **GET `data|metadata|entry/{*key}`**, **GET `resolve/{*key}`**

### Store API - Data & Metadata

- **GET `/api/store/data/{*key}`** - Retrieve raw data
- **PUT `/api/store/data/{*key}`** - Store raw data
- **DELETE `/api/store/data/{*key}`** - Delete data
- **GET `/api/store/metadata/{*key}`** - Retrieve metadata as JSON
- **PUT `/api/store/metadata/{*key}`** - Update metadata from JSON

### Store API - Unified Entry (CBOR/bincode/JSON)

- **GET `/api/store/entry/{*key}?format=cbor|bincode|json`** - Get data + metadata
- **PUT `/api/store/entry/{*key}?format=cbor|bincode|json`** - Set data + metadata atomically
- **DELETE `/api/store/entry/{*key}`** - Delete entry

### Store API - Directory Operations

- **GET `/api/store/listdir/{*key}`** - List directory contents
- **GET `/api/store/is_dir/{*key}`** - Check if key is directory
- **GET `/api/store/contains/{*key}`** - Check if key exists
- **GET `/api/store/keys?prefix={prefix}`** - List the keys directly in the prefix directory (root by default)
- **PUT `/api/store/makedir/{*key}`** - Create directory
- **DELETE `/api/store/removedir/{*key}`** - Remove directory

### Store API - Upload

- **POST `/api/store/upload/{*key}`** - Upload files via multipart/form-data

### Store API - Optional GET-based Operations

Enable with `.with_destructive_gets()` on `StoreApiBuilder`:

- **GET `/api/store/remove/{*key}`** - Delete via GET (legacy)
- **GET `/api/store/removedir/{*key}`** - Remove directory via GET (legacy)
- **GET `/api/store/makedir/{*key}`** - Create directory via GET (legacy)

## Configuration

### Enable Destructive GET Operations

```rust
let store_router = StoreApiBuilder::new("/liquer/api/store")
    .with_destructive_gets() // Enable GET-based destructive operations
    .build();
```

### Custom Environment

```rust
use liquers_core::context::Environment;

// Any type implementing Environment<E> can be used
let custom_env_ref: EnvRef<MyEnvironment> = my_env.to_ref();

let query_router = QueryApiBuilder::<MyEnvironment>::new("/q").build();
let store_router = StoreApiBuilder::<MyEnvironment>::new("/store").build();
```

## Examples

Run the basic server example:

```bash
cargo run -p liquers-axum --example basic_server
```

Then test with:

```bash
# Query execution
curl http://localhost:3000/liquer/q/text-Hello

# Store operations
curl http://localhost:3000/liquer/api/store/keys
curl -X PUT -d 'test data' http://localhost:3000/liquer/api/store/data/test.txt
curl http://localhost:3000/liquer/api/store/data/test.txt

# Unified entry (CBOR format)
curl -H "Accept: application/cbor" http://localhost:3000/liquer/api/store/entry/test.txt

# Directory operations
curl http://localhost:3000/liquer/api/store/listdir/
curl -X PUT http://localhost:3000/liquer/api/store/makedir/mydir
```

## Format Selection

The unified entry endpoints support three serialization formats:

1. **CBOR** (default) - Most efficient for binary data
2. **Bincode** - Fast binary format
3. **JSON** - Human-readable, uses base64 for data field

Format selection priority:
1. `?format=cbor|bincode|json` query parameter
2. `Accept` header (for GET) or `Content-Type` (for PUT)
3. Default: CBOR

## Error Handling

All endpoints return consistent error responses:

```json
{
  "status": "ERROR",
  "message": "Failed to retrieve data",
  "error": {
    "type": "KeyNotFound",
    "message": "Key 'test.txt' not found",
    "key": "test.txt"
  }
}
```

HTTP status codes follow the error type (specification §3.1):
- `200 OK` / `201 Created` - Success
- `400 Bad Request` - Parse/parameter errors
- `404 Not Found` - Key not found, or a query not cached (`NotAvailable`)
- `409 Conflict` - The operation does not apply to the asset's current status (`StatusConflict`)
- `422 Unprocessable Entity` - Conversion/serialization errors
- `500 Internal Server Error` - Execution and store errors
- `501 Not Implemented` - `NotSupported`

## Testing

Run unit tests:

```bash
cargo test -p liquers-axum
```

## License

Same as parent project.
