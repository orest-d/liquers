# Python Wrapper Detailed Architecture (liquers-py Modernization)

Status: Draft  
Date: 2026-02-26  
Supersedes detail level in: `specs/PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md`

## 1. Scope and Direction

This document defines the detailed architecture for modernizing `liquers-py` (preferred over creating a new crate).

Design goals:

1. Keep `liquers-core`/`liquers-lib` semantics as source of truth.
2. Provide a Python-first API with one shared environment (`EnvRef`) per process.
3. Support both sync and async Python commands.
4. Expose both async and blocking wrappers for core services:
   1. environment,
   2. recipe provider,
   3. store,
   4. asset manager,
   5. context.
5. Use permissive conversion by default, with stricter modes available for selected boundaries.
6. Keep unsupported Python-equivalent types (`UiCommand`, `Widget`, `UIElement`) as wrapped Liquers values.
7. Register commands from Python using decorators (`first_command`, `command`) with metadata extraction from callable signature and docstring.

## 2. PyO3 Target Version and Migration Baseline

Target for this architecture:

1. Use the most recent stable PyO3 release line at implementation time (currently `0.28.x`).
2. Prefer APIs and companion libraries aligned with that release line (including async runtime bridge libraries).

Migration baseline context:

1. Current repository state pins `pyo3 = 0.21.2` in `liquers-py/Cargo.toml`.
2. This is treated as legacy baseline for migration planning only, not as the desired end-state.

Versioning rule:

1. Architecture is written against latest-PyO3 semantics.
2. If temporary compatibility shims are needed during migration, they must be explicitly marked as transitional and removed after upgrade.

## 3. PyO3 Async, Parallelism, and Deadlock Guidance

## 3.1 Async guidelines (derived from PyO3 async docs)

Rules:

1. Do not hold `Python<'py>` or `Bound<'py, T>` across `.await`.
2. Convert GIL-bound objects to owned `Py<T>` before crossing async boundaries.
3. For sync Python callables invoked from async Rust execution, use `spawn_blocking`.
4. For async Python callables, bridge coroutine/future conversion through dedicated adapter layer aligned with target PyO3 (e.g. `pyo3-async-runtimes` family for `0.28.x`).
5. Conversion in/out of Python objects happens in short, explicit GIL scopes.

Intent:

1. Preserve runtime fairness in Tokio.
2. Prevent invalid lifetime usage and reentrancy mistakes.
3. Keep Python interaction deterministic and auditable.

## 3.2 Parallelism and thread-safety guidelines (derived from PyO3 parallelism/thread-safety docs)

Rules:

1. GIL is not a replacement for Rust synchronization.
2. CPU-heavy pure Rust code called from Python should run with GIL released (using current target-version API; `detach` terminology in modern docs).
3. Rust types exposed as `#[pyclass]` must satisfy thread-safety expectations:
   1. prefer `Send + Sync` where practical,
   2. use `#[pyclass(unsendable)]` only when required and document limitation.
4. Cross-thread Python object storage must use owned handles (`Py<PyAny>`), rebinding under GIL on use.

Intent:

1. Keep pure Rust paths parallelizable even when Python commands exist.
2. Avoid accidental single-thread bottlenecks through long GIL sections.

## 3.3 Deadlock strategy

### 3.3.1 Deadlock-prone scenarios

1. Sync Python command calls `context.evaluate(...)` while still holding GIL, and nested path needs another Python command invocation that also needs GIL.
2. Async Python command path keeps a GIL-bound borrow alive while awaiting Rust future.
3. Queue inversion: running asset job synchronously waits for another asset queued behind it in finite-capacity `JobQueue`.
4. Rust lock + GIL lock inversion (lock held while entering Python; Python callback re-enters Rust and needs same lock).

### 3.3.2 Mandatory invariants

1. Never block or await while holding GIL.
2. Never hold Rust mutex/RwLock guard across Python callback invocation.
3. Nested evaluation from running command must not enqueue work that it synchronously waits for on same saturated queue.
4. `Context` wrappers must separate:
   1. Python interaction phase (GIL),
   2. async execution phase (no GIL),
   3. result conversion phase (GIL).

