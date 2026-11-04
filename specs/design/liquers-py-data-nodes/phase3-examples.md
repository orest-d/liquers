# Phase 3: Examples & Use-cases — liquers-py-data-nodes

## Overview Table

| # | Kind | Name | Scenarios |
|---|---|---|---|
| 1 | Example | A template reads a `dict` that conversion refuses | AC-5, AC-8 |
| 2 | Example | Reading a Liquers value field by field | AC-11, AC-12 |
| 3 | Test | `liquers-py/tests/test_DATANODE_mapping.py` | AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-9 |
| 4 | Test | `liquers-py/tests/test_DATANODE_protocols.py` | AC-7, AC-13 |
| 5 | Test | `liquers-py/tests/test_DATANODE_render.py` | AC-8, AC-10 |
| 6 | Test | `liquers-py/tests/test_DATANODE_views.py` | AC-11, AC-12 |
| 7 | Test | Rust: `liquers-lib/src/template/host.rs` `tests` | AC-12 |

**Validated queries** (`liquers-validate --command person --command folder_keys`; all `Ok`, each
`encoded` equal to its source):
1. `person/ns-template/render-~X~-R/templates/name.md~E`
2. `-R/data/sales.csv/-/ns-pl/from_csv`
3. `-R-dir/data/-/folder_keys`

`person` and `folder_keys` are test commands declared by the tests.

## Example 1: a template reads a `dict` that conversion refuses

The problem example of Phase 1. `person` returns a `dict` holding a `datetime`, so structural
conversion refuses it and it is kept as `py.Object`. The `ns-template/render` command asks the value
for its node (`Value::data_node` → `ForeignValue::data_node` → `PyNode`), and only `when` is opaque.

```python
import datetime, dataclasses
import liquers as lq

env = lq.Environment()
env.store_set("templates/name.md", b"{{name}}")

@env.first_command
def person() -> object:
    return {"name": "Ann", "when": datetime.datetime.now(), 1: "one"}

assert env.evaluate("person/ns-template/render-~X~-R/templates/name.md~E") == "Ann"   # AC-8
node = lq.data_node(person())                     # AC-5, the same dict read directly
assert node["name"].scalar == "Ann" and node["when"].kind == lq.NodeKind.Opaque
assert node.keys() == ["name", "when"]           # the int key is not visible
```

Two more renders of the same query give `"Ann"` (AC-8): `person` returning an instance of an
opted-in mapping class (`__liquers_data__ = True` with `keys` and `__getitem__`), and a dataclass
whose `__liquers_data__(self)` returns `dataclasses.asdict(self)`.

## Example 2: reading a Liquers value field by field

A polars frame stays in Rust; Python reads one cell. A folder needs a context, so it is read with
`await`.

```python
import sys, asyncio, pytest
from conftest import run_async
env.store_set("data/sales.csv", b"region,amount\nEU,10\nUS,20\n")
state = env.evaluate_state("-R/data/sales.csv/-/ns-pl/from_csv")   # value not converted (lazy)
assert lq.data_node(state)[0]["amount"].scalar == 10               # AC-11
assert "polars" not in sys.modules

folder = env.data_node(env.evaluate_state("-R-dir/data"))
assert folder.kind == lq.NodeKind.Map
with pytest.raises(lq.NotSupportedError):                          # AC-12
    folder.get("sales.csv")

async def main():
    return await folder.get_async("sales.csv")                     # one closure asset
entry = run_async(main)
assert entry.kind == lq.data_node(env.evaluate_state("-R/data/sales.csv")).kind  # its state
```

