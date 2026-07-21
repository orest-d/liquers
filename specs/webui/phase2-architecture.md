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
live DOM via `set_inner_html`). Interactivity is carried in the markup as a
serialized, framework-agnostic **`UiAction`** (a unification of the existing
`MenuAction`) in `data-lq-action` attributes; the browser driver attaches a single
delegated event listener that turns those into `UIContext` calls — so there are no
per-widget closures. A prerequisite deliverable makes egui optional (a new `egui`
Cargo feature). Finally — and this is the load-bearing risk — the core evaluation
engine uses `tokio::spawn`/`tokio::time`; the plan (Option A) is to keep using tokio on
wasm via its `wasm32` support on a current-thread runtime, and **prove it runs in the
browser by test** (see "Browser Runtime & Workflow").

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
  string". Stateful elements *override* it to update their existing DOM in place.

**`show_in_web` may diverge from `render_web` on purpose.** The default delegates to
`render_web` so simple elements get browser rendering for free, but an override is
free to build or patch the DOM **independently of `render_web`** — for performance
(touch only the nodes that changed instead of re-serializing a big subtree) or to
**preserve interactive state** (keep a text `<input>`'s focus/caret, a scroll
position, an open `<details>`). `render_web` remains mandatory (it is the SSR source
of truth and what unit tests assert on); `show_in_web` is the optional, browser-only
optimization/interaction hook and carries no obligation to call `render_web`.

**Why string-first (pros):**
1. **Parity for free** — SSR and browser render byte-identical markup because both
   flow from the same `render_web`. No risk of the two modes drifting.
2. **Testable without a browser** — `render_web` is pure `&self -> String`, so unit
   tests assert on HTML substrings with no wasm, no DOM. Biggest practical win.
3. **"DOM from a string" is the simple, supported path** — `Element::set_inner_html`
   builds the node tree for us; we hand-build nodes only in the rare in-place override.
4. **Event delegation, not closures** — one listener on the root reads the
   `data-lq-action` of the nearest ancestor of `event.target` and dispatches. No
   per-node `Closure` objects, so none of the closure-lifetime/leak bookkeeping.
5. **Less code, alias-free recursion** — no sink type; the `&self` SSR path needs no
   `take_element`/`put_element` because it never mutates.

**Cons and how they are handled:**
1. *`set_inner_html` discards transient DOM state* for the replaced subtree. Handled:
   (a) re-render a root only on `AppRunner::needs_repaint()`; (b) stateful widgets
   override `show_in_web` to patch in place (see below).
2. *`innerHTML` demands escaping discipline* — all interpolated text passes through
   `escape_html`; the only raw path is our own trusted table/image builders.
3. *`set_inner_html` never executes `<script>`* — non-issue; delegation is wired in Rust.
4. *Coarser than fine-grained diffing* — acceptable (YAGNI); the override is the hatch.

**Rejected alternative — a `WebUi` sink** (`enum WebSink { Ssr(String), Dom{..} }`
with one `show_in_web(&mut WebUi)` dispatching internally): same markup, but not
unit-testable without wasm, reintroduces per-widget closures, more code.

**Browser update granularity (a consequence to make explicit).** `render_web` always
renders an element's *full* subtree because SSR needs a complete document from one
call. In the browser this means the driver must update at **the granularity of the
element the `AppRunner` targeted**: `AppRunner` delivers snapshots/results to a
specific `UIHandle`, and on such an update the driver calls `render_element_dom(that
handle)`, invoking *that element's* `show_in_web` (so a console patches only its
result panel, keeping the `<input>` focused). Whole-root `innerHTML` is used only for
**structural changes** (a node added/removed/replaced), where descendant transient
state resets anyway. Exact dirty-tracking is a Phase 4 implementation detail.

## Data Structures

### `UiAction` — shared, framework-agnostic action (unifies `MenuAction` + web)

The existing `MenuAction` (`None | Quit | Query(String)` in `ui_spec_element.rs`) and
the web backend's interaction needs are the *same concept*: "what a control does when
triggered." Rather than a web-only `WebAction`, this design promotes it to a single
reusable type in the shared UI layer, used by egui menus, the web backend, and any
future backend. (Backend-specific actions are deliberately *not* modelled now; when a
concrete need arises, a variant is added then — see "Future extension" below.)

```rust
// liquers-lib/src/ui/action.rs  (new, shared — NOT web-specific)
/// What a UI control does when triggered, as portable data rather than a closure.
///
/// This is the one action type across all backends: `ui_spec` menus, the web backend's
/// `data-lq-action` attributes, and egui buttons all interpret the same `UiAction`.
/// Keeping actions as serializable data (not Rust closures) is what makes them work
/// uniformly for SSR (emit as an attribute for later hydration), for the live browser
/// (a delegated listener dispatches them), and for egui (a click handler runs them) —
/// and it matches the framework's "events are queries" philosophy from
/// `UI_INTERFACE_FSD.md`, where an interaction is a query to run, not imperative code.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    /// Do nothing. The default; also the target of a disabled/placeholder control.
    None,
    /// Request application shutdown (maps to `UIContext::request_quit`, or a
    /// backend-appropriate equivalent such as closing the browser tab's app root).
    Quit,
    /// Submit `query` bound to the element that owns the control (its own handle, or
    /// the active element if the control has none). Covers the common
    /// "run this query here" case and matches `MenuAction::Query`.
    Query(String),
    /// Submit `query` bound to an explicitly named element `handle`. Used when a
    /// control targets a *different* element than the one it renders in (e.g. an
    /// orthodox-commander button that refreshes the opposite pane).
    QueryOn { handle: UIHandle, query: String },
    /// Read the live text of the DOM/input control named `input_id` at trigger time
    /// and submit it as a query for `handle`. This is how a text field submits on
    /// Enter without the renderer having to know the (future) typed value; on egui it
    /// maps to reading the field's buffer.
    SubmitInput { handle: UIHandle, input_id: String },
}
```