### 3.3.3 Queue-safe nested evaluation policy

Introduce policy abstraction:

```rust
pub enum NestedEvaluationMode {
    Queue,
    InlineIfPossible,
}

pub trait EvaluationSchedulingPolicy: Send + Sync {
    fn mode_for_nested_call(&self, depth: usize) -> NestedEvaluationMode;
}
```

Baseline policy:

1. top-level evaluate: `Queue`,
2. nested evaluate from active command: `InlineIfPossible`.

Intent:

1. Prevent self-starvation when queue capacity is low.
2. Preserve bounded queue behavior for top-level traffic.

## 4. Milestones

## Milestone 1 (baseline)

1. Modernized `liquers-py` module split.
2. Shared global environment initialization API.
3. Value wrapper with permissive conversion + opaque Python object fallback.
4. `first_command` and `command` decorators with metadata extraction.
5. Baseline command registration/execution:
   1. sync Python call execution fully supported,
   2. async callable detection and metadata support,
   3. alias-based dispatch via `pycall`/`pycall_async` bridge.
6. Blocking + async wrappers for environment/store/asset manager/context/recipe provider.
7. Default registration of `liquers-lib` commands.

## Milestone 2

1. Full coroutine execution bridge for async Python commands.
2. Embedded `liquers-axum` server lifecycle (foreground/background) from Python.
3. Typed Python exception hierarchy + richer traceback/cause mapping.
4. Optional enum round-trip restoration for annotated Python Enum parameters.

## Milestone 3

1. GUI mode launcher integration (egui primary, ratatui optional/future).
2. Unified process orchestration for combined server + GUI modes.

## 5. Module and Python Package Organization

PyO3 supports submodules via `add_submodule`, so `liquers_py` should expose structured module tree.

Proposed Python module tree:

1. `liquers_py` (root)
2. `liquers_py.env`
3. `liquers_py.commands`
4. `liquers_py.value`
5. `liquers_py.errors`
6. `liquers_py.server`
7. `liquers_py.gui` (future)

Rust-side module layout:

1. `src/python_env.rs`
2. `src/python_commands.rs`
3. `src/python_value.rs`
4. `src/python_error.rs`
5. `src/python_server.rs`
6. `src/python_gui.rs`
7. `src/lib.rs` (root `#[pymodule]` wiring + `add_submodule`)

Guideline:

1. Keep root module minimal and stable.
2. Put feature growth into submodules to reduce root API churn.

## 6. CommandMetadataRegistry-First Model

Milestone 1 prioritizes first-class `CommandMetadataRegistry` support. Python should not manage Rust `CommandExecutor` internals directly.

Architecture decision:

1. Register one internal bridge command (or two):
   1. `pycall` (sync)
   2. `pycall_async` (future/coroutine path)
2. Python decorators add metadata entries as `CommandDefinition::Alias` pointing to bridge commands.
3. Alias head parameters encode callable resolution information (module/function ID or registry key).
4. Query evaluation remains standard Liquers execution through `EnvRef.evaluate*`.

Intent:

1. Keep Python command registration dynamic without mutating low-level executor table per command.
2. Match existing PoC direction while making metadata first-class.

## 7. Value Model and Trait Contracts

## 7.1 Core extension type

```rust
pub enum PythonValueExtension<E> {
    RustExt { value: E },
    PythonObject { value: pyo3::Py<pyo3::PyAny> },
}
```

## 7.2 Concrete value type

```rust
type PyLiquersValue = liquers_lib::value::CombinedValue<
    liquers_lib::value::SimpleValue,
    PythonValueExtension<liquers_lib::value::ExtValue>,
>;
```

## 7.3 Required trait implementations

Implementations required for `PythonValueExtension<E>`:

