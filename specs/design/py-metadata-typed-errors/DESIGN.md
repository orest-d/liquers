---
id: PY-METADATA-TYPED-ERRORS
kind: design
title: Every liquers-py wrapper method raises the typed LiquersError
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [py]
issues: [PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION]
created: 2026-10-27
---
# Every liquers-py wrapper method raises the typed LiquersError

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

`reference/PYTHON_INTEGRATION.md` §6 says that a Liquers error reaches Python as the `LiquersError`
subclass for its `ErrorType`. The older wrapper classes bypass this. They turn the error into a
plain `Exception` holding only the message. This design sends every such failure through the
typed mapping. A serde (JSON or YAML) failure becomes `SerializationError`, which is what
`liquers-core` itself uses for one.

### Problem Example

```python
import liquers as lq
try:
    lq.Metadata.from_json("not json")
except lq.LiquersError as e:
    assert e.type == lq.ErrorType.SerializationError   # expected
# today: escapes as a plain Exception("expected ident at line 1 column 2"), with no `type`
```

Re-counted at HEAD (`04667a80`, `grep -n "PyException" liquers-py/src/*.rs`). The issue's 22 sites
are 23: `recipes.rs` has a seventh, written `PyErr::new::<…PyException, _>`. The same pattern
appears in two more files, for 56 sites in all:

| File | Liquers `Error` | serde | other | Methods |
|---|---|---|---|---|
| `metadata.rs` | 7 | 8 | — | `Metadata.get_asset_info`, `is_error`, `query`, `type_identifier`, `set_extension`, `error_result`; `MetadataRecord.error_result`; `from_json`/`to_json` of `Metadata`, `MetadataRecord`, `AssetInfo`, `LogEntry` |
| `recipes.rs` | 1 | 6 | — | `Recipe(...)`; `RecipeList`/`Recipe` `to_json`, `from_json`; `Recipe.to_yaml`, `from_yaml` |
| `expiration.rs` | — | — | 1 (chrono) | `ExpirationTime(spec)` |
| `command_metadata.rs` | 1 | 22 | — | `CommandPreset(...)`; JSON/YAML of `EnumArgument`, `ArgumentType`, `ArgumentInfo`, `CommandDefinition`, `CommandPreset`, `CommandMetadata`, `CommandMetadataRegistry`; `ArgumentInfo.default`, `argument_type` |
| `plan.rs` | — | 10 | — | JSON/YAML of `Step`, `ParameterValue`, `ResolvedParameterValues`, `Plan` |

The deliberate fallback in `error.rs` `error_to_pyerr` (it is used only when `liquers._errors`
cannot be imported) stays as it is.

### Scope and Acceptance Criteria

- **AC-1** Serialization failures are typed
  WHEN any `from_json` / `from_yaml` of a wrapper class is given text that is not JSON (or YAML)
  at all, such as `"{"`. Any valid JSON is not enough, because `Metadata.from_json` falls back to a
  bare JSON value (`liquers-core` `Metadata::from_json`), so only invalid syntax fails for every class
  THEN it raises `lq.SerializationError` (a `lq.LiquersError`), with `type ==
  ErrorType.SerializationError` and the serde message in `message`
- **AC-2** A stored error keeps its type
  WHEN a Python command `missing` raises `lq.KeyNotFoundError("no such key")`, and
  `env.get_asset("missing").metadata.error_result()` is called after the asset fails (core
  `Metadata::error_result` returns the record's `error_data`); or when `lq.Metadata()` gets
  `with_error("boom")` and then `error_result()`
  THEN the first raises `lq.KeyNotFoundError`, and the second `lq.GeneralError` with message `boom`
- **AC-3** Constructor failures are typed
  WHEN `lq.Recipe("a/b-~", "t", "d")` or `lq.CommandPreset("a/b-~", "l", "d")` gets a query that
  does not parse, or `lq.ExpirationTime("not a time")` gets a spec it cannot read
  THEN `Recipe` and `CommandPreset` raise `lq.ParseError`, and `ExpirationTime` raises
  `lq.GeneralError`, the type `lq.Expires` raises for a bad spec today
- **AC-4** Existing handlers still work
  WHEN a caller catches these failures with `except Exception`
  THEN they are still caught, because `LiquersError` derives from `Exception`
- **AC-5** No untyped mapping is left
  WHEN the check `ac5_no_untyped_mapping_grep` runs (`grep -n "PyException" liquers-py/src/*.rs`)
  THEN the only matches are in `error.rs`

Non-goals: Python argument-validation errors that are Python's own (`PyValueError`,
`PyIndexError`, `PyTypeError`) stay as they are; `Metadata`'s fields stay methods
(`py-metadata-fields-as-properties`); no change in `liquers-core`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a bug fix that restores intended behaviour (rule 2).
  - The 9 Liquers-`Error` sites break §6 of `PYTHON_INTEGRATION.md` as written.
  - The 46 serde sites are not covered by §6's wording, which speaks only of "a Liquers error".
    The intent is plain from three sources. `LANGUAGE-INTEGRATION_GUIDE.md` `OBJECT04` says
    "invalid parse produces `ERROR`", meaning the structured error. The issue's own expected
    behaviour names `SerializationError`. And `liquers-core` maps every JSON failure to
    `SerializationError` (`value.rs`). No document promises a plain `Exception` anywhere.
  - Step 6 adds the sentence to §6 that makes this explicit. It adds one `pub(crate)` helper and no structure (rule 3). No signature,
  class or method changes, and the exceptions stay `Exception` subclasses (rule 4). All of it is in
  `liquers-py` (rule 6). Size `M`: five files of one-line edits plus a helper in `error.rs`, one crate.
