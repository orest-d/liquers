# Phase 2: Solution & Architecture - liquers-web

## Overview

`liquers-web` is a thin `#[wasm_bindgen]` facade over machinery that already exists and already
runs in a browser. The central architectural finding of this phase is that **no new `Environment`
and no new `CommandExecutor` are required**: `liquers_lib::environment::DefaultEnvironment<V, P>` is
generic over the value type and already cfg-selects `ImmediateAssetManager` on `wasm32`
(`liquers-lib/src/environment.rs:19-22`), and `CommandRegistry::register_async_command` already
accepts a non-`Send` `'static` closure on wasm (`liquers-core/src/commands.rs:463-470`). A JS
command is therefore an ordinary registered async command whose closure owns a `js_sys::Function`.

The crate contributes three things: a value bridge, a `#[wasm_bindgen]` object/eval/command surface,
and a Promise bridge. Everything else is reuse.

## Decision requiring user confirmation: where the JS value variant lives

This is the one open architectural fork. Phase 1 decision 2 fixed the *semantics* (structural by
default, opaque opt-in, direct `JsValue` retention); it did not fix *which enum* carries it.

### Option X — own value type in `liquers-web`

```rust
pub type WebValue = CombinedValue<SimpleValue, JsExt>;   // JsExt defined in liquers-web
```

Clean crate separation; `liquers-lib` is untouched. **Cost:** commands are registered against a
concrete `E::Value`, so `WebValue` can use *none* of `liquers-lib`'s existing command library — not
`register_core_commands!` (`to_text`, `to_metadata`, …) and not the `lui` namespace. Phase 1 named
`UIUSE`/`UIDEF` over the existing `webui` DOM backend as the intended next milestone; Option X does
not preclude it but makes it substantially more expensive.

### Option Y — a cfg-gated variant on `ExtValue` (**recommended**)

```rust
// liquers-lib/src/value/mod.rs
pub enum ExtValue {
    Image { value: Arc<image::DynamicImage> },
    #[cfg(feature = "polars")]     PolarsDataFrame { .. },
    #[cfg(feature = "egui")]       UiCommand { .. },
    #[cfg(feature = "egui")]       Widget { .. },
    UIElement { value: Arc<dyn crate::ui::element::UIElement> },
    /// Opaque JavaScript value. Browser-only.
    #[cfg(all(target_arch = "wasm32", feature = "webui"))]
    Js { value: JsOpaque },
}
```

`liquers-web` then uses `liquers_lib::value::Value` (= `CombinedValue<SimpleValue, ExtValue>`)
unchanged, and inherits the whole wasm-viable command library.

Four reasons this is the recommendation:

1. **The pattern is already established here.** `ExtValue` carries `#[cfg(feature = "polars")]` and
   `#[cfg(feature = "egui")]` variants today, so cfg-gated variants plus cfg-gated match arms are an
   existing, exercised convention rather than a novel hazard.
2. **`liquers-lib` already depends on `wasm-bindgen`** through the `webui` feature
   (`liquers-lib/Cargo.toml`), so no new dependency edge is created.
3. **It composes with decision 1.** The variant only exists on `wasm32`, so on native `ExtValue`
   remains `Send + Sync` and nothing changes; on wasm the relaxed `MaybeSend`/`MaybeSync` bound is
   exactly what admits `JsValue`. Decision 1 is load-bearing under either option — with Option Y it
   is what makes the variant legal at all.
4. **It keeps `UIUSE` cheap**, which Phase 1 asked Phase 2 not to preclude.

5. **The precedent is not even hypothetical.** `liquers-py` already carries exactly this shape —
   `Py { value: Py<PyAny> }` in its value enum (`liquers-py/src/value.rs:45`) — so an opaque
   foreign-language variant beside the Rust ones is an established Liquers pattern, not an
   innovation this design is introducing.

**Cost, stated honestly (counted, not estimated):** every exhaustive `match` on `ExtValue` gains one
cfg'd arm. **14 sites**, verified:

| Location | Sites |
|---|---|
| `value/mod.rs` — `ExtValueInterface` (`as_image`, `as_polars_dataframe`, `as_ui_element`) | 3 |
| `value/mod.rs` — `ValueExtension` (`identifier`, `type_name`, `default_extension`, `default_filename`, `default_media_type`) | 5 |
| `value/mod.rs` — `DefaultValueSerializer::as_bytes` | 1 |
| `value/mod.rs` — `ExtValueInterface for Value`, matching `Value::Extended(ExtValue::…)` | 3 |
| `ui/web/html.rs:84` — `ext_to_html` | 1 |
| `egui/mod.rs:72` — `show` (native only; the new variant is wasm-only, so this arm is unreachable-by-cfg) | 1 |

Mechanical, but it must hold for all four build configurations, and per the project rule no `_ =>`
arm may be used to dodge it.

**Decided: Option Y**, with the extensibility requirement below. The rest of this document is
written against it.

## Extensibility: user-defined value types and environments

Option Y fixes the *default*, not the *only* possibility. A `liquers-web` user integrating a
third-party JavaScript library with its own data type must be able to supply their own value type
and environment. Two tiers, and most cases stop at the first.

### Tier 1 — opaque, no Rust work at all

A third-party type (an Arrow table, a Plotly figure, a WebGL handle) that only needs to *pass
through* Liquers from one JS command to another requires **no** custom value type: it is retained
opaquely by `liquers.opaque(x)` under decision 2. This is the cheap and expected path, and it is
also the fast one — O(1) instead of O(size) at the boundary. A custom Rust value type is needed only
when *Rust* commands must operate on the type structurally.

### Tier 2 — a user-defined value type and environment

**Constraint:** `#[wasm_bindgen]` cannot export generic types — it must generate JS glue for
monomorphic types. So `liquers-web` cannot export a generic environment; it must export a concrete
one. The extension mechanism is therefore *generic internals with a concrete default export*:

