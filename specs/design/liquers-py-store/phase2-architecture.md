# Phase 2: Solution & Architecture — liquers-py-store

## Overview

A new `liquers-py/src/store/` module holds three things: `PyStore`, an `AsyncStore` adapter over a
Python object whose protocol is resolved once into a `StoreMethods` table; `PyStoreFactory`, one
`StoreFactory` holding every Python store type of an environment; and `lq.Store`, the Python handle
on a built store. `EnvCell` keeps the registered factories among its declarations and chains
`PyStoreFactory` **after** `liquers_store::store_factory::default_store_factory()`. `read_only` and
`metadata` sections are applied by `StoreRouterBuilder` unchanged; the adapter supplies derived
data-only access (Decision 5). Calls into Python share one helper with the recipe adapter
(`src/pycall.rs`), extracted from `src/command.rs`.

Rejected: a fixed `python` type with an object registry (Decision 1); a Python subclass of a native
`lq.StoreBase` (the protocol is duck-typed, as for commands, and a base class would hide the
"absent method" rule); calling `is_supported` asynchronously (the trait method is synchronous).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED` | draft | P2 | Same derivation rule for web (Decision 3) | No | No | Own PR after this design (Phase 4 last step) |
| `WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY` | draft | P3 | Python seeds metadata from the key; web may not | No | No | Verify and fix in its own PR |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | The adapter checks `Key::as_absolute` itself | No | No | Deferred past M4 |
| `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` | draft | P3 | `STORE18` `NA` | No | No | Linked as the `NA` reason |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | Python half absorbed | No | No | Narrowed to web in Phase 5 |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | No `close()` call | No | No | Documented |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | A late registration rebuilds | No | No | Documented (Decision 2) |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | Error map uses today's types | No | No | Revisit if it lands |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` | draft | — | Bytes copied at the boundary | No | No | Documented |

No blocker. Command namespaces: none (no command is added).

## Interfaces

Rust, `liquers-py` only. Nothing is `pub` outside the crate except the pyclasses.

```rust
// src/pycall.rs — shared by store/ and recipe_provider.rs; command.rs keeps its own entry point.
pub(crate) struct PyMethod { object: Py<PyAny>, is_async: bool }
impl PyMethod {
    /// Looks `name` up on `object`; `None` when absent. Refuses a non-callable attribute.
    pub(crate) fn resolve(py: Python<'_>, object: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<Self>>;
}
/// `def`: spawn_blocking + attach_tracked; `async def`: coroutine on the env's LazyLoopPool,
/// cancelled on drop (the CancelOnDrop pattern of command.rs).
pub(crate) async fn call<A, R>(m: &PyMethod, loops: &Arc<LazyLoopPool>, shut: &Arc<Shut>,
    args: A, convert: R) -> Result<R::Output, PyFailure>
where A: FnOnce(Python<'_>) -> PyResult<Py<PyTuple>> + Send + 'static, R: FromPyReturn;

// src/store/adapter.rs
struct StoreMethods {               // resolved once, at adaptation
    get: PyMethod,                  // mandatory
    get_metadata: Option<PyMethod>, contains: Option<PyMethod>, is_dir: Option<PyMethod>,
    listdir: Option<PyMethod>, set: Option<PyMethod>, set_metadata: Option<PyMethod>,
    remove: Option<PyMethod>, removedir: Option<PyMethod>, makedir: Option<PyMethod>,
    is_supported: Option<Py<PyAny>>, // must be a plain callable; a coroutine function is refused
    data: DataMethods,              // provided all-or-none, else derived (Decision 5)
    data_access: bool, atomic_write: bool,   // object attributes, read once
}
pub(crate) struct PyStore { name: String, prefix: Key, methods: Arc<StoreMethods>,
    loops: Arc<LazyLoopPool>, shut: Arc<Shut>, _handle: LiveHandle }
impl PyStore {
    pub(crate) fn adapt(py: Python<'_>, name: &str, prefix: &Key, object: &Bound<'_, PyAny>,
        loops: Arc<LazyLoopPool>, shut: Arc<Shut>) -> Result<Self, Error>;
}
#[async_trait] impl AsyncStore for PyStore { /* every method; see the protocol table */ }

// src/store/factory.rs
pub(crate) struct PyStoreType { info: StoreTypeInfo, factory: Py<PyAny>,
    takes_prefix: bool, takes_kwargs: bool, _handle: LiveHandle }
pub(crate) struct PyStoreFactory { types: Arc<Vec<PyStoreType>>, loops: Arc<LazyLoopPool>, shut: Arc<Shut> }
impl StoreFactory for PyStoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo>;
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>; // attaches, checks
        // required/unknown keys, fills defaults, calls the factory, PyStore::adapt
}

// src/store/handle.rs
#[pyclass(frozen, name = "Store", module = "liquers")]
pub struct Store { source: StoreSource }
enum StoreSource { Environment(Weak<EnvCell>), Direct(Arc<dyn AsyncStore>) }
// Methods (each blocking, with an `_async` twin through aio::awaitable): get, get_bytes,
// get_metadata, set, set_metadata, remove, removedir, contains, is_dir, listdir, makedir,
// is_supported (sync only), is_read_only (sync only), sync. Static: from_object(obj, prefix="").

#[pyclass(frozen, name = "StoreArgumentInfo", module = "liquers")]
pub struct StoreArgumentInfo(pub liquers_core::store_factory::StoreArgumentInfo);

// src/environment.rs — EnvCell / Environment additions
struct Declarations { /* … */ store_types: Vec<PyStoreType> }
impl Environment {
    fn register_store_factory(&self, py, store_type: &str, factory: Bound<PyAny>,
        label: &str, doc: &str, arguments: Option<Vec<StoreArgumentInfo>>) -> PyResult<()>;
    #[getter] fn store(&self) -> Store;
    /// Every store type the chain declares, built-in and Python, as dicts (StoreTypeInfo JSON).
    fn store_types(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>>;
}
```

