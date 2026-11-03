---
title: JavaScript Integration
kind: reference
audience: both
area: [web, core/value, core/error, core/store, lib/value]
reviewed: 2026-11-01
---
# JavaScript Integration

How `liquers-web` connects browser JavaScript to Liquers: what the WebAssembly module exports, how
the environment is built and rebuilt, how values, commands and errors cross the boundary, how stores
and record streams are exposed, and how evaluation runs on the page's event loop. This is the
contract the code implements at HEAD. How to *use* the module is
[`guides/JAVASCRIPT_GUIDE.md`](../guides/JAVASCRIPT_GUIDE.md); the general rules for any language
are [`guides/LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md), whose §6
Browser JavaScript records the choices made here and why. The conformance matrix, by feature ID, is
in `liquers-web/README.md`; the designs are [`design/liquers-web/`](../design/liquers-web/) and
[`design/liquers-web-store/`](../design/liquers-web-store/).

## 1. Layout

| Part | Where | Role |
|---|---|---|
| Rust crate `liquers-web` | `liquers-web/src/` | Built as a `cdylib` for `wasm32-unknown-unknown`; also an `rlib`, so a downstream crate can reuse the generic bridge (§5, *Extending*). |
| Generated module | `wasm-bindgen` output: `liquers_web.js`, `liquers_web_bg.wasm`, `liquers_web.d.ts` | What a page imports. `--target web` gives an ES module whose default export loads the wasm. |
| Quick start | `liquers-web/examples-web/quickstart/` | A plain page, no bundler; `build.sh` produces its `dist/`. |

The crate body is `#![cfg(target_arch = "wasm32")]`: on native it compiles to an empty crate, and
the workspace's `default-members` leave it out. `JsValue` is `!Send`, and on native the core
`MaybeSend` markers resolve to `Send`, so the bridge types cannot exist there.

| Module (`src/`) | Contents |
|---|---|
| `lib.rs` | The module-level functions (§3) and the `Value` wrapper class. |
| `environment.rs` | The singleton, its rebuild, `Environment` instances, `version()`. |
| `command/` | `spec.rs` (declaration intake, argument inference), `import.rs` (batches), `adapter.rs` (the registered closure). |
| `bridge.rs`, `default_value.rs`, `value.rs` | Value conversion, generic over the value type; the `ExtValue` hooks; `JsOpaque`. |
| `error.rs` | `LiquersError`, error-type names, the validation report shape. |
| `asset.rs`, `eval.rs`, `objects.rs` | `Asset`, `State`, the `Promise` bridge, `Query`, `Key`. |
| `store/` | The `localstorage`, `http` and `js` stores, the configuration builder, the `Store` wrapper. |
| `records.rs` | `RecordBatch`, `RecordSource`, `RecordStream` (feature `records`). |
| `typescript.rs` | The interfaces injected into the generated `.d.ts`. |

**Cargo features.** `liquers-lib` is linked with `default-features = false, features = ["webui"]`:
no polars, no egui, no images, no templates. `records` (off by default) adds the record handles and
the `ns-rec` commands, taking a release build from 12.4 MB to 16.5 MB. `polars` and `egui` only forward to `liquers-lib`
so that `#[cfg]`s agree with the linked `ExtValue`; neither is a supported browser build.
`debug-handles` (test-only) adds a Rust-side count of retained JavaScript function handles for
`RUNTIME05`, and
`browser-tests` (test-only) compiles the suites that need a real `localStorage`.

**Declarations.** `wasm-bindgen` generates `liquers_web.d.ts` from the exports and their doc
comments. What it cannot see inside a `JsValue` — a command declaration, a store configuration, a
store object, a report — is declared by `typescript_custom_section`s in `typescript.rs` and referenced
through `typescript_type` extern types, so there is one generated file and nothing hand-maintained
(`STUBS`). Record interfaces are a separate section, present only with `records`.

## 2. Exposed types

| JavaScript | Wraps | Notes |
|---|---|---|
| `Environment` | `LiquersEnvironment`: the singleton (`Environment.global()`) or an isolated `EnvRef` (`new Environment()`) | §3 |
| `Value` | `liquers_lib::value::Value`, plus the environment that produced it | `toJS()`, `typeName`, `identifier`, `isOpaque` |
| `Asset` | `AssetRef<WebEnvironment>` | every method returns a `Promise` (§7) |
| `State` | `State<Value>`, plus its environment | `value()`, `toJS()`, `metadata()`, `log()`, `status` |
| `Query`, `Key` | `liquers_core::query::{Query, Key}` | `parse`, `encode`, `equals`, `filename`; `Key` adds `parent`, `toAbsolute` |
| `LiquersError` | `liquers_core::error::Error`, plus JavaScript class, stack and report | §6 |
| `Context` | a command's `Context<E>`, type-erased, valid only during its call | §4 "Context" |
| `Store` | `Arc<dyn AsyncStore>` | §8 |
| `RecordBatch`, `RecordSource`, `RecordStream` | record views and sources | §9, `records` only |
| `UIMount`, `UISubscription` | a mounted `liquers-lib` web UI and a status subscription | §10 |
| `UIElement` | `Arc<dyn liquers_lib::ui::UIElement>` | §10; `typeName`, `title`, `renderHtml()`; `kind`, `props` for a page-defined element |

`Query.equals` and `Key.equals` compare canonical encodings, since every wrapper is its own handle
and `===` would compare handles; a key's `encode()` is therefore usable as a `Map` key. `Query.parse`
and `Key.parse` throw a `LiquersError` with the parse position.

Plain-object results are not classes: metadata, asset info, logs, command info and validation reports
are plain objects whose shapes are declared in the `.d.ts` (where declared) or are the `serde` form of
the core type (§3, §7).

## 3. The environment

**Type.** `WebEnvironment = liquers_lib::environment::DefaultEnvironment<Value, ()>`, used directly:
it already selects the inline asset manager (`ImmediateAssetManager`) on `wasm32`. Every environment
is built by `new_environment()`: a type registry of `Value`'s types plus `js.Value` (§5), the built-in
commands, and `liquers-lib`'s default recipe provider chain. There is no new `Environment` or `CommandExecutor`
implementation.

**Built-in commands.** `register_builtin_commands` registers `liquers_lib::register_core_commands!` —
`to_text`, `to_metadata`, `commands_doc`, the `ns-dep` introspection commands — and, with `records`,
`register_records_commands!` (the `ns-rec` namespace). Nothing else: there are no polars, image or
`ns-template` commands (`WEB-HAS-NO-TEMPLATE-COMMANDS`).

**The singleton.** One environment per module instance, held in `thread_local`s:

- `init()` creates it, un-shared, and resolves a `Promise` (`ENVIRON01`). It is idempotent: a second
  call keeps the existing environment and its commands (`ENVIRON03`). A failed `init` leaves the
  singleton unset so a retry can succeed (`ENVIRON04`). It also installs the panic hook, which turns
  a Rust panic into a `console.error` with its message.
- The first evaluation, or `Environment.global()`, *shares* it (`Environment::to_ref`), after which
  its command registry cannot be changed in place.
- `shutdown()` clears the singleton and every retained declaration; it is idempotent (`ENVIRON06`).
  Afterwards evaluation rejects with `not_available` until `init()` runs again.

**Declarations and rebuild.** The singleton retains every accepted command (the declaration object,
or the imported metadata with its implementation), the store configuration and the named store
objects. While the environment is un-shared, a registration goes straight into its registry. Once it
is shared, `registerCommand`, `registerCommands`, `unregisterCommand`, `configureStore` and
`registerStoreObject` build a fresh environment, replay every retained declaration and the store
configuration into it, and swap the singleton — once per call, so a batch costs one rebuild. The
asset cache of the replaced environment is discarded; an evaluation already in flight keeps its own
`EnvRef` and completes against the old environment (`POST-INIT-COMMAND-REGISTRATION`).

**Explicit instances.** `new Environment()` builds an isolated environment and shares it at once. It
has the built-in commands and no store, shares nothing with the singleton (`ENVIRON05`), and is never
rebuilt. Of its methods:

| Method | On a `global()` handle | On a `new Environment()` |
|---|---|---|
| `evaluate(query)`, `evaluateQuery(q)`, `getAsset(query)` | the singleton, re-resolved on every call so a rebuild is visible | this instance |
| `describeCommand(name)`, `commandNames()` | the singleton | this instance |
| `store()` | the singleton's store | this instance's store, which is none |
| `configureStore(config)`, `registerStoreObject(name, obj)` | the singleton | **also the singleton** (`WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`) |
| `registerCommand(spec)` | refused, `not_supported` | refused, `not_supported` |
| `liquersVersion` (getter) | `liquers_core::VERSION`, the workspace version (`Environment::liquers_version`) | the same |

Commands are registered through the module-level functions, which act on the singleton. An instance
is for evaluating against the built-in set, chiefly in tests.

**Module-level functions.**

| Function | Does |
|---|---|
| `init()` | creates the singleton; `Promise<void>` |
| `isInitialized()` | whether `init()` has completed |
| `shutdown()` | releases the singleton |
| `evaluate(query)` | `Promise` of the value, converted (§5) |
| `getAsset(query)` | `Promise<Asset>` |
| `registerCommand(spec)` | a batch of one (§4); throws on refusal |
| `registerCommands(input, impls?)` | registers a batch, all or nothing; returns the report |
| `validateCommands(input, impls?)` | the report `registerCommands` would act on; never throws for a bad batch, works before `init()`, never rebuilds |
| `unregisterCommand(name)` | removes JavaScript commands named `name`; returns whether any was removed |
| `describeCommand(name)` | the metadata of the root-namespace command `name`, or `null` |
| `opaque(x)` | a `Value` retaining `x` by identity (§5) |
| `encodeParam(text)` | `text` escaped as one query parameter; throws for a character the grammar cannot represent |
| `version()` | `"liquers-web <v> (liquers-core <v>)"`, from Cargo; every crate shares the workspace version, so both are the same |

There is no synchronous evaluation entry point, deliberately (`ASYNCQ06`, `ASYNCQ07`).

**Store.** A new environment has no store: `-R/` queries reject with `asset_not_found` and writes
with `key_not_supported` until `configureStore` applies a configuration (§8). The recipe provider is
`LibKind`'s default chain, read through that store: `recipes.yaml` files, and with the `records`
feature also record-stream manifests (`[recipes_yaml, manifest]`,
`reference/ENVIRONMENT_CONFIG.md` §The recipe provider chain). A chunk name both define is an
error naming both providers.

## 4. Commands

A JavaScript command is an ordinary async command whose registered closure owns a
`js_sys::Function`; there is no generic `jscall` alias.

**Declaration.** `registerCommand(spec)` takes an object; `registerCommands(input, impls?)` takes a
declaration, an array of them, or a JSON or YAML string in the `CommandMetadataRegistry` format
(`commands: [...]`, the shape of `specs/command_registry.yaml`), with `impls` mapping a command name
to a function or to `{ run, state?, async? }`. Both run the pipeline of
[`reference/COMMAND_DECLARATION.md`](COMMAND_DECLARATION.md); stage 1 is JavaScript-specific
(`command/spec.rs`):

- `run` is removed and kept by the integration; the rest of the object, with `null` and `undefined`
  properties dropped, is the declaration document. Any `CommandMetadata` key is accepted, though the
  `.d.ts` declares only the common ones (`name`, `run`, `arguments`, `state`, `namespace`, `realm`,
  `label`, `doc`, `async`, `volatile`, `conventions`).
- `state` and `async` are read by the integration and are not metadata. `state` is `"none"` (the
  default: a source command), `"value"`, `"text"` (or `"string"`) or `"state"`; the core
  state-delivery convention is switched off, since the mode is declared. The `context` convention
  stays on unless the page writes `conventions: { context: false }` (or `conventions: false`).
  `async` is `true`, `false`, or omitted (decide per call).
- Labels keep `liquers-web`'s historical rule: a command label defaults to the name verbatim, an
  argument label to the name with `_` replaced by a space. `state_argument` is always present.
- `module` is set to `"javascript"`.
- **Argument inference.** With no `arguments` key, names are read from `run`'s source: every
  parameter must be a plain identifier and their count must equal `Function.length`. A default, rest
  or destructured parameter, or a bound or native function, is refused with an error naming it.
  The first parameter is skipped when `state` is not `"none"`, and a parameter named `context` is
  the context (below), not an argument. Inferred arguments have type `any`
  and no default; `describeCommand` reports `argumentsInferred: true`. Minification yields correct
  arity and wrong names, which affects labels, not behaviour.

Refused before anything is registered: a non-object, a missing or empty `name`, a missing `run`, an
unknown `state`, the reserved namespace `web` (kept for platform commands of `liquers-web` itself),
an argument list that cannot be inferred, and whatever `CommandDeclaration::check` and
`CommandMetadataRegistry::check()` of the batch report as errors. Every finding of a batch is
collected; the throw is a `parameter_error` whose message is the report's summary and whose `report`
property is the report (`ENVIRON08`). The names `ns`, `q` and `v` are planner instructions: `ns` is
refused by core, and a command named `q` or `v` registers but never runs
(`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`).

**Report shape.** `{ issues, errorCount, warningCount, hasErrors, summary }`, each issue
`{ severity, message, command }`, `severity` one of `debug`, `info`, `warning`, `error`, `command`
`{ realm, namespace, name }` or `null`, `summary` the refusal message or `null`.

**Replacement and removal.** Registering a key that exists replaces it and always writes
`console.warn` — `command "x" was replaced`, or `… replaces a built-in Rust command`.
`unregisterCommand(name)` drops every retained JavaScript declaration called `name`, **in any
namespace**, and forgets its inference record. Before the first evaluation it also removes the
root-namespace key from the live registry, so it removes a built-in command too; after it, it
rebuilds without the dropped declarations, so a replaced built-in comes back and a built-in name on
its own returns `false` (`WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE`,
`WEB-UNREGISTER-DEPENDS-ON-FIRST-EVALUATION`). Removal releases the function handle
(`RUNTIME05`).

**`describeCommand`.** Returns the `CommandMetadata` held for the root-namespace key, serialized as
`serde` writes it, with `arguments` always an array and `argumentsInferred` added: so an argument has
`argument_type`, a default reads `{"Value": 2}`, and empty `namespace`, `realm` and `doc` are
absent. The declared `LiquersCommandInfo` interface does not match this
(`WEB-DESCRIBE-COMMAND-SHAPE-DIFFERS-FROM-DECLARATION`).

**Call** (`command/adapter.rs`). Arguments are converted before JavaScript is entered, and no Rust
borrow is held across the call:

1. the state, unless the mode is `none`: the converted value (`value`), the state's text (`text`,
   via `State::try_into_string`), or a plain object `{ value, metadata, status, log }` (`state`);