```rust
// Generic — reusable by a downstream crate with its own V and E.
pub fn js_to_value<V: ValueInterface + JsValueBridge>(js: &JsValue, policy: ConversionPolicy)
    -> Result<V, Error>;
pub fn value_to_js<V: ValueInterface + JsValueBridge>(v: &V) -> Result<JsValue, Error>;
pub fn register_js_command<E>(cr: &mut CommandRegistry<E>, spec: JsCommandSpec) -> Result<(), Error>
where E: Environment, E::Value: JsValueBridge;
pub fn evaluate_to_promise<E>(envref: EnvRef<E>, query: Query) -> js_sys::Promise
where E: Environment, E::Value: JsValueBridge;

/// The one trait a custom value type implements to join the bridge.
pub trait JsValueBridge: ValueInterface + Sized {
    /// Structural conversion of a JS value this type understands; `Ok(None)` means
    /// "not mine, try the next rule".
    fn from_js_custom(js: &JsValue) -> Result<Option<Self>, Error>;
    /// Inverse. `Ok(None)` falls back to the standard structural mapping.
    fn to_js_custom(&self) -> Result<Option<JsValue>, Error>;
    /// Wrap an opaque JS value. Returning `Err(NotSupported)` opts out of decision 2.
    fn from_js_opaque(js: JsValue, type_tag: Arc<str>) -> Result<Self, Error>;
}
```

`liquers-web` implements `JsValueBridge` for `liquers_lib::value::Value` (the default) and exports
the concrete `LiquersEnvironment` built on it. A downstream crate — `my-app-web`, depending on
`liquers-web` — defines its own `ExtValue`-like type, implements `JsValueBridge`, and exports its
own `#[wasm_bindgen]` wrapper reusing every generic function above. It writes the wrapper struct and
its `#[wasm_bindgen]` impl; it does not rewrite the bridge, the command adapter, or the Promise
bridge.

**Requirements this places on the implementation**, which Phase 4 must honour:

- No item in the conversion, command-adapter or Promise-bridge modules may name
  `liquers_lib::value::Value` concretely. The concrete type appears **only** in the exported
  `#[wasm_bindgen]` wrappers and the `JsValueBridge` impl for the default.
- `WebEnvironment` is a type *alias*, not a newtype, so a downstream crate can substitute its own
  `DefaultEnvironment<MyValue, P>` — or an entirely custom `Environment` impl — without forking.
- These generic functions are public API and are covered by their own tests instantiated at a second
  value type, so the genericity is proven rather than assumed (Phase 3).

**Documented limitation:** a downstream crate must produce its own wasm artifact; two independently
compiled wasm modules do not share an environment. Cross-module sharing would be `POLYGLOT`, which
is `NA` for this milestone.

## Data Structures

### `JsOpaque` — the retained JavaScript value

```rust
/// Owned handle to a JavaScript value retained by identity.
///
/// Wraps `JsValue` to give it a meaningful `Debug` and to keep the type-tag and
/// media-type policy in one place. Clone is a refcount bump on the wasm-bindgen
/// heap table; drop releases the slot.
#[cfg(all(target_arch = "wasm32", feature = "webui"))]
#[derive(Clone)]
pub struct JsOpaque {
    value: JsValue,
    /// `constructor.name` captured at wrap time, or "object". Used by `type_name()`
    /// so metadata and error messages stay debuggable.
    type_tag: Arc<str>,
}
```

**Ownership rationale.** The `JsValue` is owned outright — no registry, no ambient thread-local, no
hand-rolled refcounting (Phase 1 decision 2). `type_tag` is `Arc<str>` because `ValueExtension`
requires `Clone` and the tag is immutable after construction; cloning must stay cheap since values
are cloned freely through `State`.

**Serialization.** No `Serialize`/`Deserialize` derive — consistent with `ExtValue`, which derives
only `Debug + Clone`. Byte serialization goes through `DefaultValueSerializer` and **fails** by
design (see below).

**`Debug`.** Hand-written, printing `Js(<type_tag>)` rather than delegating to `JsValue`'s `Debug`,
which can invoke JS and is unsuitable inside error paths.

### `LiquersEnvironment` — the wasm-bindgen environment wrapper

```rust
#[wasm_bindgen]
pub struct LiquersEnvironment {
    envref: EnvRef<WebEnvironment>,
}

/// The concrete environment. No new `Environment` impl — this is a type alias.
pub type WebEnvironment = DefaultEnvironment<liquers_lib::value::Value, ()>;
```

**Ownership rationale.** `EnvRef<E>` is `Arc<E>` (`liquers-core/src/context.rs:225`) and `Clone`, so
the wrapper is cheap to clone into command closures and Promise futures. The wrapper owns the
`EnvRef`; JS owns the wrapper and frees it with the generated `.free()`.

#### Both lifecycles, per Phase 1 decision 4

```rust
#[wasm_bindgen]
impl LiquersEnvironment {
    /// Explicit, isolated instance. Registers no commands with, and shares no state
    /// with, the singleton — this is what makes `ENVIRON05` (test isolation) testable.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<LiquersEnvironment, JsValue>;

    /// Handle to the global singleton. Rejects if `init()` has not resolved.
    #[wasm_bindgen(js_name = global)]
    pub fn global() -> Result<LiquersEnvironment, JsValue>;

    pub fn evaluate(&self, query: &str) -> js_sys::Promise;
    #[wasm_bindgen(js_name = registerCommand)]
    pub fn register_command(&self, spec: JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name = describeCommand)]
    pub fn describe_command(&self, name: &str) -> Result<JsValue, JsValue>;
}

/// Module-level singleton entry points — the documented default path.
#[wasm_bindgen]
pub fn init() -> js_sys::Promise;          // idempotent; resolves with LiquersEnvironment
#[wasm_bindgen]
pub fn evaluate(query: &str) -> js_sys::Promise;   // delegates to the singleton
#[wasm_bindgen(js_name = registerCommand)]
pub fn register_command(spec: JsValue) -> Result<(), JsValue>;
```

So `liquers.evaluate(q)` is the one-liner path and `new liquers.Environment()` is the isolated one;
both drive identical machinery. **Repeated `init()` is idempotent** and resolves with the existing
singleton rather than replacing it (`ENVIRON03`); a *failed* `init()` leaves the singleton unset so
a retry can succeed (`ENVIRON04`). Shutdown is `.free()` on the wrapper, and is idempotent
(`ENVIRON06`).

### Global singleton (Phase 1 decision 4)

```rust
thread_local! {
    static GLOBAL_ENV: RefCell<Option<EnvRef<WebEnvironment>>> = RefCell::new(None);
}
```

`thread_local!` rather than a `static` with a lock: wasm is single-threaded, and a `RefCell` makes
re-entrancy failures loud (`already borrowed`) instead of deadlocking. **No `RefCell` borrow may be
held across an `await` or across a call into JS** — every accessor clones the `EnvRef` out and drops
the borrow immediately. This is the mechanism that discharges Phase 1's carried-forward reentrancy
concern; see "Reentrancy" below.

