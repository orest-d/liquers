# Python Wrapper High-Level Design

Status: Draft  
Date: 2026-02-23

## Purpose
Define architecture for a new Python wrapper around Liquers that supersedes the obsolete `liquers-py` crate while reusing its proven patterns:
1. Python value bridging (`PyObject` <-> Liquers `Value`),
2. Rust/Python error conversion,
3. Python function registration as Liquers commands,
4. Python-first test suite.

## Scope
1. Runtime value model extension with Python-native values.
2. Conversion API between Python values and Liquers values, including existing `ExtValue` variants.
3. Error mapping between `liquers_core::error::Error` and Python exceptions.
4. Command registration API for Python callables (based on existing PoC `pycall` pattern).
5. Test architecture for Rust + Python integration testing.

## Out of Scope (for initial version)
1. Full UI object model transport for `Widget` / `UIElement`.
2. Distributed execution/session protocol.
3. Stable binary compatibility guarantees across Python major versions.

## Existing Patterns from `liquers-py` (obsolete but useful)
From `liquers-py/src/value.rs`, `liquers-py/src/error.rs`, `liquers-py/src/commands.rs`:
1. Value PoC already carries `Py<PyAny>` (`Value::Py`) and basic conversion helpers.
2. Error PoC maps core `ErrorType` enum to/from Python-visible enum and raises `PyException`.
3. Command PoC (`pycall`) shows:
   1. dynamic Python module/function dispatch,
   2. parameter extraction from plan arguments,
   3. state passing modes (`no`, `pyobject`, `value`, `state`).
4. Gaps in PoC:
   1. multiple `todo!()` paths in parameter handling,
   2. incomplete `ExtValue` interoperability,
   3. generic `PyException` usage instead of typed exception hierarchy,
   4. incomplete serialization strategy for Python-only values.

## Target Architecture

### 1. Crate Structure
Proposed crate: `liquers-python` (new crate; `liquers-py` kept only for reference during migration).

Suggested modules:
1. `python_value.rs`: Python value extension type + conversion logic.
2. `python_error.rs`: exception hierarchy + bidirectional error conversion.
3. `python_commands.rs`: callable registry and command adapter.
4. `python_api.rs`: `#[pymodule]` exports and Python-facing classes/functions.
5. `python_tests/`: pytest integration tests.

### 2. Value Model
Requirement: support a special value extension that can wrap arbitrary extension types (for example `liquers_lib::value::ExtValue`) and add Python-native values.

Proposed generic extension:
```rust
pub enum PythonValueExtension<E> {
    RustExt { value: E },
    PythonObject { value: pyo3::Py<pyo3::PyAny> },
}
```

And concrete wrapper types:
1. `type PyValueSimple = CombinedValue<SimpleValue, PythonValueExtension<()>>;`
2. `type PyValueExt = CombinedValue<SimpleValue, PythonValueExtension<liquers_lib::value::ExtValue>>;`

Notes:
1. This is aligned with `ExtValue` design (enum-based extension), but composable over arbitrary extension `E`.
2. `RustExt` keeps all existing Liquers extension variants unchanged.
3. `PythonObject` acts as an escape hatch for Python-only runtime objects.

### 3. Value Conversion API
Provide explicit, symmetric conversion functions:
1. `pyobject_to_value(py: Python, obj: &Bound<PyAny>) -> Result<PyValueExt, Error>`
2. `value_to_pyobject(py: Python, value: &PyValueExt) -> PyResult<PyObject>`

Conversion policy:
1. Primitive mappings:
   1. `None` <-> `none`,
   2. `bool/int/float/str/bytes` <-> scalar/bytes values,
   3. `list/tuple` <-> array,
   4. `dict[str, ...]` <-> object.
2. Existing `ExtValue` support (required):
   1. `Image`:
      1. to Python: prefer `PIL.Image.Image` when Pillow available; fallback to `bytes`/opaque wrapper.
      2. from Python: accept `PIL.Image.Image` and `numpy.ndarray` (RGB/RGBA) where available.
   2. `PolarsDataFrame`:
      1. to Python: Python Polars DataFrame if available, fallback via Arrow/IPC bytes.
      2. from Python: accept Python Polars DataFrame and convert to Rust Polars DataFrame.
   3. `UiCommand`, `Widget`, `UIElement`:
      1. to Python: opaque wrapped handle (not eagerly serialized),
      2. from Python: not required initially unless explicit adapter exists.
3. Unknown Python objects:
   1. store as `PythonObject { Py<PyAny> }`.
4. Conversion options:
   1. strict mode (error on unknown object),
   2. permissive mode (fallback to `PythonObject`).

Serialization constraints:
1. `PythonObject` is non-serializable by default.
2. Serializer must return `SerializationError` unless a configured codec exists (e.g., pickle/json adapter).
3. Metadata warning should indicate non-serializable Python object payload.

### 4. Error Conversion
Bidirectional mapping is required.

Rust -> Python:
1. Define Python exception hierarchy:
   1. `LiquersError(Exception)`
   2. type-specific subclasses (`LiquersParseError`, `LiquersParameterError`, `LiquersExecutionError`, etc.).
2. Map `liquers_core::error::ErrorType` to subclasses.
3. Include structured payload on exception object:
   1. `error_type`,
   2. `message`,
   3. `position` (if available),
   4. serialized metadata/log context where relevant.

