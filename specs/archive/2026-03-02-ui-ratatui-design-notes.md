# UIElement — ratatui Design Notes

Brief design sketch for ratatui rendering target. Validates the core UIElement
trait design. NOT a Phase 1 deliverable.

## Proposed Signature

```rust
#[cfg(feature = "ratatui")]
fn show_in_ratatui(
    &mut self,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
);
```

## Rendering Model

ratatui uses an immediate-mode rendering loop similar to egui:
1. Terminal backend clears the screen each frame
2. `Frame::render_widget()` draws widgets into rectangular areas
3. Layout is computed per-frame via `ratatui::layout::Layout`

The `&mut self` + `Arc<Mutex<dyn AppState>>` pattern maps directly to ratatui's
`StatefulWidget` model, where widgets hold mutable state across frames.

## Extract-Render-Replace

Same pattern as egui: `take_element` → `show_in_ratatui` → `put_element`.
Uses `blocking_lock()` since the TUI render loop is synchronous (runs on the
main thread, not inside the tokio runtime).

## Child Rendering

Elements that contain children compute sub-layouts:

```rust
fn show_in_ratatui(&mut self, frame: &mut Frame, area: Rect, app_state: ...) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Render children into sub-areas
    let children = {
        let state = app_state.blocking_lock();
        state.children(self.handle().unwrap()).unwrap_or_default()
    };
    for (i, child_handle) in children.iter().enumerate() {
        if i < chunks.len() {
            render_element_ratatui(frame, chunks[i], *child_handle, &app_state);
        }
    }
}
```

## Input Handling

ratatui does not handle input — that is done via `crossterm::event::read()` in the
main event loop. Input events are dispatched to the active element via the
`update()` trait method:

```rust
loop {
    terminal.draw(|frame| { /* render */ })?;

    if crossterm::event::poll(Duration::from_millis(100))? {
        let event = crossterm::event::read()?;
        // Dispatch to active element via update()
    }
}
```

This is compatible with `UpdateMessage::Custom(Box<dyn Any + Send>)` — keyboard
events can be wrapped in a custom message type.

## Design Assessment

**No issues identified.** The `&mut self` + `Arc<Mutex<dyn AppState>>` pattern
maps naturally to ratatui's stateful widget model. The extract-render-replace
pattern works identically to egui. Input handling via `update()` is clean.
