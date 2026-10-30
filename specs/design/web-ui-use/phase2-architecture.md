# Phase 2: Solution & Architecture — web-ui-use

## Overview

liquers-web gets a thin JavaScript layer over the backend `liquers-lib` already runs in the
browser. `mountUI` builds an `AppState`, puts the root element in it as a pending node, and starts
the driver from `mount_web`. Page calls become `AppMessage`s, or short `try_lock` reads. The runner
records status events, which the driver passes to page callbacks after each tick. The backend
changes are three additions to `liquers-lib`, and every wasm app can use them:

1. a driver loop that stops,
2. a runner whose environment can be replaced,
3. a runner event outbox.

The web environment's payload becomes `SimpleUIPayload`, and the web builtins register the `lui`
commands.

Rejected alternatives:

- **A JavaScript reimplementation of the driver.** It would duplicate `apply_invalidation`, focus
  handling and the event delegation.
- **Exposing `AppRunner` and `AppState` as JavaScript classes.** That exposes a lock and a channel,
  which page code would have to drive on every tick itself.
- **Delivering events from inside `run`.** Page code would then run inside the runner, which
  decision 3 rules out.
- **A web-specific payload.** Decision 1 chose `SimpleUIPayload`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `JS-COMMAND-CANNOT-ACCESS-CONTEXT` | draft | P2 | AC-3 needs `context.element` | yes (decision 2) | AC-3 only | designed in `js-command-context` (compact), Step 1 |
| `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS` | draft (filed 2026-10-30) | P2 | progress is never painted during an async query; the order of status events is unaffected | no | no | documented as a limitation; needs its own design |
| `WEB-CANCELLATION-INERT` | accepted | P3 | no UI cancellation | no | no | non-goal |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | a rebuild drops the asset cache under a live mount | no | no | AC-9 re-resolves the environment; the cache loss is documented |
| `UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API` | draft | P2 | the reference cannot be cited | no | no | the design cites code, not the FSD |
| `ui-events`, `ui-asset-snapshot-boundary`, `console-mounts-nested-element` | designs in review | — | they change backend internals, but not the three additions | no | no | the additions are kept independent of them |

## Interfaces

**liquers-lib, `src/ui/runner.rs`** (every target):

```rust
/// A status change of one element, recorded by the runner and read by the backend after `run`.
#[derive(Debug, Clone)]
pub struct RunnerEvent {
    pub handle: Option<UIHandle>,   // None: a root query with no element to show its error
    pub status: liquers_core::metadata::Status,
    pub error: Option<Error>,
}
impl<E: Environment> AppRunner<E> {
    pub fn replace_envref(&mut self, envref: EnvRef<E>);   // later evaluations only
    pub fn take_events(&mut self) -> Vec<RunnerEvent>;     // drains, in recording order
}
```

`AppRunner` gains a field `events: Vec<RunnerEvent>`, which is pushed at five places:

| Where | Event |
|---|---|
| `evaluate_pending_nodes`, when the progress element is set | `Evaluating` |
| `poll_evaluating_nodes`, when an element is set | `Ready` |
| every `AssetViewElement::new_error` site | `Error` with the error |
| the two root-query `eprintln!`s | `handle: None`, `Error` (the `eprintln!`s stay) |
| `deliver_snapshot`, when the outcome is `Delivered` | `snapshot.status` |

**Stale results (AC-6).** `evaluating` becomes `HashMap<UIHandle, (AssetRef<E>, String)>`, keeping
the query text it started. `evaluate_pending_nodes` checks again, after its `evaluate(...).await`,
that `get_source(handle)` is still `ElementSource::Query` of that text. `poll_evaluating_nodes` does
the same before it sets an element. When the source has changed, the result is dropped and no event
is recorded. The node is still pending, so the next tick evaluates the new query.

The bug this fixes is real today. On wasm, `evaluate` awaits inline, so `ui.show(B, h)` can replace
`h`'s source while query A is still awaiting. A's result then overwrites the node, and B is never
evaluated.

Events are synchronous data. A mount that never reads them still pushes them, so `take_events` is
also what keeps the vector from growing without bound. The plain `mount_web` driver drains and
drops them every tick.

