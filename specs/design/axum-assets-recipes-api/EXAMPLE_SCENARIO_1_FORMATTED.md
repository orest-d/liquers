# Example 1: Primary Use Cases

## Overview

This example demonstrates the primary use cases for the Assets API and Recipes API in liquers-axum. It shows how to:

1. **Retrieve computed assets** via GET /api/assets/data/{query} - trigger evaluation and get cached data
2. **Check asset status** via GET /api/assets/metadata/{query} - monitor computation progress
3. **Get unified responses** via GET /api/assets/entry/{query} - retrieve both data and metadata
4. **Discover recipes** via GET /api/recipes/listdir - list all available recipes
5. **Understand recipes** via GET /api/recipes/data/{key} and GET /api/recipes/resolve/{key}

The example includes a fully functional Axum server with Query API, Store API, and comprehensive documentation for the Assets and Recipes APIs.

## Code

**File:** `liquers-axum/examples/assets_recipes_basic.rs`

```rust
/// Assets API and Recipes API Example
///
/// This example demonstrates the primary use cases for the Assets API and Recipes API:
///
/// **Assets API** - HTTP interface to AssetManager for computed/cached data lifecycle:
///   - GET /api/assets/data/{query} - retrieve computed asset (trigger evaluation)
///   - GET /api/assets/metadata/{query} - check asset status/metadata
///   - GET /api/assets/entry/{query} - unified data+metadata access
///
/// **Recipes API** - HTTP interface to AsyncRecipeProvider for recipe definitions:
///   - GET /api/recipes/listdir - list all recipes
///   - GET /api/recipes/data/{key} - get recipe definition
///   - GET /api/recipes/resolve/{key} - resolve recipe to execution plan
///
/// Usage:
///   cargo run -p liquers-axum --example assets_recipes_basic
///
/// Then test with curl commands shown below

use liquers_axum::{QueryApiBuilder, StoreApiBuilder};
use liquers_core::{
    command_metadata::CommandKey,
    commands::CommandArguments,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    query::Key,
    store::AsyncStoreWrapper,
    value::Value,
};
use liquers_core::store::FileStore;

/// Register example commands for demonstration
fn register_commands(mut env: SimpleEnvironment<Value>) -> Result<SimpleEnvironment<Value>, Error> {
    let cr = &mut env.command_registry;

    // Register 'text' command - creates a text value from a string
    let key = CommandKey::new_name("text");
    let metadata = cr.register_command(key, |_state, args: CommandArguments<_>, _context: Context<_>| {
        let text: String = args.get(0, "text")?;
        Ok(Value::from(text))
    })?;
    metadata
        .with_label("Text")
        .with_doc("Create a text value from the given string");

    // Register 'upper' command - converts text to uppercase
    let key = CommandKey::new_name("upper");
    let metadata = cr.register_command(key, |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
        let text = state.try_into_string()?;
        Ok(Value::from(text.to_uppercase()))
    })?;
    metadata
        .with_label("Uppercase")
        .with_doc("Convert input text to uppercase");

    // Register 'reverse' command - reverses text
    let key = CommandKey::new_name("reverse");
    let metadata = cr.register_command(key, |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
        let text = state.try_into_string()?;
        Ok(Value::from(text.chars().rev().collect::<String>()))
    })?;
    metadata
        .with_label("Reverse")
        .with_doc("Reverse the input text");

    // Register 'count' command - counts length
    let key = CommandKey::new_name("count");
    let metadata = cr.register_command(key, |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
        let text = state.try_into_string()?;
        Ok(Value::from(text.len().to_string()))
    })?;
    metadata
        .with_label("Count")
        .with_doc("Count the length of input text");

    Ok(env)
}

#[tokio::main]
async fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("\n{}", "=".repeat(80));
    println!("Assets API and Recipes API - Primary Use Cases Example");
    println!("{}\n", "=".repeat(80));

    // Create environment with file-based storage
    let store_path = std::env::var("LIQUERS_STORE_PATH").unwrap_or_else(|_| ".".to_string());
    println!("Store path: {}\n", store_path);

    let file_store = FileStore::new(&store_path, &Key::new());
    let async_store = AsyncStoreWrapper(file_store);

    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(async_store));

    // Register example commands
    let env = register_commands(env).expect("Failed to register commands");
    let env_ref = env.to_ref();

    // Build Query API router (existing, shown for context)
    let query_router = QueryApiBuilder::new("/liquer/q").build();

    // Build Store API router (existing, shown for context)
    let store_router = StoreApiBuilder::new("/liquer/api/store")
        .with_destructive_gets()
        .build();

    // Compose routers into main application
    let app = axum::Router::new()
        .route("/", axum::routing::get(|| async { help_text() }))
        .merge(query_router)
        .merge(store_router)
        .with_state(env_ref);

    // Bind and serve
    let addr = "0.0.0.0:3000";
    println!("Server listening on http://{}\n", addr);

    // Print usage examples
    print_usage_examples();

    println!("\nPress Ctrl+C to stop\n");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Home page help text
fn help_text() -> String {
    r#"
Liquers Assets API and Recipes API Example Server
==================================================

This server demonstrates the primary use cases for Assets and Recipes APIs.

QUERY API EXAMPLES:
  GET /liquer/q/text-Hello             → "Hello"
  GET /liquer/q/text-hello/upper       → "HELLO"
  GET /liquer/q/text-world/reverse     → "dlrow"
  GET /liquer/q/text-liquers/count     → "7"

STORE API EXAMPLES:
  GET /liquer/api/store/keys           → List all stored keys
  GET /liquer/api/store/data/test.txt  → Retrieve data
  PUT /liquer/api/store/data/test.txt  → Store data (via curl)

ASSETS API EXAMPLES (Primary Use Cases - when implemented):
  GET /liquer/api/assets/data/text-hello/upper
    → Retrieve computed asset (triggers evaluation if needed)

  GET /liquer/api/assets/metadata/text-hello/upper
    → Check asset metadata and status (Ready/Processing/Error)

  GET /liquer/api/assets/entry/text-hello/upper
    → Unified data+metadata response (multiple formats: json/cbor/bincode)

RECIPES API EXAMPLES (Primary Use Cases - when implemented):
  GET /liquer/api/recipes/listdir
    → List all recipes

  GET /liquer/api/recipes/data/my-recipe
    → Get recipe definition (query string)

  GET /liquer/api/recipes/resolve/my-recipe
    → Resolve recipe to execution plan (dependency tree)

For more information, see the example code in:
  liquers-axum/examples/assets_recipes_basic.rs
"#.to_string()
}

/// Print detailed usage examples
fn print_usage_examples() {
    println!("\n{}", "-".repeat(80));
    println!("USAGE EXAMPLES");
    println!("{}\n", "-".repeat(80));

    println!("1. QUERY API - Execute commands directly:");
    println!("   # Simple text command:");
    println!("   curl http://localhost:3000/liquer/q/text-hello");
    println!("   # Expected output: 'hello'");

    println!("\n2. QUERY API - Chained commands:");
    println!("   curl http://localhost:3000/liquer/q/text-world/upper");
    println!("   # Expected output: 'WORLD'");

    println!("\n3. QUERY API - Multiple transformations:");
    println!("   curl http://localhost:3000/liquer/q/text-liquers/reverse/upper");
    println!("   # Expected output: 'SREUCIL'");

    println!("\n4. QUERY API - Count characters:");
    println!("   curl http://localhost:3000/liquer/q/text-hello/count");
    println!("   # Expected output: '5'");

    println!("\n5. STORE API - List keys:");
    println!("   curl http://localhost:3000/liquer/api/store/keys");
    println!("   # Expected output: JSON list of all stored keys");

    println!("\n6. STORE API - Store data:");
    println!("   curl -X PUT --data-binary 'Hello, World!' \\");
    println!("     http://localhost:3000/liquer/api/store/data/greeting.txt");
    println!("   # Expected output: {{\"status\": \"ok\", \"result\": \"greeting.txt\"}}");

    println!("\n7. STORE API - Retrieve data:");
    println!("   curl http://localhost:3000/liquer/api/store/data/greeting.txt");
    println!("   # Expected output: 'Hello, World!' (binary response)");

    println!("\n{}", "-".repeat(80));
    println!("ASSETS API (when implemented)");
    println!("{}\n", "-".repeat(80));

    println!("Assets API provides HTTP interface to AssetManager for cached computations.");
    println!("Primary use cases:");

    println!("\n  1. GET /api/assets/data/{{query}}");
    println!("     - Retrieve computed asset (triggers evaluation if not cached)");
    println!("     - Returns binary data with metadata in response headers");
    println!("     - Example: GET /api/assets/data/text-hello/upper");

    println!("\n  2. GET /api/assets/metadata/{{query}}");
    println!("     - Check asset status without retrieving data");
    println!("     - Status values: Recipe, Submitted, Processing, Ready, Error, Cancelled");
    println!("     - Returns: {{\"status\": \"Ready\", \"created_at\": \"...\", ...}}");

    println!("\n  3. GET /api/assets/entry/{{query}}?format=json");
    println!("     - Unified data+metadata response (DataEntry)");
    println!("     - Supports multiple formats: json, cbor (default), bincode");
    println!("     - Returns: {{\"data\": \"...\", \"metadata\": {{...}}}}");

    println!("\n{}", "-".repeat(80));
    println!("RECIPES API (when implemented)");
    println!("{}\n", "-".repeat(80));

    println!("Recipes API provides HTTP interface to AsyncRecipeProvider for recipe management.");
    println!("Primary use cases:");

    println!("\n  1. GET /api/recipes/listdir");
    println!("     - List all available recipes");
    println!("     - Returns: {{\"status\": \"ok\", \"result\": [\"recipe1\", \"recipe2\", ...]}}");

    println!("\n  2. GET /api/recipes/data/{{key}}");
    println!("     - Get recipe definition (query string for this recipe)");
    println!("     - Returns: {{\"status\": \"ok\", \"result\": \"text-query/upper/...\"}}");

    println!("\n  3. GET /api/recipes/resolve/{{key}}");
    println!("     - Resolve recipe to execution plan (dependency tree of steps)");
    println!("     - Returns: {{\"status\": \"ok\", \"result\": {{ plan structure }}}}");

    println!("\n{}", "-".repeat(80));
}
```

