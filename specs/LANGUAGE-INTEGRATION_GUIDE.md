# Liquers Language Integration Guide

Status: Draft  
Audience: integration designers, implementers, reviewers, and coding agents

## 1. Purpose

This guide defines independently targetable features for integrating Liquers with a programming or scripting *language*. It is not an API specification for one *language*. An *integration* design should select features, answer their design questions, and report implementation and test status by feature ID.

Short-term priorities are:

1. JavaScript in the browser through `wasm-bindgen`, `web-sys`, and `js-sys`
2. Starlark through `starlark-rust`
3. Python through `liquers-py`

Longer-term candidates include Rhai, JavaScript/TypeScript on Node or Deno, RustPython, generic Wasm guests, C/C++, and GDScript.

`liquers-py` is a useful but partial reference, not the conformance definition. In particular, it contains many basic wrappers and an experimental `pycall`, but also incomplete paths. See [PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md](PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md), [PYTHON-WRAPPER-ARCHITECTURE.md](PYTHON-WRAPPER-ARCHITECTURE.md), and [FEATURES/PYTHON-BASIC-OBJECTS.md](FEATURES/PYTHON-BASIC-OBJECTS.md).

## 2. Terminology and Conventions

Terms defined here are written in *italics* when used in their precise, normative sense. Ordinary uses of the same English words need not be italicized.

- *Integrated language* (or *language*): the scripting or programming language being connected to Liquers, such as JavaScript, Starlark, or Python.
- *Language runtime*: the interpreter, virtual machine, event loop, or browser realm that executes the *integrated language*.
- *Integration*: the complete adapter between Liquers/Rust and one *integrated language*.
- *Core object*: a Liquers Rust object such as `Query`, `Key`, `Recipe`, `Plan`, `Metadata`, or `Error`.
- *Wrapper*: a language-visible object that owns or safely refers to a Rust *core object* or service.
- *Language value*: a value native to the *integrated language*, such as a JavaScript object, Starlark value, or Python object.
- *Value type*: the Rust type selected by the environment that implements `ValueInterface`.
- *Value bridge*: the conversion and wrapping layer between *language values* and the *value type*.
- *Opaque language value*: a *language value* retained by identity inside the *value type* without structural conversion.
- *Structural conversion*: copying a *language value* into ordinary variants of the *value type*, such as none, boolean, number, string, bytes, array, or object.
- *Upcast*: in this guide, wrapping a concrete *language value* as an *opaque language value* in the more general *value type*.
- *Downcast*: recovering a concrete *language value* or Rust extension value from the *value type* after checking its variant/type tag.
- *State*: `State<V>`, meaning a *value type* value together with Liquers metadata and execution state.
- *Language command*: a Liquers command whose callable implementation is written in the *integrated language*.
- *Service adapter*: an implementation of a Liquers service trait, such as `AsyncStore` or `AsyncRecipeProvider`, backed by the *integrated language*.
- *UI backend*: the renderer and event-loop implementation used to display Liquers UI elements, such as egui or a browser UI.
- *Web handler*: a framework-compatible function that maps an HTTP request to a Liquers operation and maps its result to an HTTP response.
- *Embedding host*: the outer process or application that owns Liquers and one or more *language runtimes*.
- *Hard dependency*: a prerequisite feature without which the dependent feature cannot be implemented or meaningfully tested. If a feature is selected, each of its *hard dependencies* must also be selected; a *hard dependency* may not be `NA`.
- *Soft dependency*: a prerequisite feature that the dependent feature interacts with when both are selected, but that it can be implemented without. A *soft dependency* may be `NA`; the design must then state how the dependent feature behaves in its absence.

Unless qualified, “value” is informal. Designs should use *language value*, *value type*, or *State* when the distinction matters.

## 3. How to Use the Features

A feature ID is preferably a pronounceable uppercase word of at most eight alphanumeric characters. A familiar abbreviation is acceptable; exceptionally, a longer ID may be used when shortening it would reduce clarity. IDs contain no underscores. IDs are stable and must not be renamed for one *integrated language*. A specific design may split a feature into milestones, but status and tests must still roll up to the ID.

Use these requirement levels:

- **Essential**: required for a useful in-process *integration*.
- **Profile**: essential for some hosts, such as `ASYNCQ` in a browser.
- **Optional**: an independently selectable extension.

Use these implementation states:

- `NA`: intentionally not applicable, with a reason
- `NS`: not started
- `DESIGN`: design complete, implementation absent
- `PARTIAL`: some required cases work
- `COMPLETE`: all selected requirements implemented
- `CONFORMANT`: complete and all required feature tests pass

Dependencies constrain these states. A feature may not be claimed `COMPLETE` or `CONFORMANT` before every *hard dependency* has reached at least `COMPLETE`. A *soft dependency* imposes no such ordering, but when it is `NA` — or merely not yet implemented — the dependent feature's design must say what it does instead, and that statement is part of the feature's documented limitations. Selecting a feature whose *hard dependency* is `NA` is a design error, not a limitation to be recorded.

A status claim should link to design sections, implementation, and test evidence. Recommended matrix:

| Feature | Selected level | Status | Limitations | Test evidence |
|---|---|---|---|---|
| `VALUE` | Essential | PARTIAL | Opaque values cannot be serialized | `test_VALUE01_*` … |

### Test naming

Each test has a logical ID `<FEATURE><number>`, for example `VALUE01`. Put the logical ID at the start of the test-specific part of the framework name:

```python
def test_VALUE01_primitive_roundtrip(): ...
def test_COMMAND03_exception_crosses_command_boundary(): ...
```

