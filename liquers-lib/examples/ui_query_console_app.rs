//! Query Console example app with YAML-driven root UISpecElement.
//!
//! Demonstrates:
//! - Root creation via query (`submit_root_query`) — no programmatic element setup
//! - OpenDAL filesystem store at the current working directory
//! - Default recipe provider for store-backed queries
//! - QueryConsoleElement created via menu action
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
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, UIContext,
    render_element, try_sync_lock,
};
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
        query: "main_spec/ns-lui/query_console/ns-lui/add-child"
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
      action: null
layout: !windows {}
init:
  - "main_spec/ns-lui/query_console/ns-lui/add-child"
"#;

// ─── eframe App ──────────────────────────────────────────────────────────────

struct QueryConsoleApp {
    ui_context: UIContext,
    app_runner: AppRunner<CommandEnvironment>,
    _runtime: tokio::runtime::Runtime,
    initialized: bool,
}

impl QueryConsoleApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let (ui_context, app_runner) = runtime.block_on(async {
            // 1. Setup environment
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();

            // 2. Add OpenDAL filesystem store at current working directory
            let fs_op = opendal::Operator::new(
                opendal::services::Fs::default().root(".")
            )
            .expect("Failed to create OpenDAL FS operator")
            .finish();
            let store: Box<dyn AsyncStore> = Box::new(
                AsyncOpenDALStore::new(fs_op, Key::new())
            );
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

            // 6. Create UIContext and AppRunner
            let (msg_tx, msg_rx) = app_message_channel();
            let ui_context = UIContext::new(app_state, msg_tx.clone());
            let app_runner = AppRunner::new(envref, msg_rx, msg_tx);

            Ok::<_, Error>((ui_context, app_runner))
        }).expect("Failed to setup app");

        Self {
            ui_context,
            app_runner,
            _runtime: runtime,
            initialized: false,
        }
    }
}

impl eframe::App for QueryConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app_state = self.ui_context.app_state().clone();

        // Submit root query on first frame
        if !self.initialized {
            self.initialized = true;
            self.ui_context.submit_root_query(
                "main_spec/ns-lui/ui_spec/ns-lui/add-child"
            );
        }

        // Process messages and poll evaluations
        let _ = self._runtime.block_on(async {
            self.app_runner.run(&app_state).await
        });

        if self.app_runner.has_evaluating() {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let roots = match try_sync_lock(&app_state) {
                Ok(state) => state.roots(),
                Err(e) => {
                    ui.label(format!("Error: {}", e));
                    ui.spinner();
                    return;
                }
            };

            if roots.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            } else {
                for handle in roots {
                    render_element(ui, handle, &self.ui_context);
                }
            }
        });
    }
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    println!("Starting Query Console App...");
    println!("Root element will be created via query (submit_root_query)");
    println!("Use File > New Console (Ctrl+N) to open query consoles");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("Query Console App"),
        ..Default::default()
    };

    eframe::run_native(
        "Query Console App",
        options,
        Box::new(|cc| Ok(Box::new(QueryConsoleApp::new(cc)))),
    )
}
