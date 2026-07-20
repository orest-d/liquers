# Phase 2: Solution & Architecture - webui

> Note: the liquers-designer workflow auto-invokes the `rust-best-practices` skill
> here. It has now been created in this repo (`.claude/skills/rust-best-practices/`)
> and its checks were applied to this revision (ownership, trait/object-safety,
> error handling, feature-gating hygiene, no default match arms).

## Overview

The web backend is the web-native peer of the egui reference backend, added as
conditionally-compiled rendering methods on the **shared** `UIElement` trait — no
new commands, no changes to `lui`. It is **string-first**: every element defines
`render_web(&self, …) -> String`, a pure function producing the element's HTML
subtree. That one definition serves both modes — **SSR** (server: concatenate the
strings) and **browser** (wasm: the default `show_in_web` writes the string into the
live DOM via `set_inner_html`). Interactivity is carried in the markup as
serializable `WebAction`s embedded in `data-lq-action` attributes; the browser
driver attaches a **single delegated event listener** that reads those attributes and
dispatches through the existing `UIContext` channel — so there are no per-widget
closures to manage. A second, prerequisite deliverable makes egui optional: today's
egui coupling in `ui/*` and `value/mod.rs` is gated behind a new `egui` Cargo feature
so a `webui`-only build never compiles egui.

## Rendering Model — `render_web` (SSR) + `show_in_web` (DOM)

This is the central design decision, so it is spelled out before the types.

**Two methods, one markup definition:**

- `fn render_web(&self, app_state: &dyn AppState) -> String` — the *source of truth*.
  Pure, `&self`, available on every target under the `webui` feature. It returns the
  element's complete HTML subtree, escaping all text and embedding interactivity as
  `data-lq-action='{json}'` attributes. Container elements recurse by borrowing their
  children immutably (`app_state.get_element(child)`) and concatenating the children's
  `render_web` output — no mutation, no extract-replace dance.

- `fn show_in_web(&mut self, document, container, ctx, app_state)` — browser-only
  (`#[cfg(all(feature = "webui", target_arch = "wasm32"))]`). Its **default body is
  `container.set_inner_html(&self.render_web(app_state))`** — i.e. "make DOM from the
  string". Stateful elements *override* it to update their existing DOM subtree in
  place (preserving focus/caret/scroll) instead of replacing it wholesale.

**Why string-first (pros):**
1. **Parity for free** — SSR and browser render byte-identical markup because both
   flow from the same `render_web`. No risk of the two modes drifting.
2. **Testable without a browser** — `render_web` is pure `&self -> String`, so unit
   tests assert on HTML substrings with no wasm, no DOM, no `web-sys`. This is the
   biggest practical win over a DOM-sink design.
3. **"DOM from a string" is the simple, supported path** — `Element::set_inner_html`
   builds the node tree for us; we hand-build nodes only in the rare in-place override.
4. **Event delegation, not closures** — one listener on the root reads the
   `data-lq-action` of the nearest ancestor of `event.target` and dispatches. No
   per-node `Closure` objects, so none of the closure-lifetime/leak bookkeeping a
   per-widget-callback design forces.
5. **Less code, alias-free recursion** — no sink type to build; the `&self` SSR path
   needs no `take_element`/`put_element` because it never mutates.

**Cons and how they are handled:**
1. *`set_inner_html` discards transient DOM state* (focus, caret, scroll, selection)
   for the replaced subtree. Handled two ways: (a) the browser driver re-renders a
   root **only when `AppRunner::needs_repaint()` is set** — not on every keystroke;
   (b) stateful widgets (`QueryConsoleElement`) **override `show_in_web`** to patch
   only their result/content `<div>`, leaving the `<input>` node — and its focus —
   untouched.
2. *`innerHTML` demands escaping discipline.* All interpolated text passes through
   `escape_html`; the only raw-markup path is our own trusted table/image builders.
3. *`set_inner_html` never executes `<script>`.* We rely on no inline scripts;
   hydration/delegation is wired in Rust by the driver, so this is a non-issue.
4. *Coarser than fine-grained diffing.* Acceptable for this scope (YAGNI); the
   `show_in_web` override is the escape hatch where in-place patching actually matters.

