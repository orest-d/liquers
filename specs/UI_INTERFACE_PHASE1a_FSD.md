# UI Interface Phase 1a — Synchronous Query Submission & WASM Compatibility

## Overview

Phase 1a extends Phase 1 with two interconnected features:

1. **Synchronous query submission from `show_in_egui`**: UI elements rendered via
   the synchronous `show_in_egui()` method can trigger asynchronous query evaluation
   (e.g., button clicks) without access to the tokio runtime or `EnvRef`.

2. **WASM compatibility**: Replace `blocking_lock()` with `try_lock()` so the render
   path works in single-threaded WASM environments where `blocking_lock()` panics.

Both are addressed by introducing `UIContext` (bundles shared state + message channel)
and `try_sync_lock` (non-blocking lock acquisition).

**Key References:**
- `UI_INTERFACE_PHASE1_FSD.md` — Phase 1 specification (updated to v5.2 with these changes)
- `liquers-lib/examples/egui_async_prototype.rs` — Standalone prototype validating the patterns

---

## 1. UIContext

**File**: `liquers-lib/src/ui/ui_context.rs`

`UIContext` is a lightweight, cloneable struct passed to `show_in_egui()` and
`render_element()`. It bundles:

- `Arc<tokio::sync::Mutex<dyn AppState>>` — shared element tree
- `AppMessageSender` — unbounded mpsc sender for async work submission

```rust
#[derive(Clone)]
pub struct UIContext {
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
}

impl UIContext {
    pub fn new(app_state, sender) -> Self;
    pub fn app_state(&self) -> &Arc<tokio::sync::Mutex<dyn AppState>>;
    pub fn submit_query(&self, handle: UIHandle, query: impl Into<String>);
    pub fn send_message(&self, message: AppMessage);
    pub fn request_quit(&self);
    pub fn evaluate_pending(&self);
}
```

**Design rationale**: Elements don't need `EnvRef` or the tokio runtime handle.
They submit messages via the channel; the app's `update()` loop drains and processes
them. This is the same pattern used by `AssetServiceMessage` in the asset system.

---

## 2. AppMessage and Channel

**File**: `liquers-lib/src/ui/message.rs`

```rust
#[derive(Debug, Clone)]
pub enum AppMessage {
    SubmitQuery { handle: UIHandle, query: String },
    Quit,
    Serialize { path: String },
    Deserialize { path: String },
    EvaluatePending,
}

pub type AppMessageSender = tokio::sync::mpsc::UnboundedSender<AppMessage>;
pub type AppMessageReceiver = tokio::sync::mpsc::UnboundedReceiver<AppMessage>;

pub fn app_message_channel() -> (AppMessageSender, AppMessageReceiver);
```

**Why `tokio::sync::mpsc::unbounded_channel`**:
- `UnboundedSender::send()` is synchronous and non-blocking — safe from `show_in_egui`
- Already used in the codebase for `AssetServiceMessage`
- Works in single-threaded WASM tokio

---

## 3. try_sync_lock

**File**: `liquers-lib/src/ui/mod.rs`

```rust
pub fn try_sync_lock<T: ?Sized>(
    mutex: &tokio::sync::Mutex<T>,
) -> Result<tokio::sync::MutexGuard<'_, T>, Error> {
    mutex.try_lock().map_err(|_|
        Error::general_error("AppState lock held by async task".to_string())
    )
}
```

Replaces all uses of `blocking_lock()` in the render path:

| Context | Before | After |
|---------|--------|-------|
| Render loop (get roots) | `blocking_lock()` | `try_sync_lock()` + graceful fallback |
| render_element (take/put) | `blocking_lock()` | `try_sync_lock()` + placeholder on failure |
| show_in_egui (element) | receives `Arc<Mutex<AppState>>` | receives `&UIContext` |
| Async commands (lui) | `.lock().await` | `.lock().await` (unchanged) |

**WASM safety**: `try_lock()` never blocks. On single-threaded WASM, no contention
is possible during synchronous rendering, so it always succeeds.