**Future extension:** if a backend later needs an action with no shared meaning (e.g. a
browser-only "copy to clipboard"), a dedicated variant is added at that point. It is left
out now to avoid an untyped `serde_json::Value` escape hatch before there is a concrete use.

**No default match arm** anywhere: every dispatcher matches all five variants, so a new
action kind is a compile error until handled by each backend. **`MenuAction` migration:**
`MenuAction` is replaced by `UiAction` (or kept as a thin deprecated alias). Its custom
YAML `Deserialize` (accepting `null` → `None`, `"quit"` → `Quit`, `{query: "..."}` →
`Query`) is preserved on `UiAction` so existing `ui_spec` YAML keeps working unchanged;
`ui_spec_element.rs`'s `handle_menu_action` becomes a shared `dispatch_action(&UiAction,
&UIContext, own_handle)` used by both egui and web.

### `MountHandle` — keeps a browser mount alive

```rust
// liquers-lib/src/ui/web/app.rs
/// Owns the resources that keep a browser mount running: the single delegated
/// event-listener closure and the root element it is bound to.
///
/// A wasm event listener created from Rust must outlive the moment it is attached — if
/// its `Closure` is dropped, the browser callback dangles and panics. Rather than leak
/// it with `Closure::forget` (never reclaimable), the mount returns a `MountHandle` the
/// caller stores. Dropping the handle detaches the listener and frees the closure, so an
/// app can cleanly unmount. Only one closure is needed for the whole app because
/// interactivity uses event delegation, not per-node handlers — a direct benefit of the
/// string-first design.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub struct MountHandle {
    root: web_sys::Element,
    listener: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}
```

### `Html` string helpers (not a stateful sink)

```rust
// liquers-lib/src/ui/web/html.rs

/// Escape a string for safe interpolation into HTML text or a double-quoted attribute
/// value (`&`, `<`, `>`, `"`, `'`). Every piece of dynamic text rendered by the web
/// backend passes through this; it is the backend's single defense against broken markup
/// and injection, so `render_web` implementations must never interpolate untrusted text
/// without it.
pub fn escape_html(s: &str) -> String;

/// Serialize a `UiAction` into a `data-lq-action='{escaped json}'` attribute fragment
/// (leading space, single-quoted, escaped). Returns an empty string only if
/// serialization fails, so a render can always be inlined.
pub fn action_attr(action: &UiAction) -> String;

/// Render any `Value` (base or `ExtValue`) to an HTML fragment. This is the internal
/// replacement for egui's `UIValueExtension::show`: a *free function*, not a trait on
/// `Value`, because the web backend only ever renders values that are already wrapped
/// inside a `UIElement` (see "Why no WebValueExtension"). Matches every base + `ExtValue`
/// variant explicitly (egui-only variants under `cfg`).
pub fn value_to_html(value: &Value) -> String;
```

**Ownership rationale:** `UiAction` is small and owned — cloned into a `data-` attribute
string, parsed back in the browser, never shared. `MountHandle` owns its `Closure` and
holds the root `Element` by value (a cheap ref-counted JS handle). The `html` helpers are
stateless free functions returning owned `String`s.

## Trait Implementations

### `UIElement` — two new feature-gated rendering methods

Both backends are conditionally-compiled methods on the *same* shared trait; nothing else
about `UIElement` changes.

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
    /// This is the single source of truth for how the element looks on the web, shared by
    /// server-side rendering and by the browser. It is intentionally pure and takes
    /// `&self`: rendering must not mutate element state, which is what lets SSR run it with
    /// only a shared borrow and lets it be unit-tested with no DOM present.
    ///
    /// Implementations escape all dynamic text via `escape_html`, wrap their root node with
    /// the stable id `ui-element-{handle}` (for CSS and event delegation), and embed any
    /// interactivity as `data-lq-action` attributes (`action_attr`). Container elements
    /// recurse by borrowing each child from `app_state` (`get_element`) and concatenating the
    /// child's `render_web` — no locking and no extract-replace, because the whole call chain
    /// is immutable. The default renders a titled block (mirroring the egui default), so an
    /// element with no web override still renders something meaningful.
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
    /// The default body is `container.set_inner_html(&self.render_web(app_state))` — it builds
    /// the DOM subtree from the string, keeping browser output identical to SSR with zero extra
    /// code per element. Because this takes `&mut self`, the driver calls it through
    /// extract-render-replace (`take_element` → `show_in_web` → `put_element`), exactly like
    /// egui's `render_element`, so the element can hold `&mut dyn AppState` to recurse without
    /// aliasing itself. Stateful elements override this to patch their existing DOM in place —
    /// independently of `render_web` if they wish — e.g. the query console updates only its
    /// result panel so the text `<input>` keeps focus and caret. Overrides may cache `web-sys`
    /// node handles in `#[serde(skip)]` fields. Returns `Result` because `web-sys` DOM calls are
    /// fallible; errors become `liquers_core::error::Error`, never `unwrap`ped.
    #[cfg(all(feature = "webui", target_arch = "wasm32"))]
    fn show_in_web(&mut self, document: &web_sys::Document, container: &web_sys::Element,
                   _ctx: &UIContext, app_state: &mut dyn AppState) -> Result<(), Error> {
        container.set_inner_html(&self.render_web(app_state));
        Ok(())
    }
}
```

**Implementors overriding `render_web`** (each `#[cfg(feature = "webui")]`): `Placeholder`,
`AssetViewElement`, `StateViewElement`, `QueryConsoleElement`, `MarkdownElement`,
`UISpecElement`. **Overriding `show_in_web`** (in-place DOM,
`#[cfg(all(feature = "webui", target_arch = "wasm32"))]`): initially only
`QueryConsoleElement`. **Bounds:** unchanged; the trait stays object-safe (both new methods
take concrete, non-generic parameters), so `Box<dyn UIElement>` storage in `AppState` is
preserved.