- **Leading issue:** None
- **Explanation:** every site has a single obvious typed form. The serde-to-`SerializationError`
  mapping is the one `liquers-core` uses (`value.rs`, `Error::from_error(ErrorType::SerializationError, …)`).
- **Open questions:** None. Resolved by repository evidence: a bad `ExpirationTime` spec raises
  `GeneralError`. Its sibling `Expires` raises exactly that for a bad spec today (core
  `Expires::from_str` uses `Error::general_error`), so the two classes agree.

### Design Dependencies

- `py-metadata-fields-as-properties` (needs decision): **overlaps** (T3). It turns `query`,
  `type_identifier` and `is_error`, three of these sites, into properties. It is kept separate
  under E1: this design is eligible and that one is not. Land this one first, so that the
  properties raise typed errors from the start. If the order is reversed, the conflict is three
  one-line edits.
- `py-python-style-reprs`: **overlaps** (weak). It touches the same files, in different methods.
- `py-api-reference` (needs decision): **overlaps** (weak). It adds docstrings to the same
  `#[pymethods]` blocks. The edits are independent, and a rebase is trivial.
- `multi-language-traceback`: **overlaps** (weak). Its step 5 edits `liquers-py/src/error.rs` and
  `LogEntry` in `metadata.rs`. This design goes first, and that one rebases over the helper.

### Scope Changes

- **2026-10-27**: `command_metadata.rs` and `plan.rs` (33 serde sites) and the `PyErr::new` forms
  in `recipes.rs` and `command_metadata.rs` were added to the issue's 22 sites. They have the same
  root cause (T1), and leaving them would keep §6 untrue for the classes they serve.

### Consolidated Findings

- 56 sites: 9 carry a Liquers `Error` (`PyErr::from(crate::error::Error::from(e))`, the form that
  `expiration.rs` `Expires::new` already uses), 46 carry serde errors, and 1 carries a chrono error.
- No caller needs a change: no test in `liquers-py/tests/` expects a plain `Exception` from these
  methods, and no document promises one.

## Phase 2: Architecture

### Solution

Add one helper in `liquers-py/src/error.rs`, beside `error_to_pyerr`:

```rust
/// A serde (JSON/YAML) failure as the `SerializationError` exception, the type liquers-core
/// gives a serialization failure.
pub(crate) fn serialization_pyerr(error: impl std::fmt::Display) -> PyErr {
    PyErr::from(Error::from(liquers_core::error::Error::from_error(
        liquers_core::error::ErrorType::SerializationError,
        error,
    )))
}
```

`From<Error> for PyErr` attaches to the interpreter and calls `error_to_pyerr`, so the helper needs
no `py` argument. Every serde site becomes `.map_err(serialization_pyerr)`. Every Liquers `Error`
site becomes `.map_err(|e| PyErr::from(crate::error::Error::from(e)))`. `ExpirationTime::new` maps
its `chrono` error through `liquers_core::error::Error::general_error(format!("Invalid expiration
time {spec:?}: {e}"))`. The now-unused `PyException` imports in `command_metadata.rs` and `plan.rs`
are removed.

Rejected alternatives:
- **`error_to_pyerr(py, &e)` at each site.** Most of these methods take no `py`, and adding the
  parameter only to throw is noise. `From` already does the same thing.
- **`ParseError` for serde input.** `liquers-core` reserves `ParseError` for query text and uses
  `SerializationError` for JSON. Matching core keeps one meaning per type.
- **A `From<serde_json::Error>` impl.** It would cover JSON only, and it would hide the mapping
  inside `?`.

