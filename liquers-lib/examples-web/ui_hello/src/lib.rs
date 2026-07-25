//! Browser (wasm) port of the native `ui_hello` example.
//!
//! The smallest complete pipeline: register a command, evaluate a query in the browser, and let
//! the result render itself. The root node carries `ElementSource::Query("hello")`, so it starts
//! *pending* — `AppRunner` picks it up, evaluates it, and installs an `AssetViewElement` with the
//! value. Nothing here is browser-specific except `mount_web`.
//!
//! Build & serve with `trunk serve` (port 8081).

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, mount_web, AppMessageReceiver, AppMessageSender, AppState, DirectAppState,
    ElementSource,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by the register_command! / register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// A command that returns "Hello, World!".
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

    // A pending root: no element yet, just the query that produces one.
    let mut app_state = DirectAppState::new();
    app_state.add_node(None, 0, ElementSource::Query("hello".to_string()))?;

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
