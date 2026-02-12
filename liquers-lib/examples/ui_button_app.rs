//! Example egui application demonstrating a custom UIElement with button interaction.
//!
//! A `ButtonElement` renders a "Say Hello" button in egui. When clicked, it submits
//! the query `hello/ns-lui/add-instead` which:
//!
//! 1. Runs the `hello` command → produces `Value::from("Hello, world!")`
//! 2. Pipes the result to `add-instead` (namespace `lui`) which replaces the
//!    current element with an `AssetViewElement` wrapping "Hello, world!"
//!
//! After clicking, the button disappears and "Hello, world!" text appears in its place.
//!
//! **Note on the `q` instruction:** The query `hello/q/ns-lui/add-instead` (with `/q/`)
//! would behave differently — the `q` instruction wraps the preceding part as a
//! `Value::Query("hello")` instead of evaluating the `hello` command. Use `/q/` only
//! when you want to pass a query reference as a value, not when you want to pipe
//! a command's output.
//!
//! Run with: `cargo run --example ui_button_app -p liquers-lib`

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::ui::{
    app_message_channel, render_element, try_sync_lock, AppRunner, AppState, DirectAppState,
    ElementSource, UIContext, UIElement, UIHandle, UpdateMessage, UpdateResponse,
};
use liquers_lib::value::Value;

// Required by register_command! and register_lui_commands! macros.
type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

// ─── ButtonElement ──────────────────────────────────────────────────────────

/// A custom UIElement that renders as an egui button.
///
/// When the button is clicked, it submits a query via UIContext.
/// The query result replaces the button element in the UI tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ButtonElement {
    handle: Option<UIHandle>,
    title_text: String,
    /// Query to submit when the button is clicked.
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
        }
    }

    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        _app_state: &mut dyn liquers_lib::ui::AppState,
    ) -> egui::Response {
        let response = ui.button(&self.button_label);
        if response.clicked() {
            if let Some(handle) = self.handle {
                // Submit query bound to this element's handle.
                // The `add-instead` command will replace this element with the result.
                ctx.submit_query(handle, &self.query);
            }
        }
        response
    }
}

// ─── Commands ───────────────────────────────────────────────────────────────

/// A command that returns "Hello, world!".
fn hello(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("Hello, world!"))
}

/// Register commands. Separate function to allow `?` for error propagation.
fn setup_commands(env: &mut DefaultEnvironment<Value, SimpleUIPayload>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn hello(state) -> result)?;
    liquers_lib::register_all_commands!(cr)?;
    Ok(())
}

// ─── eframe App ─────────────────────────────────────────────────────────────

struct ButtonApp {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    runner: AppRunner<DefaultEnvironment<Value, SimpleUIPayload>>,
    ui_context: UIContext,
    _runtime: tokio::runtime::Runtime,
}

impl ButtonApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let (app_state, runner, ui_context) = runtime.block_on(async {
            // 1. Create environment with commands.
            let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
            env.with_trivial_recipe_provider();
            setup_commands(&mut env).expect("Failed to register commands");

            // 2. Create AppState with a root node.
            //    ElementSource::None — no auto-evaluation; the ButtonElement is set directly.
            let mut direct_state = DirectAppState::new();
            let root_handle = direct_state
                .add_node(None, 0, ElementSource::None)
                .expect("Failed to add root node");

            // 3. Set ButtonElement on the root.
            //    Query: hello/ns-lui/add-instead
            //    - `hello` produces "Hello, world!"
            //    - `ns-lui` switches to the `lui` namespace
            //    - `add-instead` (with default reference_word="current") replaces
            //       this element with an AssetViewElement wrapping the hello output.
            let mut button = ButtonElement::new("Say Hello", "hello/ns-lui/add-instead-current");
            button.set_handle(root_handle);
            direct_state
                .set_element(root_handle, Box::new(button))
                .expect("Failed to set ButtonElement");

            // 4. Wrap AppState in Arc<Mutex>.
            let app_state: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(direct_state));

            // 5. Create message channel and UIContext.
            let (msg_tx, msg_rx) = app_message_channel();
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

impl eframe::App for ButtonApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Run AppRunner (processes messages, auto-evaluates pending, polls results).
        self._runtime.block_on(async {
            if let Err(e) = self.runner.run(&self.app_state).await {
                eprintln!("[AppRunner] Error: {}", e);
            }
        });

        // Request repaint while evaluations are in flight.
        if self.runner.has_evaluating() {
            ctx.request_repaint();
        }

        // 2. Render UI.
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Liquers Button Example");
            ui.separator();

            // Render all root elements via extract-render-replace.
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
        });
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Liquers Button Example",
        options,
        Box::new(|cc| Ok(Box::new(ButtonApp::new(cc)))),
    )
}