**liquers-lib, `src/ui/web/app.rs`** (wasm):

```rust
pub struct MountOptions<E: Environment> {
    pub envref: EnvRef<E>,
    /// Called before each tick. A different environment (by `Arc` identity) replaces the runner's.
    pub current_env: Option<Box<dyn FnMut() -> Option<EnvRef<E>>>>,
    /// Called after each tick, with the lock released and the DOM updated.
    pub on_tick: Option<Box<dyn FnMut(Vec<RunnerEvent>)>>,
}
pub async fn mount_web_with<E>(root: web_sys::Element, options: MountOptions<E>,
    app_state: Arc<tokio::sync::Mutex<dyn AppState>>, sender: AppMessageSender,
    receiver: AppMessageReceiver) -> Result<MountHandle, Error>;   // same bounds as mount_web
impl MountHandle { pub fn stop(&self); pub fn is_running(&self) -> bool; }
impl Drop for MountHandle { /* stop() */ }
```

`stop` sets an `Rc<Cell<bool>>` that the loop checks after each tick. It calls
`remove_event_listener_with_callback` for both listeners (today, dropping the `Closure`s leaves
stale listeners that throw when they fire) and empties the root. The loop then returns, which drops
the runner, its `EnvRef` and its `AppState` clone. `mount_web` keeps its signature and delegates,
with no `current_env` and no `on_tick`.

**liquers-web:**

```rust
pub type WebEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;    // environment.rs
#[wasm_bindgen(js_name = UIMount)] pub struct LiquersUIMount { inner: Rc<MountInner> }   // ui.rs
#[wasm_bindgen(js_name = UISubscription)] pub struct LiquersUISubscription { … dispose() }
#[wasm_bindgen(js_name = UIElement)] pub struct LiquersUIElement { value: Arc<dyn UIElement>, id: u32 }
#[wasm_bindgen(js_name = mountUI)]
pub fn mount_ui(container: web_sys::Element, options: Option<JsMountOptions>) -> js_sys::Promise;
// and LiquersEnvironment::mount_ui with the same signature, on its own EnvHandle
```

The JavaScript surface, declared through `typescript_custom_section` the way `JsStoreConfig` is:

```ts
interface MountOptions { query?: string; spec?: string | object }   // both given: refused
class UIMount {
  readonly running: boolean;
  submit(query: string, handle?: number): void;     // as a control's query action
  show(query: string, handle?: number): number;     // a new root, or replaces `handle`; returns it
  on(kind: "status", cb: (e: UIStatusEvent) => void): UISubscription;
  elements(): UIElementInfo[];                      // { handle, parent, typeName, title, status }
  dispose(): void;
}
interface UIStatusEvent { handle: number | null; status: string; error?: LiquersError }
class UIElement { readonly typeName: string; readonly title: string; renderHtml(): string; free(): void }
```

- `query` becomes `AppState::add_node(None, 0, ElementSource::Query(q))`. The runner evaluates it,
  as it evaluates any pending node.
- `spec` (YAML text, or an object serialized to JSON, which `UISpec::from_yaml` accepts) becomes a
  root `UISpecElement`, placed and given `init` the way `ui_spec_demo` does it.
- `show` is `AppState::insert_source(&InsertionPoint::Root | Instead(h), ElementSource::Query(q))`.
- `submit` sends `AppMessage::SubmitQuery`.
- The status strings are `asset::status_name`, the same names `Asset.status` uses.
- The `UIElement` handle uses the `records.rs` handle-registry idiom (`__liquersUIElementHandle`
  getter, an id-to-`Arc` map), so `from_js_custom` gives Rust the same `Arc` back.
  `default_value.rs` maps `ExtValue::UIElement` both ways.
- `renderHtml` calls `render_web` against an empty `DirectAppState`.

All of these are sync, apart from `mountUI`, which returns a `Promise` because `mount_web_with` is
async.

## Integration Points

