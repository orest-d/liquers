# Phase 2: Solution & Architecture — liquers-py-data-nodes

## Overview

A new `liquers-py/src/data_node.rs` holds two things. `PyNode` is a core `DataNode` over a
`Py<PyAny>`: it classifies the object once, when the node is built, and reads containers live under
the interpreter lock. `lq.DataNode` is the outbound view: a root plus a path, re-resolved on each
access. `PyObjectValue` overrides the `ForeignValue::data_node` hook of `foreign-value-data-node`
to return a `PyNode`. `lq.State` keeps its core state and converts its value on first access, so a
view of a state never converts it (AC-11).

- A view's root is a Python value (a `PyNode`), or a core state read with no context, in the
  running command's context, or in the environment's.
- An async read at the top level runs in a fresh closure asset (`ad-hoc-closure-asset`).

Rejected:
- A snapshot node (convert, then `serde_json::Value`): it fails on one bad member, which is the
  problem.
- Classifying on every lookup: a `kind` that could change between two calls breaks the renderer's
  section logic, and Phase 1 question 5 fixes the kind at build.
- A view holding a node with a context inside (a `FolderNode` built at the top level): the node
  outlives its asset (Phase 1 question 6).
- One Python `__liquers_data__` hook for both forms, called on every lookup: the method form is a
  snapshot by decision (question 5).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `FOREIGN-VALUE-HAS-NO-DATA-NODE` (design `foreign-value-data-node`) | draft, design ready | P2 | The hook `PyObjectValue` overrides | Yes | Yes | Own PR first (step 1 of Phase 4) |
| `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` (design `ad-hoc-closure-asset`) | draft, design approved | P2 | `env.data_node` async reads | Yes | Yes, for AC-12 only | Own PR first |
| `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE` | draft | P3 | `lq.data_node(state)` of a large base value copies it once | No | No | Documented cost |
| `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM` | draft | P3 | An `lq.Query` / `lq.Key` member renders as JSON | No | No | Phase 1 overlap; unchanged here |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | New classes need reprs | No | No | `DataNode(Map)`, `NodeKind.Map` |

Command namespaces: none (no command is added; the `ns-template` commands read Python data
through the hook).

## Interfaces

**The adapter** (`liquers-py/src/data_node.rs`). Sync: `DataNode` is a sync trait.

```rust
/// A Python object read in place. Built under the GIL; reads take it again per lookup.
pub(crate) struct PyNode { object: Py<PyAny>, shape: Shape }
enum Shape {
    Dict,                              // exact `dict`, read with `PyDict` access, no Python code
    Sequence,                          // exact `list` / `tuple`, `PyList` / `PyTuple` access
    ProtocolMap,                       // `__liquers_data__ = True` with `keys` and `__getitem__`
    ProtocolList,                      // `__liquers_data__ = True` with `__len__` and `__getitem__`
    Scalar(Scalar<'static>),           // read once: str, int, float, bool, None, `__index__`, `__float__`
    Delegate(Arc<dyn DataNode>),       // lq.Query, lq.Key, lq.RecordView: their value's own node
    Opaque(Option<Error>),             // the error `to_json` raises, when there is a specific one
}
impl DataNode for PyNode { /* every method; `as_any` stays `None` */ }

/// Builds the node of `object` (Phase 1 question 1's table, then question 2's opt-in).
pub(crate) fn node_of(py: Python<'_>, object: &Bound<'_, PyAny>) -> PyNode;
```

- **Classification** (`node_of`), in order: `None`; `bool` before `int`; `int` read as `i64`, else
  `Opaque` with the `ConversionError` that `py_to_simple` raises (one shared constructor); `float`
  (a non-finite one keeps its `Scalar::Float`, and `to_json` raises `ConversionError`); `str`;
  `bytes` / `bytearray` / `memoryview` `Opaque`; the three wrappers; exact `dict`; exact `list` /
  `tuple`; then the opt-in. Scalar subclasses (an `IntEnum`) are read as their base type, as
  conversion does; container subclasses are not (Decision 1).
- **Opt-in.** `__liquers_data__` is looked up on the type (`type(obj).__liquers_data__`), so an
  instance `__getattr__` never runs. `True` picks the protocol shape (map before list before
  `__index__` before `__float__`; none of them gives `Opaque`). A callable is called once on the
  instance; its result is classified with the method form ignored, so a method returning another
  opted-in object reads its protocols, never loops. Any other value, or a raise, gives `Opaque`.
