/// Basic liquers-axum example server
///
/// This example demonstrates how to:
/// - Create a SimpleEnvironment with a file-based store
/// - Build Query API and Store API routers
/// - Compose them into a single Axum application
/// - Run the server on localhost:3000
///
/// Usage:
///   cargo run -p liquers-axum --example basic_server
///
/// Then test with:
///   curl http://localhost:3000/liquer/q/text-Hello
///   curl http://localhost:3000/liquer/api/store/keys

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

// Register commands with the environment
fn register_commands(mut env: SimpleEnvironment<Value>) -> Result<SimpleEnvironment<Value>, Error> {
    use liquers_core::command_metadata::ArgumentInfo;

    let cr = &mut env.command_registry;

    // Register the 'text' command - creates a text value from the given string
    let key = CommandKey::new_name("text");
    let metadata = cr.register_command(key, |_state, args: CommandArguments<_>, _context: Context<_>| {
        let text: String = args.get(0, "text")?;
        Ok(Value::from(text))
    })?;

    metadata
        .with_label("Text")
        .with_doc("Create a text value from the given string")
        .with_argument(ArgumentInfo::any_argument("text"));

    Ok(env)
}

#[tokio::main]
async fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("Starting liquers-axum basic server...");

    // Create a simple environment with file-based storage
    let store_path = std::env::var("LIQUERS_STORE_PATH").unwrap_or_else(|_| ".".to_string());
    println!("Using store path: {}", store_path);

    let file_store = FileStore::new(&store_path, &Key::new());
    let async_store = AsyncStoreWrapper(file_store);

    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(async_store));

    // Register commands
    let env = register_commands(env).expect("Failed to register commands");

    let env_ref = env.to_ref();

    // Build Query API router (GET/POST /q/{*query})
    let query_router = QueryApiBuilder::new("/liquer/q").build();

    // Build Store API router with all endpoints
    let store_router = StoreApiBuilder::new("/liquer/api/store")
        .with_destructive_gets() // Enable GET-based destructive operations for demo
        .build();

    // Compose routers into main application
    let app = axum::Router::new()
        .route("/", axum::routing::get(|| async { 
            "Liquers API Server\n\nEndpoints:\n  GET  /liquer/q/{*query} - Execute query\n  POST /liquer/q/{*query} - Execute query with JSON body\n  /liquer/api/store/* - Store API endpoints\n" 
        }))
        .merge(query_router)
        .merge(store_router)
        .with_state(env_ref);

    // Bind and serve
    let addr = "0.0.0.0:3000";
    println!("Server listening on http://{}", addr);
    println!("\nExample requests:");
    println!("  curl http://localhost:3000/liquer/q/text-Hello");
    println!("  curl http://localhost:3000/liquer/api/store/keys");
    println!("  curl -X PUT -d 'test data' http://localhost:3000/liquer/api/store/data/test.txt");
    println!("\nPress Ctrl+C to stop\n");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
