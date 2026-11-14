/// Basic liquers-axum example server
///
/// This example demonstrates how to:
/// - Create an environment with a file-based store
/// - Optionally audit the store on start (`LIQUERS_STARTUP_AUDIT=1`)
/// - Serve it with `LiquersServerBuilder`: every standard API under `/liquer`, the store's `web/`
///   folder at `/liquer/web/`, and `/` redirecting there
/// - Stop gracefully on Ctrl-C (requests get 5 s to finish)
///
/// Usage:
///   cargo run -p liquers-axum --example basic_server
///   LIQUERS_STARTUP_AUDIT=1 cargo run -p liquers-axum --example basic_server
///
/// Then test with:
///   curl http://localhost:3000/liquer/q/text-Hello
///   curl http://localhost:3000/liquer/api/store/keys
use std::sync::Arc;

use std::time::Duration;

use liquers_axum::{LiquersServerBuilder, ServerConfig};
use liquers_core::store::AsyncFileStore;
use liquers_core::{
    assets::{AssetManager, AuditMode},
    command_metadata::CommandKey,
    commands::CommandArguments,
    context::{Context, SimpleEnvironment},
    environment_builder::EnvironmentBuilder,
    error::Error,
    query::Key,
    value::Value,
};

// Register commands with the environment
fn register_commands(
    cr: &mut liquers_core::commands::CommandRegistry<SimpleEnvironment<Value>>,
) -> Result<(), Error> {
    use liquers_core::command_metadata::ArgumentInfo;

    // Register the 'text' command - creates a text value from the given string
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
        .with_doc("Create a text value from the given string")
        .with_argument(ArgumentInfo::any_argument("text"));

    Ok(())
}

#[tokio::main]
async fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("Starting liquers-axum basic server...");

    // Create a simple environment with file-based storage
    let store_path = std::env::var("LIQUERS_STORE_PATH").unwrap_or_else(|_| ".".to_string());
    println!("Using store path: {}", store_path);

    let async_store = AsyncFileStore::new(&store_path, &Key::new());

    // The builder is the recommended construction path: it configures services, freezes the
    // command registry, and returns an `EnvRef` whose asset manager is already started — so the
    // first query cannot race environment initialization.
    let mut builder = EnvironmentBuilder::<Value>::new().with_async_store(Arc::new(async_store));

    register_commands(&mut builder.command_registry).expect("Failed to register commands");

    let env_ref = builder.build().expect("Failed to build environment");

    // Optional startup audit. The default dependency policy trusts what it loads: a stored value
    // whose upstream changed between runs (a command upgraded, a file edited) is served until
    // something touches that upstream. Auditing the store on start reads each stored value's
    // metadata once and expires what no longer holds, so the server starts consistent. See
    // specs/guides/DEPENDENCY_CONSISTENCY_GUIDE.md.
    if std::env::var("LIQUERS_STARTUP_AUDIT").is_ok_and(|v| v == "1") {
        match env_ref
            .get_asset_manager()
            .trigger_dependency_audit_store(&Key::new(), AuditMode::Expire)
            .await
        {
            Ok(report) => println!(
                "Startup audit: {} stored values checked, {} expired",
                report.checked.len(),
                report.expired.len()
            ),
            Err(e) => eprintln!("Startup audit failed: {}", e),
        }
    }

    // Every standard API under /liquer, the web folder and the index redirect. Port 3000 on this
    // machine only; `host: "0.0.0.0"` would expose it to the network (there is no authentication).
    let config = ServerConfig {
        port: 3000,
        ..ServerConfig::default()
    };
    let server = LiquersServerBuilder::from_config(config)
        .bind(env_ref)
        .await
        .expect("Failed to bind the server");

    println!("Server listening on http://{}", server.local_addr());
    println!("\nExample requests:");
    println!("  curl http://localhost:3000/liquer/q/text-Hello");
    println!("  curl http://localhost:3000/liquer/api/store/keys");

    tokio::signal::ctrl_c().await.ok();
    println!("Stopping...");
    server.shutdown(Duration::from_secs(5)).await;
}
