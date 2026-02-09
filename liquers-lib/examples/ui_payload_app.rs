//! Example egui application demonstrating `evaluate_immediately` with UIPayload.
//!
//! Uses `DefaultEnvironment<Value, SimpleUIPayload>` and registers all commands
//! (including lui commands) via the `register_all_commands!` macro.
//!
//! Run with: `cargo run --example ui_payload_app -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::{
    AppState, AssetViewElement, DirectAppState, ElementSource, UIHandle, render_element,
};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

// Required by register_command! and register_all_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// A command that returns "Hello from payload app!".
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from payload app!"))
}

/// Register all commands for this app. Separate function to avoid ? inside async block.
fn setup_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    liquers_lib::register_all_commands!(cr)?;
    Ok(())
}

// ─── eframe App ─────────────────────────────────────────────────────────────

struct PayloadApp {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    envref: EnvRef<DefaultEnvironment<Value, SimpleUIPayload>>,
    _runtime: tokio::runtime::Runtime,
}

impl PayloadApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

        let (app_state_arc, envref) = runtime.block_on(async {
            // 1. Create payload-aware environment and register commands.
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
            env.with_trivial_recipe_provider();
            setup_commands(&mut env).expect("Failed to register commands");

            // 2. Create AppState with a root node.
            let mut app_state = DirectAppState::new();
            let root_handle = app_state
                .add_node(None, 0, ElementSource::Query("hello".to_string()))
                .expect("Failed to add root node");

            // 3. Wrap AppState before creating payload.
            let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(app_state));

            // 4. Create EnvRef.
            let envref = env.to_ref();

            // 5. Evaluate the root node using evaluate_immediately with payload.
            let payload = SimpleUIPayload::new(app_state_arc.clone())
                .with_handle(root_handle);
            let asset_ref = envref
                .evaluate_immediately("hello", payload)
                .await
                .expect("Failed to evaluate hello query");
            let state = asset_ref
                .get()
                .await
                .expect("Failed to get evaluation result");

            // 6. Set the resulting element in app_state.
            let value = Arc::new((*state.data).clone());
            let element = Box::new(AssetViewElement::new_value(
                "Hello".to_string(),
                value,
            ));
            {
                let mut locked = app_state_arc.lock().await;
                locked
                    .set_element(root_handle, element)
                    .expect("Failed to set element");
            }

            (app_state_arc, envref)
        });

        Self {
            app_state: app_state_arc,
            envref,
            _runtime: runtime,
        }
    }

    /// Evaluate a query for a given handle asynchronously.
    fn evaluate_node(&self, handle: UIHandle, query: &str) {
        let envref = self.envref.clone();
        let app_state = self.app_state.clone();
        let query = query.to_string();

        self._runtime.spawn(async move {
            let payload = SimpleUIPayload::new(app_state.clone())
                .with_handle(handle);
            let asset_ref = envref
                .evaluate_immediately(&query, payload)
                .await;
            match asset_ref {
                Ok(asset_ref) => {
                    match asset_ref.get().await {
                        Ok(state) => {
                            let value = Arc::new((*state.data).clone());
                            let element = Box::new(AssetViewElement::new_value(
                                "Result".to_string(),
                                value,
                            ));
                            let mut locked = app_state.lock().await;
                            if let Err(e) = locked.set_element(handle, element) {
                                eprintln!("Failed to set element: {}", e);
                            }
                        }
                        Err(e) => eprintln!("Failed to get result: {}", e),
                    }
                }
                Err(e) => eprintln!("Failed to evaluate: {}", e),
            }
        });
    }
}

impl eframe::App for PayloadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Liquers UI Payload App");
            ui.separator();

            // Render all root elements using extract-render-replace.
            let roots = {
                let state = self.app_state.blocking_lock();
                state.roots()
            };
            for handle in roots {
                render_element(ui, handle, &self.app_state);
            }

            ui.separator();
            if ui.button("Re-evaluate hello").clicked() {
                let first_root = {
                    let state = self.app_state.blocking_lock();
                    state.roots().first().cloned()
                };
                if let Some(root) = first_root {
                    self.evaluate_node(root, "hello");
                }
            }
        });
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Liquers UI Payload App",
        options,
        Box::new(|cc| Ok(Box::new(PayloadApp::new(cc)))),
    )
}
