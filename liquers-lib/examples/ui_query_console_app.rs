//! Query Console example app with YAML-driven root UISpecElement.
//!
//! Demonstrates:
//! - Root creation via query (`submit_root_query`) — no programmatic element setup
//! - OpenDAL filesystem store at the current working directory
//! - Default recipe provider for store-backed queries
//! - QueryConsoleElement created via menu action
//! - The reusable window: `NativeDriver` and `liquers_lib::egui::app::run_window`, where *Quit*
//!   closes the window
//!
//! Run with: `cargo run --example ui_query_console_app -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::AsyncStore;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::egui::app::{run_window, WindowCloser, WindowOptions};
use liquers_lib::ui::{app_message_channel, AppState, DirectAppState, NativeDriver};
use liquers_lib::value::Value;
use liquers_macro::register_command;
use liquers_store::opendal_store::AsyncOpenDALStore;

// Required by register_all_commands! / register_lui_commands! macros
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Commands ────────────────────────────────────────────────────────────────

/// Return the main window YAML spec.
fn main_spec(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(MAIN_SPEC_YAML))
}

// ─── YAML Specification ──────────────────────────────────────────────────────

const MAIN_SPEC_YAML: &str = r#"
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: New Console
      shortcut: Ctrl+N
      action:
        query: "ns-lui/query_console/q/add-child"
#        query: "main_spec/q/ns-lui/query_console/add-child"
    - !separator null
    - !button
      label: Quit
      shortcut: Ctrl+Q
      action: quit
  - !menu
    label: Help
    items:
    - !button
      label: About
      action:
        query: main_spec/q/ns-lui/add-child
layout: windows
#init:
#  - "main_spec/q/ns-lui/query_console/add-child"
"#;

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> Result<(), Error> {
    println!("Starting Query Console App...");
    println!("Root element will be created via query (submit_root_query)");
    println!("Use File > New Console (Ctrl+N) to open query consoles, File > Quit to close");

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| Error::general_error(format!("cannot start a Tokio runtime: {e}")))?;

    let mut driver = runtime.block_on(async {
        // 1. Setup environment
        let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();

        // 2. Add OpenDAL filesystem store at current working directory
        let fs_op = opendal::Operator::new(opendal::services::Fs::default().root("."))
            .map_err(|e| Error::general_error(format!("cannot open the working directory: {e}")))?
            .finish();
        let store: Box<dyn AsyncStore> = Box::new(AsyncOpenDALStore::new(fs_op, Key::new()));
        env.with_async_store(store);

        // 3. Add default recipe provider
        env.with_default_recipe_provider();

        // 4. Register commands
        let envref = {
            let cr = env.get_mut_command_registry();
            register_command!(cr, fn main_spec(state) -> result)?;
            liquers_lib::register_all_commands!(cr)?;
            env.to_ref()
        };

        // 5. Create empty AppState — root will be created by query
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(DirectAppState::new()));

        // 6. The driver owns the runner; the window ticks it every frame.
        let (sender, receiver) = app_message_channel();
        Ok::<_, Error>(NativeDriver::new(envref, app_state, receiver, sender))
    })?;

    driver
        .ui_context()
        .submit_root_query("main_spec/ns-lui/ui_spec/add-child");

    let mut options = WindowOptions::new("Query Console App");
    options.inner_size = [900.0, 700.0];
    run_window(options, &mut driver, runtime.handle(), &WindowCloser::default(), None)
}