- **Lookups.** `get(name)` on `Dict` is `PyDict::get_item`; on `ProtocolMap` it is `obj[name]`,
  where `KeyError` means `None` and any other exception gives an `Opaque` child (AC-13). `index(i)`
  likewise with `IndexError`. `keys()` reads all `str` keys in one hold into `Vec<String>` (non-`str`
  keys skipped). `items()` of a list yields `index(0..)` until the first `None`, one hold per item,
  so a list shortened mid-read ends early. `len()` is live. Children are `NodeRef::Owned` for
  containers, `NodeRef::Scalar` for scalars, `NodeRef::Shared` for delegates.
- **`to_json`** walks in one hold with an identity set on its path. A cycle, a non-`str` key (named),
  an integer outside `i64`, a non-finite float or an `Opaque` member raises `ConversionError`
  (`Error::conversion_error_with_message`), never Python's `RecursionError` (AC-6).
- **The lock.** Every lookup goes through `attach_off_worker` (`value.rs`, made `pub(crate)`):
  `block_in_place` on a runtime worker, a plain attach elsewhere (Phase 1 question 5).

**The hook** (`liquers-py/src/value.rs`). `impl ForeignValue for PyObjectValue` gains
`fn data_node(self: Arc<Self>) -> AnyNode`, returning `AnyNode::Sync(Arc::new(node_of(…)))` under
`attach_off_worker`. Sync, as the hook is.

**The view** (`data_node.rs`). Python-facing; the sync accessors are sync, the `_async` ones return
awaitables (`aio::awaitable`).

```rust
#[pyclass(frozen, eq, eq_int, name = "NodeKind", module = "liquers")]
pub enum NodeKind { Null, Bool, Integer, Float, Text, List, Map, Opaque }

#[pyclass(frozen, name = "DataNode", module = "liquers")]
pub struct DataNode { root: Arc<Root>, path: Arc<[Step]>, kind: liquers_core::data_node::NodeKind }
enum Step { Key(String), Index(usize) }
enum Root {
    Node(AnyNode),                                        // lq.data_node(x), State.data_node()
    Command { state: CoreState<Value>, context: crate::context::Context }, // context.data_node
    Env { state: CoreState<Value>, scope: Scope },        // env.data_node; `Scope::from_envref`
}
```

Python members: `kind`; `scalar` (`None` for a non-scalar; `Scalar::UInt` gives an `int`);
`node[key]` / `node[i]` (`KeyError` / `IndexError`); `get(key_or_index, default=None)`; `keys()`;
`len(node)` (`TypeError` for a scalar); iteration (child views of a list, the keys of a map);
`bool(node)` (`is_truthy`); `to_json()` (through `store::json_to_py`); `get_async`, `scalar_async`,
`len_async`, `keys_async`, `to_json_async`; `__repr__` `DataNode(Map)`.

- **Resolution.** A sync accessor resolves the root (below), walks the path with
  `AnyNode::as_sync` and `DataNode::get` / `index`, and raises `lq.NotSupportedError` at the first
  async node, naming the `_async` form. An async accessor walks with `AsyncDataNode::get` / `index`
  where needed. `kind` re-resolves for a view reached synchronously and is fixed for one reached
  through an await.
- **Roots.** `Node`: the node itself. `Command`: `ContextHost::new(core_context, Key::new())
  .node_of(&state)` while the `lq.Context` handle is valid, `GeneralError` after
  (`crate::context::Context::core`). `Env`: synchronously `Value::data_node(value)` when
  `liquers_lib::template::host::context_node_kind(&value)` is `None`, else the root is async;
  asynchronously, every read runs one closure asset whose body builds
  `ContextHost::node_of(&state)` in the asset's context and walks the path there. A sync result
  comes back as a `Root::Node` view (a sync node holds no context: only `FolderNode` and
  `RecordSourceNode` do), an async one as an `Env` view with the longer path.
- **Entry points.** `lq.data_node(x)`: an `lq.State` gives `Root::Node(Value::data_node(core))`,
  anything else `Root::Node(node_of(x))`. `State.data_node()` is the same for its state.
  `Context.data_node(x)` and `Environment.data_node(x)` take a Python value (the adapter) or an
  `lq.State` (the `Command` / `Env` root). Sync, never blocking: only an `Env` async read waits,
  and it is an awaitable.

**The closure runner** (`liquers-py/src/closure.rs`, new, shared with `liquers-py-templates`):

```rust
/// Waits for `run`, a closure asset's run (`ClosureAsset::run`, `TopLevelRender::run`), spawned
/// on the liquers-py runtime, with the GIL released (`block_on`). On an interrupt or shutdown it
/// cancels `asset`, waits for the spawned run to settle, then raises: dropping a run is not
/// cancelling it (`ad-hoc-closure-asset`).
pub(crate) fn wait_for_run<T: Send + 'static>(py: Python<'_>, shut: &Arc<Shut>,
    asset: AssetRef<PyEnvironment>,
    run: impl Future<Output = Result<Option<T>, Error>> + Send + 'static)
    -> Result<Option<T>, PyFailure>;
```

