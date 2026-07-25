//! Browser (wasm) port of the native `ui_spec_simple` example.
//!
//! A `UISpecElement` built in Rust (not from YAML) with two static children. No commands run and
//! nothing is interactive: this is the shape of the rendered tree on its own.
//!
//! Build & serve with `trunk serve` (port 8082).

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, mount_web, AppMessageReceiver, AppMessageSender, AppState, DirectAppState,
    ElementSource, StateViewElement, UIElement,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by the register_command! / register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

use liquers_lib::ui::widgets::ui_spec_element::{LayoutSpec, UISpec, UISpecElement};

/// Unused by the tree below, but registered so the environment is the same as the other examples.
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello, World!"))
}

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
        register_command!(cr, fn hello(state) -> result)?;
        liquers_lib::register_lui_commands!(cr)?;
        env.to_ref()
    };

    let mut app_state = DirectAppState::new();

    let spec = UISpec {
        init: vec![],
        menu: None,
        layout: LayoutSpec::Horizontal,
    };
    let root_handle = app_state.add_node(None, 0, ElementSource::None)?;
    let mut root_element = UISpecElement::from_spec("Simple Demo".to_string(), spec);
    root_element.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(root_element))?;

    for (position, text) in [
        (0usize, "Hello from Child 1!"),
        (1usize, "Hello from Child 2!"),
    ] {
        let child_handle = app_state.add_node(Some(root_handle), position, ElementSource::None)?;
        let mut child = StateViewElement::new(
            format!("Child {}", position + 1),
            Arc::new(Value::from(text)),
        );
        child.set_handle(child_handle);
        app_state.set_element(child_handle, Box::new(child))?;
    }

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