`register_store_factory` derives `arguments` in Python (`liquers/_stores.py`
`_declare(factory, arguments)`, reusing `_commands._type_hints` and `_argument_type`): keyword and
positional-or-keyword parameters become arguments, `prefix` excepted; a default makes one optional
and must be JSON; `bool` is checked before `int`; no annotation or a `None` default gives `Any`;
positional-only parameters and `*args` are refused; `**kwargs` sets `ArgumentCoverage::Partial`;
an uninspectable callable needs `arguments=`. A class is a factory like a function.

**Protocol, as the adapter calls it.** Keys cross as `lq.Key`, data as `bytes`, metadata as
`lq.Metadata` (accepted back also as `lq.MetadataRecord` or `dict`).

| `AsyncStore` method | Python method | Absent ⇒ |
|---|---|---|
| `get` | `get(key)` → `bytes`, `(bytes, metadata)`, `None` | registration fails |
| `get_metadata` | `get_metadata(key)` → metadata or `None` | from `get`; a directory (`is_dir`) gets directory metadata with `listdir` children |
| `contains` | `contains(key)` | `get_metadata` not absent; `KeyNotFound` → `False` |
| `is_dir` | `is_dir(key)` | `False` if `listdir` is absent; refused at registration if `listdir` exists |
| `listdir`, `set`, `set_metadata`, `remove`, `removedir`, `makedir` | same names | `Error::key_not_supported` naming it |
| `is_supported` | `is_supported(key)` → bool, a `def` | supported; an exception → `false` |
| `get_data` … `data_facts` | same names | derived (Decision 5) unless `data_access = False` |
| `rename_data`, `get_stored_metadata` | same names | no rename / derived from `get_metadata` |
| `data_capabilities` | — | from the table: `data_access`, `atomic_write`, `rename`, `modified_time`/`etag` only when `data_facts` is provided |
| `is_read_only` | — | true when every write method is absent |

Metadata that `get` omits is `self.default_metadata(key, false)` finalized with
`finalize_metadata(update=false)`, as `FileStore::get` does, so the data format comes from the key.

**Sync or async.** Every `AsyncStore` method is async and calls Python through `pycall::call`:
a `def` on `spawn_blocking` (the GIL is never taken on a runtime worker), an `async def` on the
environment's loop pool. `is_supported` and `StoreFactory::create` are synchronous by the trait:
`is_supported` takes the GIL on the calling thread, only for a key under the mount's prefix that
is absolute; `create` runs during `build()`, which already holds the GIL.

## Integration Points

