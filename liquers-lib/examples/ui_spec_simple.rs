//! Simple example showing UISpecElement with static content.
//!
//! Run with: `cargo run --example ui_spec_simple -p liquers-lib`

use std::fmt::format;
use std::sync::Arc;

use liquers_lib::ui::{
    app_message_channel, AppState, DirectAppState, ElementSource, StateViewElement, UIContext,
    UIElement, render_element, try_sync_lock,
};
use liquers_lib::ui::widgets::ui_spec_element::{InitQuery, LayoutSpec, UISpec, UISpecElement};
use liquers_lib::value::Value;

// ─── eframe App ─────────────────────────────────────────────────────────────

struct SimpleSpecApp {
    ui_context: UIContext,
    _runtime: tokio::runtime::Runtime,
}

impl SimpleSpecApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        let ui_context = runtime.block_on(async {
            // 1. Create AppState
            let mut app_state = DirectAppState::new();

            // 2. Create root UISpecElement with horizontal layout
            let spec = UISpec {
                init: vec![
                ],
                menu: None,
                layout: LayoutSpec::Horizontal,
            };

            let mut ui_spec_element = UISpecElement::from_spec("Simple Demo".to_string(), spec);

            // 3. Add root element
            let root_handle = app_state
                .add_node(None, 0, ElementSource::None)
                .expect("Failed to add root node");

            ui_spec_element.set_handle(root_handle);
            app_state
                .set_element(root_handle, Box::new(ui_spec_element))
                .expect("Failed to set root element");

            // 4. Add some child elements manually
            let child1_handle = app_state
                .add_node(Some(root_handle), 0, ElementSource::None)
                .expect("Failed to add child 1");

            let mut child1 = StateViewElement::new(
                "Child 1".to_string(),
                Arc::new(Value::from("Hello from Child 1!")),
            );
            child1.set_handle(child1_handle);
            app_state
                .set_element(child1_handle, Box::new(child1))
                .expect("Failed to set child 1");

            let child2_handle = app_state
                .add_node(Some(root_handle), 1, ElementSource::None)
                .expect("Failed to add child 2");

            let mut child2 = StateViewElement::new(
                "Child 2".to_string(),
                Arc::new(Value::from("Hello from Child 2!")),
            );
            child2.set_handle(child2_handle);
            app_state
                .set_element(child2_handle, Box::new(child2))
                .expect("Failed to set child 2");


            // 5. Create UIContext
            let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> =
                Arc::new(tokio::sync::Mutex::new(app_state));
            let (msg_tx, _msg_rx) = app_message_channel();

            UIContext::new(app_state_arc, msg_tx)
        });

        Self {
            ui_context,
            _runtime: runtime,
        }
    }
}

impl eframe::App for SimpleSpecApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Simple UISpec Demo");
            ui.label("Horizontal layout with 3 children");
            ui.separator();

            // Render all root elements
            let app_state = self.ui_context.app_state();
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

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    println!("Starting Simple UISpec Demo...");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Simple UISpec Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "Simple UISpec Demo",
        options,
        Box::new(|cc| Ok(Box::new(SimpleSpecApp::new(cc)))),
    )
}
