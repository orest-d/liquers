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
5. Use permissive value conversion by default (unknown Python objects become opaque Python values).
6. Keep unsupported Python-equivalent types (`UiCommand`, `Widget`, `UIElement`) as wrapped `Value` objects.
7. Register commands from Python using decorators (`first_command`, `command`) with metadata extraction from callable signature and docstring.

TODO: Add guidelines on how to deal with async code in pyo3 based on chaper 5 of the pyo3 documentation (https://pyo3.rs/v0.28.2/async-await.html).
TODO: Add guidelines about parallelism and mutithreading, see chapter 6 of the pyo3 documentation (https://pyo3.rs/v0.28.2/parallelism.html) and https://pyo3.rs/v0.28.2/class/thread-safety.html.
TODO: Develop a strategy to avoid deadlocks when calling a python code. Though python code may depend on a single GIL, pure rust code should always be able to run in parallell threads. Consider scenarions with sync or async python commands using context.evaluate to execute a) rust commands b) other python sync or async commands. Consider the finite queue of the asset manager. Investigate which scenarios are may cause deadlocks. 

## 2. PyO3 and Async Constraints (Design Constraints)

These constraints are mandatory and drive interface design:

1. Python C-API access requires GIL. Any conversion or callable invocation touching `PyAny` must happen inside `Python::with_gil`.
2. `Python<'py>` and `Bound<'py, T>` are GIL-bound and must not cross `await` boundaries.
3. Store Python callables/objects as owned `Py<PyAny>` handles in Rust state; bind under GIL only at call time.
4. Sync Python callable execution from async Rust paths must use `spawn_blocking` to avoid blocking Tokio worker threads. 
   FIXME: Not just to avoid blocking, but mainly to execute within sync python code.
5. Async Python callables require bridge integration (`pyo3-async-runtimes` / compatible runtime bridge), because a Python coroutine cannot be directly awaited by Rust without an adapter.
6. Blocking wrappers over async services must run futures on a dedicated runtime handle, not by nested runtime creation on every call.

## 3. Milestones

## Milestone 1 (baseline)

1. Modernized `liquers-py` architecture and module split.
2. Shared global environment initialization API.
3. Value wrapper with permissive conversion + opaque Python object fallback.
4. `first_command` and `command` decorators with metadata extraction.
5. Python command registration/execution (sync and async function definitions accepted; sync execution path fully implemented).
6. Blocking + async wrappers for environment/store/asset manager/context/recipe provider.
7. Default registration of `liquers-lib` commands.

## Milestone 2

1. Full async Python callable execution bridge.
2. Embedded `liquers-axum` server lifecycle (foreground/background) from Python.
3. Typed Python exception hierarchy + richer traceback/cause mapping.

## Milestone 3

1. GUI mode launcher integration (egui primary, ratatui optional/future).
2. Unified process orchestration for combined server + GUI modes.

## 4. Module Architecture (`liquers-py`)

1. `src/python_value.rs`
   1. value extension enum,
   2. value conversion policies/codecs,
   3. Python `Value` wrapper type.
2. `src/python_error.rs`
   1. Rust<->Python error conversion,
   2. typed Python exceptions.
3. `src/python_metadata.rs`
   1. callable signature/doc parsing,
   2. mapping to `CommandMetadata`.
4. `src/python_commands.rs`
   1. callable registry,
   2. sync/async command bridge implementation,
   3. decorators.
FIXME: There should be a first-class support for CommandMetadataRegistry, but there does not need to be support for CommandExecutor / CommandRegistry (at least not in Milestone 1). Command execution can be seen as internal to liquers-core. Python should be able to execute queries though (e.g. evaluate methods).
5. `src/python_env.rs`
   1. shared environment singleton,
   2. initialization config and builder integration,
   3. service wrappers (blocking + async).
6. `src/python_server.rs`
   1. axum server start/stop/join handles.
7. `src/python_gui.rs`
   1. GUI launch API (future milestone).
8. `src/lib.rs`
   1. `#[pymodule]` exports.
FIXME: Propose organization of puyhon modules in multiple submodules; check if pyo3 supports it.

## 5. Value Model and Conversion

## 5.1 Core value extension

```rust
pub enum PythonValueExtension<E> {
    RustExt { value: E },
    PythonObject { value: pyo3::Py<pyo3::PyAny> },
}
```

Intent:

