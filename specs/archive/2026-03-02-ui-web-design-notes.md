# UIElement — Web (Browser) Design Notes

Brief design sketch for browser-based rendering. Validates the core UIElement
trait design. NOT a Phase 1 deliverable.

## Proposed Signature

```rust
#[cfg(feature = "web")]
fn show_in_browser(
    &self,
    doc: &web_sys::Document,
    container: &web_sys::Element,
);
```

## Rendering Model

Web rendering creates/updates persistent DOM elements:
- Elements are created once and updated incrementally
- Unlike immediate-mode (egui), the DOM persists between frames
- Efficient rendering requires diffing or targeted updates

## Design Tension: Persistent vs Immediate-Mode DOM

egui and ratatui rebuild the UI each frame. The browser DOM is persistent —
elements stay in the tree and must be updated in place. Creating the full DOM
on every frame would be extremely inefficient.

## Resolution: Create Once, Update via `update()`

Elements create their DOM subtree on first `show_in_browser` call and store
references to DOM nodes for incremental updates:

```rust
struct WebPanel {
    handle: Option<UIHandle>,
    title_text: String,
    // Cached DOM references (not serializable)
    #[serde(skip)]
    dom_root: Option<web_sys::Element>,
    #[serde(skip)]
    title_node: Option<web_sys::Element>,
}

impl WebPanel {
    fn ensure_dom(&mut self, doc: &Document, container: &Element) {
        if self.dom_root.is_none() {
            let div = doc.create_element("div").unwrap();
            div.set_id(&format!("ui-element-{}", self.handle.unwrap().0));
            let h3 = doc.create_element("h3").unwrap();
            h3.set_text_content(Some(&self.title_text));
            div.append_child(&h3).unwrap();
            container.append_child(&div).unwrap();
            self.dom_root = Some(div);
            self.title_node = Some(h3);
        }
    }
}
```

Subsequent updates happen via the `update()` trait method, which modifies
cached DOM references:

```rust
fn update(&mut self, message: &UpdateMessage) -> UpdateResponse {
    match message {
        // ... handle messages, update DOM nodes directly
        _ => UpdateResponse::Unchanged,
    }
}
```

## Handle as DOM ID

The `handle()` on UIElement is essential for web rendering:
- Generates stable DOM element IDs (`ui-element-{handle}`)
- Enables targeted CSS styling per element
- Supports DOM event delegation (identify source element from event target)

## Serialization for Server-Side Rendering

typetag serialization enables SSR patterns:
1. Server creates AppState with elements, serializes to JSON
2. JSON sent to client as initial state
3. Client deserializes, hydrates DOM from element data
4. Subsequent updates happen client-side via `update()`

```rust
// Server
let json = serde_json::to_string(&app_state)?;
// Send json to client

// Client (wasm)
let app_state: DirectAppState = serde_json::from_str(&json)?;
// Render DOM from deserialized elements
```

## Design Assessment

**Tension identified but resolvable.** The persistent DOM model differs from
immediate-mode rendering. Resolution: elements manage their own DOM subtrees
and update them incrementally via `update()`. The `handle()` on UIElement
provides stable DOM IDs. typetag serialization enables SSR hydration. No
changes to the Phase 1 design are needed.
