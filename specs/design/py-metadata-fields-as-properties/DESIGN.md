---
id: PY-METADATA-FIELDS-AS-PROPERTIES
kind: design
title: Python Metadata exposes its read-only fields as properties
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-METADATA-FIELDS-ARE-METHODS]
created: 2026-10-27
---
# Python Metadata exposes its read-only fields as properties

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

`lq.Metadata` is what `State.metadata` and `AssetRef.metadata` return. It exposes its fields as
methods, while `State`, `AssetRef`, `MetadataRecord` and `AssetInfo` use properties. A wrong guess
fails silently, because a bound method compares unequal to any string. This design makes the
read-only fields properties. That changes the Python API, so the compatibility path is a decision.

### Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

state = env.evaluate_state("hello")
state.metadata.type_identifier == "Text"     # today: False, silently (a bound method)
state.metadata.type_identifier() == "Text"   # today: True; expected: the first line is True
```

`hello` validates with `liquers-validate --command hello`. At HEAD (`04667a80`),
`liquers-py/src/metadata.rs` `impl Metadata` has 13 zero-argument readers, all plain methods:
`type_identifier`, `status`, `message`, `is_error`, `filename`, `query`, `extension`, `file_size`,
`is_dir`, `unicode_icon`, `log`, `get_media_type` and `get_data_format`.

The callers are:
- `liquers-py/tests/test_VALUE_value_bridge.py:160`;
- `test_COMMAND_registration.py:152, 236`;
- `test_EVAL_evaluation.py:57, 60`;
- `PYTHON_GUIDE.md` "Reading metadata", whose example `test_GUIDE_examples.py` runs.

There is no published wheel (`PYTHON_INTEGRATION.md` §8).

### Scope and Acceptance Criteria

- **AC-1** Fields read as properties
  WHEN one of the 11 readers from `type_identifier` to `log` is read without a call, on an
  evaluated state
  THEN it gives what its method gives today. `status` gives an `lq.Status`, which compares equal
  to `"Ready"` (`PYTHON_INTEGRATION.md` §2)
- **AC-2** Plain names for media type and data format
  WHEN `metadata.media_type` and `metadata.data_format` are read
  THEN they give what `get_media_type()` and `get_data_format()` give today
- **AC-3** Read-only
  WHEN a caller assigns to one of these properties
  THEN `AttributeError` is raised; the `with_*` and `set_extension` methods are unchanged
- **AC-4** A failing field raises
  WHEN `query`, `type_identifier` or `is_error` cannot be read (legacy metadata without the field)
  THEN reading the property raises a `LiquersError` (typed once `py-metadata-typed-errors` lands)
- **AC-5** The old call form is gone
  WHEN a caller writes `metadata.type_identifier()`
  THEN `TypeError` is raised, under the recommended answer to decision 1
- **AC-6** Callers and documents agree
  WHEN the updated callers and the guide example run
  THEN they pass

Non-goals: `get_asset_info()`, `error_result()` and `to_json()` stay methods. `MetadataRecord`
does not change. There is no `extension` setter.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Turning methods into properties changes the Python API
  (rule 4), and decision 1 is open (rule 5).
- **Leading issue:** decision 1.
- **Explanation:** the change is mechanical once the path is chosen. Phases 2-4 assume the
  recommendation.
- **Open questions:**
  1. **Open design: compatibility path.** The options are:
     (a) a clean switch;
     (b) properties that return a callable value, so that `x()` works with a `DeprecationWarning`;
     (c) properties under new names, with the methods deprecated.
     **Recommended: (a)**, which the issue already proposes ("No deprecation period"). The package
     is unreleased, and PyO3 cannot register a method and a property under one name. Option (b)
     cannot work for `bool`, `int` or `None`. Option (c) keeps two names per field.
  2. **Proposed resolution:** `get_media_type` / `get_data_format` become `media_type` /
     `data_format`, and the old names are removed with the rest. Keeping them as aliases would be
     possible, but it is not recommended.

### Design Dependencies

- `py-metadata-typed-errors` (ready, eligible): **overlaps**, T3 (`query`, `type_identifier`,
  `is_error`). The issue says to "land the two together". Overlap rule E1 overrides that: the
  eligible design stays separate. That design should go first; reversed, the conflict is three
  lines.
- `multi-language-traceback` (its test 13) and `language-store-config-function` (its Example 1 and
  test 6): **overlaps**. They call `asset.metadata.log()`, which becomes `TypeError` here. Whichever
  design lands later updates the call form; both designs already record this.
- `py-python-style-reprs`, `py-api-reference`: **overlaps** (weak). They touch the same class and
  share no contract.

### Consolidated Findings

- 13 members change, 2 of them renamed; there are 5 test call sites and 1 guide section.
  `grep -rnE "metadata\.\w+\(\)"` over `liquers-py` and `specs/` finds no other Python caller. The
  matches it does find in `UNITTEST_GUIDE.md` and the command-library references are core's Rust
  API.
- A `#[getter]` returning `PyResult` raises on access (AC-4). `hasattr` does not swallow it,
  because it is not an `AttributeError`.

