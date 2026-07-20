# Phase 2: Solution & Architecture - webui

> Note: the liquers-designer workflow auto-invokes a `rust-best-practices` skill
> here. That skill is not installed in this repo (`.claude/skills/` contains only
> `liquers-designer` and `liquers-unittest`), so its checks were applied inline
> from `references/liquers-patterns.md` and the CLAUDE.md conventions instead.

## Overview

The web backend is the web-native peer of the egui reference backend. Its crux is
a small, **framework-independent render surface** — `WebUi` — that mirrors the role
of `&mut egui::Ui`, but emits to one of two sinks: a **DOM cursor** (`web-sys`,
browser/wasm, full support) or an **HTML string** (SSR, server, limited/static
support). Elements render through a single new trait method `show_in_web`, exactly
as they render through `show_in_egui`. Interactivity is expressed as serializable
`WebAction`s ("queries as callbacks", per `UI_INTERFACE_FSD.md`), which the DOM sink
turns into event listeners and the SSR sink turns into `data-lq-*` attributes for
later hydration. A second prerequisite deliverable makes egui optional: today's
egui coupling in `ui/*` and `value/mod.rs` is gated behind a new `egui` Cargo
feature so an `webui`-only build never compiles egui.

## Data Structures

### New Structs

#### `WebUi<'a>` — the render surface (framework-independent)

```rust
// liquers-lib/src/ui/web/webui.rs
pub struct WebUi<'a> {
    /// Where rendered output goes (SSR string or DOM cursor).
    sink: &'a mut WebSink,
    /// Event wiring: submit queries / send messages on interaction.
    ctx: UIContext,
    /// The element currently being rendered (used for stable DOM ids / action targets).
    current: Option<UIHandle>,
    /// Arena that keeps browser event-listener closures alive until the next
    /// full re-render. Empty / unused in SSR mode.
    #[cfg(target_arch = "wasm32")]
    closures: &'a mut Vec<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>>,
}
```

