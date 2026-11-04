# Phase 1: High-Level Design — liquers-py-data-nodes

## Purpose

Let core read Python data in place as a `DataNode`, without converting it first: a template reads
the readable fields of a `dict` that conversion refuses, and three fields of a large structure cost
three lookups, not a copy. Let Python read one field of a Liquers value without converting it. The
`DATANODE` feature of `LANGUAGE-INTEGRATION_GUIDE.md`; milestone M6, with
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md) and
[`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` until Phases 2-4 are written and reviewed; no user decision is
  open (Orest decided question 2 at the gate; the rest are proposed resolutions)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python classes.
- **Leading issue:** question 1's two deviations from the guide. Source issue
  `PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS` (P2 L, filed with this design).
- **Explanation:** `DataNode`, `AnyNode`, `NodeRef` and `Scalar` exist in `liquers-core/src/data_node.rs`.
  `PyObjectValue` (`liquers-py/src/value.rs`) is a `ForeignValue`, which has no hook to answer as a
  node. That hook is `foreign-value-data-node`, a small and ready core design that lands first in
  its own PR.
- **Open questions** (the guide's six; 2 decided, the others proposed resolutions):
  1. **Proposed resolution: the mapping** (guide question 1). `None` → `Null`; `bool` →
     `Bool`, tested before `int`; `int` → `Integer` within `i64`; `float` → `Float`, and a
     non-finite one fails `to_json`; `str` → `Text`; `bytes`, `bytearray` and `memoryview` →
     `Opaque`, never `Text`; exact `dict` → `Map` in insertion order; exact `list` and `tuple` → `List`, with
     random access. Liquers wrappers that hold a value without copying it (`lq.Query`, `lq.Key`,
     `lq.RecordView`) answer through that value's own node. Every other object is `Opaque` unless its class opts in (question 2). One table beside VALUE's table. Two
     deliberate differences from conversion, which keeps a refused structure as `py.Object`,
     never raising in permissive mode:
     - **An integer above `i64::MAX`.** VALUE refuses it with `ConversionError`, but a `DataNode`
       lookup cannot fail. So its node is `Opaque`, and `to_json` raises that same
       `ConversionError`. This is a stated deviation from the guide's "the same conversion error",
       which no infallible lookup can meet. Alternative: answer with the existing `Scalar::UInt` up
       to `u64::MAX`, a policy choice that makes the node accept what VALUE refuses.
     - **A non-`str` key.** It is not visible through `get` or `keys`. `to_json` raises
       `ConversionError` naming the key.
  2. **Decided (Decision 1): standard types, and classes that opt in to their protocols**
     (guide question 2). Without opting in, only the standard types of question 1 read as data:
     exact `dict`, `list` and `tuple` through their data (`PyDict` / `PyList` access), so no
     Python code runs (DATANODE07). Any other object is `Opaque`, a `dict` subclass included. A
     class opts in through `__liquers_data__`, in one of two forms:
     - `__liquers_data__ = True`: read through its protocols. It is a `Map` when it has `keys()`
       and `__getitem__`, a `List` when it has `__len__` and `__getitem__` but no `keys()`, and an
       `Integer` or `Float` through `__index__` or `__float__`.
     - `def __liquers_data__(self)`: returns the data to read (a dataclass, say). It is called
       when the node is built.
     Either form runs Python code mid-render, possibly reentrantly. The guide allows that when it
     is documented, and the docs say so. An exception in such a lookup makes that node `Opaque`.
     Third-party types (pandas, numpy) are not patched by liquers-py; a user subclasses or wraps
     them to opt in. Rejected: an opt-out, which would run code of every object with a
     `__getitem__`.
  3. **Proposed resolution: live view** (guide question 3). Each lookup reads the structure as it is at
     that moment, so a mutation mid-render shows (DATANODE09). A caller who needs consistency
     passes a copy, and the docs say so.
  4. **Proposed resolution: cycles** (guide question 4). Navigation is lazy, so a cyclic structure is
     navigable to any finite depth. `to_json` tracks object identities on its path and raises
     `ConversionError` on a cycle, never `RecursionError` (DATANODE06).
  5. **Proposed resolution: threads and the lock** (guide question 5). The adapter holds `Py<PyAny>` and
     takes the interpreter lock for each lookup (`Python::attach`), on whatever worker polls the
     reader. This is safe because every liquers-py wait releases the lock (DATANODE10). Waiting for
     the lock on a worker is a blocking wait in async code, bounded by the longest hold of the GIL,
     such as a long synchronous command. So each lookup runs under `tokio::task::block_in_place`,
     as liquers-py's codecs already do, and `keys` reads every key in one hold. A node wraps one
     Python object, whose type never changes, so its `kind` is stable; only the contents are live.
     The opt-in number protocols (`__index__`, `__float__`) are called once when the node is
     built and the scalar is cached, so a later raise cannot leave an `Integer` with no scalar.
     The method form of `__liquers_data__` is also called once, at build time, so its result is a
     snapshot, not a live view. A raise in either makes that node `Opaque`, which the docs state,
     because `DataNode` has no error channel. On free-threaded builds a `dict` read is atomic per lookup. The cost
     per lookup is measured on a large render in Phase 3.
  6. **Proposed resolution: the outbound view** (guide question 6). Yes: `lq.data_node(x)` returns an
     `lq.DataNode` for a Python value (the adapter) or for an `lq.State` (its value's node,
     `Value::data_node`, always synchronous). It holds the root `AnyNode` plus a path and
     re-resolves on each access. It offers `kind` (`lq.NodeKind`), `scalar`, `[key]` / `[i]`,
     `get`, `keys()`, `len`, iteration and `to_json()`. Async nodes (a store folder, a record
     source) need a context, because `ContextHost::node_of` puts one inside them.
     `context.data_node(state)` inside a command uses the command's context; that view is valid
     only while the command runs, and raises `GeneralError` after, like `lq.Context` itself. At the
     top level, `env.data_node(state)` keeps the `State` plus a path, never a node holding a
     context: each async read runs a fresh ad-hoc asset through the core primitive of
     `liquers-py-templates` Decision 1 (`AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`), builds the node in
     it, and walks the path, so its log, dependencies and cancellation stay inside a live asset.
     Async nodes are read with `await node.get_async(…)`, and a sync accessor on one raises
     `lq.NotSupportedError` (DATANODE11). `lq.data_node(state)`, with no environment, keeps
     `Value::data_node`, where a record source is `Opaque`, as in Rust.

## Decisions

1. Orest, 2026-11-04, at the Phase 1 gate, on question 2, quoted: "data nodes should be supported for
standard python types + for classes supporting suitable protocols (e.g. getitem, len...). I'd say
classes should be able to opt out rather than opt in." After the trade-offs (an opt-out runs code of
every object with a `__getitem__`): "ok, I agree with opt in - that is better". Taken as Decision 1
(question 2): standard types, and classes that opt in to their protocols. Landing
order data nodes, templates, pipelines: "ok".
2. Orest, 2026-11-04: "proceed all": Phases 2-5 pre-approved; no decision was open.

## Scope Changes

- **2026-11-04** — Phase 2, under the pre-approval: two changes AC-11 and AC-12 need.
  (1) `lq.State` converts its value on first access, so `lq.data_node(state)` of a polars frame never
  imports polars; a conversion error now raises at `state.value`, not at `evaluate_state`
  (`evaluate` is unchanged). (2) `liquers-lib/src/template/host.rs` gains `context_node_kind`, the
  kind `ContextHost::node_of` gives a value that needs a context, shipped in this design's PR. No new
  criterion; size stays `L`. Phases: 1, 2. Approved: under the pre-approval (Decision 2).

## Problem Example

```python
import liquers as lq
from datetime import datetime
env = lq.Environment()            # templates/name.md = "{{name}}"

@env.first_command
def person() -> object:
    return {"name": "Ann", "when": datetime.now()}

env.evaluate("person/ns-template/render-~X~-R/templates/name.md~E")
```

**Today** the `datetime` makes structural conversion refuse the whole `dict`. It is kept as
`py.Object`, whose node is `AnyNode::opaque()`, so the render returns `""` with no warning. **After**
the render returns `"Ann"`: `PyObjectValue` answers through the adapter, `name` is read, and only
`when` is opaque. `lq.data_node(d)["when"].kind` is `lq.NodeKind.Opaque`. The query validates
(`liquers-validate --command person`, `Ok`).

## Scope and Acceptance Criteria

- **AC-1** Nested data answers per the mapping
  WHEN `lq.data_node({"a": [1, {"b": "x"}], "c": None})` is navigated
  THEN kinds, fields, items, lengths and keys follow question 1's table, and a missing key is `None`, not an error (DATANODE01)
- **AC-2** The adapter agrees with conversion
  WHEN a corpus of convertible structures is read through the adapter and through its converted value
  THEN `to_json()` and the sorted keys are equal (DATANODE02)
- **AC-3** Scalar edges
  WHEN `True`, `2**63 - 1`, `2**63`, `2**64` and `float("nan")` are read, alone and as `dict` members
  THEN `True` is `Bool` and `2**63 - 1` is exact; the two larger integers are `Opaque`, and their `to_json()` raises the `ConversionError` that conversion raises; NaN's `to_json()` raises `ConversionError` (DATANODE03, question 1)
- **AC-4** Bytes are opaque
  WHEN `b"abc"` is read
  THEN its kind is `Opaque` (DATANODE04)
- **AC-5** A refused structure still reads
  WHEN the problem example's `dict`, with an added key `1`, is read through the adapter
  THEN `name` reads `"Ann"` and `when` is `Opaque` (DATANODE05)
- **AC-6** Cycles navigate and refuse JSON
  WHEN a `dict` containing itself is navigated three levels deep, then `to_json()` is called
  THEN navigation reads the name, and `to_json()` raises `ConversionError` (DATANODE06)
- **AC-7** Lookups run only opted-in code
  WHEN a plain `dict`, a `dict` subclass and a class with recording `keys` / `__getitem__` are read, then the class with `__liquers_data__ = True`
  THEN the first is a `Map`, the next two are `Opaque` and nothing is recorded, and the opted-in class is a `Map` read through its methods (DATANODE07, Decision 1)
- **AC-8** A retained value reads in place
  WHEN the problem example's query is evaluated, with an opted-in mapping class, and with a dataclass that defines `__liquers_data__()`
  THEN all three renders give `"Ann"` (DATANODE08)
- **AC-9** The view is live
  WHEN a `dict` is changed between two lookups through one node
  THEN the second lookup shows the new value (DATANODE09)
- **AC-10** A worker lookup does not deadlock
  WHEN a render whose data is a Python `dict` runs while the calling thread waits in `evaluate`, under a 5 s timeout
  THEN it completes (DATANODE10)
- **AC-11** The outbound view reads without converting
  WHEN `lq.data_node(state)` reads `[0]["amount"]` of a polars frame state, in a process where the `polars` Python package was never imported
  THEN the cell is returned and `polars` is still not in `sys.modules`, so the frame was not converted (DATANODE11)
- **AC-12** Async nodes are awaited, in a command and at the top level
  WHEN a command calls `context.data_node(folder_state)`, and a script calls `env.data_node(folder_state)`, each using a sync accessor, then awaiting `get_async("table.csv")`
  THEN each sync accessor raises `lq.NotSupportedError`, and each await returns that entry's node (DATANODE11)

- **AC-13** Opted-in protocols read as data
  WHEN opted-in classes with sequence and number protocols are read, and an opted-in mapping whose `__getitem__` raises
  THEN they are a `List`, an `Integer` and a `Float`, and the raising entry is `Opaque` (Decision 1)
Non-goals: Python-implemented `AsyncDataNode`; writing through a node; patching third-party types
to opt in; changing structural conversion.

## Core Interactions

Value types: `PyObjectValue` overrides `ForeignValue::data_node`. Consumers (templates, later store
handlers) read Python data through the adapter. Bindings: `lq.data_node`, `lq.DataNode`,
`lq.NodeKind`, `data_node()` on `State`, `Context` and `Environment`, `__liquers_data__`.

## Crate Placement

`liquers-py`: `src/data_node.rs` and `src/closure.rs` new (adapter, view, the awaitable closure
run); `value.rs`, `asset.rs` (lazy `State`), `context.rs`, `environment.rs`, the stubs.
`liquers-lib`: `template/host.rs` `context_node_kind` (Scope Changes). The core hook and primitive land first, in their own PRs.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Data nodes (mapping table, opt-in, live view,
  cycles, the lock).
- Guide: a "Data nodes" section in `PYTHON_GUIDE.md` (what a template sees of your data), which
  the templates chapter links to.
- Other documents: `TEMPLATE_GUIDE.md` (Python data is read in place); the `liquers-py/README.md` row; `LANGUAGE-INTEGRATION_GUIDE.md` §DATANODE and
  §6 Python; `specs/README.md`.
- Documents to update: `PYTHON_INTEGRATION.md` §5; the chapters `docs_index.py chapters-for` lists.
- API docs: module docs of `liquers-py/src/data_node.rs` and `closure.rs` (new), `value.rs`, `asset.rs`,
  `context.rs`, `environment.rs`, `liquers-lib/src/template/host.rs`; docstrings on every new Python name.

## Design Dependencies

- `foreign-value-data-node` / `FOREIGN-VALUE-HAS-NO-DATA-NODE`: **requires** (own PR first), so a
  retained value inside a query reads in place (AC-8).
- `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`: **requires** for `env.data_node` (AC-12); own PR first.
- `liquers-py-templates`: **required-by**. It lands next and reads its `data` frames through this
  adapter.
- `template-non-scalar-text-form`: **overlaps** (text form of `Query` and `Key` nodes).

## References

- Guide §DATANODE and Appendix A; `liquers-core/src/data_node.rs`; `liquers-lib/src/value/foreign.rs`,
  `value/mod.rs`; `liquers-py/src/value.rs`