**Rejected alternative — a `WebUi` sink** (`enum WebSink { Ssr(String), Dom{..} }`
with a single `show_in_web(&mut WebUi)` dispatching internally). It produces the same
markup but cannot be unit-tested without wasm, reintroduces per-widget event closures
(lifetime/arena management), and is more code — so the string-first split is preferred.

**Browser update granularity (a consequence to make explicit).** `render_web` always
renders an element's *full* subtree (children included as strings) because SSR needs a
complete document from one call. In the browser this creates a tension: if a container
is re-rendered with the default `innerHTML`, it replaces its descendants' DOM too,
bypassing any child's `show_in_web` in-place override (and discarding e.g. the console's
input focus). The rule that resolves it: **the browser driver updates at the granularity
of the specific element the `AppRunner` targeted.** `AppRunner` already delivers
snapshots and evaluation results to a specific `UIHandle`; on such an update the driver
calls `render_element_dom(that_handle)`, which invokes *that element's* `show_in_web`
(so the console's override patches only its result panel and keeps the `<input>`). A
whole-root re-render (`innerHTML` of a root container) is used only for **structural
changes** — a node added/removed/replaced in the tree — where transient descendant state
is expected to reset anyway. This keeps `render_web` a pure full-subtree function (great
for SSR and tests) while giving the browser fine enough granularity to preserve focus
where it matters. The exact dirty-tracking (mapping `AppRunner` targets to the minimal
set of `render_element_dom` calls) is an implementation detail settled in Phase 4.

## Data Structures

### New Structs / Enums

#### `WebAction` — a serializable interaction ("query as callback")

```rust
// liquers-lib/src/ui/web/action.rs
/// A user interaction, described as data rather than a closure.
///
/// The web backend never attaches Rust closures to individual DOM nodes. Instead,
/// each interactive element renders a `data-lq-action='{json}'` attribute whose value
/// is a serialized `WebAction`. A single delegated listener on the mounted root reads
/// that attribute when an event fires and turns the action back into a call on the
/// existing `UIContext` (`submit_query`, `submit_query_current`, `request_quit`).
///
/// Because the action is plain serializable data, the *same* markup works for SSR
/// (the attribute is emitted for a future hydration script) and for the live browser
/// (the delegated listener dispatches it immediately). This mirrors the framework's
/// "events are queries" philosophy from `UI_INTERFACE_FSD.md`: an interaction is a
/// query to run, not imperative UI code, so it is portable and inspectable.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WebAction {
    /// Submit `query`, binding its result to the element identified by `handle`.
    /// Used for buttons/links that target a specific element (e.g. a preset button
    /// that refreshes the console that owns it).
    SubmitQuery { handle: UIHandle, query: String },
    /// Submit `query` bound to whichever element is currently focused/active. Used
    /// where the target is "the active element", not a statically known handle.
    SubmitToCurrent { query: String },
    /// Read the live text of the DOM input whose id is `input_id` at event time and
    /// submit it as a query for `handle`. This is how the query console's edit field
    /// submits on Enter without the renderer needing the (future) input value.
    SubmitInputValue { handle: UIHandle, input_id: String },
    /// Request application shutdown (maps to `UIContext::request_quit`).
    Quit,
}
```

`WebAction` is the *only* new serializable type. It has no default match arm anywhere:
the driver's dispatch matches all four variants explicitly, so a new interaction kind
is a compile error until handled.

#### `MountHandle` — keeps a browser mount alive

```rust
// liquers-lib/src/ui/web/app.rs
/// Owns the resources that keep a browser mount running: the single delegated
/// event-listener closure and the root element it is bound to.
///
/// A wasm event listener created from Rust must outlive the moment it is attached —
/// if its `Closure` is dropped, the browser callback becomes a dangling call and
/// panics. Rather than leak it with `Closure::forget` (which can never be cleaned up),
/// the mount returns a `MountHandle` that the caller stores. Dropping the handle
/// detaches the listener and releases the closure, so an app can cleanly unmount.
///
/// Only one closure is needed for the whole app because interactivity uses event
/// delegation, not per-node handlers — this is a direct benefit of the string-first
/// design.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub struct MountHandle {
    root: web_sys::Element,
    listener: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}
```

### `Html` string helpers (not a stateful sink)

```rust
// liquers-lib/src/ui/web/html.rs

/// Escape a string for safe interpolation into HTML text or a double-quoted
/// attribute value (`&`, `<`, `>`, `"`, `'`). Every piece of dynamic text rendered
/// by the web backend passes through this; it is the backend's single defense against
/// broken markup and injection, so element `render_web` implementations must never
/// interpolate untrusted text without it.
pub fn escape_html(s: &str) -> String;

/// Serialize a `WebAction` into the value of a `data-lq-action` attribute, i.e.
/// ` data-lq-action='{escaped json}'` (leading space, single-quoted, escaped). Returns
/// an empty string only if serialization fails, so a render can always be inlined.
pub fn action_attr(action: &WebAction) -> String;

/// Render any `Value` (base or `ExtValue`) to an HTML fragment. This is the internal
/// replacement for egui's `UIValueExtension::show`: it is a *free function*, not a
/// trait on `Value`, because the web backend only ever needs to render values that are
/// already wrapped inside a `UIElement` (see "Why no WebValueExtension"). It matches
/// every base + `ExtValue` variant explicitly (egui-only variants under `cfg`).
pub fn value_to_html(value: &Value) -> String;
```

**Ownership rationale (all structs):** `WebAction` is small and owned — it is cloned
into a `data-` attribute string and (in the browser) parsed back out, never shared.
`MountHandle` owns its `Closure` and holds the root `Element` by value (a `web-sys`
`Element` is a cheap reference-counted JS handle). The `html` helpers are stateless
free functions returning owned `String`s, so `render_web` implementations compose them
with `format!`/`push_str` without threading any context object.

## Trait Implementations

### `UIElement` — two new feature-gated rendering methods

Both backends are conditionally-compiled methods on the *same* shared trait; nothing
else about `UIElement` changes.

```rust
// liquers-lib/src/ui/element.rs  (modified)
#[typetag::serde]
pub trait UIElement: Send + Sync + std::fmt::Debug {
    // ... unchanged framework-agnostic methods (type_name, handle, title, update, ...)

    /// Render this element in egui (unchanged; now feature-gated).
    #[cfg(feature = "egui")]
    fn show_in_egui(&mut self, ui: &mut egui::Ui, ctx: &UIContext,
                    app_state: &mut dyn AppState) -> egui::Response { /* existing default */ }

    /// Produce this element's complete HTML subtree as a string.
    ///
    /// This is the single source of truth for how the element looks on the web, shared
    /// by server-side rendering and by the browser. It is intentionally pure and takes
    /// `&self`: rendering must not mutate element state, which is what lets SSR run it
    /// with only a shared borrow and lets it be unit-tested with no DOM present.
    ///
    /// Implementations escape all dynamic text via `escape_html`, wrap their root node
    /// with the stable id `ui-element-{handle}` (for CSS and event delegation), and
    /// embed any interactivity as `data-lq-action` attributes built with `action_attr`.
    /// Container elements recurse by borrowing each child from `app_state`
    /// (`get_element`) and concatenating the child's `render_web` — no locking and no
    /// extract-replace, because the whole call chain is immutable.
    ///
    /// The default renders a titled block, mirroring the egui default's "just show the
    /// title" behaviour, so an element with no web override still renders something
    /// meaningful.
    #[cfg(feature = "webui")]
    fn render_web(&self, _app_state: &dyn AppState) -> String {
        format!(
            "<div id=\"{}\" class=\"lq-element lq-{}\">{}</div>",
            crate::ui::web::element_dom_id(self.handle()),
            crate::ui::web::html::escape_html(self.type_name()),
            crate::ui::web::html::escape_html(&self.title()),
        )
    }

    /// Render/update this element in the live browser DOM (wasm only).
    ///
    /// The default body is `container.set_inner_html(&self.render_web(app_state))` — it
    /// builds the DOM subtree from the string produced by `render_web`, which keeps the
    /// browser output identical to SSR with zero extra code per element. Because this
    /// takes `&mut self`, the driver calls it through the extract-render-replace pattern
    /// (`take_element` → `show_in_web` → `put_element`), exactly as egui's
    /// `render_element` does, so the element can hold `&mut dyn AppState` to recurse
    /// into children without aliasing itself.
    ///
    /// Stateful elements override this to patch their existing DOM in place instead of
    /// replacing it — e.g. the query console updates only its result panel so the text
    /// `<input>` keeps focus and caret position across re-renders. Overrides may cache
    /// `web-sys` node handles in `#[serde(skip)]` fields.
    ///
    /// Returns `Result` because `web-sys` DOM calls are fallible (a missing node, a
    /// failed `set_inner_html`); errors are surfaced as `liquers_core::error::Error`,
    /// never `unwrap`ped.
    #[cfg(all(feature = "webui", target_arch = "wasm32"))]
    fn show_in_web(&mut self, document: &web_sys::Document, container: &web_sys::Element,
                   _ctx: &UIContext, app_state: &mut dyn AppState) -> Result<(), Error> {
        container
            .set_inner_html(&self.render_web(app_state));
        Ok(())
    }
}
```

**Implementors overriding `render_web`** (each `#[cfg(feature = "webui")]`): `Placeholder`,
`AssetViewElement`, `StateViewElement`, `QueryConsoleElement`, `MarkdownElement`,
`UISpecElement` — one override per element, beside its existing `show_in_egui`.
**Implementors overriding `show_in_web`** (for in-place DOM updates,
`#[cfg(all(feature = "webui", target_arch = "wasm32"))]`): initially only
`QueryConsoleElement` (to preserve input focus); all others inherit the innerHTML
default. **Bounds:** unchanged (`Send + Sync + Debug`, `typetag::serde`); the trait
stays object-safe because both new methods take concrete (non-generic) parameters, so
`Box<dyn UIElement>` storage in `AppState` is preserved.

### Why no `WebValueExtension` — `UIElement` is sufficient

The egui backend has `UIValueExtension` on `Value` with two roles: *rendering* a value
(`show`) and *producing* egui values (`from_ui`, `from_widget`). The web backend needs
**only the render half, and not as a trait**, for a structural reason:

- Every value that reaches the UI tree is already inside a `UIElement`.
  `AppState::insert_state` wraps any non-`UIElement` value in a `StateViewElement`, and
  `AssetViewElement`/`QueryConsoleElement` hold a `Value` field. There is no code path
  that renders a bare `Value` outside an element.
- Therefore the only value→markup needed is *inside* those elements' `render_web`, which
  call the free function `value_to_html(&Value) -> String` (in `web/html.rs`).
- The web backend introduces no new `ExtValue` variants, so it needs no value-*producing*
  constructors — the reason `UIValueExtension` is a trait in the first place.

Net: the public web rendering surface is exactly `UIElement::{render_web, show_in_web}`.
No new public trait on `Value` is added.

## ExtValue Extensions (feature-gating, not new variants)

No new persistent `ExtValue` variants. The **egui-typed** variants become optional so
they vanish from a webui-only build:

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
without egui. Every exhaustive `match` over `ExtValue` in `value/mod.rs` (`identifier`,
`type_name`, `default_extension`, `default_filename`, `as_image`, `as_polars_dataframe`,
`as_ui_element`, serializer arms) gains `#[cfg(feature = "egui")]` arms for these two
variants — keeping the "no default match arm" rule intact while making the arms
conditional. `value_to_html` renders `Image` (→ `<img>` data-URI), `PolarsDataFrame`
(→ `<table>`), `UIElement` (→ delegate to `render_web`) on all builds, and the two egui
variants only under `#[cfg(feature = "egui")]` (as an inert placeholder such as
`<div class="lq-egui-only">egui widget</div>`, since they have no web form).

## Generic Parameters & Bounds

The web module is almost entirely non-generic (like `AppState`, `WebAction`, the `html`
helpers). The only generic surface is the driver glue, reusing the existing `AppRunner`
bounds verbatim:

```rust
// mount_web / render_app_ssr driver
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>,
```

**Bound justification:** identical to `AppRunner<E>` — `Environment<Value = Value>` to
evaluate queries into `Value`, `UIPayload + From<SimpleUIPayload>` to construct the
per-query payload. No new bounds introduced. `render_web`/`show_in_web` are concrete
(non-generic) so `UIElement` stays object-safe.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `render_web` | No | Pure synchronous string building over an immutable borrow. |
| `show_in_web` | No | Synchronous DOM mutation on the browser main thread. |
| `value_to_html`, `html::*`, `widgets::*`, `dataframe_to_html` | No | Pure string builders. |
| `render_element_web` (string) | No | Immutable read of one element; no lock taken (caller holds it). |
| `render_element_dom` (browser) | No | Extract-render-replace via `try_sync_lock`; single-threaded. |
| `render_app_ssr` | Yes | Locks the async `tokio::sync::Mutex<dyn AppState>`; may await. |
| `mount_web` | Yes | Drives the async `AppRunner::run` loop and query evaluation. |

**Pattern:** rendering is synchronous (string building / DOM writes); evaluation and
state polling stay async and reuse `AppRunner` untouched.

## Function Signatures

### Module: `liquers_lib::ui::web` (new)

```rust
// mod.rs
pub mod action;
pub mod app;
pub mod dataframe;
pub mod html;
pub mod widgets;

pub use action::WebAction;
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub use app::{mount_web, MountHandle};
pub use app::render_app_ssr;

/// Stable DOM id for an element: `ui-element-{n}`, or `ui-element-unset` before init.
/// Used both as the CSS/query hook and as the anchor event delegation walks up to.
pub fn element_dom_id(handle: Option<UIHandle>) -> String;

/// SSR helper: render one element (by handle) to an HTML string, resolving it from the
/// given immutable AppState borrow. Returns a small placeholder string for a pending
/// (element=None) or missing node. Mirrors `render_element`, but needs no lock and no
/// extract-replace because `render_web` is immutable.
#[cfg(feature = "webui")]
pub fn render_element_web(handle: UIHandle, app_state: &dyn AppState) -> String;

/// Browser helper: update one element's DOM subtree under `container` (by handle) via
/// the extract-render-replace pattern (`take_element` → `show_in_web` → `put_element`),
/// exactly like egui's `render_element`. wasm-only.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub fn render_element_dom(
    document: &web_sys::Document,
    container: &web_sys::Element,
    handle: UIHandle,
    ctx: &UIContext,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
) -> Result<(), Error>;
```

```rust
// app.rs
/// Browser entry point. Renders all roots of `app_state` under `root`, attaches the
/// single delegated event listener that turns `data-lq-action` attributes into
/// `UIContext` calls, then drives `AppRunner::run` and re-renders roots whenever
/// `needs_repaint()` is set. Returns a `MountHandle` the caller must keep alive to keep
/// the listener attached; dropping it unmounts. wasm-only.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub async fn mount_web<E>(
    root: web_sys::Element,
    envref: liquers_core::context::EnvRef<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender,
    receiver: AppMessageReceiver,
    initial_query: Option<String>,
) -> Result<MountHandle, Error>
where
    E: Environment<Value = Value>,
    E::Payload: UIPayload + From<SimpleUIPayload>;

/// Server-side entry point. Locks `app_state`, renders every root via
/// `render_element_web`, and returns the concatenated HTML fragment (non-interactive;
/// `data-lq-action` attributes remain for a future hydration script). Available on all
/// targets.
#[cfg(feature = "webui")]
pub async fn render_app_ssr(
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>,
) -> Result<String, Error>;
```

```rust
// widgets.rs — web analogs of egui/widgets.rs helpers, all returning HTML strings
pub fn status_html(status: liquers_core::metadata::Status) -> String;
pub fn progress_html(progress: &liquers_core::metadata::ProgressEntry) -> String; // matches AssetInfo.progress
pub fn asset_info_html(info: &liquers_core::metadata::AssetInfo) -> String;
pub fn error_html(error: &Error) -> String;
pub fn query_to_html(query: &str) -> String;   // syntax-highlighted <span> markup

// dataframe.rs
pub fn dataframe_to_html(df: &polars::frame::DataFrame, max_rows: usize) -> String;
```

**Parameter choices:** string builders take borrowed inputs (`&str`, `&ProgressEntry`,
`&DataFrame`) and return owned `String`; `WebAction` is passed by value (small, owned).
The browser driver holds `Arc<tokio::sync::Mutex<dyn AppState>>` (shared, async);
`render_element_web` takes a plain `&dyn AppState` because its caller already holds the
lock.

## Integration Points

### Crate: liquers-lib

**New files** (all under `#[cfg(feature = "webui")]`):
`src/ui/web/mod.rs`, `html.rs`, `action.rs`, `widgets.rs`, `dataframe.rs`, `app.rs`.

**Modify `src/ui/mod.rs`:**
```rust
#[cfg(feature = "webui")]
pub mod web;
#[cfg(feature = "webui")]
pub use web::{render_element_web, WebAction};
```

**Modify `src/lib.rs`:** `#[cfg(feature = "egui")] pub mod egui;` (egui compiled only
under the `egui` feature).

**Modify `src/ui/element.rs`:** gate `show_in_egui` with `#[cfg(feature = "egui")]`; add
gated `render_web` + `show_in_web`; remove the stray `use egui::debug_text::print;`
(line 3); gate `AssetViewElement`/`StateViewElement` egui rendering and add their
`render_web`.

**Modify each widget element** (`query_console_element.rs`, `markdown_element.rs`,
`ui_spec_element.rs`): gate the existing `show_in_egui` + egui imports with
`#[cfg(feature = "egui")]`; add gated `render_web` (and, for the console, `show_in_web`).

**Modify `src/value/mod.rs`:** gate `ExtValue::UiCommand`/`Widget` and their match arms
with `#[cfg(feature = "egui")]`.

**Modify `src/ui/shortcuts.rs`:** gate `Key::to_egui` (and the egui import) with
`#[cfg(feature = "egui")]`; the rest is framework-agnostic.

**Required wasm-compat fix (browser mode):** `AssetViewElement::from_asset_ref`
(`element.rs:323`) and `QueryConsoleElement::schedule_volatile_refresh`
(`query_console_element.rs:149`) call `tokio::spawn` directly, unavailable on wasm. Route
them through the existing `crate::ui::spawn_ui_task` helper so browser mode compiles and
runs. (No behavior change on native.)

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

**Version rationale:** egui versions unchanged (only made optional). Verified `eframe`
is not referenced anywhere under `liquers-lib/src/` (only by example binaries), so gating
it under `egui` cannot break the library build. `wasm-bindgen` `0.2` / `web-sys` `0.3` /
`wasm-bindgen-futures` `0.4` are the standard trio; `wasm-bindgen-futures` is already
referenced by `ui/mod.rs::spawn_ui_task` under `cfg(target_arch = "wasm32")` but is
missing from the manifest, so adding it also fixes an existing latent wasm break.
`egui_commonmark` (used by `markdown_element`) moves under `egui`; the web markdown
renderer must not depend on it.

## Relevant Commands

### New Commands

**None. Decided.** `liquers_lib::ui` is the single shared UI layer for all backends. A
backend contributes *only* conditionally-compiled rendering methods on the `UIElement`
trait (`render_web`/`show_in_web` beside `show_in_egui`) plus its render helpers — **no
new command namespace and no changes to `lui`**. The web backend works "out of the box"
against the existing tree.

**Verified against the codebase:**
- The UI core (`ui/commands.rs`, `runner.rs`, `app_state.rs`, `message.rs`, `resolve.rs`,
  `payload.rs`, `handle.rs`) references no egui at all — `lui` builds/navigates the tree
  framework-agnostically, so it renders unchanged in a webui build.
- egui coupling inside `ui/` is confined to exactly 5 files (`element.rs`, `shortcuts.rs`,
  and the 3 widget elements). These are precisely the files that gain a `render_web`
  (and gated `show_in_web`) peer, matching the "extra methods per backend" model.

**Note (conscious scope boundary):** the egui reference also ships value-producing
commands in the separate `liquers_lib::egui` module (`label`, `text_editor`,
`show_asset_info`) that emit egui-only `ExtValue::UiCommand`/`Widget`. These are out of
scope and gated out of a webui build; a query written specifically against them has no
web representation in a webui-only build — expected. Trees built with `lui` render fully.

### Relevant Existing Namespaces

| Namespace | Relevance | Key Commands |
|-----------|-----------|--------------|
| `lui` | **Primary.** Builds/navigates the tree the web backend renders. Framework-agnostic; unchanged. | `add`, `remove`, `query_console`, `markdown`, `ui_spec`, `children`, `first`, `last`, `parent`, `next`, `prev`, `roots`, `activate` |
| `egui` | **Reference only** (not compiled under webui). Value-producing render commands with no web peer. | `label`, `text_editor`, `show_asset_info` |
| root/core value commands | Produce the base `Value`s that `value_to_html` renders. | `text`, `json`, ... |

**User-confirmed:** `lui` is the correct primary namespace, and no `web`/`lweb`
value-producing command namespace is added — backends contribute rendering methods only.

## Web Endpoints (if applicable)

No new HTTP routes in this feature. `render_app_ssr` returns an HTML string designed to
be embeddable by a future `liquers-axum` handler, but no axum wiring ships here
(explicitly out of Phase 1 scope per the high-level design).

## Error Handling

Uses `liquers_core::error::Error` with typed constructors only:

| Scenario | Constructor | Example |
|----------|-------------|---------|
| Missing DOM root / node | `Error::general_error` | `Error::general_error("web root element not found".to_string())` |
| `set_inner_html` / web-sys failure | `Error::general_error` | map the `JsValue` to a message string |
| DataFrame→HTML failure | `Error::from_error` | `Error::from_error(ErrorType::General, polars_err)` |
| SSR lock unavailable | `Error::general_error` | reuse `try_sync_lock`'s error |

`render_web` and the `html`/`widgets` builders are infallible (they return `String`;
value formatting cannot fail). The fallible pieces — `show_in_web`, `render_element_dom`,
`render_app_ssr`, `mount_web` — return `Result<_, Error>`. No `unwrap`/`expect` in
library code; `web-sys` calls returning `Result`/`Option` are mapped to `Error`.

## Serialization Strategy

- `WebAction` derives `Serialize, Deserialize` so it can be embedded in a
  `data-lq-action` attribute and parsed back by the browser dispatcher / a hydration
  script. It is the only new serializable web type.
- `MountHandle` and the `html`/`widgets` helpers are transient/runtime — not serializable.
- Element structs are unchanged; existing `#[serde(skip)]` runtime fields and typetag
  registration are reused. A `show_in_web` override that caches `web-sys` node handles
  stores them in new `#[serde(skip)]` fields (browser-only, reconstructed on first
  render); the innerHTML default caches nothing.

## Concurrency Considerations

- Rendering is single-threaded (DOM on the browser main thread; SSR string building on
  one task). No new shared state is introduced.
- Shared state remains `Arc<tokio::sync::Mutex<dyn AppState>>`, accessed via the existing
  async `AppRunner` and via `try_sync_lock` during the browser `render_element_dom` pass.
- The SSR path (`render_web`) is immutable and needs no `take/put`, so it cannot contend
  with itself.
- One delegated event listener (not per-node closures) means the only long-lived JS
  callback is the single `Closure` owned by `MountHandle` — no per-frame closure churn.

## Compilation Validation

Target build matrix (exercised in Phase 4):
- `cargo check -p liquers-lib` (default: egui on) — existing behavior, must be green.
- `cargo check -p liquers-lib --no-default-features --features webui,image-support`
  — webui-only, **no egui symbols compiled**.
- `cargo check -p liquers-lib --no-default-features --features egui,image-support`
  — egui-only, no web symbols.
- `cargo check -p liquers-lib --features egui,webui` — both backends coexist.
- (wasm) `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui,image-support`.

Expected at this stage: only "missing implementation" gaps, no design-level type errors.

## References to liquers-patterns.md

- [x] Crate dependency flow respected (all changes in liquers-lib).
- [x] No new top-level value enums; egui variants gated, no new ExtValue variants.
- [x] Commands unchanged — reuse `register_command!`-based `lui` namespace.
- [x] UIElement pattern followed (new gated trait methods + per-element overrides;
      trait stays object-safe).
- [x] Error handling via typed constructors (`general_error`, `from_error`).
- [x] Async default preserved (AppRunner reused); rendering sync by necessity.
- [x] Match statements explicit; egui arms gated, no `_ =>` default arms.
- [x] No `unwrap`/`expect` in library code; `#[serde(skip)]` for transient fields.
