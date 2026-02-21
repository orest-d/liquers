# UIElement — Dioxus / Leptos Design Notes

Brief design sketch for reactive framework rendering targets. Validates the core
UIElement trait design. NOT a Phase 1 deliverable.

## Proposed Signature

```rust
#[cfg(feature = "dioxus")]
fn show_in_dioxus(&self) -> dioxus::Element;
```

## Rendering Model

Reactive frameworks (Dioxus, Leptos) use functional components with signals/state:

This is fundamentally different from immediate-mode (egui, ratatui).

## Design Tension: `&mut self` vs `&self`

The `show_in_egui` method takes `&mut self` because egui renders each frame and
elements may update internal state during rendering. Reactive frameworks do NOT
mutate during render — they read state immutably and return a virtual DOM.

**`show_in_dioxus` needs `&self` (not `&mut self`).**

## Resolution: Adapter Pattern

A `DioxusAdapter` component wraps `Arc<Mutex<dyn AppState>>` + `UIHandle`. It:
1. Reads the element state on each render (via `blocking_lock()` or signal)
2. Converts UIElement state into virtual DOM nodes
3. Does NOT call `show_in_dioxus` directly during the render phase

```rust
#[component]
fn ElementView(handle: UIHandle, app_state: Arc<tokio::sync::Mutex<dyn AppState>>) -> Element {
    // Read element data (title, type, view mode, etc.)
    let element_data = use_memo(move || {
        let state = app_state.blocking_lock();
        if let Ok(Some(elem)) = state.get_element(handle) {
            ElementSnapshot {
                title: elem.title(),
                type_name: elem.type_name().to_string(),
                // ... other readable state
            }
        } else {
            ElementSnapshot::pending()
        }
    });

    rsx! {
        div { class: "element",
            h3 { "{element_data().title}" }
            // ... render based on element type
        }
    }
}
```

## Mutation Path

The `update()` trait method handles mutation (framework-agnostic). Reactive
frameworks trigger updates via:
1. User interaction → dispatch `UpdateMessage` to element via AppState
2. Element's `update()` modifies internal state, returns `NeedsRepaint`
3. Signal change triggers re-render of the adapter component

## Phase 2 Consideration

Adding `fn show_data(&self) -> ShowData` as a framework-agnostic method that
returns renderable data. Framework-specific methods consume this data. This
decouples the rendering decision from the framework.

## Design Assessment

**Tension identified but resolvable.** The `&mut self` vs `&self` difference is
the main challenge. The adapter pattern resolves it cleanly — the element itself
doesn't need to know about Dioxus. The `update()` trait method provides the
mutation path. No changes to the Phase 1 design are needed.

## Alternative Design: AppState as Global Context, Handle as Local Hook

This design leverages Dioxus's context and hooks to fit the Liquers UI model:

### AppState as Global Context
Use Dioxus's `provide_context` and `use_context` APIs to inject `Arc<tokio::sync::Mutex<dyn AppState>>` globally. All components can access and mutate the UI tree via this context, ensuring consistent state management and easy sharing across the app.

### Handle as Local State (Hook)
Each element view component uses a custom hook (e.g., `use_handle`) to track its current `UIHandle`. This allows navigation, focus, and local updates, and fits Dioxus's idiom of local state for component-specific data.

### Element Snapshot via Memo
Instead of locking AppState every render, use a memoized snapshot:
```rust
let app_state = use_context::<Arc<tokio::sync::Mutex<dyn AppState>>>();
let handle = use_handle(initial_handle);
let element_data = use_memo(move || {
    let state = app_state.blocking_lock();
    state.get_element(handle).map(|elem| elem.snapshot())
}, [handle]);
```

### Update Path
User actions dispatch `UpdateMessage` via AppState context. The hook can expose a method to send updates:
```rust
let send_update = use_callback(move |msg: UpdateMessage| {
    let mut state = app_state.lock().await;
    if let Some(elem) = state.get_element_mut(handle) {
        elem.update(&msg);
    }
}, [app_state, handle]);
```

### Component Structure Example
```rust
#[component]
fn ElementView(initial_handle: UIHandle) -> Element {
    let handle = use_handle(initial_handle);
    let app_state = use_context::<Arc<tokio::sync::Mutex<dyn AppState>>>();
    let element_data = use_memo(...);
    let send_update = use_callback(...);

    rsx! {
        div { class: "element",
            h3 { "{element_data.title}" }
            // ... render based on element type
            button { onclick: move |_| send_update(UpdateMessage::Custom(...)), "Update" }
        }
    }
}
```

### Summary
- AppState is global context (Dioxus context API)
- Handle is local state (custom hook)
- Element data is memoized for efficient rendering
- Updates are dispatched via context and handle

This design fits Dioxus's idioms, enables efficient reactivity, and keeps element navigation and mutation ergonomic.