1. `RustExt`: preserve existing Rust extension variants (including `ExtValue`).
2. `PythonObject`: hold Python-only runtime objects that have no native Liquers representation.

TODO: I assume that PythonValueExtension implements all important Value-related interfaces:
- ValueInterface (necessary in order to be able to work in Environment)
- DefaultValueSerializer (necessary in order to be able to work in Environment)
- ExtValueInterface (Necessary to be able to support liquers-lib commands)
TODO: Note that all liquers-lib should be registered in the python envref. This may require some modifications of the liquers-lib, so that the commands can accpet generic value (with ExtValueInterface).

## 5.2 Wrapper behavior for non-Python-native Liquers values

For `UiCommand`, `Widget`, `UIElement`:

1. `value_to_pyobject`: returns `liquers_py.Value` wrapper, not eager Python-native conversion.
2. `pyobject_to_value`: if passed `liquers_py.Value`, preserve underlying Liquers value.
3. This keeps unsupported types lossless and avoids accidental serialization breakage.

## 5.3 Conversion interfaces

```rust
pub enum ConversionMode {
    Strict,
    Permissive,
}

pub trait PythonValueCodec<E>: Send + Sync {
    fn py_to_value(
        &self,
        py: pyo3::Python<'_>,
        obj: &pyo3::Bound<'_, pyo3::PyAny>,
        mode: ConversionMode,
    ) -> Result<liquers_lib::value::CombinedValue<liquers_lib::value::SimpleValue, PythonValueExtension<E>>, liquers_core::error::Error>;

    fn value_to_py(
        &self,
        py: pyo3::Python<'_>,
        value: &liquers_lib::value::CombinedValue<liquers_lib::value::SimpleValue, PythonValueExtension<E>>,
    ) -> pyo3::PyResult<pyo3::PyObject>;
}
```

Method intent:

1. `py_to_value`: canonical Python->Liquers conversion with policy-controlled fallback.
2. `value_to_py`: canonical Liquers->Python conversion used in command outputs and evaluate results.

Default policy: `Permissive`.
TODO: It is decided to always use permissive policy. However, remain as an open question. Identify cases, where it would make sense to use configurable policy. Can we identify cases that would lead to inconsistency, i.e. one requiring Strict, another Permissive policy?
Can we identify cases when Permissive policy would lead to problems?
Consider a 3rd policy: AlwaysWrapped. For such a policy, value_to_py would always return `liquers_py.Value` and the python coude would have to use Value methods to unwrap it.

## 6. Error Mapping Interfaces

```rust
pub trait PythonErrorMapper: Send + Sync {
    fn rust_to_py(&self, py: pyo3::Python<'_>, err: &liquers_core::error::Error) -> pyo3::PyErr;
    fn py_to_rust(&self, err: pyo3::PyErr) -> liquers_core::error::Error;
}
```

Method intent:

1. `rust_to_py`: produce typed Liquers Python exceptions with structured fields.
2. `py_to_rust`: convert Python exceptions to `ExecutionError`-class Liquers errors with Python class/message/traceback context.

FIXME: Is this really a one trait? To me it looks `std::from::From<liquers_core::error::Error> for PyErr`
and a function py_to_liquers_error. Check pyo3 documentation https://pyo3.rs/v0.28.2/function/error-handling.html.


## 7. Command Registration and Execution

## 7.1 Callable registration model

Each Python command registration creates:

1. stored callable handle (`Py<PyAny>`),
2. generated `CommandMetadata`,
3. Rust command executor entry bound to command key.

Registry abstraction:

```rust
pub trait PythonCallableRegistry: Send + Sync {
    fn register_callable(
        &self,
        key: liquers_core::command_metadata::CommandKey,
        callable: pyo3::Py<pyo3::PyAny>,
        metadata: liquers_core::command_metadata::CommandMetadata,
        mode: PythonCallableMode,
    ) -> Result<(), liquers_core::error::Error>;

    fn get_callable(
        &self,
        key: &liquers_core::command_metadata::CommandKey,
    ) -> Option<PythonCallableEntry>;
}
```

`PythonCallableMode`:

1. `Sync`: run in blocking section + GIL.
2. `Async`: callable returns coroutine; run through Python async bridge.