1. `ValueExtension` (from `liquers-lib::value::extended`) when `E: ValueExtension`.
2. `DefaultValueSerializer` with explicit behavior:
   1. `RustExt` delegates,
   2. `PythonObject` serialization policy driven (default: error unless codec configured).

Implementations required for `PyLiquersValue`:

1. `ValueInterface` (already derived through `CombinedValue` mechanics).
2. `DefaultValueSerializer` (via extension path).
3. `ExtValueInterface` adapter:
   1. if extended `RustExt(ExtValue)`: delegate,
   2. if `PythonObject`: return conversion error for `as_image` / `as_polars_dataframe` / `as_ui_element`.

## 7.4 liquers-lib registration compatibility

Requirement: all `liquers-lib` commands should register into Python environment by default.

Needed adaptation:

1. `liquers-lib` command registration entry points should be generic over value type where feasible:
   1. `V: ValueInterface + ExtValueInterface + DefaultValueSerializer + Clone + Send + Sync + 'static`.
2. For command groups tightly coupled to concrete `liquers_lib::value::Value`, introduce generic wrappers or adapter trait bounds.

This is expected to require targeted refactoring in `liquers-lib`, and is accepted in Milestone 1 scope.

## 8. Conversion Policy Matrix

Keep policy configurable internally, default externally to `Permissive`.

```rust
pub enum ConversionMode {
    Strict,
    Permissive,
    AlwaysWrapped,
}
```

Policy intent:

1. `Permissive`:
   1. user-facing default,
   2. unknown Python object -> `PythonObject`.
2. `Strict`:
   1. CI/validation/debug mode,
   2. fail fast on non-mappable objects.
3. `AlwaysWrapped`:
   1. deterministic transport/debug mode,
   2. `value_to_py` always returns wrapper object.

When configurable policy is necessary:

1. schema-constrained APIs (strict input guarantees).
2. reproducible serialization pipelines (reject opaque objects).
3. debugging conversion regressions.

Permissive-mode risks and mitigations:

1. Risk: non-serializable payload reaches persistence boundary.
   Mitigation: explicit serializer check and `SerializationError` with metadata warning.
2. Risk: hidden Python object leaks into contexts expecting JSON-like structure.
   Mitigation: strict mode for boundary validators.

## 9. Error Mapping Design

`impl From<liquers_core::error::Error> for PyErr` is not possible directly in `liquers-py` due Rust orphan rules (`From` and both types are foreign).

Architecture decision:

1. Keep explicit conversion functions plus local wrapper type.
2. Use these interfaces:

```rust
pub struct PyLiquersError(pub liquers_core::error::Error);

pub fn liquers_error_to_pyerr(err: &liquers_core::error::Error) -> pyo3::PyErr;
pub fn pyerr_to_liquers_error(err: pyo3::PyErr) -> liquers_core::error::Error;
```

3. Implement `From<PyLiquersError> for PyErr` (allowed: local wrapper type).

Intent:

1. Keep conversion explicit and testable.
2. Preserve typed Python exception hierarchy and rich context.

## 10. Command Registration Architecture

## 10.1 Alias mapping vs callable registry tradeoff

Two options:

1. Direct `PythonCallableRegistry` + per-command executor binding.
2. Alias mapping through `CommandMetadata.definition = Alias { command: pycall, ... }`.

Tradeoff summary:

1. Direct registry:
   1. pros: direct invocation path,
   2. cons: tighter coupling to executor lifecycle; harder late registration if executor is considered sealed.
2. Alias mapping:
   1. pros: aligns with existing command metadata mechanism and PoC,
   2. cons: indirection and bridge command complexity.

Selected baseline: alias mapping.

Hybrid extension path:

1. Keep alias mechanism as canonical metadata contract.
2. Optional in-process callable cache can accelerate resolution inside `pycall` implementation.

## 10.2 Invocation bridge details

Bridge interfaces:

