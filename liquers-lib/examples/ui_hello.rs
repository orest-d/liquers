//! Minimal egui application demonstrating the Liquers UI system.
//!
//! Shows "Hello, World!" rendered through the full UI pipeline:
//! environment setup -> command registration -> query evaluation ->
//! AssetViewElement -> extract-render-replace rendering.
//!
//! Run with: `cargo run --example ui_hello -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, Environment, SimpleEnvironment};
use liquers_core::error::Error;
use liquers_core::interpreter::evaluate;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::ui::{
    AppState, AssetViewElement, DirectAppState, ElementSource, render_element,
};
use liquers_lib::value::Value;

// Required by register_command! macro.
type CommandEnvironment = SimpleEnvironment<Value>;

/// A command that returns "Hello, World!".
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello, World!"))
}

// ─── eframe App ─────────────────────────────────────────────────────────────

struct HelloApp {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    // Keep the runtime alive so background tasks (if any) can run.
    _runtime: tokio::runtime::Runtime,
}

impl HelloApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Create tokio runtime inside the eframe callback — this ensures
        // the runtime context is available during the full app lifecycle.
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // All environment/evaluation setup must happen inside block_on
        // because SimpleEnvironment::new() internally calls tokio::spawn
        // (via DefaultAssetManager).
        let app_state_arc = runtime.block_on(async {
            // 1. Set up environment and register the hello command.
            let mut env = SimpleEnvironment::<Value>::new();
            let cr = &mut env.command_registry;
            register_command!(cr, fn hello(state) -> result)
                .expect("Failed to register hello command");

            let envref = env.to_ref();

            // 2. Create AppState with a root node.
            let mut app_state = DirectAppState::new();
            let root_handle = app_state
                .add_node(None, 0, ElementSource::Query("hello".to_string()))
                .expect("Failed to add root node");

            // 3. Evaluate the query and create an AssetViewElement with the result.
            let state = evaluate(envref, "hello", None)
                .await
                .expect("Failed to evaluate hello query");

            let value = Arc::new((*state.data).clone());
            let element = Box::new(AssetViewElement::new_value("Hello".to_string(), value));
            app_state
                .set_element(root_handle, element)
                .expect("Failed to set element");

            // 4. Wrap AppState in Arc<tokio::sync::Mutex>.
            let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(app_state));
            app_state_arc
        });

        Self {
            app_state: app_state_arc,
            _runtime: runtime,
        }
    }
}

impl eframe::App for HelloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Liquers UI Hello");
            ui.separator();

            // Render all root elements using extract-render-replace.
            let roots = {
                let state = self.app_state.blocking_lock();
                state.roots()
            };
            for handle in roots {
                render_element(ui, handle, &self.app_state);
            }
        });
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Liquers UI Hello",
        options,
        Box::new(|cc| Ok(Box::new(HelloApp::new(cc)))),
    )
}
