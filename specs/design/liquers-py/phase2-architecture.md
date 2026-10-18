# Phase 2: Solution & Architecture — liquers-py

## Overview

`liquers-py` drops its own `Value` enum and builds on `liquers_lib::value::Value`. Python objects
with no structural mapping go in `ExtValue::Foreign` as a `PyObjectValue` (`py.Object`). A
`liquers.Environment` wraps a `DefaultEnvironment<Value>` built by `default_environment_builder()`
with every `liquers-lib` command registered. Python callables are registered as async closures that
own a `Py<PyAny>`, the same pattern as liquers-web's `register_js_command`.

One tokio runtime per process, owned by liquers-py, runs every evaluation. The GIL is taken only
for the Python call and for value conversion, and is released while Rust awaits.

Rejected alternatives:
- **Keep the own `Value` enum.** Rejected because it cannot hold `ExtValue`, so polars and image
  commands could not compose (AC-3).
- **`pycall` alias commands.** Rejected by the owner on 2026-10-17. Each would also need a
  name-to-callable table beside the registry.
- **`pyo3-async-runtimes` for the asyncio bridge.** Rejected because its release cadence trails
  PyO3 and it would pin the PyO3 upgrade. The bridge needed here is one future type, about a
  hundred lines.
- **`pyo3-polars` for DataFrames.** Rejected because it couples three version trains (PyO3, Rust
  polars, Python polars). Arrow IPC bytes cost a copy and depend on nothing, and zero-copy can be
  added later without changing the API.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-VALUE-ENUM-OVERSIZED` | draft | P3 | The own enum is removed | No | No | Closed by M1 |
| `PY-VALUE-SERIALIZER-IS-A-STUB` | draft | P2 | Replaced by `liquers-lib`'s serializer plus the `py.Object` codec | No | No | Closed by M1 |
| `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` | draft | P2 | Replaced by closure registration | No | No | Closed by M2 |
| `JS-COMMAND-CANNOT-ACCESS-CONTEXT` | draft | P2 | Python commands do receive a context (AC-13), so the JS gap does not carry over | No | No | None. The Python context wrapper is a reference for the JS fix |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | `ErrorPayload.traceback` already exists. The exception class is carried in the message only | No | No | Same partial coverage as liquers-web, recorded in the conformance matrix |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | Interpreter exit with evaluations in flight | No | No | Mitigated by an `atexit` hook (Error Handling) |

There is no blocker. One gap was found that has no issue yet: a foreign value cannot be
deserialized, because `ExtValue::deserialize_from_bytes` has no arm or hook for a foreign
identifier. This design closes it with an additive hook (Interfaces, `liquers-lib`).

## Interfaces

### liquers-lib (additive)

```rust
// liquers-lib/src/value/foreign.rs
/// Rebuilds a foreign value from bytes written by its `as_bytes`. Plain `fn`: no captured state.
pub type ForeignDecoder = fn(bytes: &[u8], data_format: &str) -> Result<Arc<dyn ForeignValue>, Error>;

