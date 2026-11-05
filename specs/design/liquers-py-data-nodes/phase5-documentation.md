# Phase 5: Documentation - liquers-py-data-nodes

## Completion Preconditions

- [x] Implementation is finished and validated (commits `071e970a`, `ab25ba92`, `7c953da7`;
  `bash scripts/test-liquers-py.sh`: 446 passed, Rust unit tests pass)
- [x] All user comments are answered or incorporated (Orest 2026-11-04: standard types read
  automatically, other classes opt in with `__liquers_data__`)
- [x] All review comments are answered or incorporated (two reviewers per phase, Phases 1-4)
- [x] Documentation is consistent with the implemented and tested behavior (commit `faf6d472`)
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Python data reads in place as a Liquers data node, in both directions, as approved.

- **Inbound.** `PyObjectValue` overrides `ForeignValue::data_node` (from `foreign-value-data-node`)
  with `PyNode`, a node over the live object: a template or any data-node reader sees a retained
  `dict` (one that conversion keeps as `py.Object`, for example because it holds a `datetime`)
  field by field. Exact `dict`, `list`, `tuple` and the scalars read with no Python code run;
  another class reads only if its *type* opts in, with `__liquers_data__ = True` (the mapping and
  sequence protocols) or a method returning the data. Each lookup takes the GIL through
  `attach_off_worker`, so a runtime worker never deadlocks (AC-10).
- **Outbound.** `lq.data_node(x)`, `context.data_node(x)` and `env.data_node(x)` give an
  `lq.DataNode` view (a root plus a path; `lq.NodeKind`). A folder or record source state needs a
  context: in a command it reads through the command's context, at the top level each async read
  runs in its own closure asset (`ad-hoc-closure-asset`).
- **`lq.State.value`** converts lazily, on first read, so `DATANODE11` reads a polars cell without
  importing polars.

Deviation from Phase 2, recorded there: cancelling the awaitable of an `env.data_node` async read
also cancels its private closure asset (it has no other waiter). Nothing was omitted.

## Documentation Delivered

### New Reference Documents
None: the contract is a section of the existing Python reference.

### New Guide Documents
None: the usage is a section of the existing Python guide.

### Existing Documents Reviewed or Updated
`affects_docs`, each reviewed against the code and tests, with a History row and `reviewed:`
2026-11-04 (`PYTHON_INTEGRATION.md` and `PYTHON_GUIDE.md` 2026-11-05, after the Phase 5 review's
corrections: what a `__liquers_data__` method's result reads as, a raise in `keys()`, and which
async read a cancel stops):
- `specs/reference/PYTHON_INTEGRATION.md`: new §14 Data nodes; §2, §3 names; §5 lazy `State.value`.
- `specs/guides/PYTHON_GUIDE.md`: §7 "Reading data field by field" (examples run by
  `test_GUIDE_examples.py`).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 Python *Data nodes*.
- `specs/guides/TEMPLATE_GUIDE.md` §5: a retained language object reads in place.
- `liquers-py/README.md`: `DATANODE` row, `COMPLETE`, tests `DATANODE01`-`11` mapped.
- `specs/README.md`: see Links.

### Module Docs
`liquers-py/src/data_node.rs` (new), `closure.rs` (new), `context.rs`, `environment.rs`,
`asset.rs`, `value.rs`; `liquers-lib/src/template/host.rs` (`context_node_kind`).
`check_api_docs.py` on each: 0 errors, 0 warnings.

### User-Guide Chapters
`docs_index.py chapters-for` lists `docs/guides/python/{01..15}`, `rust/` and `web/` chapters
drawing on these documents; all are `status: outline`, so no chapter review was due.

### Links and Capability Map
`specs/README.md`'s M6 entry is updated when the three M6 designs are complete (the pipelines PR,
the last of them).

## Issues Filed

- `ASYNC-DATA-NODE-HAS-NO-KEYS`: an async map node cannot list its field names (no `keys_async`).
- Known and linked, not new: `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE`,
  `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM`.

## Important Learning

- A `DataNode` lookup cannot fail, so a Python exception inside one makes that node `Opaque`; only
  `to_json` reports errors. Both deviations from `LANGUAGE-INTEGRATION_GUIDE.md` §DATANODE (an
  integer above `i64`, a non-`str` key) follow from it.
- Opt-in on the *type* keeps `DATANODE07` honest: an instance `__getattr__` never runs.
- A sync node never needs a context in the current core; only folder and record-source nodes do,
  and `context_node_kind` keeps that fact in one place.

## Conformance and Remaining Work

Requested (Orest, 2026-11-04): standard Python types as data nodes, classes by opt-in. Approved:
Phases 1-4 as written. Implemented: all of it, AC-1 to AC-13, with the one recorded deviation.
Nothing remains outside the filed issues; `PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS` is closed.

## Validation

`bash scripts/test-liquers-py.sh` (446 passed, with the Rust unit tests); `check_api_docs.py` on the
touched modules; `python3 scripts/docs_index.py --check` (0 errors).
