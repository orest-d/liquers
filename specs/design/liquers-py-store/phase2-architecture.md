# Phase 2: Solution & Architecture — liquers-py-store

## Overview

A new `liquers-py/src/store/` holds `PyStore`, an `AsyncStore` adapter over a Python object whose
protocol is resolved once into a `StoreMethods` table; `PyStoreFactory`, one `StoreFactory` holding
an environment's Python store types; and `lq.Store`, the Python handle on a store. `EnvCell` keeps
the registered types among its declarations and chains `PyStoreFactory` **after**
`liquers_store::store_factory::default_store_factory()`. `read_only` and `metadata` are applied by
`StoreRouterBuilder` unchanged; the adapter supplies derived data-only access (Decision 5). Calls
into Python go through `src/pycall.rs`, shared with `liquers-py-recipes`.

Rejected: a fixed `python` type with an object registry (Decision 1); a native `lq.StoreBase` to
subclass (the protocol is duck-typed, as for commands, and a base class would hide the "absent
method" rule); an async `is_supported` (the trait method is synchronous).

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED` | draft | P2 | Same rule for web (Decision 3) | No | No | Own PR (Phase 4 last step) |
| `WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY` | draft | P3 | Python seeds metadata from the key | No | No | Own PR after verification |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | Adapter calls `Key::as_absolute` | No | No | Deferred past M4 |
| `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` | draft | P3 | `STORE18` `NA` | No | No | The `NA` reason |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | Python half absorbed | No | No | Narrowed to web in Phase 5 |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | No `close()` | No | No | Documented |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | Late registration rebuilds | No | No | Documented |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | Error map on today's types | No | No | Revisit if it lands |
| `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING` | draft | P3 | Store errors carry a flat traceback (AC-3) | No | No | Documented |
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | — | No adapter lock, last write wins (`STORE06`) | No | No | Documented |
| `STORE-CONFORMANCE-VALIDATION-TOOL` | accepted | P2 | AC-7 runs the suite from pytest and Rust | No | No | None |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` | draft | P3 | Bytes copied at the boundary | No | No | Documented |

No blocker. Command namespaces: none.

## Interfaces

`liquers-py` only; nothing is `pub` outside the crate except the pyclasses.

```rust
// src/pycall.rs — used by store/ and recipe_provider.rs; command.rs keeps its own entry point
// on the same helpers.
pub(crate) struct PyMethod { object: Py<PyAny>, is_async: bool }
impl PyMethod {
    pub(crate) fn resolve(py: Python<'_>, object: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<Self>>;
}
/// `def`: spawn_blocking + exit::py_call (refused once the interpreter exits); `async def`: a
/// coroutine on `loops`, cancelled on drop (command.rs's CancelOnDrop). `subject` names the
/// store or provider in cancellation and exit messages.
pub(crate) async fn call<A, R>(m: Arc<PyMethod>, loops: Arc<LazyLoopPool>, shut: Arc<Shut>,
    subject: String, args: A, convert: R) -> Result<R::Output, PyFailure>
where A: FnOnce(Python<'_>) -> PyResult<Py<PyTuple>> + Send + 'static,
      R: FromPyReturn + Send + 'static, R::Output: Send + 'static;

// src/store/adapter.rs
struct StoreMethods {               // resolved once, at adaptation
    get: Arc<PyMethod>,
    get_metadata, contains, is_dir, listdir, set, set_metadata, remove, removedir, makedir:
        Option<Arc<PyMethod>>,
    is_supported: Option<Py<PyAny>>,   // a plain callable; a coroutine function is refused
    data: DataAccess,                  // Provided(DataMethods) | Derived | None
    atomic_write: bool,                // the object's `atomic_write` attribute
}
pub(crate) struct PyStore { name: String, prefix: Key, methods: Arc<StoreMethods>,
    loops: Arc<LazyLoopPool>, shut: Arc<Shut>, _handle: LiveHandle }
impl PyStore {
    pub(crate) fn adapt(py: Python<'_>, name: &str, prefix: &Key, object: &Bound<'_, PyAny>,
        loops: Arc<LazyLoopPool>, shut: Arc<Shut>) -> Result<Self, Error>;
}
#[async_trait] impl AsyncStore for PyStore { /* every method, per the table below */ }

// src/store/factory.rs
pub(crate) struct PyStoreType { info: StoreTypeInfo, factory: Py<PyAny>,
    takes_prefix: bool, takes_kwargs: bool, _handle: LiveHandle }
pub(crate) struct PyStoreFactory { types: Vec<Arc<PyStoreType>>, loops: Arc<LazyLoopPool>, shut: Arc<Shut> }
impl StoreFactory for PyStoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo>;
    /// Python::attach (re-entrant: build already holds the GIL); checks required and unknown
    /// `config:` keys, fills defaults, calls the factory, then PyStore::adapt.
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

// src/store/handle.rs
#[pyclass(frozen, name = "Store", module = "liquers")]
pub struct Store { source: StoreSource }
enum StoreSource { Environment(Weak<EnvCell>), Direct { store: Arc<dyn AsyncStore>, loops: Arc<LazyLoopPool> } }
// get, get_bytes, get_metadata, set, set_metadata, remove, removedir, contains, is_dir, listdir,
// makedir, sync, each blocking with an `_async` twin; is_supported and is_read_only (sync only);
// close(); static from_object(obj, prefix=""). An Environment source resolves `cell.envref(py)`
// on the calling thread before an `_async` future is spawned, so a lazy build never runs on a
// runtime worker. A Direct handle owns a private lazy loop pool (started only by an `async def`
// method), stopped by close(), on drop, or at interpreter exit.

#[pyclass(frozen, name = "StoreArgumentInfo", module = "liquers")]
pub struct StoreArgumentInfo(pub liquers_core::store_factory::StoreArgumentInfo);

// src/environment.rs
struct Declarations { /* … */ store_types: Vec<Arc<PyStoreType>> }
// Environment: register_store_factory(type, factory, *, label="", doc="", arguments=None);
//   #[getter] store -> Store; store_types() -> list[dict] (the chain's StoreTypeInfo as JSON)
```

`liquers/_factories.py` `_declare(factory, arguments, *, exempt, reserved)` derives a declaration
from the factory's signature (shared with recipes; stores pass `exempt={"prefix"}`): keyword and
positional-or-keyword parameters become arguments; a default makes one optional and must be JSON;
`bool` is checked before `int`; no annotation or a `None` default gives `Any`; positional-only
parameters and `*args` are refused; `**kwargs` sets `ArgumentCoverage::Partial`; an uninspectable
callable needs `arguments=`; a class is a factory like a function.

**The protocol.** Keys cross as `lq.Key`, data as `bytes`, metadata as `lq.Metadata` (accepted back
also as `lq.MetadataRecord` or `dict`). Metadata that `get` omits is `default_metadata(key, false)`
finalized with `finalize_metadata(update=false)`, as `FileStore::get` does.

| `AsyncStore` method | Python | Absent ⇒ |
|---|---|---|
| `get` | `get(key)` → `bytes`, `(bytes, metadata)`, `None` | adaptation fails |
| `get_metadata` | `get_metadata(key)` → metadata or `None` | from `get`; a directory (`is_dir`) gets directory metadata with `listdir` children |
| `contains` | `contains(key)` | derived from `get_metadata`; `KeyNotFound` → `False` |
| `is_dir` | `is_dir(key)` | `False` without `listdir`; refused at adaptation with `listdir` |
| `listdir`, `set`, `set_metadata`, `remove`, `removedir`, `makedir` | same names | `key_not_supported` naming the method |
| `is_supported` | `is_supported(key)`, a `def` | supported; an exception or interpreter exit → `false` |
| data-only five | same names, all or none | **derived** when `get` and `listdir` exist: `get_data` = `get`'s bytes, `listdir_data` = `listdir`, `set_data` / `remove_data` = `set` with empty metadata / `remove` (else `key_not_supported`), `data_facts` = `{size: len(data), modified: None, etag: None}`; none (`data_access` false) without `listdir` or with `data_access = False` |
| `rename_data` | same | no rename |
| `get_stored_metadata` | same | from a **provided** `get_metadata`, else `Ok(None)` (never key-synthesized) |
| `data_capabilities` | — | `data_access` as above; `atomic_write` from the attribute; `rename`; `modified_time`/`etag` only with a provided `data_facts` |
| `is_read_only` | — | true when every write method is absent |

Under a `metadata` section the pipeline writes reserved names through `set_data` (`__metadata__`
sidecars or folders, `.__staged__` objects, journals: `liquers-core/src/store/storage.rs`,
`writes.rs`), so a Python store with writes must accept them; documented.

**Sync or async.** Every `AsyncStore` method is async and uses `pycall::call`, so a runtime worker
never takes the GIL. `is_supported` and `StoreFactory::create` are synchronous by the trait:
`is_supported` takes the GIL on the calling thread only when the object has the method and the key
is absolute and under the prefix; `create` runs inside `build()`, which holds the GIL.

## Integration Points

- `liquers-py/src/environment.rs`: `Declarations.store_types`; `build()` passes
  `default_store_factory()` chained with `PyStoreFactory` to `with_store_config`; registration marks
  `Pending` and keeps `declarations.store` (Decision 2). **Build guard:** a thread-local list of the
  cells building on this thread, pushed after the lock is taken and popped after `build()`; `envref`,
  `invalidate` and every registration check it before `lock_py_attached` and raise
  `ConfigurationError` for their own cell (another environment is unaffected). A factory that hands
  work for its environment to another thread and joins it still deadlocks: documented.
  `shutdown()` takes `store` and `store_types` out with `mem::take` and drops them after the guard.
  `validate()` calls no factory.
- `liquers-py/src/command.rs`: the call path moves to `src/pycall.rs`; `PyCallable`'s counter becomes
  the shared `LiveHandle`, also counting store objects, store types and recipe providers
  (`live_handle_count()` and the `debug-handles` comment in `liquers-py/Cargo.toml` updated).
- `liquers-py/src/exit.rs`: `py_call(subject, f)`, `attach_tracked` with the `EXITING` check.
- `liquers-py/src/error.rs`: `pyerr_to_store_error(py, err, key, store, write)`.
- `liquers-py/src/lib.rs`, `python/liquers/_factories.py` (new), `__init__.py`, `tests/`.
- No change in `liquers-core`, `liquers-store` or `liquers-lib`.

## Error Handling

| Case | Error |
|---|---|
| Missing `get`, `listdir` without `is_dir`, `async def is_supported`, non-callable attribute | `Error::configuration_error` naming the mount and the method |
| Taken type name (any name `default_store_factory().resolve` accepts, e.g. `filesystem`, `opendal_*`; or a registered Python type) | `configuration_error` at registration |
| Missing required / unknown `config:` key; factory raised or returned an awaitable | `configuration_error` naming mount, prefix and cause, with the traceback |
| Factory uses its environment during the build | `configuration_error("a store factory may not use its environment")` |
| Relative key | `Error::key_not_absolute(key)`, before Python |
| Absent optional method | `Error::key_not_supported(key, "<store> (no '<method>' method)")` |
| `None` from a read | `Error::key_not_found(key)` |
| Raised `LiquersError`; cancellation | its own error; `Error::cancelled` naming the store |
| Other exception; wrong return type | `Error::key_read_error` (reads) or `key_write_error` (writes), with the traceback or the expected type |
| `lq.Store` after `shutdown()` or `close()` | `shut_down_error()` |

## Relevant Commands

None added; no namespace. `store_config` handlers stay Rust-only (`STORE18` `NA`).

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | integrators | py | new §Stores (registration, protocol, derivation, reserved names, errors, threads, staged use, `lq.Store`); §3 Store; §8 limits |
| `specs/guides/PYTHON_GUIDE.md` | guide | Python users | py | new chapter "Stores" |
| `specs/reference/STORE_CONFIG_FSD.md` | reference | store authors | store/config | integration-registered types in `store_types()` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | py, web | §STORE "cannot contribute a store type" limited to JavaScript; §6 Python |
| `liquers-py/README.md` | readme | users | py | `STORE` row |
| docstrings in `src/store/*.rs`, `_factories.py` | API | Python users | py | every class and method |
| `specs/README.md` | map | all | docs | the line becomes "built" (Phase 5) |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `environment.rs`, `command.rs`, `exit.rs`, `error.rs`, `lib.rs`, new `store/`, `pycall.rs`, `_factories.py`, `Cargo.toml` comment |
| Crates and workflows affected | `liquers-py`; its CI (3.10, 3.14, 3.14t) |
| Existing tests likely to change | `test_RUNTIME_runtime.py` handle counts; command tests after the `pycall` extraction |
| New validation | `test_STORE_*.py` (per store), Rust unit tests in `store/` |
| Compatibility / data / concurrency / performance / security | New API only. No adapter lock (`STORE06`). A derived `contains` doubles `get`; `is_supported` takes the GIL per routed key. A store or factory holding its own `lq.Environment` is a cycle only `shutdown()` breaks. Relative keys refused before Python |
| Recovery | Revert the PR; nothing persistent changes |
| Certainty and open questions | High for the adapter (mirrors `JsStore`); medium for the build guard, tested on 3.14t |