/// Registers the decoder for one foreign identifier. Process-global (`OnceLock<RwLock<HashMap>>`),
/// because `ValueInterface::deserialize_from_bytes` is a static method with no environment.
/// Re-registering the same identifier replaces the decoder.
pub fn register_foreign_decoder(identifier: &'static str, decoder: ForeignDecoder);
pub fn foreign_decoder(identifier: &str) -> Option<ForeignDecoder>;
```

`ExtValue::deserialize_from_bytes` consults `foreign_decoder(type_identifier)` before it returns
its unknown-identifier error. The function is sync, as are serialization and the trait it extends.

Per-environment gating stays with the type registry. A pickle-off environment registers
`py.Object` with no data formats, so the write path refuses it. On the read path,
`deserialize_stored_value` checks only `registry.contains`. Phase 2 therefore adds one check in
`liquers-core/src/assets.rs`: a registered type whose `supported_data_formats` lacks the stored
format is treated as *degraded* (`Ok(None)`), the same outcome as an unknown type. Without the
check, a pickle-off environment would unpickle whatever a store held, and the guide forbids pickle
as an implicit trusted format (§6). This is the design's only behavior change in core.

### liquers-py (Rust, `liquers._native`)

```rust
// value.rs: replaces the own enum. Re-exports liquers_lib::value::Value.
pub struct PyObjectValue {
    object: Py<PyAny>,          // Send + Sync; a drop without the GIL is deferred by PyO3
    type_name: String,          // "module.QualName", captured at wrap time
    pickle: bool,               // the creating environment's codec setting
}
impl ForeignValue for PyObjectValue { /* origin "python", identifier "py.Object" */ }
// as_bytes("pickle") takes the GIL and calls pickle.dumps when `pickle` is set, otherwise
// SerializationError. try_into_string uses str(); try_into_json_value refuses.

#[derive(Clone, Copy)]
pub enum ConversionMode { Permissive, Strict }

pub fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>>;   // sync, GIL held
pub fn py_to_value(obj: &Bound<'_, PyAny>, mode: ConversionMode, pickle: bool)
    -> Result<Value, Error>;                                               // sync, GIL held
```

The conversion table is in plan §3.2. In outline:
- `None`, `bool`, `int` (i64, then a `BigInt`-overflow ConversionError), `float`, `str`, `bytes`,
  `list` and `dict[str, …]` map structurally;
- `polars.DataFrame` goes through IPC bytes into `ExtValue::PolarsDataFrame`;
- a `PIL.Image.Image` is PNG-encoded into `ExtValue::Image`;
- `numpy.ndarray` becomes nested lists;
- anything else becomes `py.Object` (permissive) or a ConversionError naming the type (strict).

The `bool` check comes before `int`, because `bool` is a subclass of `int`.

```rust
// environment.rs
#[pyclass(name = "Environment", frozen)]
pub struct PyEnvironment { inner: Arc<EnvCell> }

struct EnvCell {
    state: std::sync::Mutex<EnvState>,     // never held across an await
    store: Arc<dyn AsyncStore>,            // created once, survives rebuilds (AC-17 after AC-10)
    commands: std::sync::Mutex<Vec<PyCommandSpec>>,  // replayed on rebuild
    mode: ConversionMode,
    pickle: bool,
}
enum EnvState {
    Building(EnvironmentBuilder<Value, (), LibKind>),
    Built(EnvRef<DefaultEnvironment<Value>>),
}

#[pymethods] impl PyEnvironment {
    #[new] #[pyo3(signature = (*, conversion = "permissive", pickle = false))]
    fn new(conversion: &str, pickle: bool) -> PyResult<Self>;
    fn validate(&self) -> PyResult<Vec<PyIssue>>;                          // AC-12
    fn evaluate(&self, py: Python<'_>, query: &str) -> PyResult<Py<PyAny>>; // sync
    fn evaluate_async(&self, py: Python<'_>, query: &str) -> PyResult<Py<PyAny>>; // asyncio.Future
    fn store_set(&self, py: Python<'_>, key: &str, data: &Bound<'_, PyAny>) -> PyResult<()>; // AC-17
    fn _register(&self, spec: &Bound<'_, PyDict>, func: Py<PyAny>) -> PyResult<()>;
    fn unregister(&self, name: &str, namespace: &str) -> PyResult<bool>;
}
```

`EnvState` follows liquers-web's lifecycle. Registrations go straight into the builder until the
first evaluation builds the environment. After that, a registration rebuilds the environment and
replays the retained specs, which drops the asset cache. That is correct, because cached assets may
depend on the replaced command. The store `Arc` is reused, so stored data survives.

```rust
// command.rs
struct PyCommandSpec { key: CommandKey, metadata: CommandMetadata,
                       func: Arc<Py<PyAny>>, state: StateMode, is_async: bool }

fn register_py_command(registry: &mut CommandRegistry<DefaultEnvironment<Value>>,
                       spec: &PyCommandSpec) -> Result<Replaced, Error>;
enum Replaced { Nothing, Builtin, Python }  // drives the AC-10 warning
```

Every Python command goes through `registry.register_async_command`, whose returned future runs
as follows:
- **Sync `def`.** The call is wrapped in `tokio::task::spawn_blocking(|| Python::attach(...))`.
  Blocking Python code then never stalls a runtime worker. A nested `context.evaluate` (AC-13)
  runs on that blocking thread, where `Handle::block_on` is allowed, with the GIL released by
  `py.detach`.
- **`async def`.** The closure takes the GIL and calls the function to get a coroutine. It submits
  that coroutine to the liquers-py loop thread with `asyncio.run_coroutine_threadsafe`, and awaits
  a `tokio::sync::oneshot` completed from the concurrent future's done callback.

Arguments are read as in liquers-web, with `args.get_value(i, name)` per declared argument,
converted with `value_to_py` and passed as positional arguments.

```rust
// context.rs: replaces the todo!() wrapper
#[pyclass(name = "Context", frozen)]
pub struct PyContext { inner: Context<DefaultEnvironment<Value>> }
#[pymethods] impl PyContext {
    fn evaluate(&self, py: Python<'_>, query: &str) -> PyResult<Py<PyAny>>;
    fn info(&self, message: &str); fn warning(&self, message: &str); fn error(&self, message: &str);
    fn is_cancelled(&self) -> bool;
}
```

A command receives the context when its signature has a parameter named `context`, the macro DSL's
convention.

```rust
// runtime.rs
fn runtime() -> &'static tokio::runtime::Runtime;    // OnceLock, multi-thread, started lazily
fn py_loop() -> PyResult<Py<PyAny>>;                 // daemon thread running loop.run_forever()
#[pyclass] struct EvaluationFuture;                  // see "asyncio bridge"
```

**asyncio bridge (`evaluate_async`).** The bridge runs these steps:
1. Create `loop.create_future()` on the running loop.
2. Spawn the evaluation on the runtime.
3. On completion, call `loop.call_soon_threadsafe(set_result | set_exception)`, skipping a future
   that is already cancelled.
4. A done callback on the asyncio future calls `AssetRef::cancel()` when the future was cancelled
   (AC-14).

**Where `async def` runs.** It runs on the liquers-py loop thread, which revises Phase 1's
recommendation of the caller's loop. Inside the asset manager there is no route back to the
caller's loop: `Context` carries no Python handle, and its payload slot (`P`) changes caching. The
cost is that a coroutine cannot use objects bound to the caller's loop, such as a client session
created there. That restriction is documented, and a follow-up issue records caller-loop
execution.

### liquers (Python package)

```python
# python/liquers/_commands.py
def command(func=None, *, name=None, namespace="", label=None, doc=None, env=None): ...
def first_command(func=None, **kw): ...     # same, StateMode.NoState
# Builds a declaration from inspect.signature + docstring and calls env._register(spec, func).
# The first positional parameter named state/value/text is the state. `context` is injected.
# A keyword-only parameter or **kwargs raises TypeError naming it (AC-9). *args maps to `multiple`.

# python/liquers/_errors.py
class LiquersError(Exception):
    def __new__(cls, kind, message, **fields): ...  # dispatches to the subclass for `kind`
    kind: ErrorKind; message: str; query: str | None; key: str | None
    position: Position | None; traceback: str | None
class KeyNotFoundError(LiquersError): ...          # one subclass per ErrorType, generated

# python/liquers/__init__.py
default_environment() -> Environment                 # lazily created, shared (AC-11)
evaluate(query) / evaluate_async(query)              # delegate to default_environment()
```

The declaration is built Python-side and finished Rust-side by `CommandDeclaration`:
`from_introspection` → `apply_conventions` → `fill_defaults` → `finish`. The derived label and the
argument rules then match every other integration.

## Integration Points

- **liquers-lib**
  - `src/value/foreign.rs`: the decoder registry.
  - `src/value/mod.rs`: `ExtValue::deserialize_from_bytes` consults the registry.
- **liquers-core**
  - `src/assets.rs` `deserialize_stored_value`: the format check.
- **liquers-py**
  - `Cargo.toml`: depends on `liquers-lib` with the default features minus `egui`, and on `tokio`
    with `rt-multi-thread`.
  - Rewritten: `src/value.rs`, `src/context.rs` and `src/lib.rs`.
  - New: `src/environment.rs`, `src/command.rs` and `src/runtime.rs`.
  - Unchanged: `src/error.rs`, which keeps the `ErrorType` pyclass as `ErrorKind`, plus `query.rs`,
    `metadata.rs`, `command_metadata.rs` and the rest of the M0 wrappers.
  - `src/recipes.rs` drops its use of the own enum.
  - New Python files: `python/liquers/_commands.py`, `_errors.py` and `_env.py`.
  - The `__init__.py` exports grow from 43 names to roughly 55.
- **Tests:** `liquers-py/tests/test_<FEATURE>_*.py`, one file per selected feature, with the guide's
  Appendix A IDs, plus Rust unit tests for the conversion table and the decoder hook.

## Error Handling

| Failure | Constructor | Python sees |
|---|---|---|
| Python exception in a command | `Error::from_error(ErrorType::ExecutionError, "ValueError: bad")` with `traceback` set from `traceback.format_exception` | `ExecutionError` subclass (AC-7) |
| A command raises `LiquersError` | its own `kind`/fields mapped back to `ErrorType` and constructor | that subclass (AC-8) |
| Strict mode, unknown object | `Error::conversion_error(type_name, "Value")` | `ConversionError` (AC-5) |
| `int` outside i64 | `Error::conversion_error_with_message` | `ConversionError` |
| `py.Object` written without pickle | `Error::from_error(ErrorType::SerializationError, …)` | `SerializationError` (AC-6) |
| Unpickling fails | `Error::from_error(ErrorType::SerializationError, e)` | `SerializationError` |
| Keyword-only or `**kwargs` parameter | none (raised in Python) | `TypeError` before registration (AC-9) |
| Invalid configuration | `IssueReport` from `validate()`, then `build()`'s error | list of issues, then `ConfigurationError` (AC-12) |
| Sync `evaluate` on a running loop | none (raised by the binding) | `RuntimeError` naming `evaluate_async` (AC-16) |
| Panic inside a command closure | `Error::unexpected_error` via `catch_unwind` at the closure boundary | `UnexpectedError`, never an abort |
| Interpreter exit with evaluations in flight | none | `atexit` stops the loop thread. Pending Python futures are not completed |

`Error::new` is not used. Every `match` over `ErrorType`, `ExtValue` and `StateMode` is explicit.

## Relevant Commands

No new commands. Python commands register into the default namespace unless `namespace=` is given.
They interact with `liquers-lib`'s root namespace (`to_text` in AC-2) and with `pl` (AC-3).
`repeat(state, count: int = 2)` corresponds to the DSL line
`fn repeat(state, count: i64 = 2) -> result`. Its signature follows
`COMMAND_REGISTRATION_GUIDE.md`, with no location-dependent arguments.

## Documentation Architecture

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (guide, integrators, area `py`):
  - §6 Python is rewritten around the chosen mechanisms;
  - the §5 VALUE paragraph that describes the own-enum design is corrected;
  - a History row is added.
- `liquers-py/README.md` (crate readme, users): quick start, the conformance matrix, and the
  loop-binding limitation of `async def`.
- `specs/reference/VALUE_TYPE_SYSTEM.md` (reference, area `value`): the foreign decoder hook and
  the format check on read, with a History row.
- `specs/README.md`: the entry moves to "complete" in Phase 5.
- `affects_docs`: `[specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md]`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 12 in liquers-py, 2 in liquers-lib, 1 in liquers-core |
| Crates and workflows affected | liquers-py builds liquers-lib, so it is a much larger build that needs the OpenSSL headers. CI's liquers-py row gets longer |
| Existing tests likely to change | the two `test_OBJECT_*` files only. The core format check runs against the existing store and asset suites |
| New validation | pytest per feature ID; Rust unit tests for conversion, the decoder hook and the core format check |
| Compatibility / data / concurrency / performance / security | **Core read check:** a stored value whose type is registered but whose format is not becomes degraded instead of read. That is safe only if every built-in `TypeInfo` lists the formats it reads, which a test over `type_descriptions()` asserts first. **GIL:** each Python command takes the GIL, so Python commands run serially, as documented. **DataFrames** cost one IPC copy per crossing. **Pickle** stays off by default and is gated on both write and read |
| Recovery | each milestone is its own commit, and the core and lib changes are separable and revertible |
| Certainty and open questions | High on value, environment and commands, which follow liquers-web closely. Medium on the asyncio bridge, which is new code: cancellation and interpreter shutdown need the ASYNCQ04/05 tests early. Decision for the owner: running `async def` on liquers-py's loop instead of the caller's |