```rust
pub trait PythonCommandInvoker<E: liquers_core::context::Environment>: Send + Sync {
    fn invoke_sync(
        &self,
        state: &liquers_core::state::State<E::Value>,
        args: liquers_core::commands::CommandArguments<E>,
        context: liquers_core::context::Context<E>,
        descriptor: &PyCallableDescriptor,
    ) -> Result<E::Value, liquers_core::error::Error>;

    fn invoke_async(
        &self,
        state: liquers_core::state::State<E::Value>,
        args: liquers_core::commands::CommandArguments<E>,
        context: liquers_core::context::Context<E>,
        descriptor: &PyCallableDescriptor,
    ) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<E::Value, liquers_core::error::Error>> + Send + 'static>>;
}
```

Usage in runtime:

1. `pycall` command executor decodes alias head parameters into `PyCallableDescriptor`.
2. It delegates to `invoke_sync`.
3. `pycall_async` delegates to `invoke_async`.
4. Conversion and error mapping happen at invoker boundary.

## 10.3 Full-feature `pycall` extension requirements

`pycall` must support all current command parameter mechanisms:

1. `ParameterValue::DefaultValue`, `OverrideValue`, `ParameterValue`.
2. Link variants (`DefaultLink`, `OverrideLink`, `ParameterLink`, `EnumLink`) by evaluating links before Python call.
3. `Injected` parameters:
   1. `context`,
   2. optional environment/service wrappers.
4. Variadic mapping (`MultipleParameters`) to Python `*args`.
5. Named mapping for decorator-defined keyword style (future extension).
6. Stateful modes from PoC (`no`, `pyobject`, `value`, `state`) with documented semantics.
7. Sync + async callable dispatch.

## 11. Decorators and Metadata Extraction

Python API:

1. `@liquers.first_command(...)`
2. `@liquers.command(...)`

Behavior:

1. `first_command`:
   1. does not consume prior Liquers state (`state_argument = None` by default).
2. `command`:
   1. consumes prior state (`state_argument = Some("state")` by default).
3. both:
   1. parse function signature,
   2. parse docstring,
   3. generate `CommandMetadata`,
   4. register alias entry in `CommandMetadataRegistry`.

Metadata extractor:

```rust
pub trait CommandMetadataExtractor: Send + Sync {
    fn from_callable(
        &self,
        py: pyo3::Python<'_>,
        callable: &pyo3::Bound<'_, pyo3::PyAny>,
        options: MetadataExtractionOptions,
    ) -> Result<liquers_core::command_metadata::CommandMetadata, liquers_core::error::Error>;
}
```

### 11.1 Complete `CommandMetadata` mapping table

| `CommandMetadata` field | Source | Available from Python function signature alone? | Notes |
|---|---|---|---|
| `realm` | decorator option | No | default `""` unless set |
| `namespace` | decorator option | No | default `"root"` semantics |
| `name` | function `__name__` or decorator `name=` | Yes | decorator overrides |
| `label` | function name normalized or decorator `label=` | Partial | signature gives name only |
| `module` | callable `__module__` | No | callable metadata |
| `doc` | function docstring | No | callable metadata |
| `presets` | decorator/config | No | not inferable from signature |
| `next` | decorator/config | No | not inferable from signature |
| `filename` | decorator option | No | not inferable |
| `state_argument` | decorator kind + override | No | policy-derived |
| `arguments` | signature parameters | Yes | includes name/order/default/typing |
| `cache` | decorator option | No | default true |
| `volatile` | decorator option | No | default false |
| `expires` | decorator option | No | policy/config |
| `is_async` | callable coroutine introspection | No | from callable, not signature |
| `definition` | `Alias` to `pycall`/`pycall_async` | No | set by registration path |

Enum handling decision:

1. Milestone 1: enum annotations map to Liquers `ArgumentType::Enum`/`GlobalEnum` semantics; callable receives resolved primitive value per `EnumArgumentType`.
2. Milestone 2: optional enum rehydration adapter converts primitive back to Python Enum instance before invocation.

## 12. Shared Environment Singleton: Alternatives and Winner

Three alternatives:

