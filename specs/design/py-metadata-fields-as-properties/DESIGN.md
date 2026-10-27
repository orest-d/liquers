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

`lq.Metadata` is what `State.metadata` and `AssetRef.metadata` return, and it exposes its fields
as methods. Every neighbouring class exposes its fields as properties (`State.value`,
`AssetRef.status`, every field of `MetadataRecord` and `AssetInfo`). A wrong guess fails
silently: a bound method compares unequal to any string. This design turns the read-only fields
into properties. That is a change to the Python API, so its compatibility path is a decision.

### Problem Example

```python
import liquers as lq
env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

state = env.evaluate_state("hello")
state.metadata.type_identifier == "Text"     # today: False, silently (a bound method)
state.metadata.type_identifier() == "Text"   # today: True; guides/PYTHON_GUIDE.md uses this form
lq.MetadataRecord().type_identifier          # a str: the record's form is a property
# expected: state.metadata.type_identifier == "Text" is True
```

(`hello` validates as one action with `liquers-validate --command hello`.) Verified at HEAD
(`04667a80`), `liquers-py/src/metadata.rs` `impl Metadata`: the zero-argument readers are
`type_identifier`, `status`, `message`, `is_error`, `filename`, `query`, `extension`, `file_size`,
`is_dir`, `unicode_icon`, `log`, `get_media_type` and `get_data_format`, all plain methods. The
callers are `liquers-py/tests/test_VALUE_value_bridge.py:160`,
`test_COMMAND_registration.py:152, 236`, `test_EVAL_evaluation.py:57, 60`, and
`specs/guides/PYTHON_GUIDE.md` "Reading metadata": the example, the sentence before the table, and
the table. `test_GUIDE_examples.py` runs that example. The package has no published wheel
(`PYTHON_INTEGRATION.md` §8).

### Scope and Acceptance Criteria

- **AC-1** Fields read as properties
  WHEN `state.metadata.type_identifier`, `.status`, `.message`, `.is_error`, `.filename`, `.query`,
  `.extension`, `.file_size`, `.is_dir`, `.unicode_icon` or `.log` is read on an evaluated state
  THEN each gives the value its method gives today (`"Text"`, `Status` `Ready`, …), with no call
- **AC-2** Media type and data format are properties under plain names
  WHEN `metadata.media_type` and `metadata.data_format` are read
  THEN they give what `get_media_type()` and `get_data_format()` give today
- **AC-3** Read-only
  WHEN a caller assigns `metadata.status = …`
  THEN `AttributeError` is raised; the `with_*` and `set_extension` mutators are unchanged
- **AC-4** A failing field raises
  WHEN `query`, `type_identifier` or `is_error` cannot be read (legacy metadata without the field)
  THEN reading the property raises a `LiquersError`, typed once `py-metadata-typed-errors` lands
- **AC-5** The old call form follows the decided compatibility path
  WHEN a caller writes `metadata.type_identifier()`
  THEN under the recommended path it raises `TypeError: 'str' object is not callable`, and the
  guide and README say so
- **AC-6** Callers and documents agree
  WHEN the Python suite and `test_GUIDE_examples.py` run
  THEN they use the property form and pass

Non-goals: `get_asset_info()`, `error_result()`, `to_json()` and `from_json()` stay methods,
because they compute, convert or raise rather than read a field. `MetadataRecord`, which is
already consistent, does not change. There is no setter for `extension`, which stays
`set_extension()`.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Methods become properties, a change to the Python binding's
  interface (rule 4), and the compatibility path is an open question (rule 5).
- **Leading issue:** the compatibility path for existing `metadata.x()` calls.
- **Explanation:** the change is mechanical once the path is chosen. Phases 2-4 assume the
  recommendation.
- **Open questions:**
  1. **Open design: compatibility path.** Options:
     (a) a clean switch with no deprecation;
     (b) properties returning a callable value, so that `x()` keeps working with a
     `DeprecationWarning`;
     (c) properties under new names, with the old methods kept and deprecated.
     **Recommended: (a).** The package is unreleased, so no installed user depends on the
     method form. PyO3 cannot register a method and a property under one name. Option (b) cannot
     work for `bool`, `int` or `None` (they cannot be subclassed or called), and it would make
     `is_error` truthy whatever its value. Option (c) leaves two names per field, which is the
     inconsistency this design removes.
  2. **Proposed resolution: `get_media_type` / `get_data_format`.** They become `media_type` /
     `data_format`, and the old methods are removed with the rest under (a). Their names differ, so
     keeping them as deprecated aliases is possible; it is not recommended, for one consistent
     break.

### Design Dependencies

- `py-metadata-typed-errors` (ready, eligible): **overlaps** (T3: `query`, `type_identifier` and
  `is_error` are change sites in both). It is kept separate under E1, because that design is eligible and
  this one is not. Preferred order: that design first, so that AC-4 is typed from the start. If the
  order is reversed, the rebase is three one-line conflicts.
- `py-python-style-reprs`: **overlaps** (weak). Its `Metadata` repr reads the same Rust accessors,
  not the Python members.
- `py-api-reference` (needs decision): **overlaps** (weak). Docstrings for the new
  properties would serve it.

### Consolidated Findings

- 13 members change: 11 keep their names and 2 are renamed. There are 5 test call sites and 1
  guide section, all listed in the Problem Example.
- A `#[getter]` that returns `PyResult` raises on access. That is the AC-4 behaviour, and Python's
  `hasattr` will not swallow it, because it is not an `AttributeError`.
