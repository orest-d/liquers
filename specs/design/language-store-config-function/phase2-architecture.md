# Phase 2: Solution & Architecture — Language-defined store_config handlers

Written under Phase 1's recommended answers (Q1 (a), Q2 per-integration lifecycle, Q3 no bytes,
Q4 and Q5 as stated).

## Overview

`StepBuilder` becomes an `Arc`'d closure, with the same native/wasm `cfg` pair `CompiledFn` uses,
and `StoreConfigFunctions::register_step` becomes generic over the closure, so every existing
`fn` builder still compiles. A language builder captures the callable, turns the spec's literal
arguments into language values once at build, and returns a `CompiledStep` that converts the
record, calls the function and converts the result back. Each binding retains its handlers beside
its commands and registers them into `command_registry.store_config` on every (re)build, so the
store sections compile against them through the existing path (`EnvironmentBuilder::build` in
Python, `apply_store` in `liquers-web`).

Rejected: a second table of boxed builders beside the `fn` table (two lookups and two
registration paths for one concept); `Box` instead of `Arc` (`StoreConfigFunctions` is `Clone`, and
`post-init-command-registration` clones the registry); boxing `ComponentBuilder` too (no use case,
Phase 1 non-goal); an async handler (a chain cannot await: `store-adapters` Phase 2 §4); running
the language function per *chain* instead of per *step* (a chain mixes built-in and language steps).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | Python has no language store; not needed here | No | No | `overlaps` |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | Late registration means a rebuild (web) or a refusal (Python) | No | No | AC-5 states it |
| `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` (`store-config-key-scoped-handlers`) | draft | P3 | Edits `compile_plan`'s handler step; a language handler is then key-scopable like any | No | No | Rebase whichever lands second |
| `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING` | draft | P3 | A handler's exception carries a traceback | No | No | Uses `pyerr_to_error` / `js_error_to_liquers` unchanged |

## Interfaces

`liquers-core/src/store/realm.rs` (sync; builders run at store build, steps on every store call):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub type StepBuilder = std::sync::Arc<
    dyn Fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error> + Send + Sync>;
#[cfg(target_arch = "wasm32")]
pub type StepBuilder =
    std::sync::Arc<dyn Fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>>;

impl StoreConfigFunctions {
    pub fn register_step<F>(&mut self, cmr: &mut CommandMetadataRegistry,
                            metadata: CommandMetadata, builder: F) -> Result<(), Error>
    where F: Fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>
             + crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync + 'static;
}

/// The literal arguments of a handler step, in declaration order; a `multiple` argument as a
/// JSON array. For builders that hand them to another language.
pub fn literal_parameters(parameters: &ResolvedParameterValues)
    -> Result<Vec<serde_json::Value>, Error>;
```

`compile_plan` calls the builder by reference instead of copying a `fn` pointer. No new trait,
command, `ExtValue` variant or registry change.

`liquers-py` (new module `src/store_config.rs`):

```rust
#[derive(Clone)]
pub struct PyStoreConfigHandler { pub key: CommandKey, pub metadata: CommandMetadata,
                                  pub callable: Arc<Py<PyAny>> }
impl PyStoreConfigHandler {
    pub fn new(object: Py<PyAny>, baseline: serde_json::Value, declaration: &serde_json::Value)
        -> Result<Self, Error>;     // CommandDeclaration as PyCommandSpec::new does; refuses async def
    pub fn register(&self, registry: &mut CommandRegistry<PyEnvironment>) -> Result<(), Error>;
}
```

Python surface: `env.store_config_handler` (decorator, options `name`, `label`, `doc`), backed by
`Environment._register_store_config_handler(callable, baseline, declaration)` and a
`liquers._commands` helper that reuses `_baseline` with the first parameter as the subject.
The function is called as `fn(record, *arguments)` with an `lq.MetadataRecord`.

`liquers-web` (new module `src/store/config_handler.rs`): `JsStoreConfigHandler { metadata:
CommandMetadata, run: js_sys::Function }`, parsed with `JsCommandSpec::parse` (realm forced to
`store_config`); module function `#[wasm_bindgen(js_name = registerStoreConfigHandler)]` in
`src/lib.rs`; its declaration type in `src/typescript.rs`. The function is called as
`run(record, ...arguments)` with the record as `metadata_to_js_value` renders it.

