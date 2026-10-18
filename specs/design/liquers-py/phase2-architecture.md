# Phase 2: Solution & Architecture — liquers-py

## Overview

`liquers-py` uses `liquers_lib::value::Value`. Python objects with no structural mapping become an
`ExtValue::Foreign` holding a `PyObjectValue` (`py.Object`).

`lq.Environment` keeps the Python declarations it has received and builds a
`DefaultEnvironment<Value>` from them on first use, then rebuilds it whenever a registration
arrives late. This follows liquers-web's `rebuild_with`.

Python callables are async closures owning `Arc<Py<PyAny>>`, registered with
`register_async_command`. One process-wide tokio runtime runs every evaluation. The GIL is held
only to convert and to call Python.

Rejected alternatives:
- **Keep the own enum.** It cannot hold `ExtValue` (AC-3).
- **`pycall` aliases.** The owner decided against them on 2026-10-17.
- **`pyo3-polars`.** It couples three version trains.
- **Keep an `EnvironmentBuilder` inside the pyclass.** The builder holds a
  `Box<dyn StoreFactory>` with no `Send` bound, so the pyclass could not be `Sync`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-VALUE-ENUM-OVERSIZED`, `PY-VALUE-SERIALIZER-IS-A-STUB` | draft | P3, P2 | own enum removed | No | No | closed by step "value" |
| `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` | draft | P2 | replaced by closures | No | No | closed by step "commands" |
| `FOREIGN-VALUE-CANNOT-BE-DESERIALIZED` | draft | P2 | AC-6 needs it | In this design | No | closed by this design |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | a rebuild leaks the old manager (an `Arc` cycle, `assets.rs` `DefaultAssetManager`) | No | No | call the old manager's `shutdown()` on rebuild |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | the class name rides in the message, the stack in `traceback` | No | No | partial, as in liquers-web; recorded in the matrix |
| `POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE` | draft | P3 | DataFrames use Parquet | No | No | none |

## Interfaces

### `liquers-lib` (additive)

- `pub type ForeignDecoder = fn(&[u8], &str) -> Result<Arc<dyn ForeignValue>, Error>;`
- `pub fn register_foreign_decoder(identifier: &'static str, decoder: ForeignDecoder)`
  - a global `RwLock<HashMap>`, because `deserialize_from_bytes` is static and has no environment;
  - lock poisoning maps to `Error::unexpected_error`.
- `pub fn foreign_decoder(identifier: &str) -> Option<ForeignDecoder>`
  - consulted by `ExtValue::deserialize_from_bytes` (`value/mod.rs`) before its unknown-identifier
    error.

### `liquers-core` (one rule)

`assets.rs` `deserialize_stored_value` gets one rule. A registered type whose
`supported_data_formats` is empty returns `Ok(None)` (degraded). A formatless type has no byte form
by the write path's own rule (`assets.rs:934`), so this changes nothing for built-ins. Its purpose is
that a pickle-off environment, whose registry lists `py.Object` with no formats, never unpickles
(AC-18). The degraded-read error says which case applies: unregistered, or registered without a
byte form.

### `liquers-py` (Rust)

- **`PyObjectValue { object: Py<PyAny>, type_name: String, pickle: bool }`**, `impl ForeignValue`.
  - `origin` is `"python"`, `identifier` is `"py.Object"`, `default_extension` is `pickle`.
  - `as_bytes("pickle")` calls `pickle.dumps` under the GIL when `pickle` is set, and returns
    `SerializationError` otherwise.
  - `type_info` lists `pickle` only when `pickle` is set.
- **`pub fn py_type_info(pickle: bool) -> TypeInfo`** is the instance-free twin of `type_info`, and a
  unit test asserts that the two agree.
  - The environment registers it with `with_type_registry`.
  - The decoder (`pickle.loads`) is registered once, at module init.
- **`value_to_py(py, &Value) -> PyResult<Py<PyAny>>`** and
  **`py_to_value(&Bound<PyAny>, ConversionMode, pickle) -> Result<Value, Error>`** are sync and need
  the GIL.
  - The table is plan §3.2, with `bool` checked before `int`.
  - `polars.DataFrame` crosses as Parquet bytes into `ExtValue::PolarsDataFrame`.
  - A `list` or `dict` already on the current conversion path (an identity set) raises
    `Error::conversion_error` naming the cycle (VALUE07). Shared references that are not cycles
    are copied.
- **`enum ConversionMode { Permissive, Strict }`**
- **`#[pyclass(frozen)] Environment { inner: Arc<EnvCell> }`**
  - `EnvCell` holds:
    - `Mutex<EnvState>`, where `EnvState` is `Pending`, `Built(EnvRef<DefaultEnvironment<Value>>)`
      or `Shut`;
    - the store `Arc<dyn AsyncStore>`, built once from `config.store` (a memory store when the
      configuration names none) and passed to every rebuild with `with_async_store`, so data
      survives rebuilds;
    - the retired `EnvRef`s of earlier builds: in-flight evaluations finish against them, and
      `shutdown()` shuts their managers down (the `Arc` cycle in `DefaultAssetManager` otherwise
      keeps them alive);
    - the retained `Vec<PyCommandSpec>`;
    - the `EnvironmentConfig` (store, recipes, `assets: AssetManagerOptions`) and the imported
      command metadata.
  - No lock is held across an await.
  - `build` runs under `runtime().enter()`, because the queued manager spawns tasks.
  - `build` registers the liquers-lib domains on `builder.command_registry` with
    `register_core_commands!`, `register_image_commands!`, `register_polars_commands!`,
    `register_records_commands!`, `register_search_commands!` and `register_template_commands!`,
    with `type CommandEnvironment = DefaultEnvironment<Value>`. It does not use
    `register_all_commands!`, which needs `UIPayload`.
- **Environment methods**, all sync. Each releases the GIL (`py.detach`) around `block_on`:
  - `new(*, config: dict | str | None = None, conversion = "permissive", pickle = False)`, where
    `config` is an `EnvironmentConfig` as a dict or a YAML string
  - `command(...)` and `first_command(...)`: the decorators bound to this environment
  - `import_metadata(yaml: str)`: a `CommandMetadataRegistry` document merged into the registry
    at build (ENVIRON08)
  - `command_metadata(name, namespace="") -> CommandMetadata | None`; the existing wrapper gains
    `label`, `doc` and `arguments` getters
  - `validate() -> list[Issue]`
  - `evaluate(query) -> object`
  - `evaluate_state(query) -> State`
  - `get_asset(query) -> AssetRef`
  - `store_set(key, data)` calls `AsyncStore::set` with a `MetadataRecord` carrying the key,
    `type_identifier`, an explicit `data_format` and `Status::Source` (the read path keeps only
    data-bearing statuses). `str` is `Text`/`txt`, `bytes` is `Bytes`/`b`, anything else goes
    through `py_to_value` in its type's default format (`pickle` for `py.Object`).
  - `unregister(name, namespace="") -> bool`
  - `shutdown()`
  - `live_handle_count() -> int`, only under the test-only `debug-handles` feature, which
    `scripts/test-liquers-py.sh` passes to `maturin develop` (RUNTIME05)
- **`#[pyclass(frozen)] State`** holds a value and a `Metadata` and is read-only (VALUE13: nothing is
  discarded, nothing is mutable).
- **`AssetRef`** offers `get() -> State`, `status -> Status`, `metadata` and `cancel()`, which
  runs `cancel()` on the runtime. The existing `Status` wrapper gains `__eq__` and `name`.
- **`register_py_command(&mut CommandRegistry<_>, &PyCommandSpec) -> Result<Replaced, Error>`**
  - `PyCommandSpec { key, metadata: CommandMetadata, func: Arc<Py<PyAny>>, delivery: StateDelivery }`
  - It calls `registry.unregister(key)` first, so a replacement drops the old executor and its
    `impl_version`. `Replaced` (`Nothing | Builtin | Python`) drives the AC-10 warning.
  - The closure runs `spawn_blocking(|| Python::attach(...))`. It reads arguments with
    `args.get_value(i, name)` and delivers the state per `StateDelivery`: `State`, `Value`, `Text`
    or `None`, with `Reserved` treated as `Value`.
- **`#[pyclass(frozen)] Context`**, valid only during its command call:
  - `evaluate(query) -> object` runs `ctx.evaluate(q).await` then `ctx.wait_for_dependency`, via
    `Handle::block_on` on the blocking thread with the GIL released;
  - `info`, `warning` and `error` return `PyResult<()>`;
  - `is_cancelled`.
- **`runtime() -> &'static Runtime`**: a lazily created multi-thread runtime in a `OnceLock`.

### `liquers` (Python)

- `command(func=None, *, name=None, namespace="", label=None, doc=None, volatile=None, cached=None, env=None)`
  and `first_command(...)`.
  - These build a baseline from `inspect.signature`, `typing.get_type_hints` and the docstring.
  - Rust finishes it through `CommandDeclaration::from_introspection` → `enhance` →
    `apply_conventions` → `fill_defaults` → `finish`.
  - `*args` maps to `multiple`, and a parameter named `context` is injected.
  - A keyword-only parameter or `**kwargs` raises `TypeError` naming it.
- `LiquersError(kind, message, **fields)` dispatches to one subclass per `ErrorType`. Its fields are
  `kind`, `message`, `query`, `key`, `position` and `traceback`.
- `default_environment()` is lazy and shared, and module-level `evaluate` delegates to it.

## Deviations from the plan of record

| Plan item | Here |
|---|---|
| M3 async | moved to `liquers-py-async` |
| `config={"codecs": {"pickle": true}}` | `pickle=` keyword argument; `config` carries the environment configuration |
| `liquers.Value` scalar operators (VALUE12) | NA, Phase 1 question 4 |
| `debug-handles` counter (RUNTIME05) | kept: a test-only feature counting live `PyObjectValue`s and callables |
| `language_context` in core | not done: `traceback` already exists and the class rides in the message |
| M1 recipe thin path | moved to the STORE/RECIPE design, with the default recipe provider unchanged |
| `reset()` | dropped: a new `Environment()` is the reset, and ENVIRON03's policy is documented |

## Integration Points

| Crate | Files |
|---|---|
| liquers-lib | `src/value/foreign.rs` (registry), `src/value/mod.rs` (lookup) |
| liquers-core | `src/assets.rs` `deserialize_stored_value` |
| liquers-py | `Cargo.toml` (+`liquers-lib`, `tokio`); rewrite `src/value.rs`, `src/context.rs`, `src/lib.rs`; new `src/environment.rs`, `src/command.rs`, `src/runtime.rs`, `src/asset.rs`; `src/recipes.rs` drops the own enum |
| liquers (py) | `python/liquers/_commands.py`, `_errors.py`, `_env.py`, `__init__.py` |

## Error Handling

| Failure | Constructor → Python |
|---|---|
| Python exception in a command | `Error::execution_error(msg).with_traceback(tb)` → `ExecutionError` |
| A command raises `LiquersError` | its `ErrorType` and fields, rebuilt with the typed constructor |
| `BaseException` (`KeyboardInterrupt`, `SystemExit`) | `Error::execution_error`, with the class named; never swallowed silently (ERROR04) |
| Strict mode, unknown object; `int` beyond i64 | `Error::conversion_error` |
| Pickle off, or pickling fails | `Error::from_error(ErrorType::SerializationError, …)` |
| Invalid configuration | `IssueReport` → `validate()`, then `ConfigurationError` |
| Panic in a command | the `spawn_blocking` `JoinError::is_panic` → `Error::unexpected_error`; PyO3 turns a panic at the binding boundary into `PanicException` |
| Use after `shutdown()` | `Error::general_error("environment is shut down")` |
| Pickle-off read of `py.Object` bytes | degraded read; the error names the identifier and says the type has no byte form in this environment |

All matches over `ErrorType`, `StateDelivery` and `ExtValue` are explicit.

## Relevant Commands

No new commands. Python commands go to the root namespace unless `namespace=` is given. They meet
`to_text` (root) and `pl/head` (AC-3). `repeat` corresponds to
`fn repeat(text, count: i64 = 2) -> result`.

## Documentation Architecture

`affects_docs`:
- `guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 rewritten, the §5 VALUE paragraph corrected, a History
  row added;
- `reference/VALUE_TYPE_SYSTEM.md`: the decoder hook, with a History row;
- `reference/ASSETS.md`: the formatless read rule, with a History row.

Also `liquers-py/README.md` (quick start, matrix with NAs) and `specs/README.md` (status).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 14: 12 in liquers-py, 1 in liquers-lib, 1 in liquers-core |
| Crates and workflows affected | liquers-py now builds liquers-lib, so the build is larger and its matrix row slower |
| Existing tests likely to change | the two `test_OBJECT_*` files only |
| New validation | pytest per feature ID; Rust tests for conversion, the decoder hook, `py_type_info` agreement and the read rule |
| Compatibility / data / concurrency / performance / security | Rebuilds retire, not stop, the old environment: memory grows with the number of late registrations until `shutdown()`. Cyclic containers are refused. Two Python threads evaluating at once both make progress, since the GIL is released while waiting. Renaming nothing in the Python API: `ErrorType` keeps its name. Python commands serialize on the GIL. A DataFrame costs one Parquet encode per crossing. Pickle is gated on write and on read |
| Recovery | each step is its own commit; the core and lib changes revert independently |
| Certainty and open questions | high; see Phase 1 Design Readiness |
