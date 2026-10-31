# Phase 2: Solution & Architecture — web-ui-define

## Overview

**One Rust element type stands for every element a language defines.** It is called
`DelegatedElement` and lives in `liquers-lib`. It is a serializable `UIElement`, typetag
`"DelegatedElement"`, that carries a `kind`, JSON `props` and a `title`. It forwards rendering and
events to an `ElementDelegate`, which it looks up by `kind` in a registry kept per thread.

liquers-web supplies one delegate type, `JsElementDelegate`, which holds the page's `render`,
`onEvent` and optional `mount` functions. A missing delegate renders a placeholder (AC-5).

**Events arrive as `ui-events` `UpdateMessage::UI`.** The runner delivers them with the element
taken out of `AppState` and the lock released, so `onEvent` may call back into the mount.

**`mount` and `unmount` run after a tick.** liquers-web reconciles the container's
`[data-lq-delegate]` nodes, so the shared driver gains no DOM lifecycle.

**A page renderer (AC-7) uses a headless driver loop.** It is the `mount_web_with` loop without the
DOM, and it hands the renderer the tree and each tick's `Invalidation`.

Rejected alternatives:

- One Rust type per JavaScript type. `typetag` tags are static, so this cannot work.
- Storing the delegate inside the element. Functions cannot be serialized, and a restored tree
  would lose them.
- Adding a DOM-lifecycle method to `UIElement`. The incremental driver applies markup strings and
  would need to track every node.