**Ownership rationale:**
- `sink: &'a mut WebSink` — borrowed; the sink is owned by the render driver
  (`render_element_web` / `WebAppRunner`), not by `WebUi`, so nested calls can pass
  a re-borrowed `WebUi` to children (matching egui's `&mut Ui` nesting).
- `ctx: UIContext` — cloned (it is `Clone`, cheap: `Arc` + channel sender); each
  `WebUi` may be re-targeted to a child handle via `with_current`.
- `closures` — borrowed arena; leaking per-listener `Closure`s would leak memory on
  every repaint, so they are retained centrally and dropped on full rebuild.

**Serialization:** none. `WebUi` is a transient render context (never stored,
never serialized), like `egui::Ui`.

#### `WebSink` — the two output backends

```rust
// liquers-lib/src/ui/web/webui.rs
pub enum WebSink {
    /// Server-side rendering: accumulate an HTML fragment.
    Ssr(String),
    /// Browser rendering: append DOM nodes under a cursor element.
    #[cfg(target_arch = "wasm32")]
    Dom {
        document: web_sys::Document,
        /// Element that newly created nodes are appended into.
        cursor: web_sys::Element,
    },
}
```

**Variant semantics:**
- `Ssr(String)`: available on every target (server + wasm). Pure value→markup.
- `Dom { .. }`: only compiled for `target_arch = "wasm32"`; full interactivity.

**No default match arm:** all `WebUi` methods dispatch on `&mut self.sink` with both
variants explicit (the `Dom` arm under `#[cfg(target_arch = "wasm32")]`).

#### `WebResponse` — interaction result of a widget call

```rust
// liquers-lib/src/ui/web/webui.rs
pub struct WebResponse {
    /// Stable DOM id assigned to the rendered node (`ui-element-{h}` or a
    /// per-widget salted id). Enables follow-up `on_click` wiring and CSS.
    pub id: String,
}
```

Unlike egui, `WebResponse` carries **no `.clicked()` poll** — DOM interaction is
event-driven, not immediate-mode-polled (see "Interaction Model"). Handlers are
attached via `WebAction`, not by inspecting a boolean after the fact.

#### `WebAction` — serializable interaction ("query as callback")

```rust
// liquers-lib/src/ui/web/action.rs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WebAction {
    /// Submit a query bound to a specific element handle.
    SubmitQuery { handle: UIHandle, query: String },
    /// Submit a query bound to the element being rendered (`current`).
    SubmitToCurrent { query: String },
    /// Submit the live value of a named input element as a query for `handle`.
    /// Used by the query console's edit field on Enter/change.
    SubmitInputValue { handle: UIHandle, input_id: String },
    /// Request application quit.
    Quit,
}
```

**Variant semantics:** each maps to an existing `UIContext` call (`submit_query`,
`submit_query_current`, `send_message`, `request_quit`). DOM mode dispatches
immediately in an event listener; SSR mode serializes the action to a
`data-lq-action` attribute for a future hydration script. **No default match arm.**

#### `WebAppRunner` / mount + SSR entry points

```rust
// liquers-lib/src/ui/web/app.rs

/// Browser entry point. Mounts the UI under `root`, drives the AppRunner loop,
/// and re-renders roots when `needs_repaint()` is set. wasm-only.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub async fn mount_web<E>(
    root: web_sys::Element,
    envref: liquers_core::context::EnvRef<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    receiver: AppMessageReceiver,
    initial_query: Option<String>,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>;

/// Server-side entry point. Renders the current AppState tree (all roots) to an
/// HTML string. Available on all targets. Non-interactive; emits `data-lq-*`
/// hydration attributes.
#[cfg(feature = "webui")]
pub async fn render_app_ssr(
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
    ctx: &UIContext,
) -> Result<String, Error>;
```

## Trait Implementations

### Trait extension: `UIElement::show_in_web` (feature-gated method)

Both rendering methods on the existing `UIElement` trait become feature-gated so a
build with only one backend compiles neither the other's method nor its deps:

```rust
// liquers-lib/src/ui/element.rs  (modified)
#[typetag::serde]
pub trait UIElement: Send + Sync + std::fmt::Debug {
    // ... unchanged framework-agnostic methods (type_name, handle, title, update, ...)

    #[cfg(feature = "egui")]
    fn show_in_egui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &UIContext,
        app_state: &mut dyn AppState,
    ) -> egui::Response { /* existing default */ }

    #[cfg(feature = "webui")]
    fn show_in_web(
        &mut self,
        web: &mut crate::ui::web::WebUi,
        _ctx: &UIContext,
        _app_state: &mut dyn AppState,
    ) {
        // Default: render the title as a labelled div (mirrors the egui default).
        web.labelled_block(self.type_name(), &self.title());
    }
}
```

**Implementors overriding `show_in_web` (each gated `#[cfg(feature = "webui")]`):**
`Placeholder`, `AssetViewElement`, `StateViewElement`, `QueryConsoleElement`,
`MarkdownElement`, `UISpecElement` — one override per element, alongside its existing
`show_in_egui` override.

**Bounds:** unchanged (`Send + Sync + Debug`, `typetag::serde`). No new bounds.

### Trait: `WebValueExtension` (peer of `UIValueExtension`)

```rust
// liquers-lib/src/ui/web/value_ext.rs
#[cfg(feature = "webui")]
pub trait WebValueExtension {
    /// Render this value into the web surface.
    fn show_web(&self, web: &mut WebUi);
}

#[cfg(feature = "webui")]
impl WebValueExtension for Value {
    fn show_web(&self, web: &mut WebUi) { /* explicit match over Base + ExtValue */ }
}
```

**Match coverage (no default arm):** every `SimpleValue` variant (None, Bool, I32,
I64, F64, Text, Array, Object, Bytes, Metadata, AssetInfo, Recipe, CommandMetadata,
Query, Key) and every `ExtValue` variant. `ExtValue::Image` → `<img>` data-URI;
`ExtValue::PolarsDataFrame` → HTML `<table>`; `ExtValue::UIElement` → delegate to
`show_in_web`. The egui-only variants are handled under `#[cfg(feature = "egui")]`
arms (see "ExtValue Extensions").

## ExtValue Extensions (feature-gating, not new variants)

No new persistent `ExtValue` variants. Instead, the **egui-typed** variants become
optional so they vanish from a webui-only build:

```rust
// liquers-lib/src/value/mod.rs  (modified)
#[derive(Debug, Clone)]
pub enum ExtValue {
    Image { value: Arc<image::DynamicImage> },
    PolarsDataFrame { value: Arc<polars::frame::DataFrame> },
    #[cfg(feature = "egui")]
    UiCommand { value: crate::egui::UiCommand },
    #[cfg(feature = "egui")]
    Widget { value: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>> },
    UIElement { value: Arc<dyn crate::ui::element::UIElement> },
}
```

**Rationale:** `UiCommand`/`Widget` hold egui closures/trait objects and cannot exist
without egui. Every exhaustive `match` over `ExtValue` in `value/mod.rs`
(`identifier`, `type_name`, `default_extension`, `default_filename`, `as_image`,
`as_polars_dataframe`, `as_ui_element`, serializer arms) gains
`#[cfg(feature = "egui")]` arms for these two variants. This keeps the "no default
match arm" rule intact while making the arms conditional. `ExtValueInterface` and
`WebValueExtension` never reference the egui variants except under the `egui` cfg.

## Generic Parameters & Bounds

The web module is almost entirely non-generic (like `AppState` / `WebUi`). The only
generic surface is the driver glue, reusing the existing `AppRunner` bounds verbatim:

```rust
// mount_web / WebAppRunner
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
```

**Bound justification:** identical to `AppRunner<E>` — `Environment<Value = Value>`
to evaluate queries into `Value`, `UIPayload + From<SimpleUIPayload>` to construct the
per-query payload. No new bounds introduced. `WebUi`, `WebSink`, `WebResponse`,
`WebAction` are concrete (non-generic), mirroring the non-generic `AppState`.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `WebUi::*` render methods | No | Synchronous DOM/string emission, like `egui::Ui`. |
| `UIElement::show_in_web` | No | Synchronous render pass (mirrors `show_in_egui`). |
| `render_element_web` | No | Synchronous extract-render-replace via `try_sync_lock`. |
| `render_app_ssr` | Yes | Locks the async `tokio::sync::Mutex<dyn AppState>`; may await. |
| `mount_web` | Yes | Drives the async `AppRunner::run` loop and query evaluation. |
| `WebAppRunner::run` | Yes | Reuses `AppRunner` (async) unchanged. |

**Pattern:** rendering is synchronous (single-threaded DOM / string building);
evaluation and state polling stay async and reuse `AppRunner` untouched.
`try_sync_lock` (already in `ui/mod.rs`) is used inside `render_element_web`, and is
documented as always succeeding on wasm (single-threaded).

## Function Signatures

### Module: `liquers_lib::ui::web` (new)

```rust
// mod.rs
pub mod action;
pub mod app;
pub mod dataframe;
pub mod value_ext;
pub mod webui;
pub mod widgets;

pub use action::WebAction;
pub use value_ext::WebValueExtension;
pub use webui::{WebResponse, WebSink, WebUi};

/// Extract-render-replace for one element into `web`. Mirrors `render_element`.
#[cfg(feature = "webui")]
pub fn render_element_web(web: &mut WebUi, handle: UIHandle, ctx: &UIContext);
```

```rust
// webui.rs — the render surface (signatures only)
impl WebSink {
    pub fn new_ssr() -> Self;                       // Ssr(String::new())
    #[cfg(target_arch = "wasm32")]
    pub fn new_dom(document: web_sys::Document, cursor: web_sys::Element) -> Self;
    pub fn into_html(self) -> String;               // SSR: the string; DOM: "" (nodes already live)
}

impl<'a> WebUi<'a> {
    pub fn with_current(&mut self, handle: Option<UIHandle>) -> WebUi<'_>;
    pub fn element_id(handle: UIHandle) -> String;  // "ui-element-{n}"

    // Content primitives
    pub fn heading(&mut self, text: &str) -> WebResponse;
    pub fn label(&mut self, text: &str) -> WebResponse;
    pub fn colored_label(&mut self, css_color: &str, text: &str) -> WebResponse;
    pub fn labelled_block(&mut self, type_name: &str, title: &str) -> WebResponse;
    pub fn spinner(&mut self) -> WebResponse;
    pub fn separator(&mut self);
    pub fn raw_html(&mut self, html: &str);                       // trusted, pre-escaped markup (tables/images)
    pub fn image_data_uri(&mut self, mime: &str, bytes: &[u8]) -> WebResponse;

    // Layout (closure-nested, like egui's ui.vertical(|ui| ...))
    pub fn vertical(&mut self, f: impl FnOnce(&mut WebUi));
    pub fn horizontal(&mut self, f: impl FnOnce(&mut WebUi));
    pub fn div(&mut self, css_class: &str, f: impl FnOnce(&mut WebUi));
    pub fn scroll_area(&mut self, f: impl FnOnce(&mut WebUi));

    // Interaction (event-driven, not polled)
    pub fn action_button(&mut self, text: &str, action: WebAction) -> WebResponse;
    pub fn query_input(&mut self, id: &str, value: &str, submit: WebAction) -> WebResponse;
    pub fn toggle_button(&mut self, text: &str, action: WebAction) -> WebResponse;
}
```

```rust
// widgets.rs — web analogs of egui/widgets.rs helpers
pub fn display_status(web: &mut WebUi, status: liquers_core::metadata::Status);
pub fn display_progress(web: &mut WebUi, progress: &liquers_core::metadata::ProgressEntry); // matches AssetInfo.progress
pub fn display_asset_info(web: &mut WebUi, info: &liquers_core::metadata::AssetInfo);
pub fn display_error(web: &mut WebUi, error: &Error);
pub fn query_to_html(query: &str) -> String;   // syntax-highlighted <span> markup

// dataframe.rs
pub fn dataframe_to_html(df: &polars::frame::DataFrame, max_rows: usize) -> String;
```

**Parameter choices:** all render methods take `&mut self` (mutate the sink) and
borrowed inputs (`&str`, `&Progress`, `&DataFrame`) to avoid copies. `WebAction` is
passed by value (small, owned, moved into the event listener / attribute).
`escape_html` is applied internally to all text primitives; `raw_html` is the single
trusted-markup escape hatch used only by our own table/image builders.

## Integration Points

### Crate: liquers-lib

**New files** (all under `#[cfg(feature = "webui")]`):
- `src/ui/web/mod.rs`, `webui.rs`, `action.rs`, `value_ext.rs`, `widgets.rs`,
  `dataframe.rs`, `app.rs`

**Modify `src/ui/mod.rs`:**
```rust
#[cfg(feature = "webui")]
pub mod web;
#[cfg(feature = "webui")]
pub use web::{render_element_web, WebAction, WebUi, WebValueExtension};
```

**Modify `src/lib.rs`:**
```rust
#[cfg(feature = "egui")]
pub mod egui;
```
(egui module compiled only under the `egui` feature.)

**Modify `src/ui/element.rs`:** gate `show_in_egui` with `#[cfg(feature = "egui")]`,
add gated `show_in_web`; remove the stray `use egui::debug_text::print;` (line 3) and
gate the egui-using `AssetViewElement::show_in_egui`/`StateViewElement::show_in_egui`.

**Modify each widget element** (`query_console_element.rs`, `markdown_element.rs`,
`ui_spec_element.rs`): gate the existing `show_in_egui` + egui imports with
`#[cfg(feature = "egui")]`; add gated `show_in_web`.

**Modify `src/value/mod.rs`:** gate `ExtValue::UiCommand`/`Widget` and their match
arms with `#[cfg(feature = "egui")]` (see ExtValue Extensions).

**Modify `src/ui/shortcuts.rs`:** gate `Key::to_egui` (and the egui import) with
`#[cfg(feature = "egui")]`; the rest of `shortcuts.rs` is framework-agnostic.

**Required wasm-compat fix (browser mode):** `AssetViewElement::from_asset_ref`
(`element.rs:323`) and `QueryConsoleElement::schedule_volatile_refresh`
(`query_console_element.rs:149`) call `tokio::spawn` directly, which is unavailable on
wasm. Route them through the existing `crate::ui::spawn_ui_task` helper so browser mode
compiles and runs. (No behavior change on native.)

### Dependencies — `liquers-lib/Cargo.toml`

```toml
[features]
default = ["egui", "image-support"]
image-support = ["imageproc"]
egui = ["dep:egui", "dep:eframe", "dep:egui_plot", "dep:egui_extras", "dep:egui_commonmark"]
webui = ["dep:web-sys", "dep:wasm-bindgen", "dep:js-sys", "dep:wasm-bindgen-futures"]

[dependencies]
# egui family becomes optional
egui = { version = "0.33.0", optional = true }
eframe = { version = "0.33.0", optional = true }
egui_plot = { version = "0.34.0", optional = true }
egui_extras = { version = "0.33.3", optional = true }
egui_commonmark = { version = "0.22.0", optional = true }
# webui family (optional)
wasm-bindgen = { version = "0.2", optional = true }
wasm-bindgen-futures = { version = "0.4", optional = true }
js-sys = { version = "0.3", optional = true }
web-sys = { version = "0.3", optional = true, features = [
  "Document", "Window", "Element", "HtmlElement", "HtmlInputElement",
  "Node", "Event", "EventTarget", "InputEvent", "KeyboardEvent",
] }
```

**Version rationale:** egui versions unchanged (only made optional). Verified
`eframe` is not referenced anywhere under `liquers-lib/src/` (only by example
binaries), so gating it under `egui` cannot break the library build. `wasm-bindgen`
`0.2` / `web-sys` `0.3` / `wasm-bindgen-futures` `0.4` are the standard trio;
`wasm-bindgen-futures` is *already* referenced by `ui/mod.rs::spawn_ui_task` under
`cfg(target_arch = "wasm32")` but is currently missing from the manifest, so adding it
also fixes an existing latent wasm break. `egui_commonmark` (used by
`markdown_element`) moves under the `egui` feature; the web markdown renderer must not
depend on it.

## Relevant Commands

### New Commands

**None required for Phase 1.** The web backend is pure rendering + the
`WebValueExtension`; it introduces no query-language commands. The entire `lui`
namespace (`add`, `remove`, `query_console`, `markdown`, `ui_spec`, navigation) is
already framework-agnostic and drives the web tree unchanged.

**Optional (candidate, pending user decision):** a small `web`/`lweb` namespace
mirroring the egui reference commands (`egui/label`, `egui/show_asset_info`) — i.e.
`web/label`, `web/show_asset_info` — producing web-renderable values. Deferring keeps
Phase 1 minimal (YAGNI); the egui reference registers these but they are not needed to
render `lui`-built trees.

### Relevant Existing Namespaces

| Namespace | Relevance | Key Commands |
|-----------|-----------|--------------|
| `lui` | **Primary.** Builds/navigates the UI element tree the web backend renders. Framework-agnostic; unchanged. | `add`, `remove`, `query_console`, `markdown`, `ui_spec`, `children`, `first`, `last`, `parent`, `next`, `prev`, `roots`, `activate` |
| `egui` | **Reference only** (not compiled under webui). Value-producing render commands whose web peers are the optional `web` namespace. | `label`, `text_editor`, `show_asset_info` |
| root/core value commands | Produce the base `Value`s the `WebValueExtension` renders (text, json, etc.). | `text`, `json`, ... |

**Ask user:** (1) Is `lui` the correct primary namespace, and (2) should Phase 1
include the optional `web`/`lweb` value-producing commands mirroring `egui`'s, or
defer them?

## Web Endpoints (if applicable)

No new HTTP routes in this feature. `render_app_ssr` returns an HTML string designed
to be embeddable by a future `liquers-axum` handler, but no axum wiring ships here
(explicitly out of Phase 1 scope per the high-level design).

## Error Handling

Uses `liquers_core::error::Error` with typed constructors only:

| Scenario | Constructor | Example |
|----------|-------------|---------|
| Missing DOM root / node | `Error::general_error` | `Error::general_error("web root element not found".to_string())` |
| Query submit failure | (channel send is best-effort, already ignored in `UIContext`) | — |
| DataFrame→HTML failure | `Error::from_error` | `Error::from_error(ErrorType::General, polars_err)` |
| SSR lock unavailable | `Error::general_error` | reuse `try_sync_lock`'s error |

Rendering methods themselves do not return `Result` (mirroring `egui::Ui` methods,
which are infallible); fallible pieces (`render_app_ssr`, `mount_web`,
`dataframe_to_html`) return `Result<_, Error>`. No `unwrap`/`expect` in library code;
DOM `web_sys` calls that return `Result`/`Option` are mapped to `Error` or handled.

## Serialization Strategy

- `WebUi`, `WebSink`, `WebResponse` are transient — **not** serializable (like
  `egui::Ui`). No derives.
- `WebAction` derives `Serialize, Deserialize` so SSR can emit it as a
  `data-lq-action` JSON attribute and a future hydration script can parse it.
- Element structs are unchanged: their existing `#[serde(skip)]` runtime fields and
  typetag registration are reused as-is. Browser-only cached DOM references (should
  any element choose to cache them later) would be `#[serde(skip)]`; the Phase 1
  design re-renders on change rather than caching nodes, so no new skipped fields are
  required.

## Concurrency Considerations

- Rendering is single-threaded (DOM in the browser main thread; SSR string building
  on one task). `WebUi` is neither `Send` nor shared — it lives on the stack for one
  render pass, exactly like `egui::Ui`.
- Shared state remains `Arc<tokio::sync::Mutex<dyn AppState>>`, accessed via the
  existing async `AppRunner` and via `try_sync_lock` during render.
- Browser mode is single-threaded, so `try_sync_lock` never contends there.
- Event-listener closures are retained in the `WebUi.closures` arena and dropped on
  full re-render, preventing per-frame closure leaks.

## Interaction Model (immediate-mode vs retained DOM)

The DOM is retained; egui/ratatui are immediate-mode. Rather than rebuild the DOM
every animation frame, the web driver re-renders a root **only when the runner reports
`needs_repaint()`** (an evaluation completed, a snapshot arrived, or an action fired),
clearing that root's container and re-emitting its subtree. This keeps a single render
path shared with SSR and matches egui's mental model, at the cost of coarse-grained
DOM rebuilds. Fine-grained incremental DOM diffing (create-once + patch, per
`UI_WEB_DESIGN_NOTES.md`) is a deliberate **future optimization**, not in this scope
(YAGNI / minimal-viable, per the phased pattern). Interaction never relies on
polling a `.clicked()` bool; it is expressed as `WebAction`s attached as listeners
(DOM) or `data-lq-action` attributes (SSR).

## Compilation Validation

Target build matrix (to be exercised in Phase 4):
- `cargo check -p liquers-lib` (default: egui on) — existing behavior, must be green.
- `cargo check -p liquers-lib --no-default-features --features webui,image-support`
  — webui-only, **no egui symbols compiled**.
- `cargo check -p liquers-lib --no-default-features --features egui,image-support`
  — egui-only, no web symbols.
- `cargo check -p liquers-lib --features egui,webui` — both backends coexist.
- (wasm) `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui,image-support`.

Expected at this stage: only "missing implementation" gaps, no design-level type
errors.

## References to liquers-patterns.md

- [x] Crate dependency flow respected (all changes in liquers-lib).
- [x] No new top-level value enums; egui variants gated, no new ExtValue variants.
- [x] Commands unchanged — reuse `register_command!`-based `lui` namespace.
- [x] UIElement pattern followed (new gated trait method + per-element overrides).
- [x] Error handling via typed constructors (`general_error`, `from_error`).
- [x] Async default preserved (AppRunner reused); rendering sync by necessity.
- [x] Match statements explicit; egui arms gated, no `_ =>` default arms.
- [x] No `unwrap`/`expect` in library code; `#[serde(skip)]` for transient fields.
