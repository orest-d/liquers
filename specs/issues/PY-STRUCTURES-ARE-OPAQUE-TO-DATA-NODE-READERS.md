---
id: PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS
kind: feature
title: A Python dict that structural conversion refuses is opaque to templates and other data-node readers
status: closed
priority: P2
complexity: L
area: [py, lib/value]
design: liquers-py-data-nodes
created: 2026-11-04
github:
---

## Problem

Core reads data through `DataNode` (`liquers-core/src/data_node.rs`): templates, and store
handlers in future. `liquers-py` has no `DataNode` adapter over Python data. A `dict` reaches core
only through structural conversion (`py_to_simple` in `liquers-py/src/value.rs`), which is
all-or-nothing: one non-`str` key or one member such as a `datetime` makes the whole `dict` a
`py.Object`, and `PyObjectValue` (a `ForeignValue`) answers `AnyNode::opaque()`. There is also no
Python view of a Liquers value's data node (`lq.data_node`, guide §DATANODE direction 2).

**Example.** A Python command returns `{"name": "Ann", "when": datetime.now()}` and the query
pipes it to `ns-template/render-~X~-R/templates/name.md~E` (`templates/name.md` = `{{name}}`).
Today the result is the empty string. Expected: `Ann` (guide Appendix A `DATANODE05`, `DATANODE08`).

## Impact

Templates silently render nothing for Python data with one unconvertible member; a large structure
read for three fields is copied whole. `DATANODE` is `NS` in `liquers-py` (guide §6 Python).

## Expected behaviour

The guide's §DATANODE, tests `DATANODE01`–`11`. Retained values need the core hook
`ForeignValue::data_node` (`FOREIGN-VALUE-HAS-NO-DATA-NODE`, design `foreign-value-data-node`).

## Discovery

Plan update for milestone M6 of the liquers-py plan, 2026-11-04: the guide's feature table lists
`DATANODE`, and no issue recorded that liquers-py does not implement it.

## Resolution

Closed 2026-11-05 by [`design/liquers-py-data-nodes/`](../design/liquers-py-data-nodes/DESIGN.md):
`PyNode` behind `PyObjectValue`'s `ForeignValue::data_node`, `lq.data_node` and the
`lq.DataNode` views (commits `071e970a`, `ab25ba92`); tests `liquers-py/tests/test_DATANODE_*.py`.
