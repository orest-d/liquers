---
title: Python Integration
kind: reference
audience: both
area: [py, core/value, core/error, lib/value, core/store, store/config, core/assets]
reviewed: 2026-11-01
---
# Python Integration

How `liquers-py` connects Python to Liquers: which Rust types it exposes, how values, commands and
errors cross the boundary, and how it waits, threads and shuts down. This is the contract the code
implements at HEAD. How to *use* the package is
[`guides/PYTHON_GUIDE.md`](../guides/PYTHON_GUIDE.md); the general rules for any language are
[`guides/LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md), whose §6 Python
records the choices made here and why. The conformance matrix, by feature ID, is in
`liquers-py/README.md`; the designs are [`design/liquers-py/`](../design/liquers-py/DESIGN.md) and,
for async and free-threading, [`design/liquers-py-async/`](../design/liquers-py-async/DESIGN.md);
for stores and recipe providers, [`design/liquers-py-store/`](../design/liquers-py-store/DESIGN.md)
and [`design/liquers-py-recipes/`](../design/liquers-py-recipes/DESIGN.md); for records, packaging
and type declarations, [`design/liquers-py-records/`](../design/liquers-py-records/DESIGN.md),
[`design/liquers-py-package/`](../design/liquers-py-package/DESIGN.md) and
[`design/liquers-py-stubs/`](../design/liquers-py-stubs/DESIGN.md).

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
| `_env.py` | `default_environment()` (one `Environment`, created lazily under a lock), `evaluate()` and `evaluate_async()`. |
| `_commands.py` | The decorators, and declaration stage 1 (§4). |
| `_errors.py` | `LiquersError` and one subclass per `ErrorType`, generated at import (§6). |
| `_factories.py` | `_declare`: a store or recipe provider factory's declaration from its signature (§9, §10). |
| `_records.py` | The pyarrow side of the record classes: IPC bytes to and from Arrow, normalization, pandas and polars (§11). |
| `_extras.py` | `require()` and the table of extras (§12). |
| `__init__.pyi`, `_native.pyi`, `py.typed` | The type declarations (§13). A class added to `src/lib.rs` is declared here too. |

Cargo features: `extension-module` (default) leaves the Python symbols to the loading interpreter,
which a wheel needs and a Rust test binary cannot link with, so Rust tests run with
`--no-default-features`. `debug-handles` is test-only: it adds `Environment.live_handle_count()`
and the `test_panic` command. `liquers-lib` is linked with `image-support`, `polars`, `records`,
`records-ipc`, `records-parquet` and `templates`, and `liquers-records` with `ipc`; `egui` is not
linked, since a Python process has no egui event loop. `abi3` (off by default) builds against
CPython's limited API from 3.14 (`pyo3/abi3-py314`), for the `cp314-abi3` wheel (§12).

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
| `Store`, `StoreArgumentInfo` | a handle on a store (§9) and one declared factory argument (`liquers_core::store_factory::StoreArgumentInfo`) |
| `RecipeProviders` | a handle on the environment's recipe provider chain (§10) |
| `Expires`, `ExpirationTime`, `DependencyRelation`, `PlanDependency` | expiration and dependency types |
| `ErrorType`, `Error` | `liquers_core::error`; `Error` is a data object, not the exception (§6) |
| `Environment`, `Issue` | the environment handle (§3) and one `IssueReport` finding |
| `Context`, `AssetRef` | a command's context and an asset reference, each watching its environment's shut flag |
| `State` | a snapshot of a value, converted to Python, and a copy of its metadata; `record_view()` gives the record view behind it, or `None` |
| `RecordView`, `RecordColumn`, `RecordBuffer`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin`, `RecordSource`, `RecordStream`, `RecordChunk` | `liquers_records` views, sources and schema types (§11) |

`Plan.to_json` / `to_yaml` and `Plan.from_json` / `from_yaml` round-trip within one version
only: a serialized plan is an inspection format, not a stable schema (`RECIPES_AND_PLANS.md`
§Serialization and future plan rewriting); persist the query or recipe instead.

`Status` compares equal to its variant's name (`asset.status == "Ready"`) and hashes as that
string. `Key[i]` accepts negative indices and raises `IndexError` out of range.

## 3. The environment

**Type.** Every handle builds `PyEnvironment = liquers_lib::environment::DefaultEnvironment<Value>`
with `liquers-lib`'s `Value` and payload `()`, from `liquers_lib::environment::default_environment_builder`.
The type registry is `Value`'s, plus `py.Object` (§5).

**Construction.** `Environment(*, config=None, conversion="permissive", pickle=False, async_loops=1,
job_capacity=None)`. `config` is
an `EnvironmentConfig` (`reference/ENVIRONMENT_CONFIG.md`), as a YAML string or as a `dict`
(converted through JSON). The constructor refuses an unknown `conversion` with
`ConfigurationError`; a malformed `config` (an unknown `recipes` keyword among them) fails to parse.
Nothing else is built yet. The pickle codec is a
keyword rather than a `config` entry because `EnvironmentConfig` has no codec section. There is
no `reset()`: a new `Environment()` is a fresh environment.

`async_loops` is the number of event loops `async def` commands run on (§7). `job_capacity` sets
`config.assets.job_capacity`, the number of commands the asset manager runs at once; a value that
differs from one already in `config` is a conflict. Both are integers of at least 1, and anything
else raises `ConfigurationError`.

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
`config.store.stores` is empty, otherwise the configured router through a factory chain of
`liquers_store::store_factory::default_store_factory()` then the Python store types registered
with `register_store_factory` (§9). The asset manager takes `config.assets`.
The recipe provider chain is `config.recipes`, resolved against `liquers-lib`'s `LibKind` and the
Python entry types registered with `register_recipe_provider_factory` (§10): absent or
`default` is `[recipes_yaml, manifest]`, `trivial` or `[]` resolves nothing, and a list such as
`["recipes_yaml"]` or `[{"type": "manifest"}]` is that chain (`reference/ENVIRONMENT_CONFIG.md`
§The recipe provider chain). An unknown entry type fails the build with `ConfigurationError`.
A factory runs during the build, which holds the environment's lock: a factory that uses its
own environment gets `ConfigurationError` ("may not use its environment") instead of waiting for
itself.

**Rebuild.** The environment is built on first use. A later `command`, `first_command`,
`unregister`, `import_metadata`, `register_store_factory` or `register_recipe_provider_factory`
marks it for a rebuild, and the next use builds a new
environment from all declarations **with the same store**. The replaced environment is moved to a
retired list, not dropped, because evaluations may still be running in it; its asset cache does
not carry over.

**Shutdown.** `shutdown()` sets a shared *shut* signal that every wait started from the handle
watches (§7), stops the asset managers of the current and every retired environment, then stops
the loop pool, cancelling the coroutines still running. It is idempotent, and it raises
`RuntimeError` when called from an `async def` command, whose loop it would have to stop. Afterwards every method that would use or change the environment raises
`GeneralError`, registration included; `validate()` still answers. An `AssetRef` taken before
shutdown still returns a result that is already ready, and a wait still outstanding raises
`Cancelled`. Shutdown releases the retired environments, and a built environment is freed with
the last reference to it; work still in flight keeps its own environment until it finishes.

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
| `evaluate_async(query)`, `evaluate_state_async(query)`, `get_asset_async(query)`, `store_set_async(key, data)` | the same, awaitable (§7) |
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

**Reserved names.** `ns`, `q` and `v` are planner instructions, and core refuses all three as
command names: the registration fails with "Command name '…' is reserved: it is a query
instruction".

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
becomes `UnexpectedError`, and a cancelled join `Cancelled`. A sync callable that returns a
coroutine fails with `ExecutionError` (the coroutine is closed, not run).

**Async commands.** A callable is async when `inspect.iscoroutinefunction` holds for it or, for
an object that is not a function, for `type(obj).__call__`; its metadata then has
`is_async = True`. Its call runs in three parts:

1. On a blocking thread, as above, the arguments are built and the callable is called; the
   coroutine it returns goes to a loop of the environment's pool, picked round-robin, with
   `asyncio.run_coroutine_threadsafe`.
2. The command's future waits for the coroutine's future to finish without holding a thread.
3. On a blocking thread, the result is converted as above and the context is invalidated.

A cancellation guard exists from the start of part 1. When the command's future is dropped (the
asset is cancelled or replaced, or the environment shuts down), it cancels the coroutine's
future, so the coroutine sees `asyncio.CancelledError`, and invalidates the context; a cancel
that arrives during part 1 is applied as soon as the coroutine is scheduled. A coroutine that
ends cancelled, or raises `asyncio.CancelledError` or `concurrent.futures.CancelledError`, makes
the asset `Cancelled`. Any other exception is handled as for a sync command, traceback included.