- `liquers-py/src/environment.rs`: `Declarations.store_types`; `build()` passes
  `default_store_factory()` chained with `PyStoreFactory` to `with_store_config`; registration
  marks the environment `Pending` and keeps `declarations.store` (Decision 2); a thread-local
  `BUILDING` marker set around `build()` makes `envref`, `invalidate` and every registration on
  that thread fail with `ConfigurationError` instead of re-taking the lock; `shutdown()` moves
  `store_types` out and drops them after the guard. `validate()` does not call factories.
- `liquers-py/src/command.rs`: the blocking/coroutine call path moves to `src/pycall.rs`;
  `PyCallable`'s counter becomes the shared `LiveHandle` (also counting store objects, store
  types and recipe providers), so `live_handle_count()` covers them.
- `liquers-py/src/error.rs`: `pyerr_to_store_error`.
- `liquers-py/src/lib.rs`: registers `Store`, `StoreArgumentInfo`.
- `liquers-py/python/liquers/_stores.py` (new), `__init__.py` exports; `liquers-py/tests/`.
- No change in `liquers-core`, `liquers-store` or `liquers-lib`.

## Error Handling

| Case | Error |
|---|---|
| Missing `get`, `listdir` without `is_dir`, `async def is_supported`, non-callable attribute | `Error::configuration_error` naming the mount (the store name) and the method |
| Taken type name (any name `default_store_factory().resolve` accepts, or a registered Python type) | `ConfigurationError` at registration |
| Missing required / unknown `config:` key; factory raised; factory returned an awaitable | `Error::configuration_error` naming the mount, prefix and cause (Python traceback kept) |
| Environment used by a factory during its build | `Error::configuration_error("a store factory may not use its environment")` |
| Relative key | `Error::key_not_absolute(key)` before Python |
| Absent optional method | `Error::key_not_supported(key, name)` with the method named in the message |
| `None` from a read | `Error::key_not_found(key)` |
| Raised `LiquersError` / cancellation | its own error / `Error::cancelled` (as `pyerr_to_error`) |
| Other exception | `Error::key_read_error` (reads) or `Error::key_write_error` (writes), with the traceback |
| Wrong return type | `Error::key_read_error` naming the expected type |
| `lq.Store` call after `shutdown()` | `shut_down_error()` |
| Unparsable `data_facts.modified` | `key_read_error`, from `BackendTime::parse` |

## Relevant Commands

None added; no namespace. `store_config` handlers stay Rust-only (`STORE18` `NA`).

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | new §Stores: registration, protocol table, derivation, errors, threads, staged use, `lq.Store`; §3 Store paragraph; §8 limitations |
| `specs/guides/PYTHON_GUIDE.md` | guide | new chapter "Stores": a store and its factory, mounting, Rust stores, `metadata` sections, `env.store` |
| `specs/reference/STORE_CONFIG_FSD.md` | reference | integration-registered types appear in `store_types()` (Python) |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | §STORE "What the language cannot contribute" limited to JavaScript; §6 Python store notes |
| `liquers-py/README.md` | readme | `STORE` row: 01-17 per store, 12 override half and 18 `NA` |
| docstrings in `src/store/*.rs`, `_stores.py` | API | every class and method (`test_DOCS_docstrings.py`) |
| `specs/README.md` | map | the line becomes "built" in Phase 5 |

`affects_docs`: as in `DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `environment.rs`, `command.rs`, `error.rs`, `lib.rs`, new `store/`, `pycall.rs`, `_stores.py` |
| Crates and workflows affected | `liquers-py`; the liquers-py CI workflow (3.10, 3.14, 3.14t) |
| Existing tests likely to change | `test_RUNTIME_runtime.py` handle counts (more kinds counted); command tests after the `pycall` extraction |
| New validation | `test_STORE_*.py` (STORE01-17 per store), Rust unit tests in `store/` |
| Compatibility / data / concurrency / performance / security | New API only. Concurrency: no adapter lock (`STORE06`), free-threaded runs exercise it. Performance: a derived `contains` doubles `get`; `is_supported` takes the GIL per routed key. Security: relative keys refused before Python |
| Recovery | Revert the PR; nothing persistent changes |
| Certainty and open questions | High for the adapter (mirrors `JsStore`); medium for the `BUILDING` guard on free-threaded Python, tested in Phase 3 |
