//! Example egui application demonstrating `AppRunner` with UIPayload.
//!
//! Uses `DefaultEnvironment<Value, SimpleUIPayload>` and registers all commands
//! (including lui commands) via the `register_all_commands!` macro.
//!
//! Run with: `cargo run --example ui_payload_app -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::{
    AppRunner, AppState, DirectAppState, ElementSource,
    UIContext, app_message_channel, render_element, try_sync_lock,
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
    runner: AppRunner<DefaultEnvironment<Value, SimpleUIPayload>>,
    ui_context: UIContext,
    _runtime: tokio::runtime::Runtime,
}

impl PayloadApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

        let (app_state, runner, ui_context) = runtime.block_on(async {
            // 1. Create payload-aware environment and register commands.
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
            env.with_trivial_recipe_provider();
            setup_commands(&mut env).expect("Failed to register commands");

            // 2. Create AppState with a root node (pending evaluation).
            let mut direct_state = DirectAppState::new();
            let _root_handle = direct_state
                .add_node(None, 0, ElementSource::Query("hello".to_string()))
                .expect("Failed to add root node");

            // 3. Wrap AppState in Arc<Mutex>.
            let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(direct_state));

            // 4. Create message channel.
            let (msg_tx, msg_rx) = app_message_channel();

            // 5. Create UIContext for rendering.
            let ui_context = UIContext::new(app_state.clone(), msg_tx.clone());

            // 6. Create EnvRef and AppRunner.
            let envref = env.to_ref();
            let runner = AppRunner::new(envref, msg_rx, msg_tx);

            (app_state, runner, ui_context)
        });

        Self {
            app_state,
            runner,
            ui_context,
            _runtime: runtime,
        }
    }
}

impl eframe::App for PayloadApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Run AppRunner (processes messages, auto-evaluates pending, polls results).
        self._runtime.block_on(async {
            if let Err(e) = self.runner.run(&self.app_state).await {
                eprintln!("[AppRunner] Error: {}", e);
            }
        });

        // Request repaint if there are in-flight evaluations.
        if self.runner.has_evaluating() {
            ctx.request_repaint();
        }

        // 2. Render UI.
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Liquers UI Payload App");
            ui.separator();

            // Render all root elements using extract-render-replace.
            let roots = match try_sync_lock(&self.app_state) {
                Ok(state) => state.roots(),
                Err(_) => {
                    ui.spinner();
                    ctx.request_repaint();
                    return;
                }
            };
            for handle in roots {
                render_element(ui, handle, &self.ui_context);
            }

            ui.separator();
            if ui.button("Re-evaluate hello").clicked() {
                let first_root = match try_sync_lock(&self.app_state) {
                    Ok(state) => state.roots().first().cloned(),
                    Err(_) => None,
                };
                if let Some(root) = first_root {
                    self.ui_context.submit_query(root, "hello");
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