**Context.** `evaluate(query)` evaluates through the core context, as a dependency of the current
asset, and waits with `wait_for_dependency`. `info`, `warning` and `error` append to the asset's
log. An `error` entry also fails the evaluation: the command keeps running, and once it returns
the query raises `ExecutionError` with the first such message, whatever the command returned
(`Context::error` in core). `is_cancelled()` reads the asset's cancellation flag.
`evaluate_async(query)` is the awaitable `evaluate`, for `async def` commands.

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
| `lq.RecordView`, `lq.RecordSource` | `ExtValue::RecordView`, `ExtValue::RecordSource`: the same `Arc` (§11) |
| `pyarrow.Table`, `pyarrow.RecordBatch` (recognised by type name) | `ExtValue::RecordView`, copied through Arrow IPC (§11) |
| anything else | permissive: `ExtValue::Foreign(PyObjectValue)`; strict: `ConversionError` naming the type |

A container that contains itself raises `ConversionError`; shared, non-cyclic references are
copied. A container with any item that has no structural form is not converted at all: it falls
through to the last row and is kept whole.

**Liquers to Python** (`value_to_py`): the simple variants map back as above (`I32` and `I64` to
`int`, `Array` to `list`, `Object` to `dict`, `Metadata` to `lq.MetadataRecord`, `AssetInfo` to a
`list` of `lq.AssetInfo`). A `PyObjectValue` returns the retained object itself, so identity is
kept; a foreign value of another runtime raises `ConversionError` naming that runtime. A DataFrame
is rebuilt with `polars.read_parquet` and an image with `PIL.Image.open`, raising
`ConversionError` when the library is missing. A record view becomes `lq.RecordView` (or, with
exactly one row and one field, that cell's scalar) and a record source `lq.RecordSource` (§11).
Every other extended variant (UI values) raises `ConversionError` with its type identifier.

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
needs no Python change. Error types also fall into overlapping categories
(`ErrorType.categories()`, from `liquers_core::error::ErrorCategory`): each has an abstract class —
`StoreError`, `KeyedAssetError`, `NotFoundError` (also a `LookupError`) — and a variant's class
derives from the class of every category it is in, so `KeyNotFoundError` is a `StoreError` and a
`NotFoundError`, `AssetNotFoundError` a `KeyedAssetError` and a `NotFoundError`. If `liquers._errors` cannot be imported — only in a broken installation —
the error becomes a plain `Exception` with the rendered message.

A JSON or YAML failure in a wrapper method (`from_json`, `to_yaml`, …) raises `SerializationError`; a
bad constructor argument raises the error its core constructor returns.

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
parallel on the runtime. The lock is taken with PyO3's `lock_py_attached`, which detaches while it
waits, so a waiting thread never holds the GIL (or, free-threaded, a stop-the-world); and no
Python object is released while it is held, since a `__del__` may call back into the environment.
On a GIL build, Python commands take the GIL one at a time.

**The runtime and the bridge.** `runtime()` binds the runtime to `pyo3-async-runtimes` on first use
(`init_with_runtime`), so the awaitable methods run on it and the bridge never builds its own. A
query that needs no Python never waits for the GIL: Python commands and every conversion run on
blocking threads or loop threads, and `py.Object` (de)serialization attaches only inside
`block_in_place`, which hands the worker's queue to another thread first.

**Awaitables.** Each `_async` method (`Environment.evaluate_async`, `evaluate_state_async`,
`get_asset_async`, `store_set_async`; `AssetRef.get_async`, `get_state_async`, `cancel_async`;
`Context.evaluate_async`; `lq.evaluate_async`) does its synchronous part at call time (the query is
parsed, the value of `store_set_async` converted, the environment built), so those errors raise
from the call. It then spawns the core future on the runtime, raced against the shut signal, and
returns an `asyncio.Future` of the caller's running loop (`future_into_py_with_locals`); with no
running loop the call raises `RuntimeError`. The result is converted to Python on a blocking
thread. The future can be awaited more than once. Timeouts are the caller's (`asyncio.timeout`);
progress streams are not offered.

| Event | Result |
|---|---|
| The awaitable is cancelled, or dropped | The awaiter gets `asyncio.CancelledError`; the evaluation goes on (detach). `AssetRef.cancel_async()` cancels the asset |
| The asset is cancelled or replaced | Awaiters raise `lq.CancelledError`; a running coroutine sees `asyncio.CancelledError` |
| `shutdown()` | Every wait, blocking or awaited, ends with `lq.CancelledError`; running coroutines are cancelled |

**The loop pool.** An environment owns `async_loops` event loops, each on its own thread
(`liquers-py-loop-N`), started by the first async command it runs and shared by every rebuild. A
failed start is not kept, so the next command tries again. `shutdown()` stops the pool: each loop
cancels its tasks, waits up to 5 s (`STOP_GRACE`) for them, closes, and its thread is joined; a
coroutine that ignores cancellation, or a thread still blocked after that, is left behind with a
`RuntimeWarning`. An `atexit` hook stops a pool still running when the interpreter exits, and a pool
dropped without either is stopped on a thread of its own.

**Interpreter exit.** Every time a liquers-owned thread (a runtime blocking thread, a loop thread)
attaches to the interpreter, the attach is counted, and a Python command call also records its
asset (`liquers-py/src/exit.rs`). An `atexit` hook registered at import runs after every pool's
hook: it refuses new command calls with `Cancelled`, requests cancellation of the running ones,
and waits up to 5 s for every counted attach to end. A command still running after that (one that
never checks `context.is_cancelled()`) is left behind with a `RuntimeWarning`. Without the wait, a
command attached on another thread when the interpreter finalizes aborts the process on CPython
before 3.14 (`PyGILState_Release`), which is why this hook exists. Environments are not shut down
by it.

**Blocking calls on a loop.** A blocking method (`evaluate`, `evaluate_state`, `get_asset`,
`store_set`, `AssetRef.get` / `get_state` / `cancel`, `Context.evaluate`) called on a pool thread
raises `RuntimeError` naming its `_async` form, because it would stall every coroutine on that loop
and a nested evaluation needing the loop would never finish. Called under any other running loop it
works, with a `RuntimeWarning`. `shutdown()` raises on a pool thread. `AssetRef.status` and
`metadata` are exempt: each reads one lock and does not wait for the asset.

**Job capacity.** All commands, Rust and Python, sync and async, share the asset manager's one job
queue (`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`); `job_capacity` bounds it. A nested evaluation is a
dependency wait, which runs the dependency inline when no slot is free, so `Context.evaluate[_async]`
cannot deadlock even at `job_capacity=1`. A top-level `env.evaluate_async` from inside a command does
wait for a free slot, as the sync `env.evaluate` does, and deadlocks at capacity 1.

**Free-threaded Python.** The extension declares `gil_used = false`, so importing `liquers` keeps a
free-threaded interpreter (3.14t) running without the GIL; it is tested on 3.14t. Process state is
`OnceLock`s and atomics, and every handle class (`Environment`, `Context`, `AssetRef`, `State`,
`Issue`) is frozen. The query and metadata classes are not frozen: PyO3 guards each with a borrow
flag, so mutating one object from two threads at once raises `RuntimeError` instead of racing. A
Python command may run on several threads and loops at once; its own state is its author's to
protect.

## 8. Limitations

- One job queue serves every command; a command cannot be given its own queue or limit
  (`CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS`).
- Exiting while a command ignores cancellation for more than 5 s can still abort the process on
  CPython before 3.14 (`PY-EXIT-WITH-UNCANCELLABLE-COMMAND-CRASHES-BEFORE-3-14`).
- CI runs the Python suite on CPython 3.10, 3.14 and 3.14t (3.14t without polars, which has no
  free-threaded wheel); PyPy is not tested (`py-pypy-ci-job`).
- No `realm=` or `arguments=` on the decorators (`PY-COMMAND-DECORATOR-CANNOT-SET-REALM`,
  `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`); the context is passed positionally
  (`COMMAND_DECLARATION.md` §9).
- Every registration rebuilds the environment and drops its asset cache
  (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`). The store is built once and kept,
  so only a build that failed for want of a store type is affected by registering it.
- `store_config` handlers cannot be written in Python (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`);
  a Python mount runs the Rust ones (§9).
- A Python store type may not take a built-in store type's name, nor a Python recipe provider
  type a built-in entry type's name or a keyword.
- `env.recipes.recipe(key)` for a key no member has raises `GeneralError`, not `KeyNotFoundError`
  (`RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR`). Python recipe providers are invisible to
  `liquers-validate` (`VALIDATE-CANNOT-SEE-NON-STANDARD-RECIPE-PROVIDERS`). A provider's changed
  answer does not expire assets made from its earlier one (`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`).
- Stores and recipe providers have no `close()`; they are released by `shutdown()`
  (`ENVIRONMENT-ORDERLY-SHUTDOWN`).
- Core's progress diagnostics go to stderr and cannot be filtered (`CORE-TRACING-MIGRATION`).
- There is no generated API reference (`PY-API-REFERENCE-NOT-GENERATED`), although every public
  name has a docstring (`tests/test_DOCS_docstrings.py` keeps it so); `Metadata` exposes its fields as methods where `State` and
`AssetRef` use properties (`PY-METADATA-FIELDS-ARE-METHODS`); `repr` of several classes is Rust's debug format
  (`PY-REPRS-SHOW-RUST-DEBUG-FORMAT`).
- No Pyodide target. Wheels cover CPython 3.14+ only, and other platforms build from source (§12).
- Records: Python cannot implement a view or a source, and every exit to Arrow, pandas or polars
  copies (§11).

## 9. Stores

**Registration.** `Environment.register_store_factory(type, factory, *, label="", doc="",
arguments=None)` declares the store type `type`. A configuration entry
`{"type": type, "prefix": …, "config": {…}}` is then built by calling `factory` with the
`config:` keys as keywords, plus `prefix=` (the mount's prefix as text) when the factory has a
parameter of that name. `_factories._declare` reads the declaration from the factory's signature:
each keyword parameter is a `StoreArgumentInfo` (a default makes it optional; `bool`, `int`/`float`,
`str`, `list`, `dict` annotations or defaults give the argument type, anything else `any`), and
`**kwargs` accepts undeclared keys (`ArgumentCoverage::Partial`). Positional-only and `*args`
parameters, a parameter named `type`, a default with no JSON form and an `async def` factory are
refused; `arguments=[StoreArgumentInfo(…)]` declares a factory whose signature cannot be inspected.
At build a missing required key, an undeclared key, a raising factory (its traceback kept) and an
awaitable result are each a `ConfigurationError` naming the mount. Registration refuses a built-in
type name (`memory`, `filesystem`, `fs`, any `opendal_*`, …) and a second registration of a name.
`Environment.store_types()` lists the built-in types then the Python ones, as dicts.

**Protocol.** The returned object is adapted once (`liquers-py/src/store/adapter.rs`
`StoreMethods::resolve`); a method added later is not seen. Each method may be `def` or
`async def`, takes `lq.Key` (then `bytes` data and `lq.Metadata` where it writes), and is used
whenever present:

| Method | When absent |
|---|---|
| `get(key)` → `bytes`, `(bytes, metadata)` or `None` | required |
| `get_metadata(key)` | derived from `get`; a directory (`is_dir`) gets a directory record |
| `contains(key)` | derived from `get_metadata` (or `get`), a directory included |
| `listdir(key)` → names or `lq.Key`s | `listdir_data` when the data methods are provided, else `KeyNotSupported`; `is_dir` is then `False` (a flat store) |
| `is_dir(key)` | required when `listdir` is present |
| `set`, `set_metadata`, `remove`, `removedir`, `makedir` | `KeyNotSupported` naming the method; a store with no write method is read-only |
| `is_supported(key)` (plain `def` only) | every absolute key under the prefix is supported |
| `get_data`, `set_data`, `remove_data`, `listdir_data`, `data_facts` | all five or none; when none, data-only access is derived from `get` and `listdir` (writes through `set` and `remove`) |
| `rename_data`, `get_stored_metadata` | not offered / `get_metadata`, else no stored metadata |

Metadata may be returned as `lq.Metadata`, `lq.MetadataRecord`, a `dict` or `None`; missing fields
are filled from the key as for any store. `None` from `get` is absence (`KeyNotFound`); a raised
`LiquersError` keeps its type; any other exception is `KeyReadError` / `KeyWriteError` with the
Python traceback; a wrong return type is a read error naming the method. A relative or escaping
key (`a/../../x`) is refused with `KeyNotAbsolute` before Python is called. The attributes
`atomic_write = True` and `data_access = False` declare the store's write atomicity and opt it out
of data-only access.

**Configuration stays effective.** A Python mount is wrapped like any mount: `read_only` refuses
every write before Python is called, and a `metadata:` section runs the Rust `on_write` / `on_read`
chains around the store's data-only access (`reference/STORE_CONFIG_FSD.md`). A `metadata:`
section on a store without data-only access (no `listdir`, or `data_access = False`) fails the
build naming the mount.

**Threads.** Every store call runs Python off the runtime's workers: a `def` method on the
blocking pool, an `async def` method on the environment's loop pool (§7). A query on a Rust mount
therefore never waits for a Python store call. Only `is_supported`, synchronous by the trait,
attaches on the calling thread, and only for keys under the mount's prefix.

**`lq.Store`.** `env.store` is the environment's store with every mount and layer, resolved on
each call so a rebuild is followed. `lq.Store.from_object(obj, prefix="")` adapts an object
directly, without an environment, with a private loop that `close()` stops. Its methods are the
store's: `get`/`get_bytes`, `get_metadata`, `set`, `set_metadata`, `remove`, `removedir`,
`makedir`, `contains`, `is_dir`, `listdir`, `sync` (settles interrupted writes, returning the
repaired keys), `is_supported`, `is_read_only`, each with an `_async` twin except the last two. After `shutdown()` (or `close()`) every call raises.

**Staged use.** The recommended order is: register the factories (store, recipe provider), build
the environment from its configuration on first use, then use it. Registering later is allowed and
rebuilds (§3).

## 10. Recipe providers

**Registration.** `Environment.register_recipe_provider_factory(type, factory, *, label="",
doc="", arguments=None)` declares the `recipes:` entry type `type`; an entry `{"type": type, …}`
is built by calling `factory` with the entry's other keys as keywords. The declaration is read as
for a store factory (§9), with no exempt parameter. The name may not be one of `LibKind`'s entry
types (`recipes_yaml`, `manifest`, …), a keyword (`default`, `trivial`, `none`, `no_recipes`) or
one registered before, because Python types are consulted before the built-in ones. The same type
may be listed several times with different arguments; each entry is a member, named `type#position`
when its type repeats. An entry identical to an earlier one fails the build.

**Protocol.** The provider object is adapted once (`liquers-py/src/recipe_provider.rs`); each
method may be `def` or `async def` and receives `lq.Key`:

| Method | Meaning | When absent |
|---|---|---|
| `recipe_opt(key)` | `lq.Recipe`, query text, or `None` for "not mine" | required |
| `assets_with_recipes(folder)` | names, or `lq.Key`s directly in `folder`, that it lists | lists nothing |
| `contains(key)` | whether `key` is listed | membership in the listing |
| `directory_changed(folder)` | called after each write the asset manager mediates; a raised exception is reported on stderr and ignored | not called |

A recipe without a `cwd` gets the key's folder. Query text becomes `Recipe(query)`, and text that
does not parse is the parse error. Another return type, or a listed entry outside `folder`, is an
`ExecutionError`; a raised `LiquersError` keeps its type, any other exception is `ExecutionError`
with the traceback. `can_make` is "`recipe_opt` is not `None`", so a provider may make keys it does
not list. Two members answering one key is a `ConfigurationError` naming both. A recipe's plan is
`Recipe::plan_with_dependencies`, the one the built-in providers build, so `volatile` and `expires`
reach the plan and the asset.

**`env.recipes`** (`lq.RecipeProviders`) asks the built chain, every member included:
`recipe_opt`, `recipe`, `recipe_plan` (an `lq.Plan`), `assets_with_recipes` (a list of
`lq.ResourceName`), `contains`, `can_make`, each with an `_async` twin. After `shutdown()` every
call raises. `lq.Recipe(query, title="", description="", *, cwd=None, volatile=False,
expires=None)` builds a recipe; every field is a settable property.

**Threads.** Provider methods run as store methods do (§9); a provider may evaluate through its
own environment from `recipe_opt`. Every Python provider is asked for every key it could own, so
`recipe_opt` should answer `None` quickly for keys outside its folder.

## 11. Records

Record views and sources (`specs/reference/RECORD_STREAMS.md`) cross into Python as wrappers
around the same `Arc`s, with no copy. The classes are in `liquers-py/src/records/`; every one is
frozen, so they are safe on a free-threaded interpreter without locks.

| Class | Wraps | Members |
|---|---|---|
| `RecordView` | `Arc<dyn RecordView>` | `schema`, `chunk_id` (`lq.Key`, `lq.Query` or `None`), `origins`, `len()`, `num_columns`, `column(name or index)`, `columns()`, `value(row, col)`, `select_columns(*cols)`, `slice(offset, length)` (clamped), `row(i)`, `cell(row, col)`, `item()`, `to_list()` (row dicts), `to_dict()` (column lists); `from_records`, `from_dict`, `from_arrow`, `from_pandas`, `from_polars`; `to_arrow`, `to_pandas`, `to_polars` |
| `RecordColumn` | one `ColumnMut` | `name`, `type`, `null_count`, `dim`, `len()`, `[i]` (negative indices), `to_list()`, `validity` / `offsets` / `data` (each an `lq.RecordBuffer` or `None`); the buffer protocol for fixed-width columns |
| `RecordBuffer` | one part of a column | `format`, `len()`, the buffer protocol |
| `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin` | the schema types | `FieldSchema(type, *, name=None, key=None, label=None, description="", nullable=True)`; `RecordSchema(fields, *, type_identifier=None)` with `fields`, `names`, `field(name)` (`KeyError`), `to_dict()`; `KeyRole.Data` is Rust's `KeyRole::None` |
| `RecordSource` | `Arc<dyn RecordSource>` and where it reads from | `schema`, `bounded`, `manifest` (a dict or `None`), `chunks()`, `chunk(i)` / `chunk_async(i)`, `materialize(max_rows=1_000_000)` / `materialize_async`, `iter()` and `aiter()` |
| `RecordStream` | one traversal | `next()` and `anext()`, one chunk per step |
| `RecordChunk` | one entry of `chunks()` | `index`, `kind` (`"query"`, `"key"` or `"memory"`), `query`, `evaluable` |

**Crossing.** `ExtValue::RecordView` reaches Python as `lq.RecordView`, except a view of exactly one
row and one field, which reaches it as that cell's scalar (`liquers_lib::value::record_view_is_single_cell`,
the rule `try_into_json_value` already follows). `ExtValue::RecordSource` reaches it as
`lq.RecordSource`. Either wrapper returned from a command, or passed as an argument, enters Liquers
as the same `Arc`. A `pyarrow.Table` or `pyarrow.RecordBatch` (recognised by type name) enters as a
record view through `from_arrow`.

**Cells.** `bool`, `int`, `float`, `str`, `bytes`; a date is `datetime.date`, a timestamp a
timezone-aware `datetime.datetime` in UTC (microseconds), a vector a `list` of `float`, a null
`None`. A date is built with `date.fromordinal`, since the limited API has no `PyDate`.

**Building from Python data.** `from_records(rows, *, fields=None)` takes an iterable of dicts and
`from_dict(columns, *, fields=None)` a dict of equal-length iterables. Without `fields`, each
field's type is inferred from its non-null cells: one Python type gives its field type (`bool`
before `int`), `int` with `float` widens to `Float`, any other mix raises `ConversionError` naming
the field and row, and an all-null field is `Text`. `fields=` is a dict from field name to an
`lq.FieldType`, its lowercase name or an `lq.FieldSchema`, or an `lq.RecordSchema`:
- a declared type casts every cell, losslessly or not at all: numbers between the numeric types
  when exact, text parsed as the type, anything to text (through the CSV cell formatter), a date
  to a timestamp at midnight UTC and a midnight timestamp to a date;