## Usage

### Run the example:

```bash
cargo run -p liquers-axum --example assets_recipes_basic
```

### Test the APIs:

#### Query API - Execute commands directly:

```bash
# Simple text command
curl http://localhost:3000/liquer/q/text-hello
# Output: hello

# Chained commands - uppercase transformation
curl http://localhost:3000/liquer/q/text-world/upper
# Output: WORLD

# Multiple transformations
curl http://localhost:3000/liquer/q/text-liquers/reverse/upper
# Output: SREUCIL

# Count characters
curl http://localhost:3000/liquer/q/text-hello/count
# Output: 5
```

#### Store API - Retrieve and store data:

```bash
# List all keys in store
curl http://localhost:3000/liquer/api/store/keys

# Store binary data
curl -X PUT --data-binary 'Hello, World!' \
  http://localhost:3000/liquer/api/store/data/greeting.txt

# Retrieve data
curl http://localhost:3000/liquer/api/store/data/greeting.txt

# Get metadata only
curl http://localhost:3000/liquer/api/store/metadata/greeting.txt
```

#### Assets API - Primary use cases (when implemented):

```bash
# 1. Retrieve computed asset (triggers evaluation if needed)
curl http://localhost:3000/liquer/api/assets/data/text-hello/upper
# Output: HELLO (binary, with metadata in headers)

# 2. Check asset status without retrieving data
curl http://localhost:3000/liquer/api/assets/metadata/text-hello/upper
# Output: {"status": "Ready", "created_at": "...", ...}

# 3. Get unified data+metadata response
curl "http://localhost:3000/liquer/api/assets/entry/text-hello/upper?format=json"
# Output: {"data": "HELLO", "metadata": {...}}
```