1. Rust global singleton (`once_cell::sync::OnceCell` + lock).
   1. pros: fast, explicit, testable from Rust.
   2. cons: process-global, careful reset needed for tests.
2. Python module-level storage (`liquers_py._envref`).
   1. pros: intuitive from Python.
   2. cons: weaker Rust-side guarantees and more Python-state coupling.
3. Python capsule / interpreter-state keyed storage.
   1. pros: better subinterpreter isolation potential.
   2. cons: highest complexity, not needed in baseline.

Winner for Milestone 1: option 1.

Implementation contract:

```rust
pub trait PythonEnvironmentProvider<E: liquers_core::context::Environment>: Send + Sync {
    fn init(&self, cfg: PythonLiquersConfig) -> Result<(), liquers_core::error::Error>;
    fn get(&self) -> Result<liquers_core::context::EnvRef<E>, liquers_core::error::Error>;
    fn reset_for_tests(&self) -> Result<(), liquers_core::error::Error>;
    fn reconfigure_registry(
        &self,
        cmr: liquers_core::command_metadata::CommandMetadataRegistry,
    ) -> Result<(), liquers_core::error::Error>;
}
```

## 13. Initialization Requirements

`PythonLiquersConfig` must support:

1. store configuration via `liquers-store::StoreRouterBuilder` from YAML/JSON/object.
2. recipe provider:
   1. default/trivial provider in baseline,
   2. custom provider hook later.
3. default asset manager in baseline.
4. command metadata registry override (direct object or YAML/JSON load).
5. register all `liquers-lib` commands by default.

## 14. Blocking and Async Service Wrappers

Each core service has two wrappers:

1. async wrapper for Python async code,
2. blocking wrapper for sync Python code.

Services covered:

1. environment,
2. store,
3. asset manager,
4. recipe provider,
5. context.

Runtime bridge contract:

```rust
pub trait RuntimeBridge: Send + Sync {
    fn block_on<F: core::future::Future + Send + 'static>(
        &self,
        fut: F,
    ) -> Result<F::Output, liquers_core::error::Error>
    where
        F::Output: Send + 'static;
}
```

Blocking wrapper rule:

1. For sync Python callers, wrap `block_on` in a GIL-release section so waiting does not monopolize GIL.
2. If a temporary `0.21.2` compatibility layer exists, this maps to `Python::allow_threads`; target-line implementation should use the corresponding modern API.

## 15. Server and GUI Usage Modes

Python API must provide three usage modes:

1. Library mode (Milestone 1): initialize environment and execute queries.
2. Server mode (Milestone 2): run `liquers-axum` foreground/background with lifecycle handle.
3. GUI mode (Milestone 3): run GUI frontend; optionally combined with server.

## 16. Legacy and Compatibility Notes

Existing `liquers-py` PoC points used as migration reference:

1. `pycall` bridge (`liquers-py/src/commands.rs`).
2. alias registration helper (`liquers-py/src/command_metadata.rs`).

Migration intent:

1. preserve working concepts,
2. remove `todo!()` paths,
3. make parameter handling complete,
4. align error/type behavior with current Liquers architecture.

## 17. External Old Python Implementation Note

Requested external references are still useful for compatibility auditing:

1. `https://github.com/orest-d/liquer/blob/master/liquer/commands.py`
2. `https://orest-d.github.io/liquer/site/apidocs/liquer/commands.html`

This environment cannot reliably fetch those pages from shell tooling, so compatibility details in this draft are based on local repository state and current requirements. Add a dedicated compatibility delta section when direct source retrieval is available.

## 18. PyO3 References Used

1. Async/await guide: `https://pyo3.rs/main/async-await`
2. Parallelism guide: `https://pyo3.rs/main/parallelism`
3. Thread safety for `#[pyclass]`: `https://pyo3.rs/main/class/thread-safety`
4. Error handling: `https://pyo3.rs/main/function/error-handling`
5. Submodule support API: `PyModule::add_submodule` docs in `pyo3` API docs (`PyModuleMethods`)