Inside a command, `folder_keys(state: lq.State, context)` does the same with
`context.data_node(state)`, and its `get_async` reads through the command's context.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `2**63`, `2**64` as a node | `Opaque`; `to_json()` raises the `ConversionError` conversion raises | `test_scalar_edges` |
| `float("nan")` | kind `Float`; `to_json()` raises `ConversionError` | `test_scalar_edges` |
| A `dict` containing itself | `d["self"]["self"]["self"]["name"]` reads; `to_json()` raises `ConversionError`, not `RecursionError` | `test_cycles_navigate_and_refuse_json` |
| A `dict` changed between two lookups | the second lookup sees the change | `test_view_is_live` |
| A `dict` subclass, a class with recording `__getitem__` | `Opaque`, nothing recorded | `test_only_opted_in_code_runs` |
| An opted-in mapping whose `__getitem__` raises `ValueError` for one key | that entry `Opaque`, the others read | `test_raising_entry_is_opaque` |
| `__liquers_data__` returning an object that itself opts in by method | read through its protocols, no loop | `test_method_result_is_not_reentered` |
| A render over a Python `dict` while the caller waits in `evaluate` | completes within 5 s | `test_worker_lookup_does_not_deadlock` |
| An opted-in `__getitem__` that calls `env.evaluate` mid-render | completes (every wait releases the GIL) | `test_reentrant_getitem_completes` |
| A command view used after the command returned | `lq.GeneralError` | `test_command_view_expires` |
| A `dict` subclass marked opted-in only on an instance (`obj.__liquers_data__ = True`) | `Opaque`: the opt-in is read from the type | `test_opt_in_is_read_from_the_type` |
| A wrapper exposing another value through `__liquers_data__` returning a `dict` | read as that `dict`; the method is called once per node (snapshot) | `test_method_form_is_a_snapshot` |
| `__liquers_data__`, `__index__` or `__float__` raising | that node `Opaque`; `to_json()` raises with the Python error as cause | `test_raising_protocols_are_opaque` |
| An `IntEnum` member; an `OrderedDict` | `Integer`; `Opaque` (exact types only) | `test_exact_types_only` |
| `to_json()` of a `dict` with an `int` key | `ConversionError` naming the key | `test_to_json_names_a_non_str_key` |
| An `Env` view read after `env.shutdown()` | `lq.GeneralError` (shut down); no asset | `test_env_view_after_shutdown` |
| `env.shutdown()` while an `Env` read waits on a slow step | `lq.CancelledError`; the read's closure asset ends `Cancelled` | `test_env_read_cancelled_at_shutdown` |
| The asyncio task awaiting `get_async` cancelled | `asyncio.CancelledError` in the task; the closure asset ends `Cancelled` | `test_cancelled_await_cancels_the_read` |
| `state.value` of an unconvertible state | raises at `.value`, not at `evaluate_state` | `test_state_value_is_lazy` |
| 10 000-entry `dict` rendered with `{{#items}}` | timed and recorded in the Learning Log (no assertion) | `test_large_render_cost` |

## Test Plan

- `test_DATANODE_mapping.py`:
  - `test_nested_mapping` (AC-1): kinds, fields, items, lengths and keys of
    `{"a": [1, {"b": "x"}], "c": None}`; `get("missing")` is `None`, `node["missing"]` raises `KeyError`.
  - `test_adapter_agrees_with_conversion` (AC-2): for a corpus of convertible structures (exact
    types only), returned by a command `cmd`, `lq.data_node(x).to_json()` equals
    `lq.data_node(env.evaluate_state("cmd")).to_json()` (the converted value's JSON), and sorted
    keys agree.
  - `test_scalar_edges` (AC-3); `test_bytes_are_opaque` (AC-4);
    `test_refused_structure_still_reads` (AC-5); `test_cycles_navigate_and_refuse_json` (AC-6);
    `test_view_is_live` (AC-9); edge cases, no AC: `test_exact_types_only`,
    `test_to_json_names_a_non_str_key`.
- `test_DATANODE_protocols.py`: `test_only_opted_in_code_runs` (AC-7);
  `test_opted_in_protocols` (AC-13): sequence → `List`, `__index__` → `Integer`, `__float__` →
  `Float`; `test_raising_entry_is_opaque` (AC-13); `test_method_result_is_not_reentered` (AC-13); edge cases,
  no AC: `test_opt_in_is_read_from_the_type`, `test_method_form_is_a_snapshot`,
  `test_raising_protocols_are_opaque`.
- `test_DATANODE_render.py`: `test_retained_value_renders_in_place` (AC-8, three variants);
  `test_worker_lookup_does_not_deadlock` (AC-10); `test_reentrant_getitem_completes` (AC-10);
  `test_large_render_cost` (no AC: a measurement).
- `test_DATANODE_views.py`: `test_frame_cell_without_conversion` (AC-11, run in a subprocess so
  `sys.modules` starts clean); `test_state_value_is_lazy` (AC-11);
  `test_async_nodes_in_a_command` and `test_async_nodes_at_top_level` (AC-12); edge cases, no AC:
  `test_command_view_expires`, `test_env_view_after_shutdown`, `test_env_read_cancelled_at_shutdown`,
  `test_cancelled_await_cancels_the_read`.
- Rust, `liquers-lib/src/template/host.rs` `tests`: `context_node_kind_agrees_with_node_of`
  (AC-12): for a directory listing, a record source and a text value, the kind it returns is the
  kind `node_of` builds.
- Run: `bash scripts/test-liquers-py.sh`, and `cargo test -p liquers-lib --lib template`.

## Learning Log

- A `DataNode` lookup cannot fail, so every Python failure inside one becomes an `Opaque` node; only
  `to_json` can report it. Both deviations from the guide (an integer above `i64`, a non-`str` key)
  follow from this.
- A sync node never holds a context in the current core; only `FolderNode` and `RecordSourceNode`
  do. `context_node_kind` keeps that fact in one place.