2. each argument's resolved value, converted (§5), in declaration order;
3. the `Context`, inserted where the declaration placed it (below);
4. `run.apply(null, args)`.

A throw becomes an error (§6). A thenable result
is awaited unless `async: false`, which refuses it with `execution_error`; with `async: true` a plain
result is accepted. The result is converted structurally (§5); a `Value` from `opaque()` is kept by
identity. A JavaScript command's implementation version is not derived from its source.

**Context** (`command/context.rs`, `COMMAND07`). An argument named `context`, in any position, or
an inferred parameter of that name, is the execution context: core's `context` convention removes it
from `arguments` and records its index in `registration.context`, and `run` receives a `Context` at
that index (counted after the state, when the command takes one). It is not a query argument and
`describeCommand` does not list it. The class follows `liquers-py`'s `lq.Context`:

| Member | Does |
|---|---|
| `info(m)`, `warning(m)`, `error(m)` | add an entry to the asset's log; after `error` the evaluation fails once `run` returns, with that message |
| `evaluate(query)` | `Promise` of the query's value, parsed with `parse_query`, evaluated as a dependency of this asset (`Context::evaluate` and `wait_for_dependency`), converted in this environment; a failure rejects with a `LiquersError` |
| `isCancelled()` | core's cancellation flag; always `false` today (`WEB-CANCELLATION-INERT`) |
| `element` | the handle of the UI element whose action ran the command, as a `number`, or `null` outside a UI |

