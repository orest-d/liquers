# Phase 2: Solution & Architecture — liquers-py

## Overview

`liquers-py` uses `liquers_lib::value::Value`. A Python object with no structural mapping becomes an
`ExtValue::Foreign` holding a `PyObjectValue` (`py.Object`).

`lq.Environment` keeps every Python declaration it receives. It builds a
`DefaultEnvironment<Value>` from them on first use, and rebuilds when a registration arrives late,
as liquers-web's `rebuild_with` does.

Python callables are async closures owning `Arc<Py<PyAny>>`, registered with
`register_async_command`. One process-wide tokio runtime runs every evaluation. The GIL is held
only to convert values and to call Python.

Rejected alternatives:
- **Keeping the own enum.** It cannot hold an `ExtValue` (AC-3).
- **`pycall` aliases.** The owner decided against them on 2026-10-17.
- **`pyo3-polars`.** It ties together three version trains.
- **Holding an `EnvironmentBuilder` in the pyclass.** Its `Box<dyn StoreFactory>` is not `Send`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-VALUE-ENUM-OVERSIZED`, `PY-VALUE-SERIALIZER-IS-A-STUB` | draft | P3, P2 | the own enum is removed | No | No | closed by step 4 |
| `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` | draft | P2 | replaced by closures | No | No | closed by step 7 |
| `FOREIGN-VALUE-CANNOT-BE-DESERIALIZED` | draft | P2 | AC-6 and AC-18 need it | In this design | No | closed by steps 1–2 |
| `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` | draft | P3 | the same read path | No | No | designed around: `store_set` always writes `data_format` |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | a rebuild leaves the old manager alive | No | No | retire it, and shut it down in `shutdown()` |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | the class name rides in the message | No | No | partial, as in liquers-web |

DataFrames cross as Parquet because IPC is refused (`POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE`).

## Interfaces

### `liquers-lib` (additive)

- `pub type ForeignDecoder = fn(&[u8], &str) -> Result<Arc<dyn ForeignValue>, Error>;`
- `pub fn register_foreign_decoder(identifier: &'static str, decoder: ForeignDecoder)` and
  `pub fn foreign_decoder(identifier: &str) -> Option<ForeignDecoder>`.
  - The decoders live in a process-global `RwLock` map, because `deserialize_from_bytes` is static.
  - `ExtValue::deserialize_from_bytes` consults the map before its unknown-identifier error.

### `liquers-core` (one rule)

`deserialize_stored_value` (`assets.rs`) treats a registered type with empty
`supported_data_formats` as degraded (`Ok(None)`). By the write path's own rule such a type has no
byte form, so no built-in type is affected. A pickle-off registry lists `py.Object` without
formats, so that environment never unpickles (AC-18). The degraded error says whether the type is
unregistered or formatless.

### `liquers-py` (Rust)

All of these are synchronous. Each releases the GIL (`py.detach`) around `block_on`.

- `PyObjectValue { object: Py<PyAny>, type_name: String, pickle: bool }`, `impl ForeignValue`:
  - `origin` is `python`, `identifier` is `py.Object` and `default_extension` is `pickle`;
  - `as_bytes("pickle")` calls `pickle.dumps` only when `pickle` is set, otherwise it fails with
    `SerializationError`;
  - `type_info` lists `pickle` only when it is set.
- `py_type_info(pickle: bool) -> TypeInfo`, the instance-free twin, registered through
  `with_type_registry`. The decoder (`pickle.loads`) is registered at module init.
- `value_to_py(py, &Value) -> PyResult<Py<PyAny>>` and
  `py_to_value(&Bound<PyAny>, ConversionMode, pickle) -> Result<Value, Error>`:
  - the conversions are the plan's §3.2 table;
  - `bool` is checked before `int`;
  - a `polars.DataFrame` crosses as Parquet;
  - a `list` or `dict` met again on the current conversion path is refused (VALUE07).
- `enum ConversionMode { Permissive, Strict }`.
- `#[pyclass(frozen)] Environment` wraps `Arc<EnvCell>`, which holds:
  - `Mutex<EnvState>`, where `EnvState` is `Pending | Built(EnvRef) | Shut`;
  - the store `Arc`, built once and kept across rebuilds;
  - the retired `EnvRef`s;
  - the specs it has retained;
  - the `EnvironmentConfig`;
  - the imported metadata.

  `Environment` methods:
  - `new(*, config=None, conversion="permissive", pickle=False)`, where `config` is an
    `EnvironmentConfig` given as a dict or as YAML;
  - `command(...)` and `first_command(...)`;
  - `import_metadata(yaml)`;
  - `validate() -> list[Issue]`;
  - `command_metadata(name, namespace="")`;
  - `evaluate(query)`, `evaluate_state(query) -> State` and `get_asset(query) -> AssetRef`;
  - `store_set(key, data)`;
  - `unregister(name, namespace="") -> bool`;
  - `shutdown()`;
  - `live_handle_count()`, available only under the test-only `debug-handles` feature.
- `State`: a read-only pair of a value and its `Metadata` (VALUE13).
- `AssetRef`: `get()`, `status -> Status`, `metadata` and `cancel()`.
- `Status`: built from every core variant, raising `ValueError` on an unknown name; it gains `__eq__`.
- `CommandMetadata` gains `label`, `doc` and `arguments`; `ArgumentInfo` gains `name` and `default`.
- `register_py_command(&mut CommandRegistry<_>, &PyCommandSpec) -> Result<Replaced, Error>`:
  - it unregisters any existing command first;
  - it sets `impl_version` from a hash of the callable's code, so a replaced command's stored
    results are not reused;
  - `Replaced` (`Nothing | Builtin | Python`) drives the AC-10 warning;
  - state is delivered per `StateDelivery` (`Reserved` counts as `Value`).
- `#[pyclass(frozen)] Context`, valid during its command call:
  - `evaluate(query)` runs `Context::evaluate`, then `wait_for_dependency`;
  - `info`, `warning` and `error` return `PyResult<()>`;
  - `is_cancelled()`.
- `runtime() -> &'static Runtime`: lazy and multi-threaded.

### `liquers` (Python)

- `command` and `first_command` are decorators bound to `default_environment()`, with
  `name`, `namespace`, `label`, `doc`, `volatile`, `cached` and `env` as keyword arguments.
  - The baseline comes from `inspect.signature`, `get_type_hints` and the docstring.
  - Rust finishes it with `CommandDeclaration`.
  - `*args` maps to `multiple`, and a parameter named `context` is injected.
  - A keyword-only parameter or `**kwargs` raises `TypeError`.
- `LiquersError(type, message, **fields)` dispatches to a subclass named after the `ErrorType`
  variant plus `Error` (`KeyNotFoundError`). Its fields are `type`, `message`, `query`, `key`,
  `position` and `traceback`.
- `default_environment()` and the module-level `evaluate`.

## Integration Points

| Crate | Files |
|---|---|
| liquers-lib | `src/value/foreign.rs`, `src/value/mod.rs` |
| liquers-core | `src/assets.rs` |
| liquers-py | `Cargo.toml` (+`liquers-lib`, `liquers-macro`, `tokio`, `futures`, `async-trait`, `serde_json`) |
| liquers-py, rewritten | `src/value.rs` and `src/context.rs` (the old `Environment` there goes) |
| liquers-py, new | `src/environment.rs`, `src/builtins.rs` (domain macros), `src/command.rs`, `src/runtime.rs`, `src/asset.rs` |
| liquers-py, extended | `src/error.rs`, `src/metadata.rs`, `src/command_metadata.rs`, `src/lib.rs`, `README.md` |
| liquers (Python) | `python/liquers/_commands.py`, `_errors.py`, `_env.py`, `__init__.py` |
| scripts | `scripts/test-liquers-py.sh`, `scripts/check-build-matrix.sh` |

## Error Handling

| Failure | Constructor → Python |
|---|---|
| A Python exception, including a `BaseException` | `Error::execution_error(msg).with_traceback(tb)` → `ExecutionError` |
| A command raises `LiquersError` | its own `ErrorType` and fields |
| Strict mode meets an unknown object; an `int` beyond i64; a cycle | `Error::conversion_error` |
| Pickle is off, or pickling fails | `Error::from_error(ErrorType::SerializationError, …)` |
| Invalid metadata | `IssueReport`, then `ConfigurationError` |
| A panic in a command | `JoinError::is_panic` → `Error::unexpected_error` |
| Use after `shutdown()` | `Error::general_error` → `GeneralError` |
| A pickle-off read of `py.Object` bytes | degraded; the error names the identifier |

Every match over `ErrorType`, `StateDelivery` and `ExtValue` is explicit.

## Relevant Commands

There are no new commands. A Python command goes to the root namespace unless `namespace=` is set.
The commands they meet here are `to_text`, `commands_doc` and `pl/head`. The DSL equivalent of
`repeat` is `fn repeat(text, count: i64 = 2) -> result`.

## Documentation Architecture

`affects_docs`, each gaining a History row:
- `guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 is rewritten and the §5 VALUE paragraph corrected.
- `reference/VALUE_TYPE_SYSTEM.md`: the decoder hook.
- `reference/ASSETS.md`: the formatless read rule.

Also updated: `liquers-py/README.md` (quick start and the matrix of `NA`s), `CLAUDE.md`
(liquers-py notes) and `specs/README.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 22, almost all in liquers-py |
| Crates and workflows affected | liquers-py builds liquers-lib without `egui` (about 8–10 GB with the routine loop). Its matrix row becomes the heaviest |
| Existing tests likely to change | the two `test_OBJECT_*` files |
| New validation | per-feature pytest suites; Rust tests for the decoder hook, the read rule and `py_type_info` agreement |
| Compatibility | `ErrorType` keeps its name. The plan-level API changes are in the Phase 1 log |
| Data | pickle is gated on both write and read |
| Concurrency | Python commands serialize on the GIL; the wait releases it |
| Performance | one Parquet encode per DataFrame crossing |
| Memory | retired environments live until `shutdown()` |
| Recovery | one commit per step |
| Certainty | high; see the Phase 1 Design Readiness |