- a declared field the data does not have is a column of nulls;
- with a `RecordSchema`, a key the schema does not name is an error, and the schema's order wins;
- a null in an `Id` field or in a not-nullable field is refused.

A field's label defaults to its name with `_` replaced by spaces.

**Arrow, pandas and polars.** Every route copies, through bytes (`liquers-py/src/records/arrow.rs`,
`python/liquers/_records.py`):
- `to_arrow()` writes the view as Arrow IPC (`liquers-records`' writer, with the interpreter lock
  released) and reads it with pyarrow. The table's schema metadata carries `liquers.schema` (roles
  and labels), and `liquers.chunk_id` and `liquers.origins` as JSON (the chunk id in serde form,
  the key's segments). `to_pandas()` is `to_arrow().to_pandas()`; `to_polars()` is
  `polars.read_ipc` of the same bytes. Only `to_arrow` keeps the metadata.
- `from_arrow(table)` first normalizes, in Python, the columns liquers' IPC reader does not take,
  when no value is lost: large and view strings and binaries, integers narrower than 64 bits,
  `float16` and `float32`, dictionary encoding, `date64`, timestamps of another unit (refused when
  a value would change), and fixed-size float lists (to `float32` vectors). Anything else (a
  nested list, a struct, a decimal) is a `ConversionError` naming the column. `from_pandas` goes
  through `pyarrow.Table.from_pandas` without the index; a pandas integer column with a missing
  value arrives as `Float` unless `fields=` declares it.
- `from_polars(df)` crosses as Parquet and through the polars bridge
  (`liquers_lib::records::polars`), so it needs no pyarrow; a dtype the bridge cannot carry is a
  `ConversionError`.

A missing package raises `ConversionError` naming the extra that provides it (§12).

**Buffers.** A fixed-width column (`Int`, `UInt`, `Float`, `Date`, `Timestamp`, `Vector`) lends
its values through the buffer protocol: `memoryview(column)` or `numpy.asarray(column)`, with the
formats `q`, `Q`, `d`, `i`, `q` and a 2-D `f` of shape `(rows, dim)`. The parts are lent the same
way: `validity` and a `Bool` column's `data` are bitmaps (`B`), `offsets` are `i`, a `Text` or
`Binary` column's `data` is `B`. Every buffer is read-only: a writable request raises
`BufferError`, and a column of `Bool`, `Text` or `Binary` has no buffer of its own. A buffer keeps
its column alive, so it outlives the view it came from, and the data never moves.

**Where a source reads its chunks** (`liquers-py/src/records/resolver.rs`). Each wrapper of a
source carries a `SourceOrigin`: the environment, held weakly, its shut flag, and, inside a
command, the command's `Context`. `PyChunkResolver` reads through the context while it is valid,
so a chunk read inside a command becomes a dependency of the asset being computed
(`ContextResolver`), and through the environment afterwards (`EnvResolver`). A source therefore
outlives its command. It never keeps an environment alive: once the environment is dropped, or
after `shutdown()`, every read raises the shut-down `GeneralError`, and a source converted with no
environment in reach (`Scope::detached`) raises `GeneralError` saying it has none. A view already
read keeps working.

**Traversal.** `iter(source)` and `aiter(source)` open a `RecordStream` that reads one chunk per
step, so a manifest of a thousand chunks evaluates one per `next()`. `chunk(i)` reads only chunk
`i` (`liquers_records::read_chunk`); `chunks()` lists the chunks without evaluating any.
`materialize(max_rows)` concatenates every chunk into one view and refuses a source with more rows.
A step taken while another step of the same stream is still running raises `GeneralError`
("another step of this record stream…"). A shutdown while a step waits ends that step with
`CancelledError`. The synchronous calls (`next()`, `chunk`, `materialize`, `to_arrow`) follow the
blocking-call rule of §7: on a liquers loop thread they raise `RuntimeError` naming the `_async`
variant.

**Open problems.**
- Python cannot implement the view or source traits, so a Python object cannot be a lazy source
  (`RECORDS11` is `NA`; `liquers-py-records` Decision 4).
- A command that returns raw CSV bytes cannot be a manifest chunk: bytes serialize only as `bin`
  or text (`BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT`). A chunk command returns a view instead.
- Every exit to Arrow, pandas or polars copies; only the column buffers are lent.

## 12. Packaging

**Version and license.** The workspace states both once (`[workspace.package]`: `version =
"0.1.0"`, `license = "AGPL-3.0-or-later"`), and every crate inherits them. `liquers_core::VERSION`
reads the version at compile time; `Environment::liquers_version()` (a provided trait method)
reports it, `Environment.liquers_version` in Python and `liquersVersion` in JavaScript return it,
and `lq.__version__` is its PEP 440 form (`-alpha.N` becomes `aN`, `-beta.N` `bN`, `-rc.N`
`rcN`). `__version__` stays out of `__all__` (`OBJECT08`).

**Wheels.** Fifteen per release, all built with `--profile wheel` (release, stripped, thin LTO, one
codegen unit) and the crate's default features:

| Platform | Runner | Builds |
|---|---|---|
| Linux x86_64, aarch64 | `ubuntu-24.04`, `ubuntu-24.04-arm`, in the manylinux_2_28 container | `cp314-abi3` (`--features abi3`, PyO3's `abi3-py314`), `cp314t`, `cp315t` |
| macOS arm64 | `macos-15` | the same three |
| Windows x86_64, arm64 | `windows-2025`, `windows-11-arm` (LLVM's `clang-cl` for `aws-lc-sys` and `ring`) | the same three |

Everything else installs from the sdist, which needs Rust 1.95 and a C compiler: CPython
3.10-3.13, PyPy 3.11+, macOS x86_64, musllinux. `abi3t` is not used: the wheel maturin builds for
it does not import (`PY-ABI3T-WHEEL-NOT-USED`). The Linux abi3 wheel measured 30.9 MB compressed.

**Extras and `require`.** `[project.optional-dependencies]` declares `polars`, `images` (pillow),
`arrow` (pyarrow), `pandas` (pyarrow and pandas) and `all`. A package with no wheel for a platform
is excluded by an environment marker (pyarrow on Windows arm64), so an install never builds a
third-party package from source; polars on a free-threaded interpreter has no marker and is
documented instead. `lq.require(extra)` (`python/liquers/_extras.py`) imports the extra's modules
and raises `ModuleNotFoundError` naming the `pip install 'liquers-py[extra]'` that fixes it, or
saying the extra is not available on this platform; an unknown extra is a `ValueError`.

**Native requirements.** A wheel links only system libraries: TLS is rustls with `aws-lc-rs`
built in (`AWS_LC_SYS_USE_SYSTEM=0`), and nothing is grafted. `scripts/check-wheel.py` checks each
wheel against `liquers-py/packaging/allowed-libraries.toml` (the manylinux_2_28 policy, macOS's
system paths, Windows' system DLLs and VC runtime) and refuses one over 90 MB. The wheel carries
`THIRD-PARTY-NOTICES.html`, generated by `cargo about` from the resolved lock at release time and
not committed.

**The release pipeline** (`.github/workflows/liquers-py-release.yml`). On a `liquers-py-v*` tag:
`lock` (one `Cargo.lock`; `scripts/check-workspace-metadata.py` checks the tag against the
workspace version and every crate's inherited version and license), `notices`, `sdist`, the
fifteen `wheels`, `dist` (exactly the expected set), `test` (each wheel installed in a fresh venv
on its own runner, running `liquers-py/tests/package/`), then `github-release`, `testpypi` (whose
uploaded files are compared with the built ones), and `pypi`. Publishing uses trusted publishing,
and the `pypi` job waits for the protected `pypi` environment's reviewer: **a release reaches PyPI
only when Orest approves it.** A manual run, or a pull request changing the workflow, builds and
tests everything and publishes nothing. `liquers-py-package.yml` builds and tests the Linux x86_64
abi3 wheel on pull requests that touch packaging.

**Cutting a release.** Raise `[workspace.package] version`, commit, push a tag
`liquers-py-v<version>`, and approve the `pypi` environment once TestPyPI holds the same files.
A release can be yanked on PyPI but never replaced.

**Open problems.** Source-only platforms (macOS x86_64, musllinux, PyPy); polars has no
free-threaded wheel; the 90 MB margin under PyPI's limit; the manual PyPI approval;
`requires-python` cannot express a different floor for wheels and the sdist; abi3t.

## 13. Type declarations

The package is typed (`py.typed`). `liquers/__init__.pyi` declares every native class under its
runtime home `liquers`, the module functions, the decorators and the generated error classes;
`liquers/_native.pyi` re-exports those names, so a checker following `liquers._native` sees the
same declarations. The six pure-Python modules carry inline annotations.

- **Values are `Any`**: a query's result, `State.value`, a command's inputs and outputs.
- **Third-party types are `Any`** too, with a comment naming the runtime type (`to_arrow() -> Any
  # pyarrow.Table`): pyarrow ships no type information, so a precise type would be *Unknown* under
  pyright strict. The stubs import no third-party package.
- **The decorators keep the function's signature**: `command` and `first_command` are overloaded,
  bare (`Callable[_P, _R] -> Callable[_P, _R]`) and with options (returning a decorator protocol),
  on the module and on `Environment`; an `async def` command stays a coroutine function.
- Every awaitable is `Awaitable[T]`; `Query | str` and `Key | str` parameters accept both forms;
  every native class is `@final`; the PyO3 enums are classes whose members are class attributes
  of their own type; settable properties have setters. `Query.segments` returns the stub-only
  `_QuerySegment`, since `_native` does not export that class
  (`PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED`).

**The checks** (`liquers-py/tests/test_STUBS_declarations.py`, on CPython 3.14 with the GIL only,
with `mypy==2.4.1` and `pyright[nodejs]==1.1.414` pinned in `scripts/test-liquers-py.sh`):
`stubtest liquers` with `tests/typing/stubtest-allowlist.txt` (only `debug-handles` names, each
with a reason; an unused entry fails); a comparison of `dir(liquers._native)` and of the generated
error classes, bases included, with the stubs; `mypy -p liquers` under `--strict`; and the samples
in `tests/typing/`, `good/` passing and `bad/` failing on exactly the marked lines, under both
mypy `--strict` and pyright `strict`. A change to the package's surface updates the stubs in the
same change.

**Open problems.** The stubs are written by hand and kept honest by the checks rather than
generated; values are `Any`, so a checker cannot follow a value through a query; generating the
declarations from the PyO3 signatures can be revisited when PyO3's own stub generation matures.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | §11 Records (classes, crossing, building with `fields=`, Arrow/pandas/polars routes, buffers, where a source reads, traversal), §12 Packaging (version, wheels, extras, native requirements, the release pipeline and its approval), §13 Type declarations; §1, §2, §5 and §8 updated: records cross to Python, type stubs and published wheels exist. | `design/liquers-py-records/`, `design/liquers-py-package/`, `design/liquers-py-stubs/` (phase-5) |
| 2026-11-01 | §9 Stores and §10 Recipe providers written in Python (registration, protocol, threads, `lq.Store`, `env.recipes`); §1-§3 and §8 updated; the Python recipe provider limitation is lifted. | `design/liquers-py-store/`, `design/liquers-py-recipes/` (phase-5) |
| 2026-10-31 | §8: CI runs the Python suite on CPython 3.10, 3.14 and 3.14t; the supported floor is 3.10. §7: interpreter exit cancels the running Python commands and waits for them; §8 names the case it cannot cover. | design `language-binding-tests-in-ci` |
| 2026-10-30 | §3: `config.recipes` accepts every value (`default`, `trivial`, a list of entries); the constructor no longer refuses a non-default `recipes`. Known limitations: the recipe-provider limitation now names a Python-written provider only. | design `recipe-provider-chain-config` |
| 2026-10-28 | §6: a wrapper method's JSON/YAML failure raises `SerializationError`, and a bad constructor argument the core constructor's error; no wrapper raises a plain `Exception`. | design `py-metadata-typed-errors` |
| 2026-10-27 | §8: every public name has a docstring now; only the generated API reference is missing. | design `py-docstrings` |
| 2026-10-27 | §3 `async_loops` and `job_capacity`, the `_async` methods, shutdown stops the loop pool; §4 async commands; §7 the bridge, awaitables, loop pool, blocking-call rule, job capacity, free-threading; §8 the synchronous-only limitation is lifted; the shutdown pin leak is fixed (`CORE-JOB-QUEUE-KEEPS-ENVIRONMENT-PINS-AFTER-SHUTDOWN`). | `design/liquers-py-async/` (phase-5) |
| 2026-10-27 | §2: `Plan`'s JSON/YAML round-trips within one version only. | `design/plan-serialization-internal-contract/` |
| 2026-10-27 | Reserved names: `q` and `v` are refused as command names like `ns`. | design `reserved-instruction-command-names` |
| 2026-10-25 | §6: error category classes (`StoreError`, `KeyedAssetError`, `NotFoundError`); §8: the pickle-off and missing-resource limitations are fixed (`AssetNotSupportedError`, `AssetNotFoundError`). | `design/key-without-recipe-answer/` |
| 2026-10-22 | §Shutdown, limitations: a built environment is freed (the reference cycle is fixed); `RUNTIME05` passes. | `design/environment-manager-reference-cycle/` |
| 2026-10-20 | `context.error` raises `ExecutionError` with the first logged message once the command returns. | `CONTEXT-ERROR-OUTCOME-DEPENDS-ON-SCHEDULING` |
| 2026-10-19 | Created from the implemented `liquers-py`. | `design/liquers-py/` (phase-5) |