### Why no `WebValueExtension` — `UIElement` is sufficient

Every value that reaches the UI tree is already inside a `UIElement`
(`AppState::insert_state` wraps non-`UIElement` values in `StateViewElement`;
`AssetViewElement`/`QueryConsoleElement` hold a `Value` field). So bare-value rendering only
happens *inside* those elements' `render_web`, via the free function
`value_to_html(&Value) -> String`. The web backend adds no value-*producing* constructors
(the reason egui's `UIValueExtension` is a trait), so it needs no trait on `Value`. Public web
surface = `UIElement::{render_web, show_in_web}`.

## ui Module Usage Map & Adaptation

This maps every part of `liquers_lib::ui` against webui, per the "verify it all works"
request. Legend: **reuse** = compiles/works unchanged; **gate** = wrap egui bits in
`#[cfg(feature="egui")]`; **web** = add `render_web` (+ maybe `show_in_web`); **wasm** =
needs a wasm-runtime adaptation (see next section).

| Part | For SSR | For browser | Notes |
|------|---------|-------------|-------|
| `app_state.rs` (AppState, DirectAppState, NodeData) | reuse | reuse | egui-free; the tree + serialization are backend-agnostic. |
| `handle.rs` (UIHandle) | reuse | reuse | Used verbatim as the `ui-element-{n}` DOM id. |
| `message.rs` (AppMessage, channel, AssetSnapshot) | reuse | reuse | egui-free (only a doc comment mentions egui). |
| `resolve.rs` (InsertionPoint, navigation) | reuse | reuse | Pure tree math. |
| `payload.rs` (UIPayload, SimpleUIPayload) | reuse | reuse | egui-free. |
| `ui_context.rs` (UIContext) | reuse | reuse | The event-delegation dispatcher calls its `submit_query`/`request_quit`. |
| `commands.rs` (`lui` namespace) | reuse | reuse | egui-free; builds/navigates the tree. |
| `runner.rs` (AppRunner) | reuse | **wasm** | Logic reused; but Phase-2/3 use `evaluate` and the asset engine spawns tokio tasks — see runtime section. |
| `shortcuts.rs` | gate | gate + web | `Key::to_egui` gated; keyboard handling in the browser reads `KeyboardEvent` and maps to the same `KeyboardShortcut`/`UiAction`. |
| `element.rs` `Placeholder` | web | web | Trivial `render_web` (title block). |
| `element.rs` `AssetViewElement` | web | web + **wasm** | `render_web` for progress/value/error/metadata; its `from_asset_ref` uses `tokio::spawn` → route via `spawn_ui_task`. |
| `element.rs` `StateViewElement` | web | web | `render_web` delegates to `value_to_html`. |
| `widgets/markdown_element.rs` | web | web | Web markdown → HTML via a pure renderer (e.g. `pulldown-cmark`), *not* `egui_commonmark`; gate the egui path. |
| `widgets/query_console_element.rs` | web | web + **wasm** | `render_web` (toolbar+content) + `show_in_web` override (preserve input focus); `schedule_volatile_refresh` uses `tokio::spawn`+`tokio::time::sleep` → wasm timer. |
| `widgets/ui_spec_element.rs` | web | web | Menu bar + layouts (horizontal/vertical/grid/tabs/windows) → HTML/CSS; menus emit `UiAction` in `data-lq-action`; layouts map to fl/grid CSS. |
| `value/mod.rs` `ExtValue` render | web | web | `value_to_html` handles base + Image/DataFrame/UIElement; egui variants gated. |

**Identified rendering-side issues (tractable):**
- *Markdown*: `egui_commonmark` is egui-only. The web renderer needs a pure markdown→HTML
  crate (`pulldown-cmark`, added under the `webui` feature) — a small, isolated addition.
- *ui_spec layouts*: `Windows`/`Tabs` are trivial in egui but need HTML/CSS equivalents
  (tabs = buttons + shown panel; windows = absolutely-positioned draggable divs, or,
  for a first cut, plain stacked panels). `Windows` drag is a browser nicety deferred.
- *Keyboard shortcuts*: egui consumes shortcuts inside its frame; the browser must add a
  `keydown` listener mapping to `KeyboardShortcut` → `UiAction`. Shared `shortcuts.rs`
  parsing is reused; only the event source differs.
- *Focus preservation*: handled by the `QueryConsoleElement::show_in_web` override.

## Browser Runtime & Workflow

### The load-bearing issue: async runtime on wasm

The core evaluation engine is built on tokio's runtime. Stock tokio *does* target
`wasm32-unknown-unknown` (current-thread `rt` + `time` + `sync` + `macros`), so the plan is
to keep it — but this must be configured correctly and proven by test. The spawn/timer sites
that must run in the browser:

- `liquers-core/src/assets.rs` — **~20** `tokio::spawn` / `tokio::time::sleep` sites
  (in-flight evaluation, service-message pumps, expiration monitor).
- `liquers-core/src/context.rs` — 2 `tokio::spawn` sites.
- `liquers-lib/src/environment.rs` — `init_with_envref` spawns `load_command_versions`.
- `liquers-lib/src/ui/element.rs:323`, `query_console_element.rs:149` — UI-layer spawns
  (+ a `tokio::time::sleep`).

`tokio::spawn` needs an active runtime context and `tokio::time` needs the timer driver.
**SSR is unaffected** (it runs on a native server with a real tokio runtime); this is
strictly a **browser-mode** problem, and it is the primary gate on a working browser
example.

**Chosen resolution (Option A — keep tokio on wasm, verify by test).** tokio has partial
`wasm32-unknown-unknown` support and the plan is to **keep the code calling `tokio::*`
unchanged**, running on a tokio **current-thread runtime** entered so `tokio::spawn` /
`tokio::time` have a context. The `spawn_ui_task` helper (native `tokio::spawn` vs
`wasm_bindgen_futures::spawn_local`) still applies at the UI layer; the core keeps raw
`tokio::spawn`.

**tokio features actually needed on wasm (verified against the source, not just the
manifest):**
- `sync` — used (`tokio::sync::{Mutex, RwLock, mpsc}`, ~10 sites). Runtime-agnostic; wasm-fine.
- `macros` — used (`tokio::select!` in `assets.rs`, 2 non-test sites; plus `#[tokio::test]`).
- `rt` — needed for `tokio::spawn` and to enter a current-thread runtime.
- `time` — used (`tokio::time::sleep`); the one whose wasm timer must be confirmed by test.
- `fs` — used **only** by `AsyncFileStore` in `store.rs` (a native filesystem store, useless
  in the browser). It is currently gated only by `#[cfg(feature = "async_store")]`, which also
  provides `AsyncMemoryStore` (wanted on wasm). Fix: additionally gate `AsyncFileStore` (and
  its `tokio::fs` block) with `#[cfg(not(target_arch = "wasm32"))]`, then drop `fs` on wasm.
- `net`/`io`/`rt-multi-thread` — unused.

So the manifest split is `sync + rt + macros + time` on wasm, `+ fs` on native:

  ```toml
  [target.'cfg(not(target_arch = "wasm32"))'.dependencies]
  tokio = { version = "1.47.1", features = ["sync", "rt", "macros", "time", "fs"] }
  [target.'cfg(target_arch = "wasm32")'.dependencies]
  tokio = { version = "1.47.1", features = ["sync", "rt", "macros", "time"] }  # no fs/net
  ```

- The browser bootstrap enters a current-thread `tokio::runtime::Runtime` before spawning
  (or drives the top-level future via `spawn_local` with the runtime entered). `mount_web`
  owns this.

**JobQueue on wasm — trivial but sound.** `JobQueue<E>` is capacity-based (`running_count`,
`capacity`, default 4) using `tokio::spawn` + `AtomicUsize` — no `spawn_blocking`, no
`rt-multi-thread`. On the single-threaded wasm runtime there is no real parallelism, so the
`AssetManager` is constructed with **capacity 1**: jobs are serialized (start-one, then the
next), which is exactly what a single thread wants. The queue machinery itself is unchanged
and works as-is; capacity 1 is just the natural degenerate case, not a special code path.

**This must be proven, not assumed.** Whether stock tokio's `spawn`/`time` actually run
under the browser event loop — including the `AssetManager`'s job queue and expiration
monitor — is the **top Phase-3/4 risk and a gating success criterion**: the Playwright
example only passes if the async evaluation engine actually turns on wasm.

**Fallbacks if plain tokio proves insufficient on wasm** (documented, not adopted now):
(A′) swap in `tokio_with_wasm` — a drop-in that re-exports tokio's `spawn`/`time`/`sync`
backed by `wasm-bindgen-futures`/browser timers — via the same cfg-gated dependency alias,
zero code change; (B) isolate spawning/timing behind an `Environment`-provided `Spawn` seam
(investigated: feasible, but a moderate refactor of ~23 core sites and threading a spawner
into `AssetManager` construction). These are the contingency, kept on the shelf; Option A is
the path.

### How the workflow runs in a browser (high level)

1. **Load.** The page loads a wasm bundle (built by `trunk`); wasm-bindgen calls the
   exported `start()`.
2. **Set up (Rust, in `start`).** Build the `Environment` (memory store / trivial recipe
   provider — no native-only backends), register the app's commands + `lui`, build a
   `DirectAppState` with a root element, wrap it `Arc<tokio::sync::Mutex<…>>`, create the
   `UIContext` + `AppRunner` + message channel. (Identical shape to the egui example's
   setup, minus `eframe` and `Runtime::new()`.)
3. **Mount.** `mount_web(root, …)` renders all roots into the root `<div>` via
   `render_element_web` + `set_inner_html`, and attaches the single delegated listener.
4. **Drive.** Instead of eframe's frame loop, `mount_web` runs an async loop scheduled by
   `requestAnimationFrame` (via `web-sys`, no extra dep): each tick `await`s
   `AppRunner::run(&app_state)` (process messages, evaluate pending nodes, poll
   evaluations, deliver snapshots), then, when `needs_repaint()`, re-renders the affected
   element(s) via `render_element_dom`. All async work runs on the wasm executor (the
   current-thread tokio runtime), single-threaded.
5. **Interact.** A click/keydown bubbles to the delegated listener, which finds the nearest
   `[data-lq-action]`, deserializes the `UiAction`, and calls the matching `UIContext`
   method (e.g. `submit_query`). That enqueues a query; the next `run()` tick evaluates it
   and updates the tree; the following repaint reflects it in the DOM. This is exactly the
   egui flow (`ui.button().clicked()` → `ctx.submit_query`) with the browser event system
   substituted for egui's immediate-mode input.

## Function Signatures

### Module: `liquers_lib::ui::web` (new) + `liquers_lib::ui::action` (new, shared)

```rust
// ui/action.rs  (shared)
pub enum UiAction { /* as above */ }
/// Dispatch a UiAction against the UI: submit queries / request quit, using `own_handle`
/// as the default target for `Query`. Shared by egui and web click handling.
pub fn dispatch_action(action: &UiAction, ctx: &UIContext, own_handle: Option<UIHandle>);

// ui/web/mod.rs
pub mod app;
pub mod dataframe;
pub mod html;
pub mod widgets;

/// Stable DOM id for an element: `ui-element-{n}`, or `ui-element-unset` before init.
pub fn element_dom_id(handle: Option<UIHandle>) -> String;

/// SSR helper: render one element (by handle) to HTML from an immutable AppState borrow.
/// Small placeholder for a pending/missing node. Needs no lock and no extract-replace.
#[cfg(feature = "webui")]
pub fn render_element_web(handle: UIHandle, app_state: &dyn AppState) -> String;

/// Browser helper: update one element's DOM under `container` via extract-render-replace
/// (`take_element` → `show_in_web` → `put_element`). wasm-only.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub fn render_element_dom(document: &web_sys::Document, container: &web_sys::Element,
    handle: UIHandle, ctx: &UIContext,
    app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> Result<(), Error>;

// ui/web/app.rs
/// Browser entry point (wasm-only). Renders all roots under `root`, attaches the single
/// delegated `data-lq-action` listener, then drives `AppRunner::run` on a
/// requestAnimationFrame loop, re-rendering on `needs_repaint()`. Returns a `MountHandle`
/// the caller must keep alive.
#[cfg(all(feature = "webui", target_arch = "wasm32"))]
pub async fn mount_web<E>(root: web_sys::Element, envref: EnvRef<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>, sender: AppMessageSender,
    receiver: AppMessageReceiver, initial_query: Option<String>) -> Result<MountHandle, Error>
where E: Environment<Value = Value>, E::Payload: UIPayload + From<SimpleUIPayload>;

/// Server-side entry point (all targets). Locks `app_state`, renders every root via
/// `render_element_web`, returns the concatenated HTML fragment.
#[cfg(feature = "webui")]
pub async fn render_app_ssr(app_state: &Arc<tokio::sync::Mutex<dyn AppState>>) -> Result<String, Error>;

// ui/web/widgets.rs — web analogs of egui/widgets.rs, all returning HTML strings
pub fn status_html(status: liquers_core::metadata::Status) -> String;
pub fn progress_html(progress: &liquers_core::metadata::ProgressEntry) -> String; // matches AssetInfo.progress
pub fn asset_info_html(info: &liquers_core::metadata::AssetInfo) -> String;
pub fn error_html(error: &Error) -> String;
pub fn query_to_html(query: &str) -> String;   // syntax-highlighted <span> markup

// ui/web/dataframe.rs
pub fn dataframe_to_html(df: &polars::frame::DataFrame, max_rows: usize) -> String;
```

## Examples, Browser Setup & Testing

A **working browser example is an explicit success criterion.** The first target is a
webui port of `ui_spec_demo` (menu-driven dashboard), because it exercises init queries,
`lui` tree mutation, `UiAction` menu handling, and nested rendering with no store or rich
value dependencies.

### Where the example lives

Cargo `[[example]]` targets can't carry an `index.html`/wasm entry, so the browser example
is a small **`cdylib` example crate** — proposed `liquers-lib/examples-web/ui_spec_demo/`
(its own `Cargo.toml` depending on `liquers-lib` with `--no-default-features --features
webui`), containing `src/lib.rs` (the `#[wasm_bindgen(start)]` entry), `index.html`, and
`Trunk.toml`. The native egui `examples/ui_spec_demo.rs` stays as-is; the two share the
YAML specs and command definitions (factored into a tiny shared module or duplicated for a
first cut).

### Rust entry (`src/lib.rs`), setup code

```rust
use wasm_bindgen::prelude::*;
// ... liquers_lib imports (DefaultEnvironment, SimpleUIPayload, DirectAppState, UISpecElement,
//     AppRunner, UIContext, app_message_channel, ui::web::mount_web, register_lui_commands!)

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();               // readable panics in the console
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.get_element_by_id("app").unwrap();

    // 1. Environment + commands (no Runtime::new(); the wasm executor drives futures)
    let mut env = DefaultEnvironment::<Value, SimpleUIPayload>::new();
    env.with_trivial_recipe_provider();
    let envref = {
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn dashboard(state) -> result).map_err(err_to_js)?;
        liquers_lib::register_lui_commands!(cr).map_err(err_to_js)?;
        env.to_ref()
    };

    // 2. AppState with a root UISpecElement (same as the native example)
    let mut app_state = DirectAppState::new();
    let root_handle = app_state.add_node(None, 0, ElementSource::None).map_err(err_to_js)?;
    let spec = UISpec::from_yaml(DASHBOARD_YAML).map_err(err_to_js)?;
    let mut element = UISpecElement::from_spec("Dashboard".into(), spec);
    element.set_handle(root_handle);
    app_state.set_element(root_handle, Box::new(element)).map_err(err_to_js)?;

    // 3. Wire runner + mount + drive
    let app_state_arc: Arc<tokio::sync::Mutex<dyn AppState>> = Arc::new(tokio::sync::Mutex::new(app_state));
    let (tx, rx) = app_message_channel();
    let _mount = mount_web(root, envref, app_state_arc, tx.clone(), rx, None).await.map_err(err_to_js)?;
    std::mem::forget(_mount);   // keep the delegated listener alive for the app's lifetime
    Ok(())
}
```

### HTML + JavaScript glue

`index.html` (trunk finds the wasm via `data-trunk`):

```html
<!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8"/>
    <link data-trunk rel="rust" data-wasm-opt="s"/>
    <link data-trunk rel="css" href="app.css"/>
    <title>Liquers webui — UISpec demo</title>
  </head>
  <body>
    <div id="app"></div>
  </body>
</html>
```

Trunk auto-generates the JS loader, so no hand-written JS is required. The equivalent
**manual** JS (if not using trunk's injection) is just:

```html
<script type="module">
  import init from './pkg/ui_spec_demo.js';   // wasm-bindgen glue emitted by the build
  await init();                                // runs #[wasm_bindgen(start)] start()
</script>
```

### Build & run (trunk)

```bash
cargo install trunk                                   # once
rustup target add wasm32-unknown-unknown              # once
cd liquers-lib/examples-web/ui_spec_demo
trunk serve --open                                    # builds wasm + serves at http://127.0.0.1:8080
# production build → dist/ :
trunk build --release
```

### Testing with Playwright (success criterion)

A headless Playwright test drives the served example and asserts on the real DOM — this is
what makes "working example" objectively verifiable. Sketch (`tests/webui.spec.ts`, run
against `trunk serve`):

```ts
import { test, expect } from '@playwright/test';

test('ui_spec_demo renders and reacts to a menu action', async ({ page }) => {
  await page.goto('http://127.0.0.1:8080');
  // 1. Root UISpec renders its menu bar (server/browser parity means these nodes exist)
  await expect(page.locator('#app .lq-UISpecElement')).toBeVisible();
  await expect(page.getByText('Add Dashboard')).toBeVisible();
  // 2. Clicking a menu button dispatches its UiAction (data-lq-action) → a query runs
  const before = await page.locator('#app .lq-element').count();
  await page.getByText('Add Dashboard').click();
  await expect.poll(async () => page.locator('#app .lq-element').count()).toBeGreaterThan(before);
});
```

The Playwright run (build → serve → navigate → assert → click → assert) is the end-to-end
gate; it is wired into the Phase 3 test plan and Phase 4 execution as the definition of
"the browser example works." (Native `render_web` unit tests and an SSR string test cover
the non-browser layers without a browser.)

## ExtValue Extensions (feature-gating, not new variants)

No new persistent `ExtValue` variants; the **egui-typed** variants become optional so they
vanish from a webui-only build:

```rust
// liquers-lib/src/value/mod.rs  (modified)
#[derive(Debug, Clone)]
pub enum ExtValue {
    Image { value: Arc<image::DynamicImage> },
    PolarsDataFrame { value: Arc<polars::frame::DataFrame> },
    #[cfg(feature = "egui")] UiCommand { value: crate::egui::UiCommand },
    #[cfg(feature = "egui")] Widget { value: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>> },
    UIElement { value: Arc<dyn crate::ui::element::UIElement> },
}
```

Every exhaustive `match` over `ExtValue` in `value/mod.rs` gains `#[cfg(feature = "egui")]`
arms for these two variants (keeping "no default match arm" intact). `value_to_html` renders
`Image` (→ `<img>` data-URI), `PolarsDataFrame` (→ `<table>`), `UIElement` (→ delegate to
`render_web`) on all builds; the two egui variants render an inert placeholder only under
`#[cfg(feature = "egui")]`.

## Generic Parameters & Bounds

The web module is almost entirely non-generic (like `AppState`, `UiAction`, the `html`
helpers). The only generic surface is the driver glue, reusing the existing `AppRunner`
bounds verbatim (`E: Environment<Value = Value>`, `E::Payload: UIPayload +
From<SimpleUIPayload>`). No new bounds; `render_web`/`show_in_web` are concrete so
`UIElement` stays object-safe.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `render_web`, `value_to_html`, `html::*`, `widgets::*`, `dataframe_to_html`, `dispatch_action` | No | Pure synchronous string building / channel sends. |
| `show_in_web`, `render_element_dom` | No | Synchronous DOM mutation on the browser main thread. |
| `render_element_web` | No | Immutable read of one element; caller holds the lock. |
| `render_app_ssr`, `mount_web` | Yes | Lock the async `AppState` mutex / drive the async `AppRunner`. |

Rendering is synchronous; evaluation and state polling stay async and reuse `AppRunner`
(on a current-thread tokio runtime in the browser).

## Integration Points

### Crate: liquers-lib

**New files:** `src/ui/action.rs` (shared, always compiled); `src/ui/web/{mod,html,widgets,
dataframe,app}.rs` (under `#[cfg(feature = "webui")]`); `examples-web/ui_spec_demo/` (the
wasm example crate).

**Modify `src/ui/mod.rs`:** `pub mod action; pub use action::{UiAction, dispatch_action};`
and `#[cfg(feature = "webui")] pub mod web;` with web re-exports.

**Modify `src/lib.rs`:** `#[cfg(feature = "egui")] pub mod egui;`.

**Modify `src/ui/element.rs`:** gate `show_in_egui`; add `render_web` (+ `show_in_web` where
needed); remove the stray `use egui::debug_text::print;` (line 3); route
`AssetViewElement::from_asset_ref`'s `tokio::spawn` through `spawn_ui_task`.

**Modify widget elements:** gate `show_in_egui`+egui imports; add `render_web`; for the
console add `show_in_web` and route its `tokio::spawn`/`tokio::time::sleep` through
`spawn_ui_task` / a wasm timer.

**Modify `src/ui/widgets/ui_spec_element.rs`:** replace `MenuAction` with the shared
`UiAction` (keeping the YAML `Deserialize`); `handle_menu_action` → `dispatch_action`.

**Modify `src/value/mod.rs`:** gate `ExtValue::UiCommand`/`Widget` + their match arms.

**Modify `src/ui/shortcuts.rs`:** gate `Key::to_egui`.

**Modify `liquers-core/src/store.rs`:** gate `AsyncFileStore` (and its `tokio::fs` block)
with `#[cfg(not(target_arch = "wasm32"))]` so `AsyncMemoryStore` (needed on wasm) stays but
the native filesystem store is excluded there.

**wasm tokio features:** cfg-split `liquers-core`'s manifest so the `wasm32` target drops
`fs`/`net` (keeps `sync`/`rt`/`macros`/`time`) — see "Browser Runtime". No `tokio_with_wasm`
unless testing shows plain tokio is insufficient on wasm.

**Browser bootstrap detail:** `mount_web` constructs the `AssetManager` with **capacity 1**
(single-threaded wasm → serialized jobs; the queue is otherwise unchanged) and enters a
current-thread tokio runtime before spawning.

### Dependencies — `liquers-lib/Cargo.toml`

```toml
[features]
default = ["egui", "image-support"]
image-support = ["imageproc"]
egui = ["dep:egui", "dep:eframe", "dep:egui_plot", "dep:egui_extras", "dep:egui_commonmark"]
webui = ["dep:web-sys", "dep:wasm-bindgen", "dep:js-sys", "dep:wasm-bindgen-futures",
         "dep:pulldown-cmark", "dep:console_error_panic_hook"]

[dependencies]
egui = { version = "0.33.0", optional = true }
eframe = { version = "0.33.0", optional = true }
egui_plot = { version = "0.34.0", optional = true }
egui_extras = { version = "0.33.3", optional = true }
egui_commonmark = { version = "0.22.0", optional = true }
wasm-bindgen = { version = "0.2", optional = true }
wasm-bindgen-futures = { version = "0.4", optional = true }
js-sys = { version = "0.3", optional = true }
pulldown-cmark = { version = "0.12", optional = true, default-features = false }  # web markdown → HTML
console_error_panic_hook = { version = "0.1", optional = true }
web-sys = { version = "0.3", optional = true, features = [
  "Document", "Window", "Element", "HtmlElement", "HtmlInputElement", "Node",
  "Event", "EventTarget", "InputEvent", "KeyboardEvent", "DomTokenList",
] }

# wasm keeps stock tokio (Option A) but must drop the `fs`/`net` features there.
# This split belongs in every crate that enables tokio `fs` — notably liquers-core.
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokio = { version = "1.47.1", features = ["sync", "rt", "macros", "time", "fs"] }
[target.'cfg(target_arch = "wasm32")'.dependencies]
tokio = { version = "1.47.1", features = ["sync", "rt", "macros", "time"] }
```

**Version rationale:** egui unchanged (only made optional). `eframe` verified unused in
`liquers-lib/src/` (examples only), so gating it is safe. `wasm-bindgen 0.2` / `web-sys 0.3`
/ `wasm-bindgen-futures 0.4` are the standard trio; `wasm-bindgen-futures` is already
referenced by `spawn_ui_task` but missing from the manifest. `pulldown-cmark` replaces
`egui_commonmark` for web markdown. **Runtime (Option A):** keep stock `tokio` on wasm with
`sync`/`rt`/`macros`/`time` (no `fs`/`net`), driven by a current-thread runtime in
`mount_web`; this must be validated by the browser example (top Phase-3/4 risk). If plain
tokio proves insufficient on wasm, the contingency is `tokio_with_wasm` as a drop-in via the
same cfg alias — decided in Phase 4 based on the test result, not adopted now.

## Relevant Commands

**None. Decided (user-confirmed).** `liquers_lib::ui` is the single shared UI layer; a
backend contributes only conditionally-compiled rendering methods on `UIElement` plus render
helpers — no new command namespace, no `lui` changes. Verified: the UI core is egui-free, and
egui coupling in `ui/` is confined to `element.rs`, `shortcuts.rs`, and the 3 widget elements
— exactly the files that gain `render_web`/`show_in_web`. The egui reference's own
value-producing commands (`liquers_lib::egui`: `label`, `text_editor`, `show_asset_info`) are
out of scope and gated out; `lui`-built trees render fully. `lui` is the primary namespace.

## Web Endpoints

No new HTTP routes here. `render_app_ssr` returns an HTML string designed to be embeddable by
a future `liquers-axum` handler; axum wiring is out of Phase 1 scope.

## Error Handling

Typed `liquers_core::error::Error` only. `render_web` and the `html`/`widgets` builders are
infallible (`-> String`). Fallible pieces — `show_in_web`, `render_element_dom`,
`render_app_ssr`, `mount_web` — return `Result<_, Error>`; `web-sys`/`JsValue` failures map to
`Error::general_error`. `wasm_bindgen(start)` maps `Error` → `JsValue` via a small
`err_to_js` helper for the console. No `unwrap`/`expect` in library code (the example's
`start()` may `unwrap` DOM lookups, as example/bootstrap code).

## Serialization Strategy

- `UiAction` derives `Serialize, Deserialize` (custom YAML `Deserialize` for `MenuAction`
  back-compat) — embedded in `data-lq-action` and parsed back by the browser dispatcher.
- `MountHandle`, `html`/`widgets` helpers: transient, not serializable.
- Element structs unchanged; existing `#[serde(skip)]` runtime fields + typetag reused. A
  `show_in_web` override caching `web-sys` nodes stores them in new `#[serde(skip)]` fields
  (browser-only, rebuilt on first render).

## Concurrency Considerations

- Rendering is single-threaded (browser main thread / one SSR task). No new shared state.
- Shared state stays `Arc<tokio::sync::Mutex<dyn AppState>>`, via `AppRunner` (async) and
  `try_sync_lock` during the browser `render_element_dom` pass.
- Browser is single-threaded: `try_sync_lock` never contends; `Send` bounds are vacuous; all
  async runs on the current-thread tokio runtime entered by `mount_web`.
- One delegated listener (owned by `MountHandle`) is the only long-lived JS callback — no
  per-frame closure churn.

## Compilation Validation

- `cargo check -p liquers-lib` (default: egui) — existing behavior, green.
- `cargo check -p liquers-lib --no-default-features --features webui,image-support` — webui
  only, **no egui symbols**.
- `cargo check -p liquers-lib --no-default-features --features egui,image-support` — egui only.
- `cargo check -p liquers-lib --features egui,webui` — both coexist.
- `cargo build --target wasm32-unknown-unknown -p <example-web-crate>` (via `trunk build`) —
  the real browser build, including the wasm tokio config.

## References to liquers-patterns.md

- [x] Crate dependency flow respected (changes in liquers-lib; wasm tokio-feature split also in core manifest).
- [x] No new top-level value enums; egui variants gated; no new ExtValue variants.
- [x] Commands unchanged — reuse `register_command!`-based `lui`.
- [x] UIElement pattern followed (gated methods + per-element overrides; object-safe).
- [x] `UiAction` unifies `MenuAction` — one shared, serializable action type; no default arms.
- [x] Error handling via typed constructors; no `unwrap`/`expect` in library code.
- [x] Async default preserved (AppRunner reused); rendering sync by necessity.
- [x] `#[serde(skip)]` for transient fields.