Rust may use `fn value01_primitive_roundtrip()`, and browser tests may use `test("ASYNCQ02 promise rejects with structured error", ...)`. File or module names should also include the feature ID when practical, for example `test_VALUE_value_bridge.py`. Do not reuse a logical test ID for a different contract.

The tests listed below are the default conformance inventory. An *integration* design must mark each one required or `NA` with a reason; `CONFORMANT` means all applicable tests pass.

## 4. Feature Overview

| ID | Feature | Level | Depends on |
|---|---|---|---|
| `OBJECT` | Basic object model | Essential | — |
| `ERROR` | Error bridge | Essential | `OBJECT` |
| `RUNTIME` | Runtime, ownership, and portability constraints | Essential | — |
| `VALUE` | Value bridge | Essential | `OBJECT`, `ERROR`, `RUNTIME` |
| `ENVIRON` | Environment setup and lifecycle | Essential | `OBJECT`, `ERROR`, `RUNTIME` |
| `EVAL` | Query evaluation API | Essential | `ENVIRON`, `VALUE`, `ERROR` |
| `COMMAND` | Language command registration | Essential | `OBJECT`, `ENVIRON`, `VALUE`, `ERROR`, `RUNTIME` |
| `ASYNCQ` | Async query execution | Profile | `EVAL`, `RUNTIME` |
| `ASYNCCMD` | Async language commands | Optional | `COMMAND`, `ASYNCQ`, `RUNTIME` |
| `STORE` | Language-defined async store | Optional | `OBJECT`, `ENVIRON`, `ERROR`, `RUNTIME` |
| `RECIPE` | Language-defined recipe provider | Optional | `OBJECT`, `ENVIRON`, `EVAL`, `ERROR`, `RUNTIME` |
| `UIUSE` | Use an existing Liquers UI | Optional | `EVAL`, `VALUE`, `ERROR`, `ASYNCQ` (soft) |
| `UIDEF` | Define UI elements or a UI backend in the language | Optional | `UIUSE`, `COMMAND`, `RUNTIME` |
| `POLYGLOT` | Multiple-language interoperability | Optional | `ENVIRON`, `VALUE`, `COMMAND`, `ERROR`, `RUNTIME` |
| `WEBSERV` | Start and extend `liquers-axum` | Optional | `ENVIRON`, `ERROR`, `ASYNCQ` (soft), `COMMAND` (soft) |
| `WEBAPI` | Integrate Liquers handlers into a language web framework | Optional | `ENVIRON`, `VALUE`, `ERROR`, `ASYNCQ` (soft) |

Dependencies are *hard* unless marked `(soft)`. The soft cases are those where Liquers, not the *integrated language*, can supply the asynchrony or the callable:

- `UIUSE` → `ASYNCQ`: a *UI backend* may deliver asset status, progress, and events through callbacks scheduled onto the *language runtime*, so a *language* with no async model can still drive a UI. Without `ASYNCQ`, the design must state how progress and cancellation are delivered.
- `WEBSERV` → `ASYNCQ`: the Axum server owns a Rust async runtime, so starting and shutting it down does not require language-visible awaitables. Without `ASYNCQ`, the design must state how foreground/background start, readiness, and shutdown are expressed.
- `WEBSERV` → `COMMAND`: needed only to attach *language*-implemented handlers or middleware. Serving the standard Rust routes requires no *language command*.
- `WEBAPI` → `ASYNCQ`: an ASGI-style adapter awaits Liquers directly, but a WSGI-style synchronous adapter needs only a controlled runtime bridge. Without `ASYNCQ`, the design must state which adapters are supported and how blocking is bounded.

Recommended minimum profiles:

- **General baseline**: `OBJECT ERROR RUNTIME VALUE ENVIRON EVAL COMMAND`
- **Browser JavaScript**: baseline plus `ASYNCQ`; `STORE`, `RECIPE`, `UIUSE`, `UIDEF`, and web features are selected as needed
- **Starlark baseline**: baseline with a deliberately restricted `VALUE`; `ASYNCQ` and `ASYNCCMD` may initially be `NA`
- **Python baseline**: general baseline; later milestones should add `ASYNCQ`, `ASYNCCMD`, `STORE`, `RECIPE`, and selected UI/web features

**Out of scope: cache.** There is deliberately no cache feature. The `liquers_core::cache` module (`BinCache`, `Cache`) is legacy and scheduled for removal — assets provide caching, as noted in [PROJECT_OVERVIEW.md](PROJECT_OVERVIEW.md). An *integration* should not expose it, and an existing wrapper such as `liquers-py/src/cache.rs` should be dropped rather than modernized. This is a scope decision for the guide as a whole, so it does not need a per-design `NA` entry.

## 5. Feature Guidance

### OBJECT — Basic object model

**Contract.** Expose stable *language* representations for at least `Query`, `Key`, command metadata, metadata/status, `Recipe`, `Plan`, and structured `Error` data. *Wrappers* should preserve the Rust *core object* rather than duplicate its semantics. Provide parse/encode, equality, and deterministic inspection.

**Objects/API to map or implement:**

- `Query`, `Key`, `Position`, and their component types (`ActionRequest`, parameters, segments, and resource names)
- `CommandKey`, `CommandMetadata`, `ArgumentInfo`, and `ArgumentType`
- `Metadata`, `MetadataRecord`, `AssetInfo`, status/log/progress types, and expiration types
- `Recipe`, `RecipeList`, `Plan`, and their public component enums/structs
- Shared enum and collection conversion conventions used by all other features