Python -> Rust:
1. `PyErr` -> `liquers_core::error::Error` mapping utility:
   1. default `ExecutionError`,
   2. include Python exception class name and message,
   3. attach traceback string in error message/context.
2. Preserve original cause chain where possible.

### 5. Python Command Registration
Need first-class registration of Python callables as Liquers commands (PoC exists in `pycall`).

Proposed API surface:
1. Python:
   1. decorator `@liquers.command(...)` for static registration,
   2. runtime API `registry.register_py_function(name, func, metadata=...)`.
2. Rust:
   1. store callable as `Py<PyAny>` in command adapter state,
   2. invoke under GIL inside command execution bridge.

Argument binding strategy:
1. Resolve Liquers plan args first (same as Rust commands).
2. Bind by declared command metadata order.
3. Support injected args:
   1. `context`,
   2. optionally `state`,
   3. future injected providers.
4. Support variadic `*args` mapping from multiple Liquers parameters.

Execution model:
1. Synchronous Python callable:
   1. run in `spawn_blocking` + GIL section (do not block async runtime worker).
2. Async Python callable:
   1. phase 1: out of scope or explicit `NotSupported`,
   2. phase 2 option: integrate with `pyo3-asyncio`.

Result handling:
1. Python return object converted via `pyobject_to_value`.
2. Exceptions mapped via Python -> Rust error conversion.

### 6. Metadata and Type Identity
For Python-origin values:
1. `type_identifier` values:
   1. `"python_object"` for opaque Python value,
   2. existing identifiers for mapped Rust/ExtValue types (`image`, `polars_dataframe`, etc.).
2. `type_name`:
   1. for opaque Python object, Python class name if available.
3. `data_format`:
   1. `python_object` values default to non-serializable unless codec attached.

## Architecture Flow (Runtime)
1. Python code calls Liquers evaluate/register API.
2. Wrapper converts Python inputs -> Liquers value model (`PyValueExt`).
3. Liquers core executes plan/commands.
4. If command is Python callable:
   1. command bridge acquires GIL and invokes callable,
   2. converts arguments and result.
5. Errors converted both directions as crossing happens.
6. Final result converted Liquers value -> Python object.

## Test Suite Design

### 1. Rust Unit Tests (crate-local)
1. Value conversion:
   1. primitive roundtrip,
   2. array/object roundtrip,
   3. unknown object -> `PythonObject`,
   4. strict mode rejects unknown object.
2. `ExtValue` conversion:
   1. image conversion in/out,
   2. polars dataframe conversion in/out,
   3. unsupported UI types produce clear errors/wrappers.
3. Error mapping:
   1. every `ErrorType` maps to expected Python exception class,
   2. `PyErr` conversion preserves message and class name.
4. Command bridge:
   1. callable invoked with bound args,
   2. injected context available,
   3. Python exception -> Liquers `ExecutionError`.

### 2. Python Integration Tests (`pytest`)
Structure: `liquers-python/tests/`.

Required suites:
1. `test_value_conversion.py`
2. `test_error_conversion.py`
3. `test_command_registration.py`
4. `test_command_execution.py`
5. `test_extvalue_interop.py`

Scenarios:
1. register Python function, execute via query, assert result.
2. state/context injection behavior.
3. exception raised in Python function produces typed Liquers error.
4. image and dataframe roundtrips through evaluate/store boundary.
5. non-serializable Python object behavior and metadata warning.

Tooling:
1. Build/install wrapper in test env (e.g., `maturin develop` or wheel install).
2. CI matrix:
   1. Linux/macOS,
   2. selected Python versions,
   3. optional extras for Pillow/Polars tests.

## Migration Plan (High-Level)
1. Keep `liquers-py` unchanged as reference; implement new crate in parallel.
2. Implement value/error conversion core first.
3. Add command registration bridge and minimal decorator API.
4. Port selected PoC tests to pytest.
5. Mark old crate deprecated once parity goals are met.

## Open Design Decisions
1. Name/location of new crate (`liquers-python` vs modernized `liquers-py`).
2. Default conversion mode: strict vs permissive fallback to `PythonObject`.
3. Required optional dependencies in baseline (`Pillow`, `polars`, `numpy`).
4. Async Python callable support in V1 or deferred.
5. Whether to allow pickle-based serializer for `PythonObject` in baseline.

## Acceptance Criteria
1. New wrapper design supports generic extension composition (`PythonValueExtension<E>`).
2. Bidirectional conversion functions exist and cover primitives + `ExtValue` core variants.
3. Rust/Python error conversion is typed and symmetric.
4. Python functions can be registered and executed as Liquers commands.
5. Automated pytest suite validates end-to-end behavior.

## TODO

Both sync and async python commands should be supported.
- Investigate the https://github.com/orest-d/liquer/blob/master - This is the older python version of the currently implemented system.
- Specifically, check https://orest-d.github.io/liquer/site/apidocs/liquer/commands.html and/or https://github.com/orest-d/liquer/blob/master/liquer/commands.py.  Commands are registered in python with command or first_command decorators. These use command_metadata_from_callable to extract information for construction of command metadata from the signature of the python function, they also extract the doc string. Include the similar logic of mapping info from function signature to command metadata. Create a table showing this mapping, include the complete CommandMetadata structure fields and indicate which information is available or missing in the function signature.
- Design first_command and command  functions, that would act as python decorators to register python commands in the liquers CommandMetadataRegistry.
