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
use liquers_axum::{AssetsApiBuilder, QueryApiBuilder, RecipesApiBuilder, StoreApiBuilder};
use liquers_core::store::AsyncFileStore;
use liquers_core::{
    command_metadata::CommandKey,
    commands::CommandArguments,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    query::Key,
    value::Value,
};

/// Register example commands for demonstration
fn register_commands(mut env: SimpleEnvironment<Value>) -> Result<SimpleEnvironment<Value>, Error> {
    let cr = &mut env.command_registry;

    // Register 'text' command - creates a text value from a string
    let key = CommandKey::new_name("text");
    let metadata = cr.register_command(
        key,
        |_state, args: CommandArguments<_>, _context: Context<_>| {
            let text: String = args.get(0, "text")?;
            Ok(Value::from(text))
        },
    )?;
    metadata
        .with_label("Text")
        .with_doc("Create a text value from the given string");

    // Register 'upper' command - converts text to uppercase
    let key = CommandKey::new_name("upper");
    let metadata = cr.register_command(
        key,
        |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
            let text = state.try_into_string()?;
            Ok(Value::from(text.to_uppercase()))
        },
    )?;
    metadata
        .with_label("Uppercase")
        .with_doc("Convert input text to uppercase");

    // Register 'reverse' command - reverses text
    let key = CommandKey::new_name("reverse");
    let metadata = cr.register_command(
        key,
        |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
            let text = state.try_into_string()?;
            Ok(Value::from(text.chars().rev().collect::<String>()))
        },
    )?;
    metadata
        .with_label("Reverse")
        .with_doc("Reverse the input text");

    // Register 'count' command - counts length
    let key = CommandKey::new_name("count");
    let metadata = cr.register_command(
        key,
        |state: &_, _args: CommandArguments<_>, _context: Context<_>| {
            let text = state.try_into_string()?;
            Ok(Value::from(text.len().to_string()))
        },
    )?;
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

    let async_store = AsyncFileStore::new(&store_path, &Key::new());

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

    // Build Assets API router (with WebSocket support)
    let assets_router = AssetsApiBuilder::new("/liquer/api/assets")
        .with_websocket_path("/liquer/api/assets/ws")
        .build();

    // Build Recipes API router
    let recipes_router = RecipesApiBuilder::new("/liquer/api/recipes").build();

    // Compose routers into main application
    let app = axum::Router::new()
        .route("/", axum::routing::get(|| async { help_text() }))
        .merge(query_router)
        .merge(store_router)
        .merge(assets_router)
        .merge(recipes_router)
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
"#
    .to_string()
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