TODO: Though this approach might work, it may possibly run into unforseen difficulties. Another drawback is that to create a Python callable, the module with the command would need to be imported at the time of construction of CommandExecutor (which is init, since the command executor is sealed after the envref is created). As an alternative solution, a mapping approach has been developed, which stores a mapping of a virtual command defined only in command metadata registry (but not in command executor) to a real command. For now, there is one such mapping supported: CommandDefinition::Alias. In PoC, every python call is mapped (via an alias) to a pycall command, that imports the module and executes the python function. Please support this mechanism. Investigate wheter PythonCallableRegistry is a viable option and what are the tradeoffs.

## 7.2 Invocation bridge interface

```rust
pub trait PythonCommandInvoker<E: liquers_core::context::Environment>: Send + Sync {
    fn invoke_sync(
        &self,
        state: &liquers_core::state::State<E::Value>,
        args: liquers_core::commands::CommandArguments<E>,
        context: liquers_core::context::Context<E>,
        entry: &PythonCallableEntry,
    ) -> Result<E::Value, liquers_core::error::Error>;

    fn invoke_async(
        &self,
        state: liquers_core::state::State<E::Value>,
        args: liquers_core::commands::CommandArguments<E>,
        context: liquers_core::context::Context<E>,
        entry: &PythonCallableEntry,
    ) -> std::pin::Pin<Box<dyn core::future::Future<Output = Result<E::Value, liquers_core::error::Error>> + Send + 'static>>;
}
```

Method intent:

1. `invoke_sync`: execute sync Python callables safely in async runtime contexts.
2. `invoke_async`: execute coroutine-returning callables using runtime bridge.

TODO: This needs more details. It is unclear where and how should this be used and how it would work.

## 8. Decorator API: `first_command` and `command`

Python API:

1. `@liquers.first_command(...)`
2. `@liquers.command(...)`

Behavior:

1. `first_command`:
   1. command does not consume prior Liquers state (`state_argument = None`).
   2. first callable positional parameter maps to first metadata argument.
2. `command`:
   1. command consumes prior state (`state_argument = Some("state")` by default).
   2. first callable positional parameter is treated as state unless explicitly overridden.
3. Both decorators:
   1. parse function signature + docstring,
   2. build `CommandMetadata`,
   3. register callable in active `CommandMetadataRegistry`.

### 8.1 Metadata extraction interface

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

Method intent:

1. Extracts command name, docs, args, defaults, typing hints, variadic behavior.
2. Applies decorator overrides (`name`, `namespace`, `realm`, cache/volatile/async flags, etc.).

### 8.2 Complete `CommandMetadata` mapping table

| `CommandMetadata` field | Source | Available from Python function signature alone? | Notes |
|---|---|---|---|
| `realm` | decorator option | No | default `""` unless set |
| `namespace` | decorator option | No | default `"root"` semantics |
| `name` | function `__name__` or decorator `name=` | Yes | decorator overrides |
| `label` | function name (normalized) or decorator `label=` | Partial | signature gives name only |
| `module` | callable `__module__` | No | from callable metadata, not signature |
| `doc` | function docstring | No | from `__doc__` |
| `presets` | decorator option / external config | No | not inferable from signature |
| `next` | decorator option / external config | No | not inferable from signature |
| `filename` | decorator option | No | not inferable |
| `state_argument` | decorator kind (`command` vs `first_command`) and override | No | derived policy |
| `arguments` | signature parameters | Yes | includes name/order/default/typing |
| `cache` | decorator option (default true) | No | policy/config |
| `volatile` | decorator option (default false) | No | policy/config |
| `expires` | decorator option | No | policy/config |
| `is_async` | callable kind (`inspect.iscoroutinefunction`) | No | callable introspection |
| `definition` | always `Registered` for Python callable registration | No | set by registration path |

Argument-level mapping (`ArgumentInfo`) from signature:

1. `name`: parameter name.
2. `label`: derived from name (`_` -> space) unless decorator override.
3. `default`: Python default value if JSON-convertible; otherwise omitted or explicit error in strict mode.
4. `argument_type`: from type annotation mapping (`str`, `int`, `float`, `bool`, optional numeric forms, enums, fallback `Any`).
5. `multiple`: true for variadic `*args` target parameter.
6. `injected`: true for reserved injectable names (at minimum `context`; optionally wrapper types via explicit marker).
7. `gui_info`, `hints`, `presets`: decorator/config driven; not inferable from signature.