`element` comes from the environment's payload through the `JsPayload` trait: `()` answers `null`,
and `liquers_lib::ui::SimpleUIPayload` answers its handle. `call_js_command` therefore requires
`E::Payload: JsPayload`. A context lives until `run` (or its promise) settles: a drop guard empties
its slot when the call ends, on every path, and every member then throws a `LiquersError` saying the
context is valid only during the call. A command bound to imported metadata (`registerCommands`
with a registry document) receives no context, because imported metadata has no `registration`
record (`WEB-IMPORTED-COMMAND-RECEIVES-NO-CONTEXT`).

## 5. Values

The value type is `liquers_lib::value::Value`. A JavaScript object retained by identity is a
`JsOpaque` inside `ExtValue::Foreign`, type identifier `js.Value`, `type_name` its constructor name.
All conversion is in `bridge.rs`, generic over `V: JsValueBridge`.

**JavaScript to Liquers** (`js_to_value`, policy `Structural`), tested in this order:

| JavaScript | Value |
|---|---|
| a record handle (`records`) | the `RecordView` or `RecordSource` it holds; a freed handle is `conversion_error` |
| a `Value` wrapper (marked `__liquersValueWrapper`) | its value; an opaque wrapper re-wraps the original object |
| `null`, `undefined` | `None` (the two are not distinguished) |
| `boolean` | `Bool` |
| `number`, integral and within ±2^53 | `I64` |
| other `number`, NaN and ±Infinity included | `F64` |
| `string` | `Text` |
| `BigInt` | `I64`; outside the `i64` range `conversion_error` |
| `Uint8Array`, `ArrayBuffer` | `Bytes` |
| `Array`, plain object (prototype `Object` or `null`) | through JSON (`try_from_json_value`), with the nested rules below |
| anything else — class instance, `Map`, `Set`, `Date`, function, DOM node | `conversion_error` naming the path and constructor, suggesting `liquers.opaque(...)` |