## Integration Points

- `liquers-core/src/store/realm.rs`: type alias, `register_step`, `compile_plan` call site,
  `literal_parameters`, the `StoreConfigFunctions` doc comment ("plain `fn` pointers") corrected.
- Unchanged callers, compile-checked: `store/handlers.rs`, `store/test_support.rs`,
  `liquers-lib/src/store_config.rs`.
- `liquers-py/src/environment.rs`: `Declarations` gains `handlers: Vec<PyStoreConfigHandler>`;
  `configured_builder` registers them after the commands; `EnvCell::register_handler` refuses when
  `declarations.store` is `Some` (built), otherwise replaces by key and calls `retire`.
- `liquers-web/src/environment.rs`: `REGISTERED_HANDLERS` thread-local beside `REGISTERED_SPECS`;
  `new_environment` users (`rebuild_with`, the pending path) replay them before `apply_store`;
  `register_store_config_handler_on` applies directly while `PENDING_ENV` is set, else rebuilds.
- `liquers-web/src/command/spec.rs` intake: a `registerCommand` declaration with realm
  `store_config` is refused, naming `registerStoreConfigHandler`.

## Error Handling

| Failure | Error |
|---|---|
| Handler registered after Python's store was built | `Error::configuration_error` naming the handler and saying to register before first use |
| `async def` handler (Python) | `Error::configuration_error` at registration |
| Function raises (Python / JavaScript) | `pyerr_to_error` / `js_error_to_liquers(.., ErrorType::ExecutionError)`; then the stage rule in `StoreConfigFunctions::compile` (warning on read, abort otherwise) |
| Result is not a record, `None` or `undefined`; a Promise | `Error::conversion_error_with_message(<type returned>, "a metadata record", <handler name>)`, then the stage rule |
| Argument not convertible (JSON → language) | `Error::configuration_error` from the builder, which `compile_plan` reports with the spec position |

## Relevant Commands

No interpreter command added or changed; `specs/command_registry.yaml` does not change (it lists
only the built-in `store_config` entries, registered in Rust). Realm: `store_config`.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/STORE_CONFIG_FSD.md` | reference | Handlers may be closures; language handlers: sync, record in/out, built at store build |
| `specs/guides/STORE_CONFIG_GUIDE.md` | guide | Registering a capturing closure |
| `specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/PYTHON_GUIDE.md` | reference, guide | `store_config_handler`, lifecycle, GIL cost |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE | guide | Answer to the handler question; `STORE18` reworded (Q2); Python blueprint uses existing accessors |
| `liquers-web/README.md`, `liquers-py/README.md` | readme | `STORE18` required for web; Python row note |

`affects_docs`: the five `specs/` documents. `specs/README.md`: this design's row.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/store/realm.rs`; `liquers-py/src/{store_config,environment,lib}.rs`, `python/liquers/_commands.py`; `liquers-web/src/{environment,lib,typescript}.rs`, `src/store/config_handler.rs`, `src/command/spec.rs` |
| Crates and workflows affected | `liquers-core` (native and wasm), `liquers-py`, `liquers-web` |
| Existing tests likely to change | None; `fn` items coerce to the generic parameter |
| New validation | Phase 3 tests; `cargo check -p liquers-core --target wasm32-unknown-unknown`; `check-stubs.sh --build` |
| Compatibility | `StepBuilder` alias and `register_step` signature change (source-compatible for `fn` callers; code naming `StepBuilder` as a `fn` breaks — none in the workspace) |
| Concurrency / performance | Python: GIL per step call; read chains run per listed entry; `block_in_place` keeps other tasks moving. JavaScript: single-threaded, a re-entrant call is impossible because a chain never awaits |
| Security | A handler runs user code on every store access, as a command already does |
| Recovery | Per-crate steps revert independently |
| Certainty | High for core (code read); medium for the bindings' surface (Q1, Q2) |
