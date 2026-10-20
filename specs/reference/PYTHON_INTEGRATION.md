---
title: Python Integration
kind: reference
audience: both
area: [py, core/value, core/error, lib/value]
reviewed: 2026-10-19
---
# Python Integration

How `liquers-py` connects Python to Liquers: which Rust types it exposes, how values, commands and
errors cross the boundary, and how it waits, threads and shuts down. This is the contract the code
implements at HEAD. How to *use* the package is
[`guides/PYTHON_GUIDE.md`](../guides/PYTHON_GUIDE.md); the general rules for any language are
[`guides/LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md), whose §6 Python
records the choices made here and why. The conformance matrix, by feature ID, is in
`liquers-py/README.md`; the design is [`design/liquers-py/`](../design/liquers-py/DESIGN.md).

## 1. Layout

| Part | Where | Role |
|---|---|---|
| Rust crate `liquers-py` | `liquers-py/src/` | Builds the native extension `liquers._native` with PyO3. |
| Python package `liquers` | `liquers-py/python/liquers/` | Re-exports the extension by name and adds the parts written in Python. |
| Distribution `liquers-py` | `liquers-py/pyproject.toml` | Built by maturin. |

The package's own modules:

| Module | Contents |
|---|---|
| `__init__.py` | Imports every class and function of `_native` **by name**, and lists them in `__all__`. A class registered in `src/lib.rs` is not public until it is added here too. |
| `_env.py` | `default_environment()` (one `Environment`, created lazily under a lock) and `evaluate()`. |
| `_commands.py` | The decorators, and declaration stage 1 (§4). |
| `_errors.py` | `LiquersError` and one subclass per `ErrorType`, generated at import (§6). |

Cargo features: `extension-module` (default) leaves the Python symbols to the loading interpreter,
which a wheel needs and a Rust test binary cannot link with, so Rust tests run with
`--no-default-features`. `debug-handles` is test-only: it adds `Environment.live_handle_count()`
and the `test_panic` command. `liquers-lib` is linked with `image-support`, `polars`, `records`,
`records-ipc`, `records-parquet` and `templates`; `egui` is not linked, since a Python process has
no egui event loop.

Every class reports `__module__ == "liquers"`, so type names in messages and default reprs read
`liquers.X`, not `liquers._native.X`. The classes cannot be pickled.

## 2. Exposed types

Each class wraps one Rust value. `Environment`, `Context`, `State`, `AssetRef` and `Issue` are
frozen; the others hold their own copy of the value.

| Python | Wraps |
|---|---|
| `Query`, `Key`, `ActionRequest`, `ActionParameter`, `ResourceName`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySource`, `Position` | the `liquers_core::query` / `parse` types; `parse` and `parse_key` call the core parser |
| `CommandMetadata`, `CommandKey`, `ArgumentInfo`, `ArgumentType`, `EnumArgument`, `EnumArgumentType`, `CommandDefinition`, `CommandPreset`, `CommandMetadataRegistry` | `liquers_core::command_metadata` |
| `Metadata`, `MetadataRecord`, `AssetInfo`, `Status`, `LogEntry`, `LogEntryKind`, `Version`, `DependencyKey`, `DependencyRecord` | `liquers_core::metadata` |
| `Plan`, `Step`, `ParameterValue`, `ResolvedParameterValues`, `build_plan` | `liquers_core::plan` |
| `Recipe`, `RecipeList` | `liquers_core::recipes` |
| `Expires`, `ExpirationTime`, `DependencyRelation`, `PlanDependency` | expiration and dependency types |
| `ErrorType`, `Error` | `liquers_core::error`; `Error` is a data object, not the exception (§6) |
| `Environment`, `Issue` | the environment handle (§3) and one `IssueReport` finding |
| `Context`, `AssetRef` | a command's context and an asset reference, each watching its environment's shut flag |
| `State` | a snapshot of a value, converted to Python, and a copy of its metadata |

`Status` compares equal to its variant's name (`asset.status == "Ready"`) and hashes as that
string. `Key[i]` accepts negative indices and raises `IndexError` out of range.

## 3. The environment

**Type.** Every handle builds `PyEnvironment = liquers_lib::environment::DefaultEnvironment<Value>`
with `liquers-lib`'s `Value` and payload `()`, from `liquers_lib::environment::default_environment_builder`.
The type registry is `Value`'s, plus `py.Object` (§5).

**Construction.** `Environment(*, config=None, conversion="permissive", pickle=False)`. `config` is
an `EnvironmentConfig` (`reference/ENVIRONMENT_CONFIG.md`), as a YAML string or as a `dict`
(converted through JSON). The constructor refuses an unknown `conversion` and any `recipes` other
than `default` with `ConfigurationError`. Nothing else is built yet. The pickle codec is a
keyword rather than a `config` entry because `EnvironmentConfig` has no codec section. There is
no `reset()`: a new `Environment()` is a fresh environment.

**Declarations.** The handle holds a lock-protected record of everything declared on it: the
Python command specs in registration order (at most one per key), the keys `unregister` removed,
and the command metadata `import_metadata` added. A build applies them in this order:

1. `liquers-lib`'s built-in commands (core, image, polars, records, search, templates);
2. the removals;
3. the Python specs;
4. the imported metadata. An entry naming no registered command is a validation error.

The builder's own validation runs next; any error-level issue fails the build with
`ConfigurationError` summarising them. `validate()` reports the same issues without building;
it does not construct the store, so a store that cannot be built is reported only by the first
use.

**Store.** The first build constructs the store: an `AsyncMemoryStore` at the root when
`config.store.stores` is empty, otherwise the configured router through
`liquers_store::store_factory::default_store_factory()`. The asset manager takes `config.assets`.
The recipe provider is the builder's default chain.

**Rebuild.** The environment is built on first use. A later `command`, `first_command`,
`unregister` or `import_metadata` marks it for a rebuild, and the next use builds a new
environment from all declarations **with the same store**. The replaced environment is moved to a
retired list, not dropped, because evaluations may still be running in it; its asset cache does
not carry over.

**Shutdown.** `shutdown()` stops the asset managers of the current and every retired environment,
and sets a shared *shut* flag that every wait started from the handle watches (§7). It is
idempotent. Afterwards every method that would use or change the environment raises
`GeneralError`, registration included; `validate()` still answers. An `AssetRef` taken before
shutdown still returns a result that is already ready, and a wait still outstanding raises
`Cancelled`. Neither shutdown nor dropping the handle frees a built
environment (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`).

**Default environment.** `lq.default_environment()` is one `Environment()`, made on first call
under a lock.

**Methods.**

| Method | Does |
|---|---|
| `evaluate(query)`, `evaluate_state(query)` | evaluates and waits; the value, or an `lq.State` |
| `get_asset(query)` | starts evaluating; an `lq.AssetRef` |
| `store_set(key, data)` | writes a value; `key` is a `str` |
| `command(fn=None, **options)`, `first_command(…)` | the decorators bound to this environment |
| `unregister(name, namespace="")` | removes a command (§4) |
| `command_metadata(name, namespace="")` | the `CommandMetadata` the built environment holds |
| `import_metadata(yaml)` | adds command metadata from a document with a `commands` list; only an unparseable document raises here, other problems surface through `validate()` and the next build |
| `validate()` | the issues the next build would meet, as `lq.Issue` |
| `shutdown()` | stops the environment |
| `live_handle_count()` | static, `debug-handles` only: Python callables held by every environment's registrations |

`query` is a `str` or an `lq.Query` everywhere.

**Writing to the store.** `store_set(key, data)` converts `data` (§5), serializes it in its
default data format — raising `SerializationError` when it has no byte form — and sets it through
the asset manager with that data format recorded.

## 4. Commands

A Python command is an ordinary async command whose registered closure owns the callable, as in
`liquers-web`; there is no `pycall` alias.

**Declaration.** The decorators implement the pipeline of
[`reference/COMMAND_DECLARATION.md`](COMMAND_DECLARATION.md). Stage 1 is in `_commands.py`: it
reads `inspect.signature` and `typing.get_type_hints` into a *baseline* document, and passes the
decorator's keywords as the author's *declaration*. Stages 2–5 run in Rust
(`CommandDeclaration::from_introspection`, `enhance`, `finish`).

| Python parameter | Baseline |
|---|---|
| first parameter of a `command` | the state; its name sets the delivery: `state` → `lq.State`, `text` → `str`, any other → the value |
| `first_command` | a leading `{"name": "none"}` entry, so no state is delivered and the first parameter is an argument |
| named `context` | not an argument; its position among the positional parameters is recorded |
| `*name` | `multiple: true` |
| annotation `str` / `int` / `float` / `bool` | `string` / `int` / `float` / `bool` |
| `Optional[int]` / `Optional[float]` / `Optional[bool]` | `int_opt` / `float_opt` / `bool_opt` |
| other annotation | `any`; an unresolvable forward reference drops every annotation of the function |
| default | `None`, `str`, `int`, finite `float` or `bool`; anything else raises `TypeError` |
| docstring | `doc`, unless `doc=` is given |

Refused with `TypeError` before anything is registered: a keyword-only parameter, `**kwargs`,
`context` after `*args`, a callable `inspect.signature` cannot read, and a callable with no
`__name__` and no `name=`. The decorator keywords are `name`, `namespace`, `label`, `doc`,
`filename`, `volatile`, `cached` and `env`. The pipeline's warnings are issued as Python warnings
at the caller's frame, except `ReservedStateDelivery`: a state parameter of any name delivering the
value is the Python convention.

**Metadata.** The finished `CommandMetadata` has `module = "python"`, which is how a registration
tells a Python command it replaces from a built-in one. Its `impl_version` hashes the function's
code object (`co_code`, `co_consts`, `co_names`), so changing the code invalidates stored results.
A closure, a bound method, or a callable with no code object gets a unique version on every
registration instead, because its result depends on state the code object does not show.

**Reserved names.** `ns` is refused as a command name by core. `q` and `v` are planner
instructions too, but are not refused, so a command with either name registers and never runs
(`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`).

**Registration.** Registering replaces whatever is registered under the key: the old command is
unregistered first, and the decorator warns `command 'x' replaces a built-in command` (or a
`Python` one). `unregister(name, namespace="")` removes the top command for the key and returns
whether there was one. A Python spec under the key is removed first, which restores the built-in
it replaced; with no spec, a built-in under the key is recorded as removed. Registering the key
again clears that record.

**Call.** The closure moves the call onto the runtime's blocking pool (`spawn_blocking`) and takes
the GIL there, so no runtime worker waits for the GIL. It builds the positional arguments:

1. the state, in its delivery form, unless the delivery is `none`;
2. each argument's resolved value, converted to Python; a `multiple` argument expands into one
   positional argument per element;
3. an `lq.Context`, inserted at the recorded `context` position.

The context is invalidated when the call returns; a retained one raises `GeneralError`. The return
value is converted back with the environment's conversion mode. A panic on the blocking thread
becomes `UnexpectedError`, and a cancelled join `Cancelled`.

**Context.** `evaluate(query)` evaluates through the core context, as a dependency of the current
asset, and waits with `wait_for_dependency`. `info`, `warning` and `error` append to the asset's
log. An `error` entry also marks the asset failed (core's `add_log_entry` sets `Status::Error`),
so the query raises `GeneralError` with that message even if the command returns normally. `is_cancelled()` reads the asset's cancellation flag.

## 5. Values

`liquers-py` has no value type of its own: it uses `liquers_lib::value::Value`, and a Python object
with no Liquers equivalent is a `PyObjectValue` inside `ExtValue::Foreign`.

**Python to Liquers** (`py_to_value`), tested in this order:

| Python | Value |
|---|---|
| `None` | `SimpleValue::None` |
| `bool` (before `int`, its superclass) | `Bool` |
| `int` | `I64`; out of range raises `ConversionError` |
| `float` | `F64`, non-finite values included |
| `str` | `Text` |
| `bytes`, `bytearray` | `Bytes` |
| `lq.Query`, `lq.Key`, `lq.Recipe`, `lq.MetadataRecord`, `lq.CommandMetadata` | `Query`, `Key`, `Recipe`, `Metadata`, `CommandMetadata` |
| `list`, `tuple` | `Array`, if every item converts structurally |
| `dict` | `Object`, if every key is a `str` and every value converts structurally |
| `polars.DataFrame` (recognised by type name) | `ExtValue::PolarsDataFrame`, copied through Parquet |
| `PIL.Image.Image` (only when `PIL.Image` is already imported) | `ExtValue::Image`, copied through PNG |
| anything else | permissive: `ExtValue::Foreign(PyObjectValue)`; strict: `ConversionError` naming the type |

A container that contains itself raises `ConversionError`; shared, non-cyclic references are
copied. A container with any item that has no structural form is not converted at all: it falls
through to the last row and is kept whole.

**Liquers to Python** (`value_to_py`): the simple variants map back as above (`I32` and `I64` to
`int`, `Array` to `list`, `Object` to `dict`, `Metadata` to `lq.MetadataRecord`, `AssetInfo` to a
`list` of `lq.AssetInfo`). A `PyObjectValue` returns the retained object itself, so identity is
kept; a foreign value of another runtime raises `ConversionError` naming that runtime. A DataFrame
is rebuilt with `polars.read_parquet` and an image with `PIL.Image.open`, raising
`ConversionError` when the library is missing. Every other extended variant (records, UI values)
raises `ConversionError` with its type identifier.

**The opaque type.** `PyObjectValue` has the type identifier `py.Object` and the `type_name`
`module.QualName` of the object's class. It holds a `Py<PyAny>`, which is `Send + Sync`; dropping
it without the GIL is deferred by PyO3. Other Liquers code sees it as foreign: a Rust command that
needs a string or a number from it gets `ConversionError`.

**Pickle.** The `py.Object` `TypeInfo` registered in an environment declares the data format
`pickle` only when the environment has `pickle=True`; with it off it declares no formats. Hence:

- **Writing.** A pickle-on environment serializes `py.Object` with `pickle.dumps`. A pickle-off one
  has no format to write in, so `store_set` raises `SerializationError`; the asset write path keeps
  such a value as metadata only.
- **Reading.** Stored bytes are decoded by a process-wide foreign decoder for `py.Object`,
  registered at module init, which calls `pickle.loads`. Core reaches it only for a type whose
  registered `TypeInfo` lists the stored format, and reads a registered type with no formats as
  *degraded* (`VALUE_TYPE_SYSTEM.md` §Reading), so a pickle-off environment never unpickles bytes,
  whoever wrote them.

Pickling can run on runtime workers; there both directions take the GIL through
`block_in_place`, so a worker waiting for the GIL hands its other tasks to another worker.
`store_set` also serializes once on the calling thread, to refuse a value with no byte form
before anything is written.

## 6. Errors

`lq.ErrorType` mirrors `liquers_core::error::ErrorType` one to one, with conversions both ways
written as exhaustive matches.

**Liquers to Python** (`error_to_pyerr`). A Liquers error becomes an instance of the `LiquersError`
subclass for its type, built by `liquers._errors._from_native` with `type`, `message`, `query`,
`key`, `position` and `traceback`. The subclass of a variant `X` is `XError`, or `X` when the name
already ends in `Error`; the subclasses are generated from `ErrorType` at import, so a new variant
needs no Python change. If `liquers._errors` cannot be imported — only in a broken installation —
the error becomes a plain `Exception` with the rendered message.

**Python to Liquers** (`pyerr_to_error`):

- A `LiquersError` keeps its type and fields. If it was raised through Python frames, the formatted
  Python traceback is appended to the traceback it already carries, separated by a newline. A
  failure that crosses Python, Rust and Python again therefore reports every Python stack it
  passed through, innermost first.
- Any other exception, `BaseException` subclasses included, becomes `ExecutionError` with the
  message `Class: message` (or `Class` alone) and the formatted Python stack as its traceback.

There is no separate field for language-specific context: a Python exception's class and message
travel in `message`, and its stack in `traceback`
(`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`). A traceback is one string; how a stack crossing
several languages, such as Rust calling Python calling another embedded language, should be
represented is open (`MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`).

## 7. Threads, waits and shutdown

**Runtime.** One multi-thread tokio runtime per process (`runtime.rs`), created on first use, with
threads named `liquers-py`. It runs every evaluation of every environment; an asset manager spawns
its workers on it.

**Waiting.** Every blocking call — `evaluate`, `evaluate_state`, `get_asset`, `store_set`,
`AssetRef.get` / `get_state` / `status` / `metadata` / `cancel`, `Context.evaluate` — goes through
`block_on`, which releases the GIL and polls the future in slices of 100 ms. Between slices it runs
Python's pending signal handlers, re-raising what they raise (`KeyboardInterrupt`) unchanged, and
checks the shut flag, ending the wait with `Cancelled` once it is set. Ending a wait drops the
future only; the asset keeps running until `AssetRef.cancel()`.

**Re-entrancy.** A Python command runs on a blocking-pool thread, not a worker, so a command that
calls `Context.evaluate` blocks its own thread only. A command can evaluate further commands to any
depth that the blocking pool has threads for (tokio's default, 512).

**Concurrency.** An `Environment` may be used from several Python threads at once. Its
declarations are behind one lock, held only to read or change them, and evaluations run in
parallel on the runtime; Python commands still take the GIL one at a time.

## 8. Limitations

- Synchronous only: no awaitable API and no `async def` commands
  ([`design/liquers-py-async/`](../design/liquers-py-async/DESIGN.md)).
- No `realm=` or `arguments=` on the decorators (`PY-COMMAND-DECORATOR-CANNOT-SET-REALM`,
  `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`); the context is passed positionally
  (`COMMAND_DECLARATION.md` §9).
- The recipe provider cannot be configured (`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`).
- Built environments are never freed (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`).
- A pickle-off read of a pickled object reports "No recipe found" rather than naming the type
  (`DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE`).
- A missing resource raises `GeneralError` "No recipe found for key …" with no `key`, not
  `KeyNotFound` (`MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR`).
- Core's progress diagnostics go to stderr and cannot be filtered (`CORE-TRACING-MIGRATION`).
- Most classes and methods have no docstrings, and there is no generated API reference
  (`PY-API-REFERENCE-NOT-GENERATED`); `Metadata` exposes its fields as methods where `State` and
`AssetRef` use properties (`PY-METADATA-FIELDS-ARE-METHODS`); `repr` of several classes is Rust's debug format
  (`PY-REPRS-SHOW-RUST-DEBUG-FORMAT`).
- No Pyodide target, no type stubs, no published wheel.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-19 | Created from the implemented `liquers-py`. | `design/liquers-py/` (phase-5) |