Inside an array or object, numbers follow the same integer rule; NaN and ±Infinity become the
strings `"NaN"`, `"inf"`, `"-inf"`; `undefined` becomes `null`; own enumerable string keys are read
with `Object.entries`; and a `Uint8Array` or any other non-plain object is `conversion_error`
naming its path. Nested bytes are refused rather than degraded to an array of numbers, because JSON
has no byte type and the degraded form would no longer be `Bytes` (`VALUE04`). Depth is limited to 64, so a cyclic structure is
`conversion_error` rather than a stack overflow.

**Liquers to JavaScript** (`value_to_js`):

| Value | JavaScript |
|---|---|
| `JsOpaque` | the original object, by identity |
| a foreign value of another runtime | `conversion_error` naming its origin |
| record view (`records`) | a `RecordBatch`, or the cell's scalar for a single-cell view |
| record source (`records`) | a `RecordSource` |
| `None` | `null` |
| `Bytes` (by identifier) | `Uint8Array` |
| anything else | its JSON form (`try_into_json_value`), objects as plain objects, never `Map` |

An integer outside ±(2^53 − 1), at any depth, is `conversion_error` rather than a rounded `number`
or a `BigInt` (`VALUE03`). A `null` nested in an array or object arrives as `null`, as a top-level
`None` does (`json_to_js` sets `serialize_missing_as_null`, which `serde-wasm-bindgen` leaves off by
default). A value with no JSON form is `conversion_error`. Conversions run *inside*
the environment that produced the value (`value_to_js_in`), so a handle created by the conversion
resolves through that environment later, never through the singleton.

**Opaque values.** `opaque(x)` is the only way a value is carried by identity. The `js.Value`
`TypeInfo` declares no data format, so an opaque value cannot be written to a store; it lives in the
asset that holds it and pins whatever it references for that asset's lifetime. A Rust command that
needs a string or JSON from it gets `conversion_error`. Identity across evaluations is not promised:
it holds while the environment keeps the asset.

