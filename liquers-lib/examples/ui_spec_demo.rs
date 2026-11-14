//! UISpec demo with menu button triggering a query action, in the reusable window
//! (`NativeDriver` and `liquers_lib::egui::app::run_window`).
//!
//! Run with: `cargo run --example ui_spec_demo -p liquers-lib`

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::egui::app::{run_window, WindowCloser, WindowOptions};
use liquers_lib::ui::{
    app_message_channel, AppState, DirectAppState, ElementSource, NativeDriver, UIElement,
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

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> Result<(), Error> {
    println!("Starting UISpec Demo...");
    println!("Click 'Add Dashboard' to add a nested dashboard");

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| Error::general_error(format!("cannot start a Tokio runtime: {e}")))?;

    let mut driver = runtime.block_on(async {
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
        let root_handle = app_state.add_node(None, 0, ElementSource::None)?;

        use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
        let spec = UISpec::from_yaml(DASHBOARD_YAML)?;
        let mut element = UISpecElement::from_spec("Dashboard".to_string(), spec);
        element.set_handle(root_handle);
        app_state.set_element(root_handle, Box::new(element))?;

        // 3. The driver owns the runner; the window ticks it every frame.
        let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
            Arc::new(tokio::sync::Mutex::new(app_state));
        let (sender, receiver) = app_message_channel();
        Ok::<_, Error>(NativeDriver::new(envref, app_state, receiver, sender))
    })?;

    let mut options = WindowOptions::new("UISpec Demo");
    options.inner_size = [800.0, 600.0];
    run_window(options, &mut driver, runtime.handle(), &WindowCloser::default(), None)
}
