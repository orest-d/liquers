# Phase 1: High-Level Design — liquers-py-data-nodes

## Purpose

Let core read Python data in place as a `DataNode`, with no structural conversion first, so a
template reads the readable fields of a `dict` that conversion refuses, and three fields of a large
structure cost three lookups instead of a copy. Also let a Python command read one field of a
Liquers value (a record view, a frame) without converting all of it. This is the `DATANODE`
feature of `LANGUAGE-INTEGRATION_GUIDE.md` (§DATANODE), the data side of
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md), and milestone M6 of the liquers-py
plan, with [`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` (question 2 adds a protocol Python authors write against)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python classes.
- **Leading issue:** `PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS` (P2 L, filed with this design).
- **Explanation:** `DataNode`, `AnyNode`, `NodeRef` and `Scalar` exist in `liquers-core/src/data_node.rs`.
  `PyObjectValue` (`liquers-py/src/value.rs`) is a `ForeignValue`, which has no hook to answer as a
  node. That hook is `foreign-value-data-node`, a small and ready core design that lands first in
  its own PR.
- **Open questions** (the guide's six; 1 and 3–6 have proposed resolutions):
  1. **Proposed resolution: the mapping** (question 1). `None` → `Null`; `bool` → `Bool`,
     tested before `int`; `int` → `Integer` within `i64`. Above that, VALUE's policy applies:
     liquers-py refuses `2**63` with `ConversionError`, so the node is `Opaque` and `to_json`
     raises that same error. `float` → `Float`, and a non-finite one fails `to_json`. `str` →
     `Text`. `bytes` and `bytearray` → `Opaque`, never `Text`. `dict` → `Map` in insertion order:
     a non-`str` key is not visible through `get` or `keys`, and makes `to_json` raise
     `ConversionError`, as conversion does. `list` and `tuple` → `List`, with random access. Values
     the bridge maps to a Liquers value (an `lq.Query`, a polars frame, an `lq.RecordView`) answer
     through that value's own node. Anything else is `Opaque`. One table beside VALUE's table; the
     only differences are key order and partial readability.
  2. **Open design: which other objects read as maps** (question 2). Recommended: an opt-in
     protocol. A class defines `__liquers_data__(self)`, returning a `dict`, `list` or scalar,
     which is called once when its node is built. This runs Python code, and the docs say so.
     Nothing else runs code: `dict`, `list` and `tuple`, subclasses included, are read through
     their data (`PyDict` / `PyList` access, never `__getitem__`, properties or `__getattr__`)
     (DATANODE07). Alternative: also read dataclasses and named tuples automatically from their
     fields.
  3. **Proposed resolution: live view** (question 3). Each lookup reads the structure as it is at
     that moment, so a mutation mid-render shows (DATANODE09). A caller who needs consistency
     passes a copy, and the docs say so.
  4. **Proposed resolution: cycles** (question 4). Navigation is lazy, so a cyclic structure is
     navigable to any finite depth. `to_json` tracks object identities on its path and raises
     `ConversionError` on a cycle, never `RecursionError` (DATANODE06).
  5. **Proposed resolution: threads and the lock** (question 5). The adapter holds `Py<PyAny>` and
     takes the interpreter lock for each lookup (`Python::attach`), on whatever worker polls the
     reader. This is safe because every liquers-py wait releases the lock (DATANODE10). On
     free-threaded builds, a `dict` read is atomic per lookup. The per-lookup cost is measured on a
     large render in Phase 3.
  6. **Proposed resolution: the outbound view** (question 6). Yes: `lq.data_node(x)` returns an
     `lq.DataNode` for a Python value (the adapter) or for an `lq.State` (its value's node), and
     `state.data_node()` does the same. It holds the root `AnyNode` plus a path and re-resolves on
     each access. It offers `kind` (`lq.NodeKind`), `scalar`, `[key]` / `[i]`, `get`, `keys()`,
     `len`, iteration and `to_json()`. An async node (a store folder, a record source) is read
     with `await node.get_async(…)`. A sync accessor on one raises `lq.NotSupportedError`
     (DATANODE11).

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
  WHEN `True`, `2**63 - 1`, `2**63`, `2**64` and `float("nan")` are read
  THEN `True` is `Bool`, `2**63 - 1` is exact, the two larger integers follow VALUE's refusal, and NaN's `to_json()` raises `ConversionError` (DATANODE03)
- **AC-4** Bytes are opaque
  WHEN `b"abc"` is read
  THEN its kind is `Opaque` (DATANODE04)
- **AC-5** A refused structure still reads
  WHEN the problem example's `dict`, with an added key `1`, is read through the adapter
  THEN `name` reads `"Ann"` and `when` is `Opaque` (DATANODE05)
- **AC-6** Cycles navigate and refuse JSON
  WHEN a `dict` containing itself is navigated three levels deep, then `to_json()` is called
  THEN navigation reads the name, and `to_json()` raises `ConversionError` (DATANODE06)
- **AC-7** Lookups run no Python code
  WHEN a `dict` subclass with a recording `__getitem__` is read
  THEN nothing is recorded (DATANODE07)
- **AC-8** A retained value reads in place
  WHEN the problem example's query is evaluated, and the same with a class that defines `__liquers_data__`
  THEN both renders give `"Ann"` (DATANODE08)
- **AC-9** The view is live
  WHEN a `dict` is changed between two lookups through one node
  THEN the second lookup shows the new value (DATANODE09)
- **AC-10** A worker lookup does not deadlock
  WHEN a render whose data is a Python `dict` runs while the calling thread waits in `evaluate`, under a 5 s timeout
  THEN it completes (DATANODE10)
- **AC-11** The outbound view reads without converting
  WHEN `lq.data_node(state)` reads one row of a large polars frame, and a sync accessor is used on a store folder's async node
  THEN one row is built, not the whole frame, and the second raises `lq.NotSupportedError` (DATANODE11)
- **AC-12** Async nodes can be awaited
  WHEN `await lq.data_node(folder_state).get_async("table.csv")` is called
  THEN it returns that entry's node

Non-goals: Python-implemented `AsyncDataNode`; writing through a node; reading a pandas frame in
place (it stays `py.Object`, opaque unless it opts in); changing structural conversion itself.

## Core Interactions

Value types: `PyObjectValue` overrides the new `ForeignValue::data_node`. Consumers (templates,
and store handlers later) read Python data through the adapter. Bindings: `lq.data_node`,
`lq.DataNode`, `lq.NodeKind`, `State.data_node()`, and the `__liquers_data__` protocol.

## Crate Placement

`liquers-py`: a new `src/data_node.rs` (the adapter and the view), plus `value.rs` (the hook) and
`asset.rs` (`State.data_node`). In `liquers-lib`, `ForeignValue::data_node` comes from
`foreign-value-data-node`, in its own PR first.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Data nodes (the mapping table beside §5's
  conversion table, the protocol, live view, cycles, the lock).
- Guide: a "Data nodes" section in `PYTHON_GUIDE.md`, in the templates chapter: what a template
  sees of your data.
- Other documents: the `liquers-py/README.md` conformance row; `LANGUAGE-INTEGRATION_GUIDE.md`
  §DATANODE and §6 Python; `specs/README.md`.
- Documents to update: `PYTHON_INTEGRATION.md` §5 (opaque `py.Object`); the user guide chapters
  that `docs_index.py chapters-for` lists.
- API docs: the module doc of the new `liquers-py/src/data_node.rs` and of `value.rs`;
  docstrings on every new Python name.

## Design Dependencies

- `foreign-value-data-node` / `FOREIGN-VALUE-HAS-NO-DATA-NODE`: **requires**, in its own PR
  first. Without it, only `lq.data_node` and the top-level render reach the adapter, not a
  retained value inside a query (AC-8).
- `liquers-py-templates`: **required-by** (soft). Its `data` frames use this adapter.
- `template-non-scalar-text-form`: **overlaps**. A `Query` or `Key` value's node gains a text
  form there, and this design inherits it.
- `liquers-py-stubs` (complete): **required-by**. The stubs declare the new names.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §DATANODE and Appendix A DATANODE
- `liquers-core/src/data_node.rs`; `liquers-lib/src/value/foreign.rs`, `value/mod.rs`
  (`ExtValue::data_node`); `liquers-py/src/value.rs` (`py_to_simple`, `PyObjectValue`)