TODO: We need to be careful about enums. They are available, but liquers will not create an enum type but convert it to type specified by EnumArgumentType. To strictly comply with python signature,
it should be converted back to enum. Schedule this to Milestone 2.  

## 9. Shared Environment and Initialization

## 9.1 Singleton model

Python library has one shared environment instance for process lifetime:

1. `init(config)` creates and stores global `EnvRef`.
2. `get_env()` returns initialized shared environment wrapper.
3. `reset_env()` (test-only or explicit advanced use) replaces singleton safely.

TODO: Specify how exactly will the singleton be stored and made available. Will it use some special crate? (singleton? lazy_static?) or will it directly store it in python somehow? Do a research, propose 3 alternative solutions, discuss pros and cons, propose a winner.


Interface:

```rust
pub trait PythonEnvironmentProvider<E: liquers_core::context::Environment>: Send + Sync {
    fn init(&self, cfg: PythonLiquersConfig) -> Result<(), liquers_core::error::Error>;
    fn get(&self) -> Result<liquers_core::context::EnvRef<E>, liquers_core::error::Error>;
    fn reconfigure_registry(
        &self,
        cmr: liquers_core::command_metadata::CommandMetadataRegistry,
    ) -> Result<(), liquers_core::error::Error>;
}
```

Method intent:

1. `init`: one-shot environment construction with explicit config.
2. `get`: stable access point for all APIs.
3. `reconfigure_registry`: replace command metadata registry (e.g., from YAML reload).

## 9.2 Initialization configuration requirements

`PythonLiquersConfig` must support:

1. Store configuration via `liquers-store` builder (`StoreRouterBuilder` from YAML/JSON/object).
2. Recipe provider:
   1. default/trivial provider in baseline,
   2. extension hook for custom provider later.
3. Asset manager:
   1. default asset manager in baseline,
   2. optional future config.
4. Command metadata registry override:
   1. set directly,
   2. load from YAML/JSON.
5. Register all commands from `liquers-lib` by default.

## 10. Blocking and Async Service Wrappers

Each major service has two Python-facing wrappers:

1. `Async*` wrapper (`async def` methods, direct future awaiting in Python).
2. `Blocking*` wrapper (sync methods internally `block_on` runtime handle).

Services:

1. environment (`evaluate`, `evaluate_immediately`, registry access),
2. store (`get/set/listdir/...`),
3. asset manager (`get_asset`, apply),
4. recipe provider (`get_recipe`/resolve interface),
5. context (logging, evaluate/apply helpers, metadata access).

Internal runtime interface:

```rust
pub trait RuntimeBridge: Send + Sync {
    fn block_on<F: core::future::Future + Send + 'static>(&self, fut: F) -> Result<F::Output, liquers_core::error::Error>
    where
        F::Output: Send + 'static;
}
```

Intent:

1. Centralize all sync-over-async behavior.
2. Avoid ad-hoc nested runtime creation.

## 11. Server and GUI Usage Modes

Python API must provide three usage modes:

1. Library mode (Milestone 1)
   1. initialize shared environment,
   2. execute queries through environment wrapper.
2. Server mode (Milestone 2)
   1. run `liquers-axum` in foreground (blocking),
   2. or background (join/stop handle).
3. GUI mode (Milestone 3)
   1. launch selected GUI frontend,
   2. optional combination with server mode in same process.

## 12. Legacy Compatibility References (Likely to Change)

Current `liquers-py` reference points retained for migration context:

1. `pycall` bridge concept in `liquers-py/src/commands.rs` (currently has `todo!()` branches and limited injection support).
TODO: Design extension of the pycall to support full feature set of current command mechanism.

2. `add_python_command` alias approach in `liquers-py/src/command_metadata.rs` (current metadata extraction is minimal).

These are migration references only; implementation details are expected to change.

## 13. External Older Python System Investigation Note

Requested links:

1. `https://github.com/orest-d/liquer/blob/master/liquer/commands.py`
2. `https://orest-d.github.io/liquer/site/apidocs/liquer/commands.html`

In this environment, outbound network resolution is unavailable, so direct retrieval was not possible during this design pass.  
Design for decorators and metadata extraction is therefore based on:

1. requirements in `PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md`,
2. current `liquers-py` PoC behavior,
3. existing Rust `CommandMetadata` model.

When network access is available, this document should be amended with one compatibility delta section against the old Python implementation.