**Extending.** A downstream crate brings its own value type by implementing `JsExtensionBridge` for
its *extension* type; a blanket impl lifts it to `CombinedValue<B, Ext>`. Implementing
`JsValueBridge` for `CombinedValue<SimpleValue, MyExt>` directly is `E0117` from another crate.
`default_value.rs` takes the same route for `ExtValue`; `tests/second_value_type.rs` is the worked
example.

## 6. Errors

There is one class, `LiquersError`, not a hierarchy: the error type travels as a string.

| Member | Meaning |
|---|---|
| `errorType` | the `ErrorType` as a stable snake-case name, such as `key_not_found`; an exhaustive match in `error_type_name` |
| `categories` | `ErrorCategory` names of the type: `store`, `keyed_asset`, `not_found` |
| `message` | the message |
| `query`, `key` | where it happened, when known; `undefined` otherwise |
| `position` | `{ offset, line, column }` of a parse error, or `null` |
| `jsClass`, `jsStack` | class and stack of the JavaScript throw this object was made from, when it is that object |
| `report` | the validation report of a refused batch, else `undefined` |
| `toString()` | `"<errorType>: <message>"` |

`new LiquersError(errorType, message, query?)` builds one from JavaScript; an unknown type name or a
query that does not parse throws `TypeError`. It is a wasm-bindgen class, not a subclass of `Error`,
so it has no JavaScript stack.

**Liquers to JavaScript.** Every rejected `Promise` and every throw carries a `LiquersError`, never a
string (`ASYNCQ02`).

**JavaScript to Liquers** (`js_error_to_liquers`):

- An object with a known `errorType` string — a `LiquersError`, or anything shaped like one — keeps
  that type and its `message`; its `query`, `key` and `position` are carried over when they have
  the types the `LiquersError` getters return, and an ill-typed field is dropped.
- An `Error` becomes `execution_error` (inside a command; `conversion_error` at a conversion
  boundary) with the message `Class: message`, followed by the stack frames. The structured
  `jsClass` and `jsStack` do not survive the asset lifecycle, so the caller sees `null` there
  (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`).
- Any other thrown value becomes `a non-Error value was thrown: <text>`; `throw undefined` becomes
  `undefined was thrown`.

## 7. Event loop, waits and cancellation

wasm is single-threaded, and all of `liquers-web` runs on the page's event loop. Every evaluation
entry point returns a `Promise` (`wasm_bindgen_futures::future_to_promise`); nothing blocks.
`Asset.status()`, `get()`, `cancel()` and `getAssetInfo()` are `Promise`s too, because the core
methods behind them are async.

**Immediate evaluation.** `ImmediateAssetManager` evaluates during `getAsset`, so an `Asset` is
already terminal when the page receives it. `cancel()` therefore resolves and changes nothing
(`WEB-CANCELLATION-INERT`); it is exposed because the shape stays when a deferring manager arrives.
A JavaScript command that awaits yields to the event loop, so other evaluations interleave with it.

**Results are reused.** A non-volatile query evaluated twice is computed once while the environment
keeps the asset. A second caller of a query whose first evaluation is still running waits for it;
when that evaluation fails, the second caller can receive `null` instead of the error
(`WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL`).

**Status and metadata.** `Asset.status()` and `State.status` give the status as a lowercase name
(`ready`, `error`, …, exhaustive in `status_name`). `State.metadata()` and the `state`-mode argument's
`metadata` are the `serde` form of `MetadataRecord`, where `status` is capitalized (`Ready`)
(`WEB-METADATA-STATUS-CASE-DIFFERS`). `State.value()` throws when the state holds an error;
`State.log()` is always an array.

**Borrow rule.** No `RefCell` borrow is held across an `await` or a call into JavaScript; every
accessor clones the `EnvRef` out. A re-entrant call — a JavaScript command calling `evaluate` — is
therefore safe — an `async` command may `await liquers.evaluate(…)` — though only
`context.evaluate(…)` records the inner result as a dependency of the outer one.

## 8. Stores

Configuration uses core's store-router format ([`reference/STORE_CONFIG_FSD.md`](STORE_CONFIG_FSD.md)),
as an object or a YAML/JSON string; both are parsed by `liquers-core`. `configureStore` returns a
`Promise`. Stores are tried in document order, first matching prefix wins; overlapping prefixes are
accepted without a warning.

| `type` | Backend | Writes | Config |
|---|---|---|---|
| `localstorage` | `window.localStorage`, full `AsyncStore` contract, directories included | yes | `namespace` (default `liquers`, no `/`), `quota_bytes` |
| `http`, `https` | `fetch` from the global scope (window, worker, Node) of `url_prefix` + key without its routing prefix | no | `url_prefix`, `keys` (required: HTTP has no listing) |
| `js` | an object the page registered with `registerStoreObject(name, obj)` | as implemented | `object` |

`localStorage` holds strings, so bytes that are not UTF-8 are base64-encoded in an envelope. It does
not exist under Node, so that store is tested only in a browser.

**Page-implemented stores** (`store/js_store.rs`, interface `LiquersStoreObject`). Only `get` is
declared required, but a `-R/` query calls `contains` (on the key, and on the folder's
`recipes.yaml`) and, for a miss, `isDir`, so a store with `get` alone answers every query with
`key_not_supported` (`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`). Each method may return a value
or a `Promise`, except `isSupported`, which must be synchronous. The method set is resolved once, at construction. An omitted method reports
`key_not_supported`, never an empty answer. `get` / `getMetadata` returning `null` or `undefined`
means absent (`key_not_found`); a throw means failure (`key_read_error`, or the thrown
`LiquersError`'s type). `getMetadata` returning `{}` means "exists, no metadata". A directory is
recognised through `isDir`, with children from `listdir`; a throw is never treated as a directory.
Recipes need `listdir` as well: the recipe provider lists the folder.

**Policy and metadata sections.** Any entry may have `read_only: true`, refusing writes with
`not_supported`, and a `metadata` section. A section reaches a `js` object only through its
data-only methods — `getData`, `setData`, `removeData`, `listdirData`, `dataFacts`, all five or none,
plus optional `renameData` and `atomicWrite` — and `getStoredMetadata` for the default `native`
model; a section on an object without them is refused when the store is built. `localstorage` offers
all of data-only access, `http` the read half. `Store.isReadOnly()` reports whether every write is
refused. `LANGUAGE-INTEGRATION_GUIDE.md` §6 Browser JavaScript has the details.

**The `Store` wrapper.** `get` (`Uint8Array`), `getMetadata` (the raw stored record), `getAssetInfo`
(the effective `media_type`, `data_format`, `filename`, `is_dir`), `set(key, bytes, metadata)`,
`setMetadata`, `remove`, `removedir`, `makedir`, `contains`, `isDir`, `listdir`, `storeName()`,
`isReadOnly()`. Partial metadata objects are accepted and completed for the key.

A `store_config` handler cannot be written in JavaScript (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`,
`STORE18` is `NA`).