**The design must answer:** Which binding technique is used—such as PyO3 classes, `wasm-bindgen` exported types, Starlark custom values, C ABI handles, or plain serialized data—and why? What common conventions govern constructors, ownership, cloning, getters/setters, `repr`/`toString`, errors, and Rust-to-language conversion? Which objects are wrapped, copied, or represented as plain *language values*? How are Rust enums represented: native language enums, tagged classes/objects, strings, or integer discriminants, and how are unknown future variants handled? Are *wrappers* mutable? How are identity, equality, hashing, subclassing, and invalidated handles handled? Which Rust fields are intentionally hidden?

**Issues and patterns.** Reimplementing the Liquers query/key parser in the *integrated language* can produce different results and can fall out of date when the Rust grammar changes; route strings through the Liquers parser instead. Avoid *wrappers* that borrow a temporary Rust object or temporary *language runtime* value. Prefer owned handles or immutable snapshots. For enums, prefer names/tags over unstable numeric ordinals and define a forward-compatibility policy. Keep convenience APIs above a thin parity layer.

**Meaningful tests:** `OBJECT01` query parse/encode roundtrip; `OBJECT02` key equality/hash; `OBJECT03` command metadata roundtrip; `OBJECT04` invalid parse produces `ERROR`; `OBJECT05` *wrapper* remains valid for its documented lifetime; `OBJECT06` every selected enum variant roundtrips; `OBJECT07` unknown enum variant follows the compatibility policy; `OBJECT08` all *wrappers* follow the documented naming and ownership conventions.

### ERROR — Error bridge

**Contract.** Map every `liquers_core::error::ErrorType` to a language-visible error while preserving message, position, query, key, and command key where available. Convert language exceptions back to Liquers errors without panicking.

**Objects/API to map or implement:**

- `Error`
- `ErrorType` (an integration may expose the clearer language alias `ErrorKind`)
- `Position` and `CommandKey` values carried by an error
- A language-native base exception/error and, if selected, typed subclasses
- Bidirectional `Error` ↔ language exception/rejection conversion functions

**The design must answer:** Is there a typed exception hierarchy or one structured error? How are traceback, cause chain, rejected Promise value, and non-error throws represented? Which error type is the fallback?

**Issues and patterns.** String-only conversion loses diagnostics. Use a structured payload and preserve the original language class/stack as context. Default an unmapped command exception to `ExecutionError`; conversion failures should be `ConversionError`, not execution failures. Never allow a foreign exception to unwind through FFI.

**Meaningful tests:** `ERROR01` every `ErrorType` maps; `ERROR02` fields survive Rust-to-language-to-Rust; `ERROR03` language exception includes class and stack; `ERROR04` invalid or non-error throw has a safe fallback; `ERROR05` no panic crosses the boundary.

### RUNTIME — Runtime, ownership, and portability constraints

**Contract.** Define the integration's execution, thread, ownership, lifetime, reentrancy, cancellation, and shutdown model. Native Liquers requires `Send + Sync`; on `wasm32`, `MaybeSend`/`MaybeSync` and boxed futures deliberately permit single-threaded, non-`Send` values and callbacks. `'static` means no borrowed runtime values may escape into stored commands or background futures.

**Objects/API to map or implement:**

- Rust-side use of `MaybeSend`, `MaybeSync`, and `BoxFuture` (normally constraints, not language-visible objects)
- An integration-defined runtime/interpreter handle
- Integration-defined owned callback/callable handles
- Task, cancellation, and shutdown handles where asynchronous/background work exists
- A reentrancy/nested-evaluation policy

**The design must answer:** Which thread/event loop owns the language runtime? Can callbacks move between threads? What is owned across `await`? How are nested evaluation, locks, cancellation, teardown, and callbacks after shutdown handled? Which compile targets are supported?

**Issues and patterns.** Do not hold a GIL, VM lock, Rust mutex guard, Starlark heap borrow, or JS borrow across blocking work or `await`. Copy, freeze, root, or use an owned handle before crossing the boundary. Separate target-specific type aliases for trait objects; `dyn Trait + MaybeSend` is not a substitute for conditional `+ Send`. Define and test a reentrancy policy rather than relying on the host runtime.

**Meaningful tests:** `RUNTIME01` native adapter satisfies required thread bounds; `RUNTIME02` wasm accepts a non-`Send` callback; `RUNTIME03` stored callback outlives registration scope safely; `RUNTIME04` nested evaluation does not deadlock; `RUNTIME05` cancellation and shutdown release handles; `RUNTIME06` panic/exception containment.

### VALUE — Value bridge

**Contract.** Define a *value bridge* between *language values* and the selected *value type*. Cover none/null, booleans, integers, floats, text, bytes, arrays, string-keyed objects, `Query`, `Key`, metadata-bearing values, and selected `ExtValue` variants. Define `identifier`, `type_name`, default media type, filename, data format, JSON conversion, and byte serialization.

**Objects/API to map or implement:**

- A concrete *value type* implementing `ValueInterface` and `DefaultValueSerializer`
- A language-visible *wrapper* for the *value type*
- A language-visible *wrapper* for `State<V>` or an explicit decision to expose it only through `EVAL`
- Bidirectional structural conversion functions
- Checked *upcast*/*downcast* functions for *opaque language values* and `ExtValue` variants
- Serializer/codec registry and conversion options
- Callable handle/ID representation if callables may be stored

**The design must answer:**

1. Does the *integrated language* have a universal value representation, such as JavaScript `JsValue`, Starlark `Value<'v>`, a JSON value, or Python `Py<PyAny>`? Can it safely be retained inside the *value type*?
2. Which *language values* receive *structural conversion*, which are exposed as *wrappers* around existing Rust values, and which must remain *opaque language values*? Provide a bidirectional mapping table for all supported types.
3. Is the *value type* a new implementation, a generic composition such as `CombinedValue`, or an extension of an existing Liquers value? How are *upcast* and *downcast* operations exposed, checked, and reported on failure?
4. Are conversions strict, permissive, or configurable? How are integer range, NaN/infinity, cycles, shared identity, mutation, and unknown objects handled?
5. Can an *opaque language value* cross stores, threads, processes, Wasm boundaries, or another *language runtime*? Which codecs are supported and trusted?
6. Does the *integrated language* support function pointers, bound methods, closures, or callable objects? May they be retained as *opaque language values*? Are they serializable? Are they accepted as *language commands* through `COMMAND`, even when they are not ordinary data values?
7. Can users operate conveniently on a *value type* *wrapper* or *State* as if it were a language scalar? Which conversions are implicit versus explicit? Can arithmetic, comparison, indexing, iteration, truthiness, and display operators be defined without hiding conversion errors or metadata loss?