The async forms pass the run to `aio::awaitable`, which already spawns it; cancelling the
awaitable ends only the wait and the asset runs to its end (the M3 rule).

**State** (`liquers-py/src/asset.rs`). `State` keeps `core: CoreState<Value>` and `scope: Scope`;
`value` converts on first access into a `pyo3::sync::PyOnceLock<Py<PyAny>>` and raises the
conversion's error there; `record_view()` reads `core`. `State::from_core` no longer converts and
cannot fail on the value. Gains `data_node()`.

**liquers-lib** (`template/host.rs`): `pub fn context_node_kind(value: &Value) -> Option<NodeKind>`,
the kind `ContextHost::node_of` gives a value that needs a context (`Map` for a directory listing,
`List` for a record source) and `None` otherwise; `inner_node_of` uses it, so the two cannot
disagree.

Commands: none. ExtValue variants: none. Python stubs: `NodeKind`, `DataNode`, `data_node`, the
new methods.

## Integration Points

- `liquers-py/src/data_node.rs` (new): `PyNode`, `node_of`, `NodeKind`, `DataNode`, `data_node`.
- `liquers-py/src/closure.rs` (new): `wait_for_run`.
- `liquers-py/src/value.rs`: the `data_node` hook; `attach_off_worker` and the integer
  `ConversionError` constructor made `pub(crate)`.
- `liquers-py/src/asset.rs`: lazy `State`; `data_node()`. Call sites of `State::from_core`
  (`environment.rs` `evaluate_state`, `aio.rs` `ToPy::State`, `command.rs` line ~357) drop the `?`
  on conversion.
- `liquers-py/src/context.rs`, `environment.rs`: `data_node`.
- `liquers-py/src/lib.rs`: register the classes and `data_node`.
- `liquers-py/python/liquers/__init__.py`, `__init__.pyi`, `_native.pyi`: names and stubs.
- `liquers-lib/src/template/host.rs`: `context_node_kind`.
- `liquers-py/tests/test_data_nodes.py` (new).

## Error Handling

| Case | Error |
|---|---|
| `to_json` on a cycle, a non-`str` key, an out-of-range `int`, a non-finite `float`, an opaque member | `Error::conversion_error_with_message` → `lq.ConversionError` |
| A sync accessor reaching an async node | `Error::not_supported` → `lq.NotSupportedError` |
| A command view used after the command returned | `Error::general_error` (as `lq.Context`) |
| An `Env` view after shutdown or with the environment gone | `shut_down_error()` |
| `node[k]` missing, `node[i]` out of range, `len` of a scalar | Python `KeyError`, `IndexError`, `TypeError` |
| A raise in an opted-in protocol | none: that node is `Opaque` (no error channel in `DataNode`) |
| `state.value` that cannot be converted | the conversion's error, now at access |

## Relevant Commands

None new. The `ns-template` commands (`render`, `expand`) read a retained Python value through the
hook, unchanged (`liquers-lib/src/template/commands.rs` `data_node`).

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | integrators | New §Data nodes: mapping table, opt-in, live view, cycles, the lock, the three roots; §5 notes lazy `State.value` |
| `specs/guides/PYTHON_GUIDE.md` | guide | Python users | "Data nodes" section: what a template sees of your data, `__liquers_data__`, views |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | §DATANODE and §6 Python: the answers and their deviations |
| `specs/guides/TEMPLATE_GUIDE.md` | guide | template authors | A line: Python data is read in place |
| `liquers-py/README.md` | readme | users | DATANODE row |
| `specs/README.md` | map | all | design status |
| `docs/guides/python/` chapters | user guide | Python users | per `docs_index.py chapters-for` (Phase 5) |

Module docs: `data_node.rs` and `closure.rs` (new); `value.rs`, `asset.rs`, `context.rs`,
`environment.rs` (their lists of members); `liquers-lib/src/template/host.rs`.

## Risks

| Risk | Mitigation |
|---|---|
| Lazy `State.value` moves a conversion error from `evaluate_state` to `.value` | Stated in §5 and the History row; a test pins it; `evaluate` is unchanged (it converts at once) |
| `block_in_place` per lookup is slow on a large render | Measured in Phase 3 (a 10 000-entry render); `keys` takes one hold |
| An opted-in `__getitem__` re-entering Liquers mid-render | Allowed: every liquers-py wait releases the lock (Phase 1 question 5); a test renders from a `__getitem__` that evaluates a query |
| A sync node holding a context breaks the `Env` result rule | Today only `FolderNode` and `RecordSourceNode` hold one, and both are async; `context_node_kind` names them, and a new one changes it there |
| `PyOnceLock` in a frozen class on free-threaded builds | `PyOnceLock` is thread-safe; the 3.14t suite runs the tests |
