---
id: PY-PYTHON-STYLE-REPRS
kind: design
title: liquers-py classes have Python-style reprs
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-REPRS-SHOW-RUST-DEBUG-FORMAT]
created: 2026-10-27
---
# liquers-py classes have Python-style reprs

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

`repr` of 27 `liquers-py` classes shows Rust's `Debug` output. In four of them it is plainly a bug:
the code writes Python string quotes, `Key('…')`, and fills them with `{:?}`, which adds a second
pair. This design fixes those four (**part A**). It then gives every other class a short
Python-style `repr` under one convention (**part B**). The convention is a choice the repository
has not made yet.

### Problem Example

```python
import liquers as lq
repr(lq.parse("-R/data/a.txt"))     # today:    Query('"-R/data/a.txt"')
                                    # expected: Query('-R/data/a.txt')
repr(lq.parse_key("data/a.txt"))    # today:    Key('"data/a.txt"');  expected: Key('data/a.txt')
repr(lq.Expires("in 5 min"))        # today:    Expires('InDuration(300s)'); expected: Expires('in 5 min')
```

(`-R/data/a.txt` validates as one `GetAsset` step on key `data/a.txt`.) Re-checked at HEAD (`04667a80`,
`grep -n -A3 __repr__ liquers-py/src/*.rs`):