- `liquers-lib/src/ui/runner.rs`: the event outbox and `replace_envref`.
- `liquers-lib/src/ui/web/app.rs`: `MountOptions`, `mount_web_with`, the stoppable `MountHandle`.
- `liquers-lib/src/ui/mod.rs`: re-exports for both.
- `liquers-web/src/environment.rs`: the payload type, and `mountUI` on `LiquersEnvironment`.
- `liquers-web/src/builtins.rs`: `register_lui_commands!(cr)?`.
- `liquers-web/src/ui.rs` (new), `lib.rs`, `default_value.rs`, `typescript.rs`.
- `liquers-web/Cargo.toml`: the web-sys `Element` and `Document` features.
- `liquers-web/tests/eval_EVAL.rs` `eval05`: the payload assertion widens, as its comment
  anticipates.
- The page-callback handle counter joins `command::adapter::live_handle_count` (`debug-handles`).

## Error Handling

| Case | Error |
|---|---|
| no `window` or `document` (Worker, Node) | `Error::from_error(ErrorType::NotSupported, "mountUI needs a DOM …")` |
| `query` and `spec` both given; `spec` not a `UISpec` | `Error::general_error(…)`, and `UISpec::from_yaml`'s own error |
| unparsable `query` (checked before the DOM, so Node can test it) | the `parse_query` error |
| a call on a disposed mount, or a handle that does not exist | `Error::general_error("the UI mount has been disposed")`, `Error::general_error("no UI element N")` |
| `AppState` lock busy during `show` or `elements` | `Error::general_error(…)` (only if a lock is held across an await; not today) |
| a page callback throws | caught, `console.error`, the loop continues |
| evaluation failure | unchanged backend path (an error element); now also an `Error` event |

All of these reach JavaScript through `liquers_error_to_js`.

## Relevant Commands

No new commands. The namespace `lui` (14 commands, `register_lui_commands!`) is registered in the
web builtins. `specs/command_registry.yaml` is exported from `liquers-lib`, which already contains
`lui`, so it is unchanged.

## Concurrency

The browser is single-threaded, so a page call can only run while the driver future is suspended.
No `AppState` guard is held across an await in `runner.rs`, so `try_lock` from a page call succeeds.
Page callbacks run in `on_tick` after `apply_invalidation` returns.

The subscriber list is cloned before it is iterated, so a callback may `dispose()` the subscription,
or the whole mount, while it runs. `stop` only sets the flag, and the loop exits at its next check.
`MountInner` holds the `MountHandle` in a `RefCell<Option<…>>` that is borrowed only inside
`dispose` and `running`, never across a call into JavaScript.

## Documentation Architecture

- `specs/reference/JAVASCRIPT_INTEGRATION.md` (reference, internal, `web`): a new "User interface"
  section covering the API above, the limits (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`, inert
  cancellation, the cache loss on rebuild) and `History`.
- `specs/guides/JAVASCRIPT_GUIDE.md` (guide): "Show a Liquers UI". Its DOM-free example (an
  element handle's `renderHtml`) runs under `scripts/test-guide-examples.sh`.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Browser JavaScript: `UIUSE` is implemented.
- `liquers-web/README.md`: the conformance row.
- `affects_docs`: those three specs (already in `DESIGN.md`).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the 2 liquers-lib files above; 8 liquers-web files; 1 new test file per loop |
| Crates and workflows affected | `liquers-lib` (`webui`), `liquers-web`; the wasm rows of `check-build-matrix.sh` |
| Existing tests likely to change | `eval05`; `liquers-lib/examples-web` Playwright specs still pass, since `mount_web` is unchanged for them |
| New validation | runner unit tests (native), Node-loop tests, `browser-tests` mount tests, one Playwright spec |
| Compatibility | `WebEnvironment` changes type: a downstream crate that named `DefaultEnvironment<Value, ()>` through the alias must follow it. A page command named `context` (via `js-command-context`) |
| Performance | `lui` adds about 14 commands to the `.wasm`, plus the runner per mount; the tick is unchanged (16 ms) |
| Security | `spec` and query text come from the page itself; markup is rendered by existing escaping (`html::escape_html`) |
| Recovery | each step is separate; `mount_web_with` can be reverted without touching `mount_web` callers |
| Certainty | high for the backend additions and the bindings; moderate for the browser tests, which need a matching chromedriver (one exists in this environment) |
