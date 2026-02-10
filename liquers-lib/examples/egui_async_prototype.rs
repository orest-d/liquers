//! Minimal egui + tokio prototype demonstrating:
//! - `try_lock()` for synchronous access to shared state from render path
//! - `tokio::sync::mpsc` channel for submitting work from render → async processing
//! - A button that submits async work, with results appearing in the UI
//!
//! Run with: `cargo run --example egui_async_prototype -p liquers-lib`

use std::sync::Arc;

// ─── Shared State ────────────────────────────────────────────────────────────

/// Simple shared state protected by a tokio Mutex.
#[derive(Debug)]
struct SharedState {
    messages: Vec<String>,
    counter: u32,
}

impl SharedState {
    fn new() -> Self {
        Self {
            messages: vec!["Ready.".to_string()],
            counter: 0,
        }
    }
}

// ─── Messages ────────────────────────────────────────────────────────────────

/// Messages sent from the render loop to the async processor.
#[derive(Debug)]
enum AppMessage {
    /// Simulate an async computation that takes some time.
    ComputeSomething { label: String },
    /// Quit the async processor loop.
    Quit,
}

// ─── eframe App ──────────────────────────────────────────────────────────────

struct PrototypeApp {
    state: Arc<tokio::sync::Mutex<SharedState>>,
    sender: tokio::sync::mpsc::UnboundedSender<AppMessage>,
    _runtime: tokio::runtime::Runtime,
}

impl PrototypeApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

        let state = Arc::new(tokio::sync::Mutex::new(SharedState::new()));
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        // Spawn the async message processor.
        let state_clone = state.clone();
        runtime.spawn(async move {
            process_messages(receiver, state_clone).await;
        });

        Self {
            state,
            sender,
            _runtime: runtime,
        }
    }
}

/// Async message processor — drains the channel and updates shared state.
async fn process_messages(
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<AppMessage>,
    state: Arc<tokio::sync::Mutex<SharedState>>,
) {
    while let Some(msg) = receiver.recv().await {
        match msg {
            AppMessage::ComputeSomething { label } => {
                // Simulate async work (e.g. query evaluation).
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                let mut locked = state.lock().await;
                locked.counter += 1;
                let counter = locked.counter;
                locked.messages.push(format!(
                    "#{}: Computed '{}'",
                    counter, label
                ));
                // Keep only last 10 messages.
                if locked.messages.len() > 10 {
                    locked.messages.remove(0);
                }
            }
            AppMessage::Quit => {
                break;
            }
        }
    }
}

impl eframe::App for PrototypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request periodic repaint so we see async results promptly.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("egui + tokio Async Prototype");
            ui.separator();

            // ── try_lock: synchronous, non-blocking read of shared state ──
            match self.state.try_lock() {
                Ok(locked) => {
                    ui.label(format!("Counter: {}", locked.counter));
                    ui.separator();
                    for msg in &locked.messages {
                        ui.label(msg);
                    }
                }
                Err(_) => {
                    // Lock held by async task — show placeholder.
                    ui.spinner();
                    ui.label("State locked by async task...");
                }
            }

            ui.separator();

            // ── Buttons submit messages via the channel (synchronous send) ──
            if ui.button("Submit async work").clicked() {
                let _ = self.sender.send(AppMessage::ComputeSomething {
                    label: "button click".to_string(),
                });
            }

            if ui.button("Submit 5 tasks").clicked() {
                for i in 1..=5 {
                    let _ = self.sender.send(AppMessage::ComputeSomething {
                        label: format!("batch {}", i),
                    });
                }
            }
        });
    }
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "egui Async Prototype",
        options,
        Box::new(|cc| Ok(Box::new(PrototypeApp::new(cc)))),
    )
}
