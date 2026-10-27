---
id: PY-QUERY-SEGMENT-REGISTRATION
kind: design
title: QuerySegment is a registered, exported class of the liquers module
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED]
created: 2026-10-27
---
# QuerySegment is a registered, exported class of the liquers module

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

`Query.segments` returns `QuerySegment` objects whose class claims module `liquers` but is not
in it. This design registers and exports the class, so it can be named, checked with
`isinstance`, documented and pickled like the other query classes.

### Problem Example

```python
seg = lq.parse("hello/to_text").segments[0]
lq.QuerySegment                    # today: AttributeError
isinstance(seg, lq.QuerySegment)   # today: AttributeError; expected: True
```

Verified at HEAD (`04667a80`): `liquers-py/src/query.rs:429` declares the class with
`module = "liquers"`, a no-argument `#[new]`, getters `filename` and `header`, and methods
`is_resource_query_segment`, `is_transform_query_segment`, `is_empty`, `is_filename`,
`is_action_request`, `encode`, `to_absolute` and the dunders. `Query::segments` (a `#[getter]`)
returns `Vec<QuerySegment>`. `liquers-py/src/lib.rs` adds the other query classes
(`Position` … `QuerySource`) but not this one; `python/liquers/__init__.py` imports and exports
`TransformQuerySegment` and `ResourceQuerySegment` but not `QuerySegment`. `hello/to_text`
validates with `--command hello`.

### Scope and Acceptance Criteria

- **AC-1** The class is reachable
  WHEN `liquers` is imported
  THEN `lq.QuerySegment` exists, `"QuerySegment" in lq.__all__`, and
  `isinstance(lq.parse("hello/to_text").segments[0], lq.QuerySegment)` holds
- **AC-2** Existing behaviour is unchanged
  WHEN the existing `test_OBJECT_parser.py` segment tests run
  THEN they pass unchanged
- **AC-3** The reference lists it
  WHEN `PYTHON_INTEGRATION.md` §2's query-class row is read
  THEN it names `QuerySegment` as the element type of `Query.segments`

Non-goals: changing what `Query.segments` returns; the segment repr (`py-python-style-reprs`);
docstrings (`py-docstrings`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Exporting a new name from the `liquers` module adds a class
  to the Python binding's interface (rule 4). The class already exists and its instances are
  already returned, so the addition is small, but it is still an addition, and question 1 is an
  interface choice (rule 5).
- **Leading issue:** question 1.
- **Explanation:** the recommended change is two lines plus a test and a reference row; the only
  question is which interface the owner wants.
- **Open questions:**
  1. **Open design question: register it, or change what `segments` returns.** Recommended:
     register and export `QuerySegment` as it is (the shape `Query.segments` already has, and what
     the parser tests use). Alternative: `segments` returns `TransformQuerySegment` /
     `ResourceQuerySegment` directly, which removes `QuerySegment` from view but changes the
     result type of an existing getter.

### Design Dependencies

- `py-docstrings`: **overlaps**. Its coverage walk names `QuerySegment` as a known exclusion
  until this lands; then it is covered through `__all__`, and whichever lands second adds its
  docstrings.
- `py-python-style-reprs`: **overlaps** (weak). Its AC-4 reaches `QuerySegment` through
  `Query.segments`; that still works after this design.

## Phase 2: Architecture

### Solution

`liquers-py/src/lib.rs`: `m.add_class::<QuerySegment>()?;` beside `TransformQuerySegment` and
`ResourceQuerySegment` (`lib.rs` already has `use crate::query::*;`).
`python/liquers/__init__.py`: import `QuerySegment` from `._native` with the other query classes
and add `"QuerySegment"` to `__all__` in the same place.

Rejected: changing `segments`' return type (question 1 alternative); a Python-side alias without
`add_class` (the class must be in `_native` to be exported).

Known-issue preflight: none.

### Changes

`liquers-py/src/lib.rs`, `liquers-py/python/liquers/__init__.py`, a new test in
`liquers-py/tests/test_OBJECT_basic_objects.py`, and `specs/reference/PYTHON_INTEGRATION.md` §2
(row and History, `reviewed:` bump). No command or registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A name clash in `__all__` | No other `QuerySegment` exists in the package | high |
| An `OBJECT` conformance test enumerates `__all__` and now meets an undocumented class | Run the whole suite; `py-docstrings`' AC-1 covers the docstring | medium |

Recovery: revert the two lines.

## Phase 3: Examples and Tests

### Examples

```python
seg = lq.parse("hello/to_text").segments[0]
assert isinstance(seg, lq.QuerySegment) and seg.is_transform_query_segment()
```

### Tests

- `liquers-py/tests/test_OBJECT_basic_objects.py` `test_OBJECT_query_segment_is_exported`: AC-1
- the existing `test_OBJECT_parser.py` suite, unchanged: AC-2
- `docs_review` of the `PYTHON_INTEGRATION.md` diff: AC-3

Run: `bash scripts/test-liquers-py.sh`.

## Phase 4: Implementation Plan

### Steps

Precondition: question 1 decided.

- [ ] 1. `liquers-py/tests/test_OBJECT_basic_objects.py` — `test_OBJECT_query_segment_is_exported` — `bash scripts/test-liquers-py.sh -k query_segment_is_exported` fails today
- [ ] 2. `liquers-py/src/lib.rs` `add_class`, `python/liquers/__init__.py` import and `__all__` — `bash scripts/test-liquers-py.sh` passes in full (AC-1, AC-2)
- [ ] 3. `PYTHON_INTEGRATION.md` §2 row, History, `reviewed:` (AC-3); issue `status: closed` with a resolution note; this design `complete`; `specs/README.md` row; `python3 scripts/docs_index.py` — `python3 scripts/docs_index.py --check`

Rollback: revert per step.

### Validation

`bash scripts/test-liquers-py.sh`, `python3 scripts/docs_index.py --check`. The diff holds two code
lines, one export line, one test, one reference row, the issue, the design and the index.