| Group | Sites |
|---|---|
| A: `'{:?}'` around a value with a text form | `query.rs` `Key`, `Query` (`{:?}` of `encode()`); `expiration.rs` `Expires`, `ExpirationTime` (`{:?}` of the enum; the constructor takes the `Display` text) |
| B1: `'{:?}'` around an enum that has no text constructor | `query.rs` `QuerySource`; `dependencies.rs` `DependencyRelation`; `metadata.rs` `LogEntryKind` (`'Info'`) |
| B2: bare `format!("{:?}", …)` | `query.rs` `ActionParameter`, `ResourceName`, `ActionRequest`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment`; `command_metadata.rs` `EnumArgument`, `ArgumentType`, `CommandKey`, `ArgumentInfo`, `CommandDefinition`, `CommandPreset`, `CommandMetadata`; `metadata.rs` `DependencyRecord`, `Metadata`, `MetadataRecord`, `LogEntry`; `recipes.rs` `Recipe`; `dependencies.rs` `PlanDependency` |
| Already Python-style | `Version`, `DependencyKey`, `Status`, `AssetRef`, `Issue`, `Position` |

No repository document sets a `repr` convention. `PYTHON_INTEGRATION.md` §1 says only that default
reprs read `liquers.X`. `PYTHON_GUIDE.md` and `liquers-py/README.md` say nothing about it.
`LANGUAGE-INTEGRATION_GUIDE.md` OBJECT lists "`repr`/`toString`" among the conventions each
integration's design must answer, and `design/liquers-py/` did not answer it.

### Scope and Acceptance Criteria

- **AC-1** Key and Query are quoted once
  WHEN `repr` is taken of `lq.parse("-R/data/a.txt")` and of `lq.parse_key("data/a.txt")`
  THEN it is `Query('-R/data/a.txt')` and `Key('data/a.txt')`, and
  `eval(repr(k), vars(lq)) == k` for the key
- **AC-2** Expiration reprs use their text form
  WHEN `repr` is taken of `lq.Expires("in 5 min")` and of `lq.ExpirationTime("never")`
  THEN it is `Expires('in 5 min')` and `ExpirationTime('never')`, and each evaluates back to an
  equal object
- **AC-3** Escaped query text is shown encoded
  WHEN `repr` is taken of `lq.parse("hello-a~_b")`, whose parameter decodes to `a-b`
  THEN it is `Query('hello-a~_b')`, the `encode()` text, quoted as Python's `repr(str)` quotes it
- **AC-4** No Debug rendering is left (part B)
  WHEN `repr` is taken of an instance of each class in rows B1 and B2
  THEN it follows the convention decided in Design Readiness, contains no Rust field syntax
  (`{ `, `: Some(`, `None,`), and starts with the class name or `<liquers.`
- **AC-5** Enum-like kinds read as Python enums (part B)
  WHEN `repr(lq.LogEntryKind.Info)` is taken
  THEN it is `LogEntryKind.Info`

Non-goals: `__str__` (it keeps its current text); classes in `plan.rs`, which have no `__repr__`
and show PyO3's default `<liquers.Plan object at …>`; any change to equality, hashing or
constructors.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Part B introduces a `repr` format that no document defines,
  so it does not restore documented behaviour (rule 2), and its convention is an open design
  question (rule 5). **Part A (AC-1 to AC-3) would be eligible on its own**: it is a bug fix in
  `query.rs` and `expiration.rs`, its intended form is the one the code already writes, and it
  changes no interface. The split is recommended in the closing report; no issue is filed here.
- **Leading issue:** the repr convention for the B classes.
- **Explanation:** part A is fully determined. Part B is a working plan under the recommended
  convention, but the result is visible in every session and test message, so the maintainer
  should choose it.
- **Open questions:**
  1. **Open design: repr convention (B1, B2).** Recommended: Python's own rule (`object.__repr__`).
     Use an expression that rebuilds the object where one exists, and otherwise
     `<liquers.Name field=value …>`, so that:
     (a) a class with an `encode()` text form shows `Name('<encode()>')`;
     (b) `LogEntryKind` shows `LogEntryKind.<Variant>`;
     (c) the rest show `<liquers.Name …>` with the fields listed in Phase 2.
     The alternative, `Name(field=value, …)` for every class, looks like a constructor call that
     most of these classes do not have.
  2. **Proposed resolution: which fields (c) shows.** Phase 2's table: at most three short
     fields per class, with each value as its Python `repr`.

### Design Dependencies

- `py-metadata-typed-errors`, `py-metadata-fields-as-properties`: **overlaps** (weak). They edit
  other methods of the same classes (`metadata.rs`). There is no order.
- `py-api-reference` (needs decision): **overlaps** (weak). Docstrings could document the
  repr, but there is no shared contract.

### Consolidated Findings

- To quote as Python does (AC-3), call `PyString::new(py, s).repr()` rather than hand-escaping. A
  `__repr__` may take `py: Python<'_>`, as `asset.rs` `AssetRef::__repr__` already does. Encoded
  query text has no quotes or non-ASCII characters (`-R/it's` and `-R/data/café.txt` do not parse;
  they need tilde entities), so the quoting never has to escape anything in practice.
- `Expires`' `Display` drops the time zone of `on <day>` (`expiration.rs`), so AC-2 asserts a
  round trip only for specs without a zone.
- Recommended split: part A as its own eligible item (AC-1 to AC-3, Phase 4 steps 1-2); part B
  waits for decision 1.

## Phase 2: Architecture

### Solution

**Part A.** In `query.rs` `Key::__repr__` and `Query::__repr__`, and in `expiration.rs`
`Expires::__repr__` and `ExpirationTime::__repr__`, take `py` and return
`format!("{}({})", name, PyString::new(py, &text).repr()?)`, where `text` is `encode()` for `Key`
and `Query` and `to_string()` for the two expiration classes. The return type becomes
`PyResult<String>`. A private helper in each file, `fn quoted(py, text: &str) -> PyResult<String>`,
avoids repeating the call.

**Part B**, under the recommended convention:

| Rule | Classes | Example |
|---|---|---|
| (a) `Name('<encode()>')` | `ActionParameter`, `ResourceName`, `ActionRequest`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment`, `QuerySource`, `DependencyRelation` | `ActionRequest('to_text')` |
| (b) `Name.Variant` | `LogEntryKind` | `LogEntryKind.Info` |
| (c) `<liquers.Name …>` | `EnumArgument` (name), `ArgumentType` (its serialized name), `CommandKey` (realm, namespace, name), `ArgumentInfo` (name, argument_type), `CommandDefinition` (key), `CommandPreset` (action), `CommandMetadata` (name, namespace), `DependencyRecord` (key, version), `Metadata` / `MetadataRecord` (status, type_identifier, query), `LogEntry` (kind, message), `Recipe` (query, title), `PlanDependency` (key, relation) | `<liquers.Metadata status='Ready' type_identifier='Text' query='hello'>` |

A field that is absent or fails (legacy `Metadata` without a query) is left out rather than
raising, because `repr` must not throw.

Rejected alternatives: keeping `Debug` (it shows Rust field names); `Name(field=…)` everywhere (it
looks like a constructor that does not exist); a generic reflection-based repr (it would expose
every field, which is the defect).

Known-issue preflight: none block. `PY-METADATA-FIELDS-ARE-METHODS` changes how the `Metadata`
fields are read in Python, not the Rust accessors that the repr uses.

### Changes

`liquers-py/src/query.rs`, `expiration.rs` (part A); `query.rs`, `command_metadata.rs`,
`metadata.rs`, `recipes.rs`, `dependencies.rs` (part B): only the `__repr__` bodies and private
helpers. A new test file is `liquers-py/tests/test_OBJECT_reprs.py`. The documents gain one
sentence in `PYTHON_INTEGRATION.md` §2 stating the convention (with a History row and a
`reviewed:` bump), and §8's "`repr` of several classes is Rust's debug format" is removed. No
commands, signatures visible from Python, or registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test or doctest compares a repr | `grep -rn "repr(" liquers-py/tests specs/guides/PYTHON_GUIDE.md` finds only a `print` and commented-out lines | high |
| `PyString::repr` fails | It cannot fail for a valid `str`. The `?` propagates anyway | high |
| A (c) field accessor fails | Leave the field out (Solution) | high |
| The convention is rejected | Part A stands alone; part B is redone from the table | medium |

Recovery: revert. Nothing is persisted.

## Phase 3: Examples and Tests

### Examples

```python
k = lq.parse_key("data/a.txt")
assert repr(k) == "Key('data/a.txt')" and eval(repr(k), vars(lq)) == k
assert repr(lq.LogEntryKind.Info) == "LogEntryKind.Info"
```

### Tests

`liquers-py/tests/test_OBJECT_reprs.py`:

- `test_OBJECT_repr_key_and_query_quoted_once`: AC-1
- `test_OBJECT_repr_expiration_uses_text_form`: `Expires("in 5 min")`, `Expires("end of day")`,
  `ExpirationTime("never")`, each with an `eval` round trip: AC-2
- `test_OBJECT_repr_query_shows_encoded_text`: `repr(lq.parse("hello-a~_b")) == "Query('hello-a~_b')"`
  and equals `"Query(" + repr(q.encode()) + ")"`: AC-3
- `test_OBJECT_repr_no_debug_rendering`: parametrized over one instance of every B1/B2 class
  (built with its constructor or from `lq.parse("hello/to_text")`, `env.evaluate_state("hello")`
  with a `hello` command), asserting the AC-4 prefix and the absence of `{ `, `: Some(`, `None,`: AC-4
- `test_OBJECT_repr_log_entry_kind_is_enum_style`: AC-5

`hello/to_text` and `hello-a~_b` validate (with `--command hello`); the first plans the actions
`hello` and `to_text`. Run:
`bash scripts/test-liquers-py.sh -k repr`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-py/tests/test_OBJECT_reprs.py`: add the AC-1 to AC-3 tests. Proof: `bash scripts/test-liquers-py.sh -k repr` fails as in the Problem Example
- [ ] 2. `query.rs` `Key`/`Query` and `expiration.rs` `Expires`/`ExpirationTime` `__repr__` (part A). Proof: the step 1 tests pass; `cargo check -p liquers-py --lib --tests`
- [ ] 3. After decision 1: add the AC-4 and AC-5 tests. Proof: they fail today
- [ ] 4. The B1/B2 `__repr__`s in `query.rs`, `dependencies.rs`, `metadata.rs`, `command_metadata.rs` and `recipes.rs`, per the Phase 2 table. Proof: `bash scripts/test-liquers-py.sh` passes in full
- [ ] 5. `PYTHON_INTEGRATION.md` §2 convention sentence and §8 line, History row and `reviewed:`; issue `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

Steps 1-2 are part A and can merge alone. Rollback: revert per step.

### Validation

`bash scripts/test-liquers-py.sh`, `cargo fmt -p liquers-py --check`,
`python3 scripts/docs_index.py --check`. The diff should touch only `__repr__` bodies, helpers, the
new test file and the documents.