## Phase 2: Architecture

### Solution

In `liquers-py/src/metadata.rs` `impl Metadata`:
- add `#[getter]` to the 11 readers, keeping their bodies and names;
- rename `get_media_type` to `media_type` and `get_data_format` to `data_format`, both
  `#[getter]`.

A `#[getter] fn name` exposes the property `name`. The fallible getters keep `PyResult`. With no
`#[setter]`, assignment raises `AttributeError`. Rejected: options (b) and (c), and a
`__getattr__` shim, which is invisible to introspection. Known-issue preflight: none blocks. No
command namespaces are involved.

### Changes

- `metadata.rs` `impl Metadata`: 13 attribute or rename edits.
- The 5 test call sites drop `()`.
- `PYTHON_GUIDE.md` "Reading metadata": the example uses the property form; "`State.value` is a
  property, but `Metadata`'s fields are methods" becomes "`Metadata`'s fields are properties, like
  `State.value`"; the table lists bare names. Add a History row.
- `PYTHON_INTEGRATION.md` §8: drop the `PY-METADATA-FIELDS-ARE-METHODS` clause. Add a History row
  and bump `reviewed:`.
- `liquers-py/README.md`: one line saying that the method form was removed before the first
  release.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An outside caller uses `x()` | There is no wheel; the README line | high |
| A missed call site | The suite and `test_GUIDE_examples.py` fail loudly | high |
| A getter raises under a debugger | Only for legacy metadata without the field, as today | medium |
| Decision 1 is (c) | Revise step 2; the tests stay | medium |

Recovery: revert.

## Phase 3: Examples and Tests

### Examples

```python
m = env.evaluate_state("hello").metadata
assert m.type_identifier == "Text" and m.status == "Ready" and m.is_error is False
assert isinstance(m.media_type, str)
```

### Tests

These go in `liquers-py/tests/test_OBJECT_conformance.py`:

- `test_OBJECT_metadata_fields_are_properties`: each AC-1 member equals its fixed value
  (`"Text"`, an `lq.Status` equal to `"Ready"`, `False`, …), and
  `inspect.isdatadescriptor(type(m).__dict__[name])` holds: AC-1
- `test_OBJECT_metadata_media_type_and_data_format`: both are `str`, and `get_media_type` is
  absent: AC-2
- `test_OBJECT_metadata_fields_are_read_only`: assignment raises `AttributeError`; on
  `lq.Metadata()`, `with_type_identifier("X")` makes `.type_identifier == "X"`: AC-3
- `test_OBJECT_metadata_failing_field_raises`: `lq.Metadata.from_json('"not an object"').query`
  raises `lq.GeneralError` once `py-metadata-typed-errors` has landed, and `Exception` before
  that: AC-4
- `test_OBJECT_metadata_method_form_is_gone`: `state.metadata.type_identifier()` raises
  `TypeError`: AC-5
- These existing tests, after the call-site edits: `test_VALUE_value_bridge.py` (the test at line
  160), `test_COMMAND_registration.py` (lines 152, 236), `test_EVAL_evaluation.py` (lines 57,
  60), and `test_GUIDE_examples.py::test_guide_example_runs`: AC-6

Run: `bash scripts/test-liquers-py.sh -k "OBJECT or GUIDE or VALUE or COMMAND or EVAL"`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `test_OBJECT_conformance.py`: add the five tests. Proof: `bash scripts/test-liquers-py.sh -k metadata` fails as the Problem Example shows
- [ ] 2. `metadata.rs` `impl Metadata`: the getters and renames. Proof: `cargo check -p liquers-py --lib --tests`; the step 1 tests pass
- [ ] 3. The 5 call sites and the `PYTHON_GUIDE.md` section. Proof: `bash scripts/test-liquers-py.sh` passes (AC-6)
- [ ] 4. The `PYTHON_INTEGRATION.md` §8 edit, its History row and `reviewed:`; the guide History row; the README line; issue `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

Prerequisite: decision 1 is answered; for (b) or (c), revise Phase 2 first. Rollback: revert per
step.

### Validation

`bash scripts/test-liquers-py.sh`, `cargo fmt -p liquers-py --check`,
`python3 scripts/docs_index.py --check`. The diff touches only `liquers-py/`, the guide, the
reference, the issue and the index.