**Issues and patterns.** Prefer lossless *structural conversion* and an explicit opaque fallback. A JSON-like representation is portable but cannot preserve arbitrary objects, callable identity, cycles, bytes without a convention, or all numeric types. *Opaque language values* must advertise their ownership and serialization limits. JavaScript `Number` cannot losslessly represent every `i64`; use checked conversion or `BigInt`. Use `Uint8Array` for bytes. Starlark values are heap-lifetime-bound, so copy primitives/containers or retain an owned frozen value; never store a borrowed `Value<'v>`. Python may permissively retain `Py<PyAny>` and use an opt-in pickle codec, with the security implications documented.

Callable values should normally be registered in a callable registry owned by `COMMAND`, while the *value type* stores only a stable callable ID or an explicitly opaque owned handle. This prevents accidental serialization and makes callable lifetime/replacement visible. Operator overloads on a *wrapper* can be ergonomic, but must define whether they operate on the underlying *language value*, convert through a scalar, or execute a Liquers operation. Operations on *State* must state whether metadata is preserved, combined, or discarded; explicit `.value`/`.to_*()` access is the safe baseline.

**Meaningful tests:** `VALUE01` primitive roundtrip; `VALUE02` nested array/object roundtrip; `VALUE03` integer boundaries; `VALUE04` bytes are not confused with text; `VALUE05` unknown object follows configured policy; `VALUE06` opaque serialization fails or uses its codec explicitly; `VALUE07` cycles/shared references follow policy; `VALUE08` representative `ExtValue` roundtrip; `VALUE09` checked *upcast* and *downcast*; `VALUE10` language-only object retains documented identity/lifetime; `VALUE11` callable retention or rejection follows policy; `VALUE12` scalar operators produce the documented result; `VALUE13` *State* operations preserve or deliberately discard metadata.

```python
def test_VALUE05_unknown_object_uses_opaque_value():
    obj = object()
    assert from_liquers(to_liquers(obj)) is obj  # if identity retention is promised
```

### ENVIRON — Environment setup and lifecycle

**Contract.** Construct and expose a coherent `EnvRef` with one command metadata registry, command executor, async store, recipe provider, asset manager, session/payload policy, and *value type*. Define default initialization, explicit configuration, repeated initialization, test reset, and shutdown.

**Objects/API to map or implement:**

- A language-visible environment/`EnvRef` *wrapper*
- An environment builder or configuration object
- `CommandMetadataRegistry` access
- Handles for the selected `CommandExecutor`, `AsyncStore`, `AsyncRecipeProvider`, and `AssetManager`
- `User`, `Session`, and payload configuration where supported
- Initialization, capability/version inspection, reset-for-test, and shutdown functions

**The design must answer:** Is the environment global, scoped, or injectable? Which services have defaults? When is configuration frozen? Can registries be reloaded? How are browser module startup, VM startup, package loading, and resource cleanup reported? Is version/capability discovery available?

**Issues and patterns.** Avoid hidden creation of multiple incompatible environments. Builders are preferable before publication; after publication, either reject mutation or define atomic replacement. A global Python environment may be ergonomic, while embedders and tests still need explicit instances. Browser startup should return a `Promise`; do not expose a blocking initializer.

**Meaningful tests:** `ENVIRON01` default environment evaluates a built-in command; `ENVIRON02` custom services are the services returned by the environment; `ENVIRON03` repeated initialization follows policy; `ENVIRON04` failed initialization is recoverable; `ENVIRON05` isolated test environments do not leak registration; `ENVIRON06` shutdown is idempotent.

### EVAL — Query evaluation API

**Contract.** Accept a query string or `Query`, evaluate it through the configured environment, and return a *language value* plus access to metadata/error information. Specify queued versus immediate evaluation and whether the API returns a *language value*, *State*, or asset handle.

**Objects/API to map or implement:**

- `Query` input and convenience string parsing
- `State<V>` result *wrapper*
- `AssetRef<E>` *wrapper* with status/value/metadata/cancel operations
- `Context<E>` *wrapper* for nested evaluation and logging where exposed
- Synchronous/immediate evaluation functions and convenience value-returning functions
- Evaluation options carrying payload/session/context where supported

**The design must answer:** What is the simplest entry point? How are payload/session/context supplied? When is evaluation considered complete? How are logs, progress, cancellation, asset status, and nested evaluation exposed? Does a sync API block, run inline, or reject use from an active event loop?

