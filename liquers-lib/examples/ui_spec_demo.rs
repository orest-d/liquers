//! Comprehensive example demonstrating UISpecElement with YAML-driven UI.
//!
//! Features demonstrated:
//! - Menu bar with keyboard shortcuts
//! - Multiple layouts: grid, tabs, horizontal, vertical
//! - Init queries to populate UI with content
//! - Query submission from menu actions
//! - Shortcut conflict detection
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

// ─── Sample Commands ────────────────────────────────────────────────────────

/// Generate a text value
fn text(_state: &State<Value>, content: String) -> Result<Value, Error> {
    Ok(Value::from(content))
}

/// Generate a numbered list
fn numbers(_state: &State<Value>, count: i64) -> Result<Value, Error> {
    let list: Vec<String> = (1..=count).map(|n| format!("Item {}", n)).collect();
    Ok(Value::from(list.join("\n")))
}

/// Generate a simple table
fn table(_state: &State<Value>, rows: i64, cols: i64) -> Result<Value, Error> {
    let mut text = String::new();
    for r in 1..=rows {
        let row: Vec<String> = (1..=cols).map(|c| format!("R{}C{}", r, c)).collect();
        text.push_str(&row.join("\t"));
        text.push('\n');
    }
    Ok(Value::from(text))
}

// ─── YAML Specifications ────────────────────────────────────────────────────

/// Main dashboard with tabs for different layouts
const DASHBOARD_YAML: &str = r#"
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: Refresh
      shortcut: Ctrl+R
      action:
        query: "-R/Dashboard/-/ns-lui/add-instead"
    - !separator
    - !button
      label: Quit
      shortcut: Ctrl+Q
      action: null
  - !menu
    label: View
    items:
    - !button
      label: Grid Demo
      shortcut: Ctrl+1
      action:
        query: "-R/Grid Demo/-/ns-lui/add-child"
    - !button
      label: List Demo
      shortcut: Ctrl+2
      action:
        query: "-R/List Demo/-/ns-lui/add-child"
    - !button
      label: Table Demo
      shortcut: Ctrl+3
      action:
        query: "-R/Table Demo/-/ns-lui/add-child"
  - !button
    label: Help
    shortcut: F1
    action:
      query: "text-Help~.Use~.menu~.or~.shortcuts~.to~.navigate/q/ns-lui/add-instead"

init:
  - "text-Welcome~.to~.UISpec~.Demo!/q/ns-lui/add-child"
  - "numbers-5/q/ns-lui/add-child"
  - "table-3-4/q/ns-lui/add-child"

layout: !tabs
  selected: 0
"#;

/// Grid layout demo
const GRID_DEMO_YAML: &str = r#"
menu:
  items:
  - !button
    label: Back
    action:
      query: "ns-lui/remove-current"

init:
  - "text-Grid~.Item~.1/q/ns-lui/add-child"
  - "text-Grid~.Item~.2/q/ns-lui/add-child"
  - "text-Grid~.Item~.3/q/ns-lui/add-child"
  - "text-Grid~.Item~.4/q/ns-lui/add-child"
  - "text-Grid~.Item~.5/q/ns-lui/add-child"
  - "text-Grid~.Item~.6/q/ns-lui/add-child"

layout: !grid
  rows: 2
  columns: 3
"#;

/// Vertical list demo
const LIST_DEMO_YAML: &str = r#"
menu:
  items:
  - !button
    label: Back
    action:
      query: "ns-lui/remove-current"

init:
  - "text-First~.Item/q/ns-lui/add-child"
  - "text-Second~.Item/q/ns-lui/add-child"
  - "text-Third~.Item/q/ns-lui/add-child"
  - "numbers-10/q/ns-lui/add-child"

layout: vertical
"#;

/// Horizontal table demo
const TABLE_DEMO_YAML: &str = r#"
menu:
  items:
  - !button
    label: Back
    action:
      query: "ns-lui/remove-current"

init:
  - "table-5-3/q/ns-lui/add-child"
  - "table-4-6/q/ns-lui/add-child"