### Object wrappers (`OBJECT`)

Uniform shape — each owns its core object, none borrows:

```rust
#[wasm_bindgen] pub struct LiquersQuery    { inner: liquers_core::query::Query }
#[wasm_bindgen] pub struct LiquersKey      { inner: liquers_core::query::Key }
#[wasm_bindgen] pub struct LiquersMetadata { inner: liquers_core::metadata::MetadataRecord }
#[wasm_bindgen] pub struct LiquersState    { inner: State<liquers_lib::value::Value> }
#[wasm_bindgen] pub struct LiquersAsset    { inner: AssetRef<WebEnvironment> }
```

Exported to JS under unprefixed names via `#[wasm_bindgen(js_name = Query)]` etc. Conventions:

| Concern | Convention |
|---|---|
| Construction | `Query.parse(str)` / `Key.parse(str)` — always through `liquers_core::parse`, never reimplemented in JS |
| Encoding | `encode()` → `String`; `toString()` aliases it |
| Equality | `equals(other)` method. **Not** `===` — wasm-bindgen objects are distinct JS handles |
| Mutability | Immutable. Mutators return a new wrapper |
| Introspection | `toJSON()` where a faithful JSON form exists |
| Invalidated handles | After `.free()`, wasm-bindgen throws on use — documented, not re-implemented |

**Enum representation.** Rust enums cross as **lowercase strings**, never integer discriminants
(`ErrorType::KeyNotFound` → `"key_not_found"`; `ArgumentType::Integer` → `"int"`, reusing the
existing serde renames at `command_metadata.rs:155-175`). Forward compatibility: an unrecognized
string from JS is a `ConversionError` naming the value; an unrecognized *Rust* variant reaching JS
serializes by name, so a JS consumer sees a new string rather than a silent mismatch. No `_ =>` arm
is used in either direction — every mapping enumerates all variants so a new one is a compile error.

### JS command declaration (`COMMAND`, Phase 1 decision 3)

The JS-side shape, per the guide's §5 `COMMAND` example:

```javascript
liquers.registerCommand({
  name: "repeat",                                   // required
  run: (state, count) => state.repeat(count),       // required
  arguments: [{ name: "count", type: "int", default: 2 }],  // optional — inferred if absent
  state: "string",                                  // optional — default "state"
  namespace: "", realm: "", doc: "", label: "",     // optional
  volatile: false,                                  // optional
});
```

Parsed into an owned Rust struct before anything is registered:

```rust
struct JsCommandSpec {
    key: CommandKey,
    metadata: CommandMetadata,
    state_mode: StateMode,
    run: js_sys::Function,
    is_async: IsAsync,
}

/// How the input `State` is presented to the JS callable.
enum StateMode {
    /// No state argument — a *source* command (`world` in `world/greet`).
    None,
    /// The converted value only.
    Value,
    /// `state.try_into_string()`.
    Text,
    /// A `LiquersState` wrapper, giving access to metadata.
    State,
}

/// Whether `run` returns a Promise. Explicit rather than sniffed at registration:
/// a plain function may still return a Promise at call time.
enum IsAsync {
    /// Declared or detected async; result is awaited.
    Async,
    /// Declared sync; a returned Promise is a `ConversionError`.
    Sync,
    /// Not declared — decided per call by testing the returned value.
    Auto,
}
```

**No default match arm** on `StateMode` or `IsAsync`.

#### Argument declaration, and simple inference over a verified-safe subset

Two paths, by design:

- **Explicit `arguments`** — the reliable path, always available, and sufficient on its own. It is
  what a command should use when it takes anything non-trivial.
- **Regex inference** — a quick, ergonomic path for the common shape, accepted **only over a subset
  where the parse is provably exact**, and refusing loudly everywhere else.

A parser-based inference covering defaults and destructuring can extend the accepted subset later
without changing this contract; that is deliberately out of the initial phase.

##### Why a subset, and which one

The candidate signals were measured rather than assumed.

Measured with `node` against the candidate signals:

| Function | `fn.length` | Finding |
|---|---|---|
| `(state, count)` | 2 | correct |
| `(state, count = 2)` | **1** | collapses at the first default parameter |
| `(state, a = 1, b = 2)` | **1** | two parameters invisible |
| `(state, ...rest)` | 1 | rest parameter invisible |
| `(state, {a, b})` | 2 | destructured parameter **has no name at all** |
| `(state, f = (x,y) => x)` | 1 | naive comma-splitting of `toString()` yields garbage |
| `fn.bind(null)` | 2 | `toString()` → `function () { [native code] }` |

`Reflect.getParameterNames` does not exist, and `Function.prototype` exposes no parameter-name
reflection. Types do not exist at all.

