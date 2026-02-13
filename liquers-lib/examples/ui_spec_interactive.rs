//! Interactive UISpec example with menu and query submission.
//!
//! Click the "Add Hello" button in the menu to add a child element.
//!
//! Run with: `cargo run --example ui_spec_interactive -p liquers-lib`

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
use liquers_lib::ui::widgets::ui_spec_element::{LayoutSpec, MenuAction, MenuBarSpec, MenuItem, TopLevelItem, UISpec, UISpecElement};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by register_lui_commands! macro
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── Commands ───────────────────────────────────────────────────────────────

/// Simple command that returns "Hello!"
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from menu button!"))
}

// ─── eframe App ─────────────────────────────────────────────────────────────

struct InteractiveSpecApp {
    ui_context: UIContext,
    app_runner: AppRunner<CommandEnvironment>,
    _runtime: tokio::runtime::Runtime,
}

impl InteractiveSpecApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let (ui_context, app_runner) = runtime.block_on(async {
            // 1. Setup environment and register commands
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
            env.with_trivial_recipe_provider();

            let envref = {
                let cr = env.get_mut_command_registry();
                register_command!(cr, fn hello(state) -> result)?;
                liquers_lib::register_lui_commands!(cr)?;
                env.to_ref()
            };

            // 2. Create AppState
            let mut app_state = DirectAppState::new();

            // 3. Create UISpec with menu
            let spec = UISpec {
                init: vec![],
                menu: Some(MenuBarSpec {
                    items: vec![
                        TopLevelItem::Menu {
                            label: "Actions".to_string(),
                            shortcut: None,
                            items: vec![
                                MenuItem::Button {
                                    label: "Add Hello".to_string(),
                                    icon: None,
                                    shortcut: Some("Ctrl+H".to_string()),
                                    action: MenuAction::Query {
                                        query: "hello/q/ns-lui/add-child".to_string(),
                                    },
                                },
                                MenuItem::Separator,
                                MenuItem::Button {
                                    label: "Clear All".to_string(),
                                    icon: None,
                                    shortcut: Some("Ctrl+C".to_string()),
                                    action: MenuAction::Query {
                                        query: "ns-lui/children-current".to_string(),
                                    },
                                },
                            ],
                        },
                        TopLevelItem::Button {
                            label: "Quick Add".to_string(),
                            icon: None,
                            shortcut: None,
                            action: MenuAction::Query {
                                query: "hello/q/ns-lui/add-child".to_string(),
                            },
                        },
                    ],
                }),
                layout: LayoutSpec::Vertical,
            };

            let mut ui_spec_element = UISpecElement::from_spec("Interactive Demo".to_string(), spec);

            // 4. Add root element
            let root_handle = app_state
                .add_node(None, 0, ElementSource::None)
                .expect("Failed to add root node");

            ui_spec_element.set_handle(root_handle);
            app_state
                .set_element(root_handle, Box::new(ui_spec_element))
                .expect("Failed to set root element");

            // 5. Create UIContext and AppRunner
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

impl eframe::App for InteractiveSpecApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app_state = self.ui_context.app_state();

        // Always run AppRunner to drain messages and poll evaluations
        let _ = self._runtime.block_on(async {
            self.app_runner.run(&app_state).await
        });

        // Keep repainting while there are in-flight evaluations
        if self.app_runner.has_evaluating() {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Interactive UISpec Demo");
            ui.label("Click 'Add Hello' in the menu or press Ctrl+H to add children");
            ui.separator();

            // Render all root elements
            let roots = match try_sync_lock(app_state) {
                Ok(state) => state.roots(),
                Err(e) => {
                    ui.label(format!("Error locking state: {}", e));
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

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    println!("Starting Interactive UISpec Demo...");
    println!("Click 'Actions > Add Hello' or press Ctrl+H to add children");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Interactive UISpec Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "Interactive UISpec Demo",
        options,
        Box::new(|cc| Ok(Box::new(InteractiveSpecApp::new(cc)))),
    )
}