**Issues and patterns.** Do not bypass the environment, planner, or asset lifecycle merely to simplify the *wrapper*. Keep a low-level asset/*State* API and a convenience *language value* API. A browser must not simulate synchronous evaluation. Nested evaluation from a *language command* needs the `RUNTIME` reentrancy policy.

**Meaningful tests:** `EVAL01` evaluate a built-in query; `EVAL02` string and wrapped query agree; `EVAL03` metadata and logs are available; `EVAL04` invalid query maps through `ERROR`; `EVAL05` payload/context reaches a command; `EVAL06` cancellation has a defined terminal result.

### COMMAND — Language command registration

**Contract.** Register a callable from the *integrated language* as a *language command* with complete `CommandMetadata`, bind *State* and query parameters, inject supported services/context, invoke the callable, and convert its result or exception. Define first-command versus transform-command behavior and duplicate registration policy.

**Objects/API to map or implement:**

- Language-facing command declaration/decorator/builder object
- `CommandMetadata`, `CommandKey`, `ArgumentInfo`, `ArgumentType`, and registry *wrappers*
- A callable registry and stable callable handle/ID
- A Rust `CommandExecutor`/`CommandRegistry` adapter for language callables
- Argument binder for `State`, ordinary parameters, variadics, and injected `Context`/services
- Register, inspect, replace, and unregister operations

**The design must answer:** What is the most natural, small, and readable declaration in this *integrated language*: a decorator/annotation, function call, builder, module scan, callable object, or data object containing a function and metadata? Show minimal and complete examples. How do signature, annotations, defaults, variadics, documentation, namespace, realm, volatility, and async status map to metadata? Which fields require explicit metadata? How is callable identity retained? Which state-passing modes exist? Is registration reversible?

**Issues and patterns.** Optimize the common declaration without making metadata invisible. Python decorators can derive metadata from annotations and docstrings. A JavaScript declaration can naturally be an object whose `run`/`execute` property is a function or closure and whose other properties contain metadata:

```javascript
liquers.registerCommand({
  name: "repeat",
  state: "string",
  arguments: [{ name: "count", type: "integer", default: 2 }],
  doc: "Repeat the input text.",
  run: (state, count) => state.repeat(count),
});
```

Starlark may use a host function such as `command(name="repeat", fn=repeat, ...)`. Metadata is the planning contract; do not infer silently when the *integrated language* lacks enough type information. Allow explicit overrides and provide metadata inspection after registration. Resolve plan arguments before language binding. Release runtime/VM locks around Rust work and reacquire only for the callback. A bridge command plus callable registry is useful when direct generic registration is awkward, but aliases and callable IDs must remain observable and debuggable.

**Meaningful tests:** `COMMAND01` register and execute a first command; `COMMAND02` transform receives state and typed parameters; `COMMAND03` exception maps through `ERROR`; `COMMAND04` defaults/enums/variadics bind; `COMMAND05` metadata matches the callable declaration; `COMMAND06` duplicate/unregister policy; `COMMAND07` context injection; `COMMAND08` returned opaque value follows `VALUE`; `COMMAND09` minimal declaration has useful metadata defaults; `COMMAND10` complete declaration preserves every supported metadata field; `COMMAND11` closure captures and retains state according to `RUNTIME`.

```python
def test_COMMAND02_transform_receives_state_and_parameter():
    @liquers.command
    def repeat(state: str, count: int = 2) -> str:
        return state * count
    assert liquers.evaluate("hello/repeat-3") == "hellohellohello"
```

### ASYNCQ — Async query execution

**Contract.** Determine whether the *integrated language* has native async execution and, when it does, expose Liquers futures through that abstraction while preserving success, error, cancellation, and lifetime semantics. This is required for browser JavaScript.

**Objects/API to map or implement:**

- Rust `BoxFuture` ↔ native Promise/future/coroutine/awaitable adapter
- Integration-defined evaluation task/handle for languages without awaitables
- Cancellation handle/token
- Optional progress/log stream, iterator, callback, or subscription
- Blocking or polling compatibility API, only where safe

**The design must answer:** Does the *integrated language* support futures, Promises, coroutines, async functions, callbacks, or only synchronous calls? Can Rust/Liquers futures be driven by or bridged to its event loop? What event loop drives the future? Is the result awaitable more than once? How does host cancellation reach Liquers and vice versa? Are progress events ordered? Can a sync *wrapper* be called inside the event loop? If native bridging is impossible, which workaround is supported and what are its limitations?

**Issues and patterns.** Bridge, do not block: Rust `Future` to JavaScript `Promise`, Python awaitable, or an explicit host task. On wasm, `JsFuture` and language callbacks are non-`Send`; use the core's wasm execution model. Exported wasm futures generally need owned (`'static`) inputs. If the *integrated language* has no async model, possible restricted workarounds are: a blocking call executed outside any async worker/event-loop thread; a task handle with `poll`/`wait`/`cancel`; callbacks scheduled onto the *language runtime*; or an explicitly synchronous inline environment. State which operations can deadlock or starve. Never offer blocking wait on a single-threaded browser event loop.

**Meaningful tests:** `ASYNCQ01` await successful evaluation; `ASYNCQ02` failure rejects/raises structured `ERROR`; `ASYNCQ03` two evaluations make progress; `ASYNCQ04` cancellation propagates; `ASYNCQ05` dropping the host handle follows policy; `ASYNCQ06` no event-loop blocking; `ASYNCQ07` documented non-async workaround completes safely; `ASYNCQ08` nested event-loop use is rejected or works without deadlock.

### ASYNCCMD — Async language commands

**Contract.** Detect or explicitly register async callables, await them without blocking Liquers workers, and convert their result/error exactly as for `COMMAND`.

**Objects/API to map or implement:**

- Async callable/coroutine/Promise detector or explicit async command declaration
- Language awaitable ↔ Rust `BoxFuture` invocation adapter
- Async command entry in the callable registry
- Cancellation/timeout bridge for a running *language command*
- Async-aware result and exception conversion

**The design must answer:** How is an async callable declared? Which runtime owns the coroutine/Promise? Can it call back into Liquers? What happens on cancellation, timeout, or runtime shutdown? Are sync and async commands represented differently in metadata?

**Issues and patterns.** Never hold *language runtime* borrows or locks across `await`. Convert arguments to owned values first. JavaScript Promise ↔ Rust Future is the natural browser bridge. Python needs a deliberate Rust/Python event-loop adapter. A Starlark *integration* may mark this feature `NA` if execution is intentionally synchronous.

**Meaningful tests:** `ASYNCCMD01` async command result; `ASYNCCMD02` async exception; `ASYNCCMD03` cancellation in both directions; `ASYNCCMD04` nested async evaluation; `ASYNCCMD05` concurrent calls do not corrupt callable state; `ASYNCCMD06` sync and async metadata differ correctly.

### STORE — Language-defined async store

**Contract.** Adapt a *language value* to the complete `AsyncStore` contract: data and metadata get/set, key support, containment, directory listing/creation/removal, and capability reporting.

**Objects/API to map or implement:**

- `AsyncStore` *service adapter*
- Language-visible store protocol/base class/interface
- `Key`, `Metadata`, `MetadataRecord`, and `AssetInfo`
- Byte-buffer conversion
- Store builder/configuration and composition objects from `liquers-store`, where selected

**The design must answer:** Which methods are mandatory versus safely defaulted? Are bytes copied or viewed? Are keys normalized? What are atomicity and consistency guarantees? How are language sync methods scheduled? Can callbacks re-enter Liquers?

**Issues and patterns.** Data and metadata must remain consistent; do not treat `set_metadata` as optional. Preserve `KeyNotFound`, read, and write error distinctions. Run blocking host I/O outside async workers. In a browser, IndexedDB-backed methods are naturally Promise-based and non-`Send`.

**Meaningful tests:** `STORE01` set/get data and metadata; `STORE02` missing key error; `STORE03` directory listing invariants; `STORE04` remove/removedir behavior; `STORE05` unsupported key; `STORE06` concurrent update policy; `STORE07` store works in end-to-end evaluation.

### RECIPE — Language-defined recipe provider

**Contract.** Adapt a provider from the *integrated language* to `AsyncRecipeProvider`: recipe lookup, optional lookup, containment, plan creation, folder recipe discovery, and asset information.

**Objects/API to map or implement:**

- `AsyncRecipeProvider<E>` *service adapter*
- Language-visible recipe-provider protocol/base class/interface
- `Recipe`, `RecipeList`, `Plan`, `Key`, `ResourceName`, and `AssetInfo`
- Provider composition/precedence/configuration object where multiple providers are supported

**The design must answer:** Does the provider return `Recipe`, query text, or plain data? Which methods may use Liquers defaults? May provider callbacks evaluate queries? How are caching, invalidation, volatility, and provider precedence handled?

**Issues and patterns.** Implement `recipe_opt` without translating “not found” into an execution error. Keep `contains`, `recipe`, and listing mutually consistent. Provider callbacks receive an environment; apply the `RUNTIME` reentrancy rules. Prefer immutable recipe snapshots across the boundary.

**Meaningful tests:** `RECIPE01` found and missing recipe; `RECIPE02` list/contains consistency; `RECIPE03` recipe produces a valid plan; `RECIPE04` provider error maps through `ERROR`; `RECIPE05` volatility/expiration metadata survives; `RECIPE06` end-to-end keyed evaluation; `RECIPE07` nested environment use follows policy.

### UIUSE — Use an existing Liquers UI

**Contract.** Allow code in the *integrated language* to start, configure, populate, and interact with an existing Liquers *UI backend*, such as egui. Define how UI-capable values, commands, asset state, progress, logs, and cancellation are exposed.

**Objects/API to map or implement:**

- `UIElement`, `UiCommand`, and legacy `ExtValue::Widget`/`WidgetValue` as opaque or usable *wrappers*
- `UIHandle`, `UIContext`, `AppState`, `AppStateRef`, `UIPayload`, and `SimpleUIPayload` where public interaction is supported
- `UpdateMessage`, `UpdateResponse`, `ElementSource`, and view-mode enums
- Existing backend runner/configuration and UI lifecycle handle
- Event subscription and disposal handles

**The design must answer:** Which existing *UI backends* are available and who owns their primary thread/event loop? Can the UI run in the foreground or background? Which UI objects are exposed as *wrappers*? How does *language* code open views, submit queries, receive events, and update existing elements? Who owns subscriptions and callbacks? How are stale assets, backpressure, thread affinity, and disposal handled?

**Issues and patterns.** Keep UI transport separate from `VALUE` primitive conversion. An existing Rust UI usually must retain control of its event loop; schedule *language* callbacks onto the *language runtime* instead of invoking them under UI locks. Use stable IDs and explicit unsubscribe/dispose. Batch or throttle progress notifications. Browser DOM objects must stay on the browser thread; opaque UI handles must not be persisted as ordinary values.

**Meaningful tests:** `UIUSE01` start or attach to an existing *UI backend*; `UIUSE02` render or inspect a representative UI value; `UIUSE03` event reaches the correct command/context; `UIUSE04` progress/status ordering; `UIUSE05` unsubscribe releases callbacks; `UIUSE06` stale update is ignored; `UIUSE07` UI error maps through `ERROR`; `UIUSE08` shutdown obeys thread-affinity rules.

### UIDEF — Define UI elements or a UI backend in the language

**Contract.** Allow the *integrated language* to implement new `UIElement` behavior for an existing *UI backend*, or to implement a new *UI backend* that consumes Liquers UI/state/events.

**Objects/API to map or implement:**

- Language-defined adapter for the `UIElement` trait
- Element descriptor/component registration API
- `UpdateMessage`/`UpdateResponse` and render/event callback adapters
- `UIHandle`, `AppState`, and backend-neutral element tree/state access
- Backend adapter/renderer protocol for a wholly language-defined *UI backend*
- Serialization/type registry for language-defined element types

**The design must answer:** Is the extension point a Rust trait adapter, declarative schema, render callback, virtual tree, message protocol, or custom component registry? Which `UIElement` methods/properties/events can *language* code implement? Can one language-defined element be used by existing egui/browser renderers? What is required to implement an entire backend? How are layout, state, event routing, async updates, accessibility, serialization, hot reload, and version compatibility represented?

**Issues and patterns.** Prefer a stable declarative UI/event model over calling foreign code for every render primitive. A language-defined element adapter must own/root its callback safely and must not hold a renderer lock while entering the *language runtime*. Separate backend-neutral element semantics from backend-specific native handles. Define fallback rendering for unknown element types.

**Meaningful tests:** `UIDEF01` register and render a language-defined element; `UIDEF02` properties and state roundtrip; `UIDEF03` event invokes the correct callback; `UIDEF04` element works in each claimed existing backend; `UIDEF05` unknown element has a fallback/error; `UIDEF06` dispose releases callbacks; `UIDEF07` minimal language-defined backend renders a standard element; `UIDEF08` async updates respect UI thread affinity.

### POLYGLOT — Multiple-language interoperability

**Contract.** Allow *language commands* and *language values* from more than one *integrated language* to share an environment with explicit ownership and conversion rules.

**Objects/API to map or implement:**

- Embedded-language runtime/configuration handle, such as a Starlark runtime owned by `liquers-lib`
- Embedded-language command registry/namespace adapter
- Neutral value/interchange representation and explicit codec registry
- Origin/runtime tag for commands, values, and errors
- Cross-runtime scheduler, cancellation, and shutdown coordination
- Outer-integration facade exposing embedded-language capabilities

**The design must answer:** Is an *integration* a top-level binding, an embeddable Liquers library feature, or both? For example, can a Starlark command/runtime implemented in `liquers-lib` be registered in an environment that is itself exposed through `liquers-py` or browser Wasm? Is there one command namespace or language-qualified realms? What is the neutral interchange set? Which *opaque language values* may cross language boundaries? Who owns each *language runtime* and schedules cross-runtime calls? How are capabilities, codecs, trust, cancellation, and shutdown propagated through the outer *integration*?

**Issues and patterns.** Treat an embedded language such as Starlark as an environment capability, not as a second top-level API that must be reimplemented by Python or JavaScript. The outer *integration* should expose registration/configuration and neutral results while `liquers-lib` owns the Starlark runtime adapter. Prefer Liquers primitives, bytes plus media/data format, and Rust `ExtValue` as the neutral boundary. Never hand a Python object directly to JavaScript or a borrowed Starlark value to another runtime. Reject unsupported opaque transfer with a typed error or require an explicit codec. Detect call cycles across runtimes and avoid holding one runtime lock while entering another.

**Meaningful tests:** `POLYGLOT01` language A command feeds language B command through primitives; `POLYGLOT02` bytes/metadata preserve type; `POLYGLOT03` opaque transfer is rejected or explicitly encoded; `POLYGLOT04` errors retain origin; `POLYGLOT05` cross-runtime nested call does not deadlock; `POLYGLOT06` name collision policy; `POLYGLOT07` shutdown releases both runtimes; `POLYGLOT08` `liquers-lib` embedded-language command works through an outer Python/browser *integration*; `POLYGLOT09` outer cancellation reaches the embedded runtime.

### WEBSERV — Start and extend liquers-axum

**Contract.** Allow the *integrated language* to configure and start the `liquers-axum` server in the foreground or background and, when selected, attach *language*-implemented HTTP handlers or middleware.

**Objects/API to map or implement:**

- Integration-defined Axum server configuration and owned server handle
- Startup/readiness/shutdown API
- Route, middleware, and language-handler registration objects
- Language-visible HTTP request, response, headers, body, and authentication/session context
- `ApiResponse<T>` and `BinaryResponse` mappings where they form part of the public API

**The design must answer:** Who owns the async runtime and listening socket? How are address, routes, store/environment, graceful shutdown, startup failure, TLS, and background-thread lifetime configured? Can *language* handlers or middleware be registered, and what request/response/authentication types do they receive? Are handlers allowed to evaluate queries or call other *language commands*?

**Issues and patterns.** Prefer `liquers-axum` routes implemented in Rust for the standard API. Keep the server bound to the same `EnvRef` as the *integration*. A foreground start may block the calling thread but must drive the runtime correctly; a background start must return an owned server handle with readiness and shutdown operations. Enter the *language runtime* only for explicitly language-defined handlers, without holding Axum/Rust locks, and apply timeouts/backpressure.

**Meaningful tests:** `WEBSERV01` start on an ephemeral port and reach readiness; `WEBSERV02` standard query route uses the configured environment; `WEBSERV03` startup error maps through `ERROR`; `WEBSERV04` graceful shutdown; `WEBSERV05` background handle owns server lifetime; `WEBSERV06` language handler receives and returns documented types; `WEBSERV07` handler exception becomes a safe HTTP error; `WEBSERV08` concurrent handlers do not block the *language runtime* unexpectedly.

### WEBAPI — Integrate with a language web framework

**Contract.** Provide efficient, framework-neutral *web handlers* for the Liquers web API that can be adapted to frameworks supported by the *integrated language*, such as Flask or FastAPI in Python.

**Objects/API to map or implement:**

- Framework-neutral internal request/response and streaming-body types
- Reusable *web handlers* for query, asset/data, metadata, recipe, command, and status operations
- Framework adapter interface
- Concrete adapters selected by the integration, such as ASGI/FastAPI, WSGI/Flask, or Fetch
- Authentication/session/context extractor
- Disconnect/cancellation and streaming adapters

**The design must answer:** What is the smallest framework-neutral request/response contract? Are adapters WSGI, ASGI, Fetch-style, callback-based, or framework-specific? Which routes can stream data, metadata, logs, and progress? How are path/query/body parsing, status codes, media types, authentication context, cancellation on disconnect, and large bodies represented? Can handlers reuse a shared `EnvRef` without copying payloads?

**Issues and patterns.** Put Liquers HTTP semantics in reusable Rust handler/service functions rather than duplicating route logic in every framework. Separate protocol logic from Axum extractors so Axum, ASGI, WSGI, Node/Deno, and other adapters can translate their native request into a small internal request model. Prefer zero-copy or bounded-copy byte bodies where the binding permits. Async frameworks should await Liquers directly; synchronous frameworks need a controlled runtime bridge or worker, never one new runtime per request.

**Meaningful tests:** `WEBAPI01` framework-neutral query handler success; `WEBAPI02` Liquers error maps to the specified HTTP response; `WEBAPI03` media type and metadata headers/body survive; `WEBAPI04` FastAPI/ASGI-style async adapter; `WEBAPI05` Flask/WSGI-style sync adapter does not create a runtime per request; `WEBAPI06` disconnect cancellation propagates; `WEBAPI07` large/streaming response follows memory limits; `WEBAPI08` authentication/session context reaches evaluation; `WEBAPI09` route behavior matches `liquers-axum`.

## 6. Language-Specific Guidance

### Browser JavaScript

Expose async APIs as Promises and bytes as `Uint8Array`; check `BigInt`/`i64` conversions. Keep `JsValue`, DOM handles, closures, and `JsFuture` on the wasm thread. Use the wasm-selected inline asset manager and the core `MaybeSend` model. Test in an actual browser with `wasm-bindgen-test`, including rejected Promises and disposal of JS closures.

### Starlark

Treat sandboxing and determinism as product requirements. Prefer copied Liquers primitives for ordinary *language values*. `starlark::values::Value<'v>` belongs to a `Heap`; values retained after evaluation must be frozen/owned or converted. Host functions registered through Starlark globals are a natural `COMMAND` adapter. Document whether evaluation budgets, cancellation, filesystem/network access, and async features are intentionally unavailable.

### Python

Modernize `liquers-py` rather than treating its current behavior as normative. Prefer owned `Py<T>` handles across async/thread boundaries, short explicit interpreter-lock scopes, permissive `VALUE` conversion with an *opaque language value* fallback, and explicit typed exceptions. Pickle may be an opt-in codec, never an implicit trusted interchange format. Organize pytest suites using the IDs in this guide.

## 7. Design and Review Checklist

Every language-specific design should contain:

1. A feature matrix with level, status, limitations, milestone, and test links.
2. A disposition for every item in each selected feature's “Objects/API to map or implement” list: mapped, implemented, internal-only, deferred, or `NA`, with the exposed language name.
3. The exact Rust value and environment types used.
4. Ownership diagrams or precise prose for every stored foreign handle.
5. Sync, async, thread, reentrancy, cancellation, and shutdown policies.
6. Conversion tables, including loss and serialization behavior.
7. Complete command metadata and argument-binding rules.
8. A test inventory whose logical IDs use the feature prefixes.
9. Explicit `NA` decisions; absence is not an `NA` decision.
10. A small end-to-end test that registers a language command and evaluates it through the real environment.

## 8. References

- Liquers core integration boundaries: `liquers-core/src/value.rs`, `error.rs`, `context.rs`, `commands.rs`, `store.rs`, `recipes.rs`, `assets.rs`, and `maybe_send.rs`
- [Command Registration Guide](COMMAND_REGISTRATION_GUIDE.md)
- [Async/Wasm Refactor Design](async-wasm-refactor/DESIGN.md)
- [Liquers Web API Specification](WEB_API_SPECIFICATION.md)
- [Liquers UI Payload Design](UI_PAYLOAD_DESIGN.md)
- [`wasm-bindgen`: JavaScript Promises and Rust Futures](https://rustwasm.github.io/docs/wasm-bindgen/reference/js-promises-and-rust-futures.html)
- [`starlark-rust` value model](https://docs.rs/starlark/latest/starlark/values/index.html)
- [`starlark-rust` evaluator overview](https://docs.rs/starlark/latest/starlark/)
