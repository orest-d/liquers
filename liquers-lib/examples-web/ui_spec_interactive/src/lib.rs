//! Browser (wasm) port of the native `ui_spec_interactive` example.
//!
//! A `UISpec` menu built in Rust rather than YAML: "Add Hello" appends a child, "Clear All"
//! removes the children. The same `UiAction` values drive both backends — in the browser they are
//! serialized into `data-lq-action` attributes and dispatched by the delegated listener.
//!
//! Build & serve with `trunk serve` (port 8083).

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, mount_web, AppMessageReceiver, AppMessageSender, AppState, DirectAppState,
    ElementSource, UIElement,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by the register_command! / register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

use liquers_lib::ui::widgets::ui_spec_element::{
    LayoutSpec, MenuAction, MenuBarSpec, MenuItem, TopLevelItem, UISpec, UISpecElement,
};

/// The command whose result becomes a child element.
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello from menu button!"))
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

    let spec = UISpec {
        init: vec![],
        menu: Some(MenuBarSpec {
            items: vec![TopLevelItem::Menu {
                label: "Actions".to_string(),
                shortcut: None,
                items: vec![
                    MenuItem::Button {
                        label: "Add Hello".to_string(),
                        icon: None,
                        shortcut: Some("Ctrl+H".to_string()),
                        action: MenuAction::Query("hello/q/ns-lui/add-child".to_string()),
                    },
                    MenuItem::Separator,
                    MenuItem::Button {
                        label: "Clear All".to_string(),
                        icon: None,
                        shortcut: None,
                        action: MenuAction::Query("ns-lui/remove-last".to_string()),
                    },
                ],
            }],
        }),
        layout: LayoutSpec::Vertical,
    };

    let mut app_state = DirectAppState::new();
    let root_handle = app_state.add_node(None, 0, ElementSource::None)?;
    let mut root_element = UISpecElement::from_spec("Interactive Spec".to_string(), spec);
    root_element.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(root_element))?;

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