**Native behavior**: Async commands hold the lock for microseconds. Contention with
the render loop is extremely rare. On the rare failure, a placeholder/spinner is
shown and `request_repaint()` ensures the next frame retries.

---

## 4. spawn_ui_task

**File**: `liquers-lib/src/ui/mod.rs`

Cross-platform async task spawning:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_ui_task<F>(future: F)
where F: Future<Output = ()> + Send + 'static
{ tokio::spawn(future); }

#[cfg(target_arch = "wasm32")]
pub fn spawn_ui_task<F>(future: F)
where F: Future<Output = ()> + 'static
{ wasm_bindgen_futures::spawn_local(future); }
```

On WASM, `Send` is not required (single-threaded). The `#[cfg]` split handles the
different trait bounds.

---

## 5. Updated Signatures

### show_in_egui

```rust
// Before:
fn show_in_egui(&mut self, ui: &mut egui::Ui,
    _app_state: Arc<tokio::sync::Mutex<dyn AppState>>) -> egui::Response;

// After:
fn show_in_egui(&mut self, ui: &mut egui::Ui, _ctx: &UIContext) -> egui::Response;
```

### render_element

```rust
// Before:
pub fn render_element(ui, handle, app_state: &Arc<Mutex<dyn AppState>>);

// After:
pub fn render_element(ui, handle, ctx: &UIContext);
```

---

## 6. Message Processing Loop

In the eframe app's `update()` method:

```rust
fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    // 1. Drain messages from UIContext's channel.
    while let Ok(msg) = self.message_rx.try_recv() {
        match msg {
            AppMessage::SubmitQuery { handle, query } => {
                self.evaluate_node(handle, &query);
            }
            AppMessage::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            AppMessage::EvaluatePending => { /* re-evaluate pending nodes */ }
            AppMessage::Serialize { .. } | AppMessage::Deserialize { .. } => { /* ... */ }
        }
    }

    // 2. Render with UIContext.
    egui::CentralPanel::default().show(ctx, |ui| {
        let roots = match try_sync_lock(self.ui_context.app_state()) {
            Ok(state) => state.roots(),
            Err(_) => { ui.spinner(); ctx.request_repaint(); return; }
        };
        for handle in roots {
            render_element(ui, handle, &self.ui_context);
        }
    });
}
```

---

## 7. Files Modified/Created

| File | Action | Description |
|------|--------|-------------|
| `liquers-lib/src/ui/message.rs` | **NEW** | AppMessage enum, channel types |
| `liquers-lib/src/ui/ui_context.rs` | **NEW** | UIContext struct |
| `liquers-lib/src/ui/mod.rs` | MODIFIED | Module decls, re-exports, try_sync_lock, spawn_ui_task |
| `liquers-lib/src/ui/element.rs` | MODIFIED | show_in_egui signature, render_element |
| `liquers-lib/examples/egui_async_prototype.rs` | **NEW** | Standalone pattern prototype |
| `liquers-lib/examples/ui_hello.rs` | MODIFIED | UIContext + try_sync_lock |
| `liquers-lib/examples/ui_payload_app.rs` | MODIFIED | UIContext + channel + message loop |
| `specs/UI_INTERFACE_PHASE1_FSD.md` | MODIFIED | Updated to v5.2 with new patterns |

---

## 8. Success Criteria

1. `cargo build -p liquers-lib` — library compiles
2. `cargo test -p liquers-lib` — all existing tests pass
3. `cargo run --example egui_async_prototype -p liquers-lib` — prototype works
4. `cargo run --example ui_hello -p liquers-lib` — updated example works
5. `cargo run --example ui_payload_app -p liquers-lib` — updated example with message loop works
6. No `blocking_lock()` calls remain in UI render path
7. `show_in_egui` takes `&UIContext` (not `Arc<Mutex<AppState>>`)
8. `render_element` takes `&UIContext` (not `&Arc<Mutex<AppState>>`)

---

*Specification version: 1.0*
*Date: 2026-02-10*
*Related: UI_INTERFACE_PHASE1_FSD v5.2*
