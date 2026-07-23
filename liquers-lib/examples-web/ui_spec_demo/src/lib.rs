//! Browser (wasm) port of the native `ui_spec_demo`: a menu-driven dashboard rendered by
//! the webui backend. Build & serve with `trunk serve`.

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::widgets::ui_spec_element::{UISpec, UISpecElement};
use liquers_lib::ui::{
    app_message_channel, mount_web, AppMessageReceiver, AppMessageSender, AppState, DirectAppState,
    ElementSource, UIElement,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by the register_command! / register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

const DASHBOARD_YAML: &str = r#"
menu:
  items:
  - !button
    label: Add Dashboard
    action:
      query: "dashboard/q/ns-lui/add-child"
layout: vertical
"#;

fn dashboard(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(DASHBOARD_YAML))
}

/// Build the environment, register commands, and the initial AppState. Returns `Error`
/// so the command-registration macros' internal `?` propagate correctly.
fn build_app() -> Result<
    (
        EnvRef<CommandEnvironment>,
        Arc<tokio::sync::Mutex<dyn AppState>>,
        AppMessageSender,
        AppMessageReceiver,
    ),
    Error,
> {
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    let envref = {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn dashboard(state) -> result)?;
        liquers_lib::register_lui_commands!(cr)?;
        env.to_ref()
    };

    let mut app_state = DirectAppState::new();
    let root_handle = app_state.add_node(None, 0, ElementSource::None)?;
    let spec = UISpec::from_yaml(DASHBOARD_YAML)?;
    let mut element = UISpecElement::from_spec("Dashboard".into(), spec);
    element.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(element))?;

    let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
        Arc::new(tokio::sync::Mutex::new(app_state));
    let (tx, rx) = app_message_channel();
    Ok((envref, app_state_arc, tx, rx))
}

fn err_to_js(e: Error) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = run().await {
            web_sys::console::error_1(&e);
        }
    });
}

async fn run() -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let root = document
        .get_element_by_id("app")
        .ok_or_else(|| JsValue::from_str("no #app element"))?;

    let (envref, app_state, tx, rx) = build_app().map_err(err_to_js)?;

    let mount = mount_web(root, envref, app_state, tx, rx, None)
        .await
        .map_err(err_to_js)?;
    std::mem::forget(mount); // keep listeners alive for the app's lifetime
    Ok(())
}