- Under (a) nothing else in the repository calls the method form. `grep -rn
  "metadata\.\w*()" liquers-py specs/guides specs/reference` at HEAD finds only the Python sites
  listed (the Rust `get_data_format()` in `UNITTEST_GUIDE.md` and the command-library references is
  core's API and does not change).

## Phase 2: Architecture

### Solution

In `liquers-py/src/metadata.rs` `impl Metadata`, mark the 11 readers `#[getter]` with their
current bodies and names. Then rename `get_media_type` to `media_type` and `get_data_format` to
`data_format`, both `#[getter]`. In PyO3 a `#[getter] fn name(&self)` exposes the property `name`,
so no `#[pyo3(name)]` is needed. The fallible getters (`query`, `type_identifier`, `is_error`) keep
their `PyResult` return types. With no `#[setter]`, assignment raises `AttributeError` (AC-3).

Rejected alternatives: options (b) and (c) of decision 1; a `__getattr__` shim (it is invisible to
introspection and slower on every access).

Known-issue preflight: `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` (P3, same methods) is ordered
in Design Dependencies and does not block. No command namespaces are involved.

### Changes

- `liquers-py/src/metadata.rs` `impl Metadata`: 13 attributes or renames. Signatures are
  unchanged apart from the two renames.
- Tests: the 5 call sites drop `()`. The Phase 3 tests are added.
- `specs/guides/PYTHON_GUIDE.md` "Reading metadata": the example uses the property form; the
  sentence "`State.value` is a property, but `Metadata`'s fields are methods" becomes "`Metadata`'s
  fields are properties, like `State.value`"; the table header becomes "`Metadata` property" and
  lists the bare names, with `media_type` in place of `get_media_type()`. Add a History row.
- `specs/reference/PYTHON_INTEGRATION.md` §8: drop the `PY-METADATA-FIELDS-ARE-METHODS` clause.
  Add a History row and bump `reviewed:`.
- `liquers-py/README.md`: one line under "Layout" or the conformance notes, saying that the method
  form was removed before the first release (AC-5).
- There are no Rust callers: `liquers-py` uses `CoreMetadata` directly.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A caller outside the repository uses `x()` | There is no published wheel; the README line documents it | high |
| A missed call site in the suite | `test_GUIDE_examples.py` and the full suite fail loudly on `'str' object is not callable` | high |
| A getter raising during a debugger's inspection | Only on legacy metadata without the field, as the method raises today | medium |
| Decision 1 is (c) | Phase 4 steps 2-3 change to adding properties under new names; the tests stay | medium |

Recovery: revert the commit. No data format changes.

## Phase 3: Examples and Tests

### Examples

```python
state = env.evaluate_state("hello")
assert state.metadata.type_identifier == "Text"
assert state.metadata.status == "Ready" and state.metadata.is_error is False
assert isinstance(state.metadata.media_type, str)              # a value, not a method
```

### Tests

In `liquers-py/tests/test_OBJECT_conformance.py` (the wrapper conventions):

- `test_OBJECT_metadata_fields_are_properties`: on `env.evaluate_state("hello")`, each AC-1
  member is not callable and equals what the method returned (values fixed in the test: `"Text"`,
  `"Ready"`, `False`, …), and `inspect.isdatadescriptor(type(m).__dict__["type_identifier"])`
  holds (a PyO3 getter is a `getset_descriptor`): AC-1
- `test_OBJECT_metadata_media_type_and_data_format`: `media_type` / `data_format` are strings, and
  `get_media_type` is absent: AC-2, AC-5
- `test_OBJECT_metadata_fields_are_read_only`: `pytest.raises(AttributeError)` on assignment;
  `with_type_identifier("X")` then `.type_identifier == "X"` on `lq.Metadata()`: AC-3
- `test_OBJECT_metadata_failing_field_raises`: `lq.Metadata.from_json('"not an object"')` (legacy
  metadata that is not an object; core's `Metadata::query` returns a `General` error) then `.query`
  raises `lq.GeneralError`. This needs `py-metadata-typed-errors`; before it, `pytest.raises(Exception)`: AC-4
- `test_OBJECT_metadata_method_form_is_gone`: `pytest.raises(TypeError)` on
  `state.metadata.type_identifier()`: AC-5
- The existing suite and `test_GUIDE_examples.py` after the call-site updates: AC-6

Run: `bash scripts/test-liquers-py.sh -k "OBJECT or GUIDE"`, then the full loop.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-py/tests/test_OBJECT_conformance.py`: add the five tests. Proof: `bash scripts/test-liquers-py.sh -k metadata` fails as the Problem Example shows
- [ ] 2. `liquers-py/src/metadata.rs` `impl Metadata`: `#[getter]` on the 11 readers; rename and mark `media_type` / `data_format`. Proof: `cargo check -p liquers-py --lib --tests`; the step 1 tests pass
- [ ] 3. The 5 test call sites and `PYTHON_GUIDE.md` "Reading metadata". Proof: `bash scripts/test-liquers-py.sh` passes in full, including `test_GUIDE_examples.py`
- [ ] 4. `PYTHON_INTEGRATION.md` §8 and its History row and `reviewed:`; the `PYTHON_GUIDE.md` History row; the `liquers-py/README.md` line; issue `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

Prerequisite: decision 1 is answered. If the answer is (b) or (c), revise Phase 2 first. Preferred:
`py-metadata-typed-errors` merged first. Rollback: revert per step.

### Validation

`bash scripts/test-liquers-py.sh`, `cargo fmt -p liquers-py --check`,
`python3 scripts/docs_index.py --check`. The diff touches only `liquers-py/`, two guides or
references, the issue and the index.