## 9. Record streams

With `records`, a record view crosses as a `RecordBatch` (`numRows`, `numColumns`, `schemaJson()`,
`column(i)` — a zero-copy descriptor into wasm memory — `columnCopy(i)`, `columnView(i)`,
`chunkIdJson()`, `sourcesJson()`), or as a scalar when it is a single cell. A record source crosses as
a `RecordSource` (`chunks()`, `bounded`, `schemaJson()`, `manifestJson()`, `stream()` — an async
iterable of batches — and `materialize(maxRows)`). Both are handles to the Rust object and cross back
unchanged. `free()` releases one; a freed handle passed back is `conversion_error`. A source resolves
its chunks through the environment that produced it. Only one `nextBatch()` may be pending on a
stream. A view built from a `column` descriptor is invalidated by `memory.grow` and dangles after
`free()`; `RecordColumn` in `src/records_column.js` re-creates it on demand. `RECORDS03`, `09` and
`11` are `NA` (`liquers-web/README.md`).

## 10. User interface

`mountUI(container, options)` (`ui.rs`; also `Environment.mountUI`) mounts the `liquers-lib` web
backend (`liquers_lib::ui::mount_web_with`) into a DOM element and resolves to a `UIMount`. The
environment's payload is `liquers_lib::ui::SimpleUIPayload` (`WebEnvironment =
DefaultEnvironment<Value, SimpleUIPayload>`), and the 14 `lui` commands are built in
(`register_lui_commands!` in `builtins.rs`), so a command run from the UI knows its element and can
place others.

**Options.** `{ query }` adds a root node whose source is the query, which the runner evaluates as
it evaluates any pending node. `{ spec }` places a root `UISpecElement` and calls its `init`, which
submits the spec's `init` queries: YAML text goes through `UISpec::from_yaml`, and an object through
`serde_wasm_bindgen` into `UISpec` (serde's externally tagged form, since JSON cannot write YAML
tags). Both together, a `query` that does not parse (checked first, so the refusal needs no DOM),
or a `spec` that is not a `UISpec` reject the promise. No `window`/`document`, or a `container`
that is not an `Element`, rejects with `not_supported`.

| `UIMount` member | Does |
|---|---|
| `running` | `false` after `dispose()` |
| `submit(query, handle?)` | sends `AppMessage::SubmitQuery`, as a control's query action does; commands place their own results |
| `show(query, handle?)` | `AppState::insert_source` at `Root`, or `Instead(handle)`; returns the element's handle |
| `on("status", cb)` | a `UISubscription`; `cb` receives `{ handle, status, error? }` |
| `elements()` | the tree, depth first: `{ handle, parent, typeName, title, status }` |
| `dispose()` | stops the mount (idempotent) and drops its subscriptions |

Handles cross as `number` (`UIHandle.0` as `f64`) and are read back with a range check. `show` and
`elements` take the `AppState` lock with `try_lock`; the runner holds no guard across an await, so
they fail only when called from inside a render. An unknown handle, or any call but `dispose` on a
disposed mount, throws a `LiquersError` (`general`).

**Status events.** `AppRunner` records a `RunnerEvent { handle, status, error }` when it sets a
progress element (`processing`; core has no "evaluating" status), when it sets an element from a
result (`ready`), at every error element (`error`, with the error), for a failing root query
(`handle: null`), and for each delivered asset snapshot (the snapshot's status). The driver drains
them after each tick, once the DOM is updated and no lock is held, and passes them to `on_tick`;
the mount updates the statuses `elements()` reports and calls the subscribers. A throwing callback
is reported to `console.error`, and later callbacks still run; the subscriber list is cloned
first, so a callback may dispose its subscription or the mount. Undrained events (a native
backend that never reads them) are capped at 1024, oldest dropped.

**Stale results.** The runner remembers the query text each in-flight evaluation started, and
before it lands a result checks that the node's source is still that query; a node whose source
changed (a `show(B, h)` while A was awaiting) is evaluated again and A's result is dropped.

**Rebuilds.** `MountOptions::current_env` is called before each tick; an environment that differs
by `Arc` identity replaces the runner's (`AppRunner::replace_envref`), so a mount on the singleton
can use a command registered after it was mounted. Results cached by the old environment are lost,
as for any rebuild (`POST-INIT-COMMAND-REGISTRATION`).

**Disposal.** `MountHandle::stop` (and drop) removes the `click` listener from the container and
the `keydown` listener from the document, empties the container and sets a flag the driver loop
checks after `run` and after `on_tick`, so a stop made while a query was awaiting never paints. The
loop then returns and drops the runner, its `EnvRef` and its `AppState` clone.

**`UIElement`.** A UI element value converts to a `UIElement` handle (`default_value.rs`) and back
as the same `Arc`, through the `records.rs` registry idiom (`__liquersUIElementHandle`, an
id-to-`Arc` map emptied by `free()`). A freed handle is refused. `renderHtml()` renders the element
on its own, against an empty `DirectAppState`.

Limits: the page does not repaint while a query awaits, because the inline asset manager evaluates
inside the driver's `run` (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`); cancellation is inert; with two
mounts on one page an unfocused menu shortcut runs in both (`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`).

