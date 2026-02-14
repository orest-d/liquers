//! UISpec demo with menu button triggering a query action.
//!
//! Run with: `cargo run --example ui_spec_demo -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, AppRunner, AppState, DirectAppState, ElementSource, UIContext,
    UIElement, render_element, try_sync_lock,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by register_lui_commands! macro
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Commands ────────────────────────────────────────────────────────────────

/// Return the dashboard YAML spec
fn dashboard(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(DASHBOARD_YAML))
}

/// Return the dashboard2 YAML spec
fn dashboard2(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(DASHBOARD2_YAML))
}

// ─── YAML Specifications ─────────────────────────────────────────────────────

const DASHBOARD_YAML: &str = r#"
menu:
  items:
  - !button
    label: Add Dashboard
    action:
      query: "dashboard/q/ns-lui/add-child"
  - !button
    label: Switch to Dashboard 2
    action:
      query: "dashboard2/ns-lui/ui_spec/q/add-instead"

layout: vertical
"#;

const DASHBOARD2_YAML: &str = r#"
menu:
  items:
  - !button
    label: Add Dashboard 2
    action:
      query: "dashboard2/q/ns-lui/add-child"
  - !button
    label: Switch to Dashboard 1
    action:
      query: "dashboard/ns-lui/ui_spec/q/add-instead"

layout: horizontal
"#;

// ─── eframe App ──────────────────────────────────────────────────────────────

struct SpecDemoApp {
    ui_context: UIContext,
    app_runner: AppRunner<CommandEnvironment>,
    _runtime: tokio::runtime::Runtime,
}

impl SpecDemoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let (ui_context, app_runner) = runtime.block_on(async {
            // 1. Setup environment and register commands
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
            env.with_trivial_recipe_provider();

            let envref = {
                let cr = env.get_mut_command_registry();
                register_command!(cr, fn dashboard(state) -> result)?;
                register_command!(cr, fn dashboard2(state) -> result)?;
                liquers_lib::register_lui_commands!(cr)?;
                env.to_ref()
            };

            // 2. Create AppState with root UISpecElement
            let mut app_state = DirectAppState::new();
            let root_handle = app_state
                .add_node(None, 0, ElementSource::None)
                .expect("Failed to add root node");

            use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
            let spec = UISpec::from_yaml(DASHBOARD_YAML).expect("Failed to parse YAML");
            let mut element = UISpecElement::from_spec("Dashboard".to_string(), spec);
            element.set_handle(root_handle);
            app_state
                .set_element(root_handle, Box::new(element))
                .expect("Failed to set element");

            // 3. Create UIContext and AppRunner
            let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(app_state));
            let (msg_tx, msg_rx) = app_message_channel();
            let ui_context = UIContext::new(app_state_arc.clone(), msg_tx.clone());
            let app_runner = AppRunner::new(envref, msg_rx, msg_tx);

            Ok::<_, Error>((ui_context, app_runner))
        }).expect("Failed to setup app");

        Self {
            ui_context,
            app_runner,
            _runtime: runtime,
        }
    }
}

impl eframe::App for SpecDemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app_state = self.ui_context.app_state();

        // Process messages and poll evaluations
        let _ = self._runtime.block_on(async {
            self.app_runner.run(&app_state).await
        });

        if self.app_runner.has_evaluating() {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("UISpec Demo");
            ui.separator();

            let roots = match try_sync_lock(app_state) {
                Ok(state) => state.roots(),
                Err(e) => {
                    ui.label(format!("Error: {}", e));
                    ui.spinner();
                    return;
                }
            };

            for handle in roots {
                render_element(ui, handle, &self.ui_context);
            }
        });
    }
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    println!("Starting UISpec Demo...");
    println!("Click 'Add Dashboard' to add a nested dashboard");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("UISpec Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "UISpec Demo",
        options,
        Box::new(|cc| Ok(Box::new(SpecDemoApp::new(cc)))),
    )
}