Known-issue preflight: `PY-METADATA-FIELDS-ARE-METHODS` (P3, same methods) does not block
(Design Dependencies). `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (P3) concerns the other
direction (Python to Liquers) and does not block. No command namespaces are involved.

### Changes

- `liquers-py/src/error.rs`: add `serialization_pyerr`.
- `liquers-py/src/metadata.rs`, `recipes.rs`, `expiration.rs`, `command_metadata.rs`, `plan.rs`:
  the 56 `map_err`s, plus imports. Every change is to a method body; no signature changes.
- `liquers-py/tests/test_ERROR_errors.py`: the Phase 3 tests.
- Documents: `PYTHON_INTEGRATION.md` §6 gains, after the `error_to_pyerr` paragraph: "A JSON or
  YAML failure in a wrapper method (`from_json`, `to_yaml`, …) raises `SerializationError`; a bad
  constructor argument raises the error its core constructor returns." Add a History row and bump
  `reviewed:`. The conformance row `ERROR` in `liquers-py/README.md` gains the new test names.
- Sync/async: everything is synchronous, as before. No command or registry changes.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A caller relies on the exception being exactly `Exception` | `LiquersError` derives from `Exception` (AC-4). No test checks `type(e) is Exception` (`grep` of `liquers-py/tests`) | high |
| `Python::attach` inside a `#[pymethods]` call | The thread is already attached, so `attach` is re-entrant. It is the same path every `Error` the bindings convert takes today | high |
| A site is missed | AC-5's grep | high |
| `Metadata.with_error(str)` stores a `general_error`, so its case cannot show that a type survives | AC-2's first case uses a stored `KeyNotFound` that came from a real failed evaluation. `MetadataRecord.with_error` is commented out (`metadata.rs`), and it stays out: exposing it would add a method | high |

Recovery: revert the commit. Nothing persistent or serialized changes.

## Phase 3: Examples and Tests

### Examples

```python
with pytest.raises(lq.SerializationError):
    lq.Plan.from_json("{")
@env.first_command
def missing():
    raise lq.KeyNotFoundError("no such key")
asset = env.get_asset("missing")
with pytest.raises(lq.KeyNotFoundError):
    asset.get()
with pytest.raises(lq.KeyNotFoundError, match="no such key"):   # today: a plain Exception
    asset.metadata.error_result()
```

### Tests

Added to `liquers-py/tests/test_ERROR_errors.py`:

- `test_ERROR_wrapper_deserialization_raises_serialization_error`: parametrized over every
  `from_json` (all classes in the Phase 1 table) and every `from_yaml` (`Recipe`, `ArgumentInfo`,
  `CommandMetadata`, `CommandMetadataRegistry`, `Plan`), given `"{"` (JSON) or `"a: ["` (YAML):
  `pytest.raises(lq.SerializationError)` and `.type == ErrorType.SerializationError`: AC-1
- `test_ERROR_stored_error_keeps_its_type`: the two cases of AC-2, as in the Example (`env` fixture): AC-2
- `test_ERROR_wrapper_constructors_raise_typed_errors`: `Recipe`, `CommandPreset` → `lq.ParseError`;
  `ExpirationTime("not a time")` → `lq.GeneralError`, as `lq.Expires("not a time")` does: AC-3
- `test_ERROR_typed_wrapper_errors_are_still_exceptions`: `except Exception` catches
  `lq.Metadata.from_json("x")`, and `issubclass(lq.SerializationError, Exception)`: AC-4
- `ac5_no_untyped_mapping_grep`: a static check, not a runtime test. `grep -n "PyException" liquers-py/src/*.rs`
  lists only `error.rs`: AC-5

Run: `bash scripts/test-liquers-py.sh -k ERROR`, then the whole loop.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-py/tests/test_ERROR_errors.py`: add the four tests. Proof: `bash scripts/test-liquers-py.sh -k ERROR`; AC-1, AC-2 and AC-3 fail today (AC-4 passes)
- [x] 2. `liquers-py/src/error.rs`: add `serialization_pyerr`. Proof: `cargo check -p liquers-py --lib --tests`
- [ ] 3. `metadata.rs`, `recipes.rs`, `expiration.rs`: replace the 23 sites; for `ExpirationTime::new`, use `general_error`. Proof: `cargo check -p liquers-py --lib --tests`
- [ ] 4. `command_metadata.rs`, `plan.rs`: replace the 33 sites; drop the `PyException` imports. Proof: `cargo check -p liquers-py --lib --tests` with no new warning; `ac5_no_untyped_mapping_grep` (AC-5)
- [ ] 5. Whole loop. Proof: `bash scripts/test-liquers-py.sh` passes, with the step 1 tests now green
- [ ] 6. `PYTHON_INTEGRATION.md` §6: the serialization sentence (Phase 2 Changes), a History row and a `reviewed:` bump; `liquers-py/README.md` conformance row `ERROR`: add the new tests; `specs/issues/PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION.md` `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

Rollback for any step: revert it. The steps are independent apart from 2 before 3 and 4.

### Validation

`bash scripts/test-liquers-py.sh` (every test passes), `cargo fmt -p liquers-py --check`, `ac5_no_untyped_mapping_grep` and `python3 scripts/docs_index.py --check`. Review the diff: only
`liquers-py/`, `PYTHON_INTEGRATION.md`, the issue and the index.
