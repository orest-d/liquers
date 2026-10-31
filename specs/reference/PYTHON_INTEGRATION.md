---
title: Python Integration
kind: reference
audience: both
area: [py, core/value, core/error, lib/value]
reviewed: 2026-10-31
---
# Python Integration

How `liquers-py` connects Python to Liquers: which Rust types it exposes, how values, commands and
errors cross the boundary, and how it waits, threads and shuts down. This is the contract the code
implements at HEAD. How to *use* the package is
[`guides/PYTHON_GUIDE.md`](../guides/PYTHON_GUIDE.md); the general rules for any language are
[`guides/LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md), whose §6 Python
records the choices made here and why. The conformance matrix, by feature ID, is in
`liquers-py/README.md`; the designs are [`design/liquers-py/`](../design/liquers-py/DESIGN.md) and,
for async and free-threading, [`design/liquers-py-async/`](../design/liquers-py-async/DESIGN.md).

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
`config.store.stores` is empty, otherwise the configured router through
`liquers_store::store_factory::default_store_factory()`. The asset manager takes `config.assets`.
The recipe provider chain is `config.recipes`, resolved against `liquers-lib`'s `LibKind`: absent or
`default` is `[recipes_yaml, manifest]`, `trivial` or `[]` resolves nothing, and a list such as
`["recipes_yaml"]` or `[{"type": "manifest"}]` is that chain (`reference/ENVIRONMENT_CONFIG.md`
§The recipe provider chain). An unknown entry type fails the build with `ConfigurationError`.

**Rebuild.** The environment is built on first use. A later `command`, `first_command`,
`unregister` or `import_metadata` marks it for a rebuild, and the next use builds a new
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
- CI runs the Python suite on CPython 3.10, 3.14 and 3.14t (3.14t without polars, which has no
  free-threaded wheel); PyPy is not tested (`py-pypy-ci-job`).
- No `realm=` or `arguments=` on the decorators (`PY-COMMAND-DECORATOR-CANNOT-SET-REALM`,
  `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`); the context is passed positionally
  (`COMMAND_DECLARATION.md` §9).
- A recipe provider cannot be written in Python yet; the chain only places the built-in entry
  types (`PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER`, liquers-py M4).
- Core's progress diagnostics go to stderr and cannot be filtered (`CORE-TRACING-MIGRATION`).
- There is no generated API reference (`PY-API-REFERENCE-NOT-GENERATED`), although every public
  name has a docstring (`tests/test_DOCS_docstrings.py` keeps it so); `Metadata` exposes its fields as methods where `State` and
`AssetRef` use properties (`PY-METADATA-FIELDS-ARE-METHODS`); `repr` of several classes is Rust's debug format
  (`PY-REPRS-SHOW-RUST-DEBUG-FORMAT`).
- No Pyodide target, no type stubs, no published wheel.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-31 | §8: CI runs the Python suite on CPython 3.10, 3.14 and 3.14t; the supported floor is 3.10. | design `language-binding-tests-in-ci` |
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