**The canonical technique was implemented and measured, not assumed.** The widely-cited
`toString()`-plus-regex parse (the top answer to
[SO 914968](https://stackoverflow.com/questions/914968/), and the mechanism AngularJS 1.x used for
dependency injection) produces:

| Signature | Returns |
|---|---|
| `(state, count)` | `["state","count"]` — correct |
| `(state, count = 2)` | `["state","count","=","2"]` |
| `(state, f = (x,y) => x)` | `["state","f","=","(x","y"]` |
| `(state, {a, b})` | `["state","{a","b}"]` |
| `(state, ...rest)` | `["state","...rest"]` |
| `function (state /*, hidden */, count)` | `["state","count"]` — correct |
| `(a,b)` after minification | `["a","b"]` — **plausible, and wrong** |
| `fn.bind(null)` | `[]` |
| `Math.max` (native) | `[]` |

The minified row is the disqualifying one: the technique does not fail, it returns confident wrong
names. Metadata would look correct and be a lie — precisely the failure the guide warns about when
it says a stale declaration is worse than none.

**The industry precedent is decisive.** AngularJS 1.x used exactly this parse for DI; minification
broke it so comprehensively that Angular added the `$inject` property, the array-annotation form
`['$scope', function($scope){}]`, and the `ngAnnotate` build step. A major framework tried
inference-from-`toString` at scale and retreated to explicit declaration.

**The stronger variant is also rejected.** Parsing with a real JavaScript parser (esprima/acorn)
instead of a regex would handle destructuring, nested parens and defaults correctly, but it is still
defeated by minification, it still cannot evaluate a default *expression* such as
`count = DEFAULT_COUNT` because that identifier lives in the function's closure, and it would link a
JavaScript parser into the wasm artifact — against decision 7's size goal. Rejected on all three
counts.

##### The rule

1. **An explicit `arguments` array always wins**, is used verbatim, and is the documented reliable
   path. The two are never merged.
2. **When `arguments` is absent, infer** — but only over the safe subset:
   - Strip comments, take the text between the first `(` and the first `)`, split on `,`.
   - **Every token must match `^[A-Za-z_$][A-Za-z0-9_$]*$`** — a plain identifier. Any token that
     does not (a default `=`, a rest `...`, a destructuring `{`/`[`) **refuses inference** with a
     `ParameterError` naming the offending parameter and directing the author to declare
     `arguments`.
   - **Cross-check `tokens.length == fn.length`.** This catches bound and native functions, whose
     `toString()` yields no recoverable parameter list, and any case where the split disagrees with
     the one reliable signal. Because defaults and rest are already refused, this is an exact
     equality rather than a one-directional check.
   - Drop the first token when `state_mode != StateMode::None`.
3. Inferred arguments get `ArgumentType::Any` — already the enum's `#[default]`
   (`command_metadata.rs:170-172`) — and no default value. A JS default like `count = 2` is *not*
   readable as metadata; a command needing one declares `arguments`.
4. Inferred arguments are flagged `inferred: true` in metadata, so `describeCommand(name)` shows
   exactly what was registered and how it was obtained (`COMMAND05`, `COMMAND09`).

##### Verified behaviour of the rule

Measured against the implementation of the rule above:

| Signature | Result |
|---|---|
| `(state, count)` | infer `["count"]` |
| `(state, a, b)` | infer `["a","b"]` |
| `(a, b)` — no state | infer `["a","b"]` |
| `(state)` | infer `[]` |
| `function (state /*, hidden */, count)` | infer `["count"]` — comments stripped correctly |
| `(state, count = 2)` | **refuse** — `"count = 2"` is not a plain identifier |
| `(state, f = (x,y) => x)` | **refuse** — `"f = (x"` is not a plain identifier |
| `(state, {a, b})` | **refuse** — `"{a"` is not a plain identifier |
| `(state, ...rest)` | **refuse** — `"...rest"` is not a plain identifier |
| `fn.bind(null)` | **refuse** — token count 0 ≠ `fn.length` 2 |
| `Math.max` (native) | **refuse** — token count 0 ≠ `fn.length` 2 |
| `(a,b)` after minification | infer `["b"]` — **correct arity, wrong name** |

Every case the regex mangles is refused with a specific reason. The subset it accepts, it parses
exactly.

##### The one residual failure, and why it is acceptable

Minification is undetectable: `(a,b)` parses cleanly and consistently, so inference accepts it and
produces `["b"]` instead of `["count"]`.

**This degrades labels, not behaviour.** Liquers binds query arguments **positionally** —
`repeat-3`, not `repeat-count=3` — and arity survives minification intact. So a minified inferred
command still evaluates correctly; what suffers is the argument *name* shown in metadata,
documentation and UI labels. That is a real cost but a bounded one, and it is the price of the
ergonomic path. Authors who need stable names, or who ship through a minifying bundler, declare
`arguments` — which is exactly what AngularJS 1.x concluded when minification broke its
`toString`-based DI and it added `$inject` and `ngAnnotate`.

Implementation must therefore make the degradation visible rather than silent: when every inferred
name is ≤ 2 characters and there is more than one, a `console.warn` notes that the names look
minified and suggests an explicit declaration. Cheap, and it surfaces the one failure the parse
cannot detect.

No JavaScript parser is linked into the wasm artifact (decision 7's size goal); the regex plus the
identifier check is the whole mechanism.

## Trait Implementations

### `ValueExtension for ExtValue` — extended, not changed

The existing impl (`liquers-lib/src/value/mod.rs:113`) gains one cfg'd arm per method. Behaviour of
the new variant:

| Method | `ExtValue::Js` result |
|---|---|
| `identifier()` | `"js"` |
| `type_name()` | the captured `type_tag` |
| `default_extension()` | `"json"` |
| `default_filename()` | `"value.json"` |
| `default_media_type()` | `"application/json"` |
| `try_into_string()` | `ConversionError` — JS `String(obj)` coercion is not a faithful text conversion |
| `try_into_json_value()` | `ConversionError` by default; structural degradation is opt-in (decision 2) |

### `DefaultValueSerializer` — deliberate failure

```rust
fn as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>       // Err(SerializationError)
fn deserialize_from_bytes(b: &[u8], type_identifier: &str, data_format: &str) -> Result<Self, Error>
```

#### Typed errors at each scope boundary (Phase 1 carried item (c))

Phase 1 required the *exact* typed error for an opaque value leaving session/realm scope. No new
`ErrorType` variant is introduced — existing ones cover every boundary:

| Boundary crossed | `ErrorType` | Constructor |
|---|---|---|
| Byte serialization (`as_bytes`, store persist) | `SerializationError` | `Error::from_error(ErrorType::SerializationError, …)` |
| `deserialize_from_bytes` with a `js` type identifier | `SerializationError` | — a `Js` value can never be reconstructed from bytes |
| Structural conversion requested (`try_into_string`, `try_into_json_value`) | `ConversionError` | `Error::conversion_error(type_tag, target)` |
| Worker or second realm (future `POLYGLOT`/worker work) | `NotSupported` | `Error::not_supported(…)` |

Every message names the captured `type_tag`, so the failure identifies *which* JS object caused it.

Both `DefaultValueSerializer` methods return `Error::from_error(ErrorType::SerializationError, …)`
for the `Js` variant, naming the type tag.
**This is safe because the core already absorbs it** — verified in Phase 1: `assets.rs:2994` falls
back to `Version::from_time_now()` and `assets.rs:3016` to `store.set_metadata(...)`. A JS value in
an asset therefore degrades to metadata-only persistence instead of failing evaluation. Consequence
to document: such assets carry a time-based version and look freshly changed to dependency tracking.

### No `CommandExecutor` implementation

`DefaultEnvironment` sets `type CommandExecutor = CommandRegistry<Self>`. JS commands are registered
**into that registry**, so the trait is not implemented again:

```rust
// Async JS command — the general case (decision 6).
cr.register_async_command(key, move |state, args, ctx| {
    let f = run.clone();                 // js_sys::Function, cheap handle clone
    let spec = spec.clone();
    Box::pin(async move { call_js_command(f, spec, state, args, ctx).await })
})?;
```

This compiles on wasm because `AsyncExecutorFn<E>` drops `+ Send + Sync` under
`#[cfg(target_arch = "wasm32")]` (`commands.rs:463-470`) — the async-wasm-refactor's work is what
makes a JS closure a legal command executor. The returned future is `'static`: the closure clones
the `Function` and takes `state`/`args`/`ctx` by value, borrowing nothing.

**Sync JS commands** additionally register through `register_command` so that
`CommandExecutor::execute` (the sync path, `commands.rs:414`) can serve them. A command declared
`IsAsync::Async` is registered *only* on the async path; if the sync path is reached for it, the
result is an `ExecutionError` stating that an async command cannot execute synchronously — rather
than a silent wrong answer.

### Unregistration — a small `liquers-core` addition

#### Why unregister is needed

Replace already covers the obvious case — a page re-registering its commands on reload — so
unregister has to earn its place on something replace cannot do. It does:

1. **Otherwise, a command's JavaScript resources are pinned for the environment's lifetime.** This
   is the substantive reason. A JS command closure owns a `js_sys::Function`, and that function's
   closure scope can retain arbitrary JS objects: DOM nodes, large `ArrayBuffer`s, event listeners,
   WebSocket or `EventSource` handles. The closure is stored in
   `Arc<Box<AsyncExecutorFn<E>>>` inside the registry's map, so nothing releases it while the
   environment lives. In a single-page app that registers commands per route or per view,
   navigating away therefore leaks every JS object those commands captured, with no way to
   reclaim them short of discarding the whole environment — and the environment is the *singleton*
   on the primary path (decision 4). `unregister` drops the map entry, the `Arc` refcount reaches
   zero, the `Function` drops, and the wasm-bindgen heap slot is released. This is `RUNTIME05`
   ("cancellation and shutdown release handles") applied to registration, and it is the same
   pinning analysis Phase 1 decision 2 made for opaque values.
2. **Test isolation on the singleton path (`ENVIRON05`).** An explicit instance can be dropped
   wholesale, but tests exercising the documented default path share one global environment.
   Without removal, each test leaks its registrations into the next.
3. **`COMMAND06` conformance without a carve-out.** The guide requires a duplicate *and* unregister
   policy. Replace-only would mean marking half the feature `NA` in a design whose whole purpose is
   to conform.

**What unregister does *not* fix, stated because it is easy to assume otherwise.** Replacing a Rust
command **destroys** it rather than shadowing it: `register_command` and `register_async_command`
`insert` into the same map keyed by `CommandKey` (`commands.rs:497`), overwriting the previous
executor, and `add_command` overwrites the metadata in place. There is no saved original, so
`unregister` removes the *replacement* and leaves the key empty — it cannot restore the built-in.
Within a session a shadowed Rust command is gone until the environment is rebuilt. This is the
reason replacement now warns; if restoring built-ins ever matters, it needs a separate mechanism
(saving the displaced executor) and is out of scope here.

#### Specification

`COMMAND` asks for register, inspect, replace **and unregister**. Replace exists; unregister does
not — neither `CommandRegistry` nor `CommandMetadataRegistry` has any removal method today
(verified: `command_metadata.rs` exposes `add_command`, `get`, `get_mut`,
`update_command_metadata_version`, `update_all_metadata_versions`, `find_command*` and nothing else).
Two purely additive inherent methods are therefore added to `liquers-core`:

```rust
// liquers-core/src/command_metadata.rs
impl CommandMetadataRegistry {
    /// Removes a command's metadata. Returns the removed metadata, or `None` if absent.
    pub fn remove_command<K>(&mut self, key: K) -> Option<CommandMetadata>
    where K: Into<CommandKey>;
}

// liquers-core/src/commands.rs
impl<E: Environment> CommandRegistry<E> {
    /// Removes a command's sync executor, async executor and metadata.
    ///
    /// Returns `true` if anything was removed. Idempotent: unregistering an absent
    /// command is `false`, not an error.
    pub fn unregister<K>(&mut self, key: K) -> bool
    where K: Into<CommandKey>;
}
```

**The correctness requirement is that all three are removed together.** `CommandRegistry` holds two
executor maps plus the metadata registry (`commands.rs:470-474`), and planning consults *metadata*
while execution consults the *executors*. Removing only the executors would leave a command that
plans successfully and then fails at execution with `unknown_command_executor`; removing only the
metadata would leave an unreachable executor. `unregister` removes from `executors`,
`async_executors` and `command_metadata_registry` in one call — which is precisely why it belongs in
core next to the maps it owns, rather than being assembled by callers.

**Additive and safe for existing code:** these are inherent methods, not trait methods, so no
implementor anywhere — including `liquers-py` — is affected.

**One documented behavioural consequence.** `add_command` preserves an existing command's
`impl_version` when replacing it (`command_metadata.rs:1062`). After `unregister`, that history is
gone, so a subsequent re-registration starts from a fresh `impl_version`. Assets computed by the
earlier command therefore see a version change and expire. This is *correct* — an unregister/
re-register genuinely is a new implementation, unlike a replace — but it differs from replace and
must be stated, because it is the kind of difference that otherwise surfaces as mysterious cache
invalidation.

**JS surface:**

```rust
#[wasm_bindgen(js_name = unregisterCommand)]
pub fn unregister_command(name: &str) -> bool;          // module-level, singleton
// and the same on LiquersEnvironment
```

Returns `false` for an unknown command rather than throwing, so teardown paths need no
existence check. Unlike replacement, unregistration is explicit by nature and emits **no** warning.

### Namespace policy

**Root is the primary path.** A declaration without a `namespace` registers into the root namespace,
where it sits alongside the Rust commands and is callable with no namespace prefix in a query. This
keeps the minimal declaration minimal.

**Any explicit namespace is supported.** `namespace: "myapp"` puts the command in `myapp`, reachable
via the usual `ns-myapp/` selector. There is deliberately **no** forced `js` namespace: a command's
namespace should reflect what it is for, not which language implemented it. The user picks.

**Duplicate policy: replace, and always warn.** Registering a name that already exists replaces both
the executor and the metadata. This is already `CommandMetadataRegistry::add_command`'s behaviour
(`command_metadata.rs:1058-1071` — it overwrites in place and preserves `impl_version`), so the
policy is inherited rather than invented, and it matches an iterative browser workflow where a page
re-registers commands on reload.

**Every replacement emits a `console.warn`** naming the command key and what it replaced —
never silently. Two messages, because the two cases differ in how surprising they are:

| Case | Message |
|---|---|
| JS command replaces a JS command | `liquers: command "<key>" was replaced` |
| JS command replaces a **Rust** command | `liquers: command "<key>" replaces a built-in Rust command` |

Registration still succeeds in both cases (`COMMAND06`). The warning exists because replacement is
indistinguishable from an accidental name collision at the point it happens, and a shadowed built-in
is otherwise invisible until a query returns the wrong thing.

This is consistent with the project's diagnostic-output rule (`CLAUDE.md`): `console.warn` is the
browser analogue of `eprintln!`, not of `println!` — it goes to the console's error/warning stream
and never contaminates a value or a returned payload.

**Reserved namespace.** `web` is reserved for platform-dependent commands provided by `liquers-web`
itself — browser-only operations such as `alert`, and later DOM or `window` access — so that a query
using them is visibly platform-bound and a native environment can reject or stub them. The initial
phase **registers no commands there**; the name is reserved now so that later additions do not have
to fight user code for it. Registration into `web` from JS is rejected with a `ParameterError`.

## Sync vs Async

| Operation | Model | Rationale |
|---|---|---|
| `Query.parse`, `Key.parse`, encoding, metadata reads | sync | Pure CPU, no I/O — a Promise would be noise |
| `evaluate(query)` | **Promise** | `ASYNCQ`. The guide forbids simulating sync evaluation in a browser |
| `init()` | **Promise** | Guide `ENVIRON`: "Browser startup should return a `Promise`; do not expose a blocking initializer" |
| `registerCommand` | sync | Registry mutation, no I/O |
| JS command `run` | value **or** Promise | Decision 6 |

**No blocking API is exposed at all** — there is no `evaluateSync`. `ASYNCQ06`/`ASYNCQ07` are
satisfied by construction rather than by discipline.

### The Promise bridge

```rust
#[wasm_bindgen]
impl LiquersEnvironment {
    /// Evaluates a query. Resolves with the value, rejects with a structured `LiquersError`.
    pub fn evaluate(&self, query: &str) -> js_sys::Promise;
}
```

Implemented with `wasm_bindgen_futures::future_to_promise`. The future owns an `EnvRef` clone and an
owned `Query`; nothing is borrowed from `&self`. Rejection carries a `LiquersError` object, never a
bare string (`ASYNCQ02`).

Direction JS → Rust uses `JsFuture::from(promise)` inside `call_js_command`.

**Cancellation** (`ASYNCQ04`): a JS `Promise` is not cancellable, so cancellation is exposed on the
*asset*, not the Promise — `LiquersAsset.cancel()`, with the Promise then rejecting with
`ErrorType::Cancelled`. This is the honest mapping and must be documented as such.

## Reentrancy — discharging the Phase 1 concern

Phase 1 decision 5 chose inline re-entrant evaluation and asked Phase 2 to show it cannot
self-deadlock against `ImmediateAssetManager`'s `std::sync::Mutex`. The argument:

1. **`liquers-web` never holds a lock it does not own.** The only state it owns is `GLOBAL_ENV`
   (`RefCell`), and every access clones the `EnvRef` and drops the borrow before any `await` or any
   call into JS. This is a checkable rule, not a hope.
2. **The dangerous pattern is a manager lock held across the JS call.** `call_js_command` therefore
   converts arguments to owned Rust values, then enters JS. It never invokes JS while holding a
   guard obtained from the asset manager.
3. **Async commands make re-entrancy safe rather than merely tolerable.** An async JS command that
   calls `evaluate` yields to the event loop instead of occupying it, so the nested evaluation runs
   as a separate task.
4. **Residual risk, stated:** a *synchronous* JS command that re-enters `evaluate` cannot complete —
   the inner evaluation needs the event loop the outer call is occupying. Policy: `evaluate` is
   Promise-only, so a sync command physically cannot await it; it can only start it. Nested
   evaluation from a sync command that then depends on the result is rejected with a typed error.

`RUNTIME04` tests this; if the test cannot be made to pass, this section is wrong and the design,
not the test, changes.

## Error Handling (`ERROR`)

```rust
#[wasm_bindgen]
pub struct LiquersError {
    inner: liquers_core::error::Error,
}
```

A **single structured error class**, not a typed hierarchy — `ErrorType` has 22 variants
(`error.rs:13-39`, including `Cancelled`) and 22 JS subclasses would be surface without benefit. The class exposes
`errorType` (string), `message`, `position`, `query`, `key`, `commandKey`, plus `jsClass` and
`jsStack` when the error originated as a JS exception.

- **Every `ErrorType` maps** (`ERROR01`) via an exhaustive match, no `_ =>` arm.
- **JS exception → `Error`:** default `ExecutionError`; a *conversion* failure is `ConversionError`,
  not an execution failure (guide `ERROR`). Class name and stack are preserved in the message tail.
- **Non-`Error` throws** (`throw 42`, `throw undefined`) → `ExecutionError` with the coerced string
  (`ERROR04`).
- **No panic crosses the boundary** (`ERROR05`): `console_error_panic_hook` is installed by `init()`,
  and every `#[wasm_bindgen]` entry point returns `Result<_, JsValue>` rather than unwinding.
- All construction uses typed constructors (`Error::conversion_error`, `Error::general_error`,
  `Error::from_error`) — never `Error::new`.

## Value Bridge Conversion Table (`VALUE`)

JS → Rust (`to_value`), structural by default:

| JavaScript | Rust | Notes |
|---|---|---|
| `null`, `undefined` | `SimpleValue::None` | Both collapse; documented as lossy |
| `boolean` | `Bool` | |
| `number` (integral, \|n\| ≤ 2^53) | `I64` | |
| `number` (non-integral) | `F64` | |
| `BigInt` | `I64` | Out-of-`i64`-range → `ConversionError`, never silent wraparound (`VALUE03`) |
| `string` | `Text` | UTF-16 → UTF-8 |
| `Uint8Array`/`ArrayBuffer` | `Bytes` | **Never** `Text` (`VALUE04`) |
| `Array` | `Array` | Recursive |
| plain object | `Object` (`BTreeMap`) | Recursive; non-string keys → `ConversionError` |
| `Query`/`Key` wrapper | `Query`/`Key` | Unwrapped, not re-parsed |
| class instance, `Map`, `Set`, DOM node, function | **`ConversionError` unless opted in** | Decision 2 |
| `liquers.opaque(x)` | `ExtValue::Js` | The explicit opt-in |

Rust → JS is the inverse, with `Bytes` → `Uint8Array` and `ExtValue::Js` → the original object.

**Cycles** (`VALUE07`): structural conversion is depth-limited and a cycle raises `ConversionError`
naming the path. It is not silently truncated. Opaque retention has no cycle problem.

**Numbers.** `i64` beyond 2^53 cannot round-trip through `number`; conversion is checked and
`BigInt` is the lossless path. `NaN`/`±Infinity` map to `F64` and are preserved.

## Integration Points

| Crate | Change |
|---|---|
| `liquers-core` | Two additive inherent methods: `CommandMetadataRegistry::remove_command` and `CommandRegistry::unregister` (see "Unregistration"). No trait changes, so no implementor is affected. |
| `liquers-lib` | `ValueExtension` bound relaxed to `MaybeSend + MaybeSync + 'static` (decision 1); `ExtValue::Js` variant + cfg'd match arms (Option Y) |
| `liquers-store` | Not a dependency in this phase (no `STORE`) |
| `liquers-web` | New crate |
| Workspace | New member; **`default-members` excludes it** so the native test loop in `CLAUDE.md` is unaffected |

### Target gating

`JsValue` is `!Send`/`!Sync` on *every* target, and `MaybeSend` = `Send` on native. So the crate's
functional body is `#[cfg(target_arch = "wasm32")]`; on native it compiles to a near-empty crate
with a documented "wasm32 only" notice. This keeps `cargo check --workspace` green without
pretending the crate works natively. `liquers-lib` is depended on with
`default-features = false, features = ["webui"]` — `egui` and `polars` are not wasm targets.

### Dependencies

`wasm-bindgen`, `wasm-bindgen-futures`, `js-sys`, `web-sys`, `serde-wasm-bindgen`,
`console_error_panic_hook`; `liquers-core`, `liquers-lib`. Dev: `wasm-bindgen-test`.

## Function Signatures

Consolidated public surface. Rust signatures; the JS name is given where it differs.

### Module-level (singleton path — the documented default)

```rust
#[wasm_bindgen] pub fn init() -> js_sys::Promise;                    // → LiquersEnvironment
#[wasm_bindgen] pub fn evaluate(query: &str) -> js_sys::Promise;     // → converted value
#[wasm_bindgen(js_name = registerCommand)]
pub fn register_command(spec: JsValue) -> Result<(), JsValue>;
#[wasm_bindgen] pub fn opaque(value: JsValue) -> LiquersValue;       // decision 2 opt-in
#[wasm_bindgen] pub fn version() -> String;                          // ENVIRON capability probe
```

### `LiquersEnvironment` (js_name `Environment`)

```rust
#[wasm_bindgen(constructor)] pub fn new() -> Result<LiquersEnvironment, JsValue>;
#[wasm_bindgen(js_name = global)] pub fn global() -> Result<LiquersEnvironment, JsValue>;
pub fn evaluate(&self, query: &str) -> js_sys::Promise;
pub fn evaluate_query(&self, query: &LiquersQuery) -> js_sys::Promise;   // js: evaluateQuery
pub fn get_asset(&self, query: &str) -> Result<LiquersAsset, JsValue>;   // js: getAsset
pub fn register_command(&self, spec: JsValue) -> Result<(), JsValue>;    // js: registerCommand
pub fn describe_command(&self, name: &str) -> Result<JsValue, JsValue>;  // js: describeCommand
pub fn command_names(&self) -> Vec<JsValue>;                             // js: commandNames
```

### `LiquersQuery` (js_name `Query`) and `LiquersKey` (js_name `Key`)

```rust
#[wasm_bindgen(js_name = parse)] pub fn parse(s: &str) -> Result<LiquersQuery, JsValue>;
pub fn encode(&self) -> String;
#[wasm_bindgen(js_name = toString)] pub fn to_string_js(&self) -> String;
pub fn equals(&self, other: &LiquersQuery) -> bool;
pub fn is_empty(&self) -> bool;
// Key additionally:
pub fn parent(&self) -> Option<LiquersKey>;
pub fn filename(&self) -> Option<String>;
pub fn to_absolute(&self, base: &LiquersKey) -> Result<LiquersKey, JsValue>;
```

### `LiquersValue`, `LiquersState`, `LiquersAsset`, `LiquersError`

```rust
// LiquersValue (js_name Value)
pub fn to_js(&self) -> Result<JsValue, JsValue>;      // js: toJS — structural, or the original object
pub fn type_name(&self) -> String;                    // js: typeName
pub fn is_opaque(&self) -> bool;                      // js: isOpaque

// LiquersState (js_name State)
pub fn value(&self) -> Result<LiquersValue, JsValue>;
pub fn metadata(&self) -> LiquersMetadata;

// LiquersAsset (js_name Asset) — the low-level API beside the convenience one (EVAL)
pub fn status(&self) -> String;
pub fn get(&self) -> js_sys::Promise;                 // → LiquersState
pub fn cancel(&self) -> Result<(), JsValue>;          // ASYNCQ04
pub fn get_asset_info(&self) -> js_sys::Promise;      // js: getAssetInfo

// LiquersError (js_name LiquersError) — all getters
pub fn error_type(&self) -> String;                   // js: errorType
pub fn message(&self) -> String;
pub fn query(&self) -> Option<String>;
pub fn key(&self) -> Option<String>;
pub fn position(&self) -> JsValue;
pub fn js_class(&self) -> Option<String>;             // js: jsClass
pub fn js_stack(&self) -> Option<String>;             // js: jsStack
```

### Internal (not exported to JS)

```rust
fn to_value(js: &JsValue, policy: ConversionPolicy) -> Result<WebValue, Error>;
fn from_value(v: &WebValue) -> Result<JsValue, Error>;
async fn call_js_command(
    run: js_sys::Function,
    spec: Arc<JsCommandSpec>,
    state: State<WebValue>,
    args: CommandArguments<WebEnvironment>,
    context: Context<WebEnvironment>,
) -> Result<WebValue, Error>;
fn js_error_to_liquers(err: JsValue, fallback: ErrorType) -> Error;
fn liquers_error_to_js(err: Error) -> JsValue;         // → LiquersError instance
```

`WebValue` is `liquers_lib::value::Value` under Option Y, or `CombinedValue<SimpleValue, JsExt>`
under Option X — the alias is the single place the two options differ in this section.

## Delivery: `STUBS` and `PACKAGE` (Phase 1 decision 7)

Both were selected at "minimal" level in Phase 1 and are part of this phase's architecture, not a
later milestone.

### Build

`trunk` for the example/quick-start page; `wasm-bindgen-cli` produces the artifact itself. The
repository already runs this toolchain — `liquers-lib/examples-web/` builds with `trunk build` and
is exercised by Playwright (`CLAUDE.md`, "Building and testing"), so `liquers-web` adopts that
existing setup rather than introducing a second one.

```
liquers-web/
  examples/quickstart/     # index.html + main.rs, `trunk build`
```

### Delivery forms

| Form | This phase | Notes |
|---|---|---|
| Plain `<script type="module">` from a website or CDN | **yes — the requirement** | No bundler assumed. `wasm-bindgen --target web` output is loaded by a hand-written `<script type="module">` with an explicit `init()` call |
| `trunk` page bundle | **yes** | The quick-start example |
| Single-file embedded wasm | deferred | Wanted next per decision 7; the `gymlog` reference is still unverified and must be examined before committing to a mechanism |
| npm / `wasm-pack` package | deferred | Explicitly later |

Because the plain-page form is the requirement, the wasm artifact must not assume a bundler's
module resolution: the generated JS glue is loaded directly and the `.wasm` URL is resolvable
relative to it.

### `STUBS`

`wasm-bindgen` emits `.d.ts` automatically from the `#[wasm_bindgen]` surface. Per the guide these
are a **review target, not an authored file**. Consequences for this design:

- The emitted `.d.ts` is only as good as the Rust signatures, so the exported surface uses concrete
  types (`&str`, `js_sys::Promise`, wrapper structs) rather than bare `JsValue` wherever a more
  specific type is possible. `JsValue` in a signature becomes `any` in the declarations, which
  satisfies a type checker while telling the user nothing.
- `registerCommand(spec: JsValue)` is the unavoidable exception — the spec object is
  heterogeneous. A hand-written `LiquersCommandSpec` interface is shipped alongside and the
  parameter is declared with it, since this is the surface users touch most.
- The `.d.ts` ships with the artifact (`STUBS07`); a build step checks it is regenerated rather
  than stale.

## Relevant Commands

**New commands: none required.** `liquers-web` is an API surface, not a command library. The only
Rust commands it adds are test fixtures for the conformance suite (source commands such as `hello`
and `number-<n>`, needed because *every query segment is a command* — there is no literal-value
segment, per the guide's Appendix A).

**Existing namespaces inherited under Option Y** (Option X inherits none):

| Namespace | Registration | wasm-viable | In this phase |
|---|---|---|---|
| root (`to_text`, `to_metadata`, …) | `register_core_commands!` | yes | **yes** |
| `lui` (14 commands) | `register_lui_commands!` | yes (backend-neutral; `webui` renders) | deferred to `UIUSE`, but available |
| `img` (47) | `register_image_commands!` | partly — `imageproc`/`resvg` unverified on wasm | no |
| `pl` (26) | `register_polars_commands!` | no — polars is not a wasm target | no (`NA`) |
| `dep` (2) | core | yes | yes |

## Open Questions for the user

1. ~~Option X vs Option Y~~ — **decided: Option Y**, plus the generic-internals extension path in
   "Extensibility" above, so a user can still supply their own value type and environment.
2. ~~Command namespaces~~ — **decided**, see "Namespace policy" below.
3. ~~Unregistration~~ — **decided: in scope.** `CommandRegistry::unregister` and
   `CommandMetadataRegistry::remove_command` are added to `liquers-core`; see "Unregistration".
   `COMMAND06` is then satisfied fully rather than by a documented limitation.

**No open questions remain.** Phase 2 is ready for approval.

## Carried into Phase 3

- Benchmark the opaque-vs-structural boundary cost (Phase 1 decision 2's unmeasured hypothesis).
- `RUNTIME04` must actually exercise the reentrancy argument above; if it cannot be made to pass,
  the design changes, not the test.
- Test cases for the inference rule, one per row of the verified-behaviour table: each accepted
  shape infers the right names, and each refused shape fails registration with its specific reason
  rather than misbinding. Plus: an explicit `arguments` array overrides inference entirely, and a
  minified-looking inference emits the warning.
- Unregistration tests (`COMMAND06`): after `unregister`, the query fails to *plan* rather than
  failing at execution — the assertion that proves metadata and both executor maps were removed
  together; `unregister` of an absent command returns `false` without error; and
  unregister-then-re-register yields a fresh `impl_version`, expiring assets computed by the earlier
  command. These are `liquers-core` unit tests, not only browser tests.
- Namespace tests: root default, explicit namespace, replace-on-duplicate, **a warning on every
  replacement** (both the JS-replaces-JS and JS-replaces-Rust messages), and rejection of
  registration into the reserved `web` namespace. The warning assertions need a captured
  `console.warn` — a spy installed by the test harness, since `wasm-bindgen-test` does not capture
  console output by default.
- The generic bridge functions must be instantiated at a **second** value type in the test suite, so
  the Tier-2 extension path is proven rather than assumed.
- `PACKAGE03`: the quick-start page must evaluate a query end to end from a plain
  `<script type="module">`, with no bundler.

## Review record

Phase 2 was reviewed by the two prescribed reviewers. Corrections applied:

- `ErrorType` has **22** variants, not 21 (`Cancelled` was missed).
- The `ExtValue` match-site count is **14**, itemized above — the initial "~8-10 plus render paths"
  estimate was low.
- Explicit environment instances had no constructor or API; added.
- Argument inference was deferred to Phase 3 despite Phase 1 assigning it here; the rule is now
  specified, including the `fn.length`-with-defaults trap.
- `STUBS`/`PACKAGE` were absent despite being selected in Phase 1; added.
- The typed error for opaque values leaving scope was unspecified; now a table over existing
  `ErrorType` variants.

Independently verified against source by the codebase reviewer: `DefaultEnvironment`'s cfg-selection
and generic value type, `CommandExecutor = CommandRegistry<Self>`, the wasm `Send`-dropping executor
aliases, `register_async_command`'s bounds, `add_command`'s replace-on-duplicate, the absence of any
unregister method, `ValueExtension`'s current bounds, the `webui` feature's wasm-bindgen
dependencies, and `Value = CombinedValue<SimpleValue, ExtValue>` with `SimpleValue: Default`.