- A virtual-tree format. Decision 1 rules it out.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `web-ui-use` (`WEB-CANNOT-USE-A-LIQUERS-UI`) | designed | P2 | mounts, `UIElement` handles, `on_tick` and `take_events` | yes | yes | implemented first |
| `ui-events` (`WEBUI-SUBMIT-QUERY-STATE-NOT-PRESERVED` and 2 others) | built (PR #225, merged 2026-10-31) | P2 | `UIEvent`, `UIEventKind`, `UpdateMessage::UI`, `AppMessage::UIEvent`, `data-lq-event` (decision 2) | done | no | used as built |
| `console-mounts-nested-element` | design in review | P3 | a delegated element shown inside a console has no handle | no | no | noted; it works once that lands |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft | P2 | an async `onEvent` follow-up query freezes the page | no | no | inherited limitation |

## Interfaces

**liquers-lib, `src/ui/widgets/delegated_element.rs`** (all targets; markup needs `webui`):

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DelegatedElement {
    pub kind: String,
    pub props: serde_json::Value,
    pub title: String,
    handle: Option<UIHandle>,          // serialized, like every other element's handle
}
#[typetag::serde] impl UIElement for DelegatedElement { /* type_name "DelegatedElement" */ }

/// Behaviour of one delegated kind. Called on the thread that registered it.
pub trait ElementDelegate {
    /// Markup for `element`, inside the node `render_web` wraps around it.
    fn render(&self, element: &DelegatedElement) -> Result<String, Error>;
    /// New props, or `None` to keep them.
    fn on_event(&self, element: &DelegatedElement, event: &UIEvent)
        -> Result<Option<serde_json::Value>, Error>;
}
pub fn register_delegate(kind: &str, delegate: Rc<dyn ElementDelegate>) -> Option<Rc<dyn ElementDelegate>>;
pub fn unregister_delegate(kind: &str) -> Option<Rc<dyn ElementDelegate>>;
pub fn delegate_for(kind: &str) -> Option<Rc<dyn ElementDelegate>>;
```

The registry is a `thread_local! { RefCell<HashMap<String, Rc<dyn ElementDelegate>>> }`.
`delegate_for` clones the `Rc` out, so no borrow is held while page code runs. A delegate is visible
on the thread that registered it. That is the browser's one thread, and it is the
thread-affinity rule `UIDEF08` asks for.

`render_web` wraps the delegate's markup in
`<div id="ui-element-N" class="lq-element lq-delegated" data-lq-delegate="{kind}">`. When there is
no delegate, it writes an escaped placeholder naming the kind. When the delegate returns an error,
it writes an escaped error box. `update(UpdateMessage::UI(e))` calls `on_event`, replaces `props`
when it returns `Some`, and answers `NeedsRepaint`. Every other message answers `Unchanged`, and the
match has no `_ =>` arm. `show_in_egui` shows a label saying the kind cannot be rendered in egui.

**liquers-lib, `src/ui/web/headless.rs`** (wasm, `webui`):

```rust
pub struct HeadlessOptions<E: Environment> {
    pub envref: EnvRef<E>,
    pub current_env: Option<Box<dyn FnMut() -> Option<EnvRef<E>>>>,
    /// The tick's invalidation and runner events, with the lock released.
    pub on_tick: Box<dyn FnMut(Invalidation, Vec<RunnerEvent>)>,
}
pub async fn run_headless<E>(options: HeadlessOptions<E>, app_state: Arc<tokio::sync::Mutex<dyn AppState>>,
    sender: AppMessageSender, receiver: AppMessageReceiver) -> Result<MountHandle, Error>;
```

This is the same loop as `mount_web_with`, with `on_tick` in place of `apply_invalidation`. It
returns the `MountHandle` that `web-ui-use` makes stoppable. Its root and listener fields become
`Option`s, so a headless handle has none, and `stop` only sets the flag.

**liquers-web** (`src/ui_define.rs`, new):

```ts
defineElement(def: { type: string; render: (props: any) => string;
  onEvent?: (props: any, event: UIUserEvent) => any; mount?: (node: Element, props: any) => (() => void) | void;
  title?: string }): void;                       // a second define of a type replaces it
undefineElement(type: string): boolean;
element(type: string, props?: any, title?: string): UIElement;     // the web-ui-use handle
mountRenderer(renderer: UIRenderer, options?: MountOptions): Promise<UIMount>;  // no container
interface UIRenderer { render(tree: UITree, changes: UIChangeRecord[] | "all"): void; dispose?(): void }
interface UITree { roots: number[]; elements: Record<number, UITreeElement> }
interface UITreeElement { handle: number; parent: number | null; children: number[];
  typeName: string; title: string; status: string; html: string; kind?: string; props?: any }
type UIChangeRecord = { inserted: number; parent: number | null; index: number }
                    | { removed: number; parent: number | null } | { replaced: number };
interface UIUserEvent { handle: number; kind: string; fields: Record<string, any> }  // ui-events' UIEvent, plus the element
// UIElement (web-ui-use) gains: readonly kind?: string; readonly props?: any   (DelegatedElement only)
// UIMount gains: dispatchEvent(handle: number, kind: string, fields?: object): void
```

- `UIUserEvent` is built from the `UIEvent`: `kind` is `UIEventKind::name()` (`"submit"` or the
  markup name), `fields` maps each `(name, Value)` through `value_to_js`, and `handle` is the
  element's own handle, which the `UIEvent` does not carry.
- `JsElementDelegate { render: Function, on_event: Option<Function>, mount: Option<Function> }`
  implements `ElementDelegate`. It calls the functions with `props` converted by
  `serde_wasm_bindgen` through the crate's map-as-object serializer, and reads the result back with
  `js_to_value(…, Structural)` followed by a JSON check.
- `element()` refuses props that are not JSON (decision 4) with a `ConversionError`.
- While `on_event` runs, `JsElementDelegate` records the handle being updated in a thread-local,
  and `show(q, thatHandle)` is refused: `deliver_update` puts the element back after the handler,
  which would silently undo the replacement.
- Redefining a type calls `invalidate_all()` on every live mount's `AppState`. liquers-web keeps a
  weak list of mounts for this.
- `mount` reconciliation runs in `on_tick`. It keeps
  `HashMap<UIHandle, (web_sys::Element, Option<Function>)>`, and uses `querySelectorAll`, which
  needs the web-sys `NodeList` feature. A `[data-lq-delegate]` node that has
  no entry, or whose node differs from the stored one, is mounted with its element's props, read
  by `try_lock` (the lock is free in `on_tick`). An entry whose node is no longer connected has
  its cleanup function called and is dropped.
- A renderer mount builds `UITree` from `AppState` on its first tick and on `Invalidation::All`,
  and then passes `Changes` as `UIChangeRecord`s, with the tree refreshed for the handles they
  name. `html` is `render_element_web(handle, state)`. `dispatchEvent` sends `AppMessage::UIEvent { handle, event }`,
  with `kind` read by `UIEventKind::from_name` (an empty kind is refused with
  `Error::general_error`) and `fields` converted to `Value`s by `js_to_value`.

Everything is sync except `mountRenderer`.

## Integration Points

- `liquers-lib/src/ui/widgets/mod.rs` and `ui/mod.rs`: the module, its re-exports, and
  `ui/web/mod.rs` for `headless`.
- `liquers-web/src/ui.rs` (from `web-ui-use`): `mountRenderer`, the mount list
  and the reconciliation hook.
- `liquers-web/src/ui_define.rs` (new), `lib.rs` exports, `typescript.rs` declarations.
- The `live_handle_count` counter covers delegate functions, `mount` cleanup functions and
  renderer objects.

## Error Handling

| Case | Error |
|---|---|
| `defineElement` without `type` or `render`, or `render` not a function | `Error::general_error(…)` naming the field |
| `mountRenderer` with an object that has no `render` function | `Error::general_error(…)` |
| props are not JSON (functions, class instances, cycles) | `Error::from_error(ErrorType::ConversionError, …)` naming the type |
| `render` throws, or returns something other than a string | `js_error_to_liquers(…, ExecutionError)`, rendered in the element as an escaped error box and written to `console.error` |
| `onEvent` throws | `ExecutionError`; props are unchanged, and the error goes to `console.error` |
| `show(q, h)` from inside `onEvent` for `h` itself | `Error::general_error(…)`: the element is being updated |
| `mount` throws | `console.error`; the element keeps its markup |
| unknown kind | no error: the placeholder (AC-5) |
| `ui.show` called from inside `render` | the busy-lock error from `web-ui-use`; `submit` works, because it is a message |

## Relevant Commands

None new. `lui/add` already places any `UIElement`, so `gauge-50/ns-lui/add-child` works.
The queries are checked in Phase 3.

## Concurrency

`render` runs inside `apply_invalidation`, under the `AppState` guard. It receives only props, so
page code there cannot need the guard (decision 1). `onEvent` runs inside `AppRunner::run`, through
`ui-events`' `deliver_update`, with the element taken out and the lock released. The page's renderer
and `mount` run in `on_tick`, after the lock is released.

No `RefCell` borrow (the registry, the reconciliation map or the mount list) is held while a page
function runs. The `Rc` or the entry is cloned out first.

## Documentation Architecture

- `specs/reference/JAVASCRIPT_INTEGRATION.md`: an "Elements and renderers" subsection covering the
  API, the call contexts above, egui being `NA`, and the `History` row.
- `specs/guides/JAVASCRIPT_GUIDE.md`: "Define an element" and "Render the tree yourself". The
  `renderHtml()` of an `element()` handle runs under Node.
- `specs/reference/UI_INTERFACE_FSD.md`: not edited, because it is being rewritten under its own
  issue.
- `affects_docs`: as `DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | 2 new liquers-lib modules plus re-exports; 2 liquers-web modules |
| Crates and workflows affected | `liquers-lib` (all targets for the element, `webui` for markup and headless), `liquers-web`; the wasm rows of the build matrix |
| Existing tests likely to change | none: everything is additive |
| New validation | native tests for `DelegatedElement` (render, fallback, serialization round trip, event); Node-loop tests for `defineElement` and `element`; `browser-tests` for events, `mount` and the renderer |
| Compatibility | a serialized tree with `DelegatedElement` nodes needs the typetag type, which exists on every target |
| Performance | one JavaScript call per delegated element per render; the reconciliation is one `querySelectorAll` per tick, only while a mount has delegated elements |
| Security | `render`'s markup is the page's own and is inserted as HTML, like Rust `render_web`; the guide says so, and the placeholder and error box escape their text |
| Recovery | each part is additive, and the renderer mount can be left out (it is the last milestone) |
| Certainty | high for the element and the delegate; the `onEvent` half follows `ui-events` as implemented in PR #225; a rename before it merges reaches only `on_event`'s parameter |