layout: horizontal
"#;

// ─── Setup Functions ────────────────────────────────────────────────────────

fn setup_environment() -> Result<DefaultEnvironment<Value, SimpleUIPayload>, Error> {
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();

    let cr = env.get_mut_command_registry();
    register_command!(cr, fn text(state, content: String) -> result)?;
    register_command!(cr, fn numbers(state, count: i64) -> result)?;
    register_command!(cr, fn table(state, rows: i64, cols: i64) -> result)?;
    liquers_lib::register_lui_commands!(cr)?;

    Ok(env)
}

// ─── eframe App ─────────────────────────────────────────────────────────────

struct SpecDemoApp {
    ui_context: UIContext,
    _runtime: tokio::runtime::Runtime,
}

impl SpecDemoApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let ui_context = runtime.block_on(async {
            // 1. Create environment and register commands
            let env = setup_environment().expect("Failed to setup environment");

            let envref = env.to_ref();

            // 2. Create AppState and root UISpecElement
            let mut app_state = DirectAppState::new();
            let root_handle = app_state
                .add_node(None, 0, ElementSource::None)
                .expect("Failed to add root node");

            // 3. Create Dashboard UISpecElement from YAML
            use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};

            let spec = UISpec::from_yaml(DASHBOARD_YAML).expect("Failed to parse dashboard YAML");
            let mut dashboard_element = UISpecElement::from_spec("Dashboard".to_string(), spec);
            dashboard_element.set_handle(root_handle);

            app_state
                .set_element(root_handle, Box::new(dashboard_element))
                .expect("Failed to set dashboard element");

            // 4. Create UIContext and AppRunner
            let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(app_state));
            let (msg_tx, msg_rx) = app_message_channel();
            let ui_context = UIContext::new(app_state_arc.clone(), msg_tx.clone());

            // Initialize the dashboard element to trigger init queries
            {
                println!("Initializing dashboard element...");
                let mut state = app_state_arc.lock().await;
                if let Ok(mut elem) = state.take_element(root_handle) {
                    elem.init(root_handle, &ui_context);
                    state
                        .put_element(root_handle, elem)
                        .expect("Failed to put element back");
                }
                println!("Dashboard element initialized");
            }

            // Run AppRunner once to process init queries and complete initial setup
            println!("Starting AppRunner to process init queries...");
            let mut app_runner = AppRunner::new(envref, msg_rx, msg_tx.clone());

            // Process messages and evaluations until everything is ready
            for i in 0..200 {
                if let Err(e) = app_runner.run(&app_state_arc).await {
                    eprintln!("AppRunner error: {}", e);
                    break;
                }
                let has_eval = app_runner.has_evaluating();
                if i % 20 == 0 {
                    println!("AppRunner iteration {}, has_evaluating: {}", i, has_eval);
                }
                if !has_eval {
                    println!("AppRunner completed after {} iterations", i + 1);
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
            println!("Initialization complete, starting UI...");

            ui_context
        });

        Self {
            ui_context,
            _runtime: runtime,
        }
    }
}

impl eframe::App for SpecDemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app_state = self.ui_context.app_state();

        // Render UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("UISpec Demo - YAML-Driven UI");
            ui.label("Use menu or keyboard shortcuts to navigate");
            ui.separator();

            // Render all root elements
            let roots = match try_sync_lock(app_state) {
                Ok(state) => state.roots(),
                Err(e) => {
                    println!("Error locking app state: {}", e);
                    ui.spinner();
                    ctx.request_repaint();
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
    println!("Starting UISpec Demo...");
    println!("Keyboard shortcuts:");
    println!("  Ctrl+R - Refresh dashboard");
    println!("  Ctrl+Q - Quit");
    println!("  Ctrl+1 - Grid demo");
    println!("  Ctrl+2 - List demo");
    println!("  Ctrl+3 - Table demo");
    println!("  F1     - Help");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0])
            .with_title("UISpec Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "UISpec Demo",
        options,
        Box::new(|cc| Ok(Box::new(SpecDemoApp::new(cc)))),
    )
}