### Elements and renderers

**One Rust type for every page-defined element.** `liquers_lib::ui::DelegatedElement` (typetag
`"DelegatedElement"`, all targets) carries a `kind`, JSON `props` and a `title`, and looks up its
behaviour by `kind` in a per-thread registry of `ElementDelegate`s (`register_delegate`,
`unregister_delegate`, `delegate_for`). Only the data is serialized, so a restored tree renders an
escaped placeholder (`lq-undefined`) for a kind that is not defined, and its markup once it is.
`render_web` wraps the delegate's markup in `<div id="ui-element-N" class="lq-element
lq-delegated" data-lq-delegate="{kind}">`; a failing delegate renders an escaped error box
(`lq-error`). `update(UpdateMessage::UI(e))` calls the delegate's `on_event` and, when it returns
new props, replaces them and answers `NeedsRepaint`; every other message is `Unchanged`. In egui
the element shows a label: egui is not a claimed backend (`NA`). `UIElement::as_any` (default
`None`) lets an integration downcast to it.

**The page's API** (`ui_define.rs`):

| Function | Does |
|---|---|
| `defineElement({ type, render, onEvent?, mount?, title? })` | registers a `JsElementDelegate`; a second define of a type replaces it, and every live mount re-renders (`invalidate_all`; a mount whose lock is held, from inside a render, after its tick) |
| `undefineElement(type)` | removes it and releases its functions; `true` if it was defined |
| `element(type, props?, title?)` | a `DelegatedElement` as a `UIElement` handle; the type need not be defined yet |
| `mountRenderer(renderer, options?)` | a mount with no container; resolves to a `UIMount` |
| `UIMount.dispatchEvent(handle, kind, fields?)` | sends `AppMessage::UIEvent`, as a `data-lq-event` click does |

`props` must be JSON data: functions, symbols, bigints, non-finite numbers, class instances (a
`Date`, a `Map`, a handle) and cycles are refused with `conversion_error`, naming what was found
and where; `undefined` in an object is left out, as `JSON.stringify` does. Props are a
`serde_json::Value`, so object keys come back sorted. They reach the page's functions through the
crate's JSON serializer (objects, not `Map`s).

**Call contexts.** `render(props)` runs while the driver paints, under the `AppState` lock; it
must return a string, and a throw or another type becomes the element's error box and a
`console.error`. `show` from inside it throws the busy-lock error; `submit` works, because it is a
message. `onEvent(props, event)` runs inside `AppRunner::run` through `ui-events`'
`deliver_update`, with the element taken out and the lock released, so it may call `show` and
`submit`; `show(q, h)` for its own element is refused (`general`: the element is being updated,
and the runner would put it back over the replacement). It returns the new props, or `undefined`
to keep them; a throw keeps them and goes to `console.error`. The event is `{ handle, kind,
fields }`: `kind` is `UIEventKind::name()`, `fields` the event's values through `value_to_js`.

**`mount`.** After each tick of a container mount (in `on_tick`, no lock held) the mount
reconciles the container's `[data-lq-delegate]` nodes against a map of handle → (node, cleanup):
an entry whose node is no longer connected has its cleanup run and is dropped; a node with no entry
(or a different node for the handle) is passed to the type's `mount(node, props)`, and a function
it returns is kept as the cleanup. Disposing the mount runs the remaining cleanups; a `mount` that disposes the mount itself has its returned cleanup run at once. A throwing
`mount` is reported, and the element keeps its markup. The pass is skipped while no definition has
a `mount` and nothing is mounted.