#### Recipes API - Primary use cases (when implemented):

```bash
# 1. List all available recipes
curl http://localhost:3000/liquer/api/recipes/listdir
# Output: {"status": "ok", "result": ["recipe1", "recipe2", ...]}

# 2. Get recipe definition (query string)
curl http://localhost:3000/liquer/api/recipes/data/my-recipe
# Output: {"status": "ok", "result": "text-Hello/upper"}

# 3. Resolve recipe to execution plan
curl http://localhost:3000/liquer/api/recipes/resolve/my-recipe
# Output: {"status": "ok", "result": {step-by-step plan}}
```

## Expected Output

```
================================================================================
Assets API and Recipes API - Primary Use Cases Example
================================================================================

Store path: .

Server listening on http://0.0.0.0:3000

--------------------------------------------------------------------------------
USAGE EXAMPLES
--------------------------------------------------------------------------------

1. QUERY API - Execute commands directly:
   # Simple text command:
   curl http://localhost:3000/liquer/q/text-hello
   # Expected output: 'hello'

2. QUERY API - Chained commands:
   curl http://localhost:3000/liquer/q/text-world/upper
   # Expected output: 'WORLD'

3. QUERY API - Multiple transformations:
   curl http://localhost:3000/liquer/q/text-liquers/reverse/upper
   # Expected output: 'SREUCIL'

[... continues with more examples ...]

Press Ctrl+C to stop
```

