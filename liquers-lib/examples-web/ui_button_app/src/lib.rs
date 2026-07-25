//! Browser (wasm) port of the native `ui_button_app` example.
//!
//! Demonstrates a **custom `UIElement` defined outside the library**. `ButtonElement` renders a
//! button whose query replaces the element with its own result (`hello/ns-lui/add-instead`).
//!
//! The interesting part is how little the port needs: the native example implements
//! `show_in_egui`, this one implements `render_web`, and both express the same interaction as a
//! `UiAction` in the markup. `update` — the headless path — is shared verbatim.
//!
//! Build & serve with `trunk serve` (port 8085).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, mount_web, AppMessageReceiver, AppMessageSender, AppState, DirectAppState,
    ElementSource, UIContext, UIElement, UIHandle, UpdateMessage, UpdateResponse,
};
use liquers_lib::value::Value;
use liquers_macro::register_command;

// Required by the register_command! / register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// A custom UIElement that renders as a button and submits a query when triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ButtonElement {
    handle: Option<UIHandle>,
    title_text: String,
    /// Query submitted when the button is activated.
    query: String,
    /// Text displayed on the button face.
    button_label: String,
}

impl ButtonElement {
    fn new(button_label: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            handle: None,
            title_text: "Button".to_string(),
            query: query.into(),
            button_label: button_label.into(),
        }
    }
}

#[typetag::serde]
impl UIElement for ButtonElement {
    fn type_name(&self) -> &'static str {
        "ButtonElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: UIHandle) {
        self.handle = Some(handle);
    }

    fn title(&self) -> String {
        self.title_text.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title_text = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
        match message {
            UpdateMessage::Custom(_) => {
                // Headless trigger: submit the query for the current handle.
                ctx.submit_query_current(&self.query);
                UpdateResponse::NeedsRepaint
            }
            UpdateMessage::AssetNotification(_) => UpdateResponse::Unchanged,
            UpdateMessage::Timer { .. } => UpdateResponse::Unchanged,
            UpdateMessage::AssetUpdate(_) => UpdateResponse::Unchanged,
        }
    }

    fn render_web(&self, _app_state: &dyn AppState) -> String {
        use liquers_lib::ui::web::html::{action_attr, escape_html};
        use liquers_lib::ui::{element_dom_id, UiAction};

        // The action carries the query as data. The delegated listener resolves the button to its
        // owning element (the `ui-element-{handle}` wrapper below), so the query is submitted for
        // this handle and `add-instead` replaces this element with the result — the same outcome
        // as the egui click handler.
        let action = action_attr(&UiAction::Query(self.query.clone()));
        format!(
            "<div id=\"{}\" class=\"lq-element lq-ButtonElement\"><button class=\"lq-menu-button\"{}>{}</button></div>",
            element_dom_id(self.handle()),
            action,
            escape_html(&self.button_label)
        )
    }
}

/// A command that returns "Hello, world!".
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello, world!"))
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
    let root_handle = app_state.add_node(None, 0, ElementSource::None)?;
    let mut button = ButtonElement::new("Say Hello", "hello/ns-lui/add-instead");
    button.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(button))?;

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