**Renderers.** `mountRenderer` runs `liquers_lib::ui::run_headless`: the same driver loop as
`mount_web_with` (both use one `spawn_driver_loop`), with no DOM. A `MountHandle` holds its DOM
parts as an `Option`, so a headless handle's `stop` only ends the loop. After each tick that
changed something the renderer's `render(tree, changes)` is called with this as the renderer
object: `changes` is `"all"` for `Invalidation::All` (the first tick among them), otherwise the
`UIChange`s as `{ inserted, parent, index }`, `{ removed, parent }` or `{ replaced }`. `tree` is
always the whole tree, `{ roots, elements }`, each element `{ handle, parent, children, typeName,
title, status, html, kind?, props? }`, where `html` is `render_element_web`. The tick's status events are recorded before the tree is
built, and an element whose status changed with no tree change is passed as `{ replaced }`. A
throwing renderer is
reported and called again on the next change. `dispose()` calls the renderer's `dispose`.

Every retained page function (definitions, cleanups, renderers) is counted in
`live_handle_count` under `debug-handles`.

## 11. Limitations

- Templates and data nodes: no `ns-template` commands, no template commands, no programmatic
  `render`, no JavaScript `DataNode` adapter (`WEB-HAS-NO-TEMPLATE-COMMANDS`; `TEMPLATE`, `DATANODE`
  not started).
- A command bound to imported metadata receives no context
  (`WEB-IMPORTED-COMMAND-RECEIVES-NO-CONTEXT`).
- Cancellation is inert (`WEB-CANCELLATION-INERT`).
- Commands cannot be registered on a `new Environment()`, and its store methods configure the
  singleton (`POST-INIT-COMMAND-REGISTRATION`, `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`).
- Registering after the first evaluation rebuilds the environment and drops its cached results
  (`POST-INIT-COMMAND-REGISTRATION`).
- `describeCommand` sees the root namespace only and returns a shape the `.d.ts` does not declare;
  `unregisterCommand` matches by name across namespaces and treats built-ins differently before and
  after the first evaluation (`WEB-DESCRIBE-COMMAND-SHAPE-DIFFERS-FROM-DECLARATION`,
  `WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE`, `WEB-UNREGISTER-DEPENDS-ON-FIRST-EVALUATION`).
- A thrown exception's class and stack arrive inside `message` (`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`).
- A page-implemented store needs `contains` and `isDir` as well as `get` to be read by queries
  (`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`).
- No IndexedDB store (`WEB-NATIVE-IO-TIER2`), no read/write store of a Liquers server
  (`WEB-HAS-NO-READ-WRITE-REMOTE-STORE`), and the store configuration is not yet an
  `EnvironmentConfig` (`WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG`).
- No generated API reference beyond the `.d.ts`, and no npm package (`WEB-API-REFERENCE-NOT-GENERATED`).
- `RECIPE` (a JavaScript recipe provider), `MODULE`, `POLYGLOT`, `WEBSERV` and `WEBAPI`
  are not implemented; the default recipe provider works through a configured store.
- The UI does not repaint while a query awaits (`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`), and an
  unfocused menu shortcut runs in every mount (`WEBUI-SHORTCUTS-RUN-IN-EVERY-MOUNT`).
- A page-defined element renders only in the web backend; egui shows a label.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | §3: the `liquersVersion` getter on `Environment`; `version()` reports the one workspace version twice. | `design/liquers-py-package/` (phase-5) |
| 2026-10-31 | §10 "Elements and renderers": `defineElement`, `undefineElement`, `element`, `mountRenderer`, `dispatchEvent`; `DelegatedElement`; `UIElement.kind`/`props`. `UIDEF` removed from Limitations. | design `web-ui-define` |
| 2026-10-31 | §10 "User interface": `mountUI`, `UIMount`, `UISubscription`, `UIElement`; the payload is `SimpleUIPayload` and `lui` is built in; §10 Limitations becomes §11. | design `web-ui-use` |
| 2026-10-31 | §2, §4: a JavaScript command receives a `Context` (`info`, `warning`, `error`, `evaluate`, `isCancelled`, `element`); `conventions: { context: false }` opts out. Limitation removed. | design `js-command-context` |
| 2026-10-30 | §3: the recipe provider is `LibKind`'s default chain, so a `records` build serves manifest chunks. | design `recipe-provider-chain-config` |
| 2026-10-26 | Created from the implemented `liquers-web`, checked against a wasm build under Node. | `WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE` |
| 2026-10-26 | A nested `null` now arrives as `null`; a failed asset rejects every `evaluate` caller; nested bytes are refused by design. Three limitations removed. | `WEB-NESTED-NULL-CROSSES-AS-UNDEFINED`, `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL`, `WEB-NESTED-BYTES-REFUSED-NOT-DEGRADED` |
| 2026-10-28 | A thrown-back `LiquersError` keeps its `query`, `key` and `position` (§6); the sentence describing their loss is corrected. | `WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT` |