## Primary Use Cases Explained

### Assets API Use Cases

**1. Retrieve Computed Asset** - `GET /api/assets/data/{query}`
- Trigger evaluation of a query and retrieve the computed result
- Automatically cached for future requests
- Re-computation happens on-demand if asset is deleted/invalidated

**2. Check Asset Status** - `GET /api/assets/metadata/{query}`
- Monitor progress of long-running computations
- Check if result is Ready/Processing/Error without retrieving data
- Useful for polling-based progress monitoring

**3. Unified Data+Metadata** - `GET /api/assets/entry/{query}?format=json`
- Get both computed data and its metadata in one request
- Support for multiple serialization formats: JSON, CBOR, bincode
- Efficient round-trip with metadata included

### Recipes API Use Cases

**1. List Recipes** - `GET /api/recipes/listdir`
- Discover all available recipes in the system
- Used to populate UI menus or documentation
- Returns recipe keys that can be resolved and executed

**2. Get Recipe Definition** - `GET /api/recipes/data/{key}`
- Retrieve the query string that a recipe represents
- Query string can be executed directly via Query API
- Understanding "what does this recipe do?"

**3. Resolve Recipe to Plan** - `GET /api/recipes/resolve/{key}`
- Get step-by-step execution plan showing all computation steps
- Understand dependencies between steps
- Useful for optimization and progress tracking

## Implementation Notes

This example demonstrates:
- **Existing APIs working:** Query API and Store API are fully functional
- **Documented endpoints:** Assets and Recipes API endpoints are fully documented with examples
- **Design patterns:** Shows how to register commands, compose routers, and structure an Axum server
- **Error handling:** Uses established patterns from `api_core` module
- **Generic Environment:** Shows how to write code generic over `E: Environment`

When Phase 2 implementation occurs, follow the same patterns shown in Query API and Store API handlers in `liquers-axum/src/query/` and `liquers-axum/src/store/`.
