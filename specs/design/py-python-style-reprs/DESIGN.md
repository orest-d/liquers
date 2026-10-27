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

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written. Part A
(`Key`, `Expires`, `ExpirationTime`, whose reprs quote Rust debug text) was split off the same day
into `design/py-encoded-reprs/`; `Query` went there and came back on review; see Scope Changes.

## Phase 1: High-Level Design

### Purpose

`repr` of 27 `liquers-py` classes shows Rust's `Debug` output. Three of them (`Key`, `Expires`,
`ExpirationTime`, the former **part A**) are a plain bug, fixed by `py-encoded-reprs`, because
their reprs can evaluate back. This design gives the other 24 classes, `Query` included, a short
Python-style `repr` under one convention (**part B**). The convention is a choice the repository
has not made yet.

### Problem Example

```python
import liquers as lq
repr(lq.ActionRequest("to_text"))
# today:    ActionRequest { name: "to_text", parameters: [], … }   (Rust Debug output, abbreviated)
# expected: under the recommended convention, ActionRequest('to_text')
repr(lq.LogEntryKind.Info)          # today: 'Info'        expected: LogEntryKind.Info
```

`repr(lq.parse("-R/data/a.txt"))` is `Query('"-R/data/a.txt"')` today (quotes inside quotes);
under the recommended convention it is `Query('-R/data/a.txt')`. (The part A sites, `Key`,
`Expires` and `ExpirationTime`, are `py-encoded-reprs`' Problem Example.) Re-checked at HEAD (`04667a80`,
`grep -n -A3 __repr__ liquers-py/src/*.rs`):

| Group | Sites |
|---|---|
| A (moved to `py-encoded-reprs`): `'{:?}'` around a value with a text constructor | `query.rs` `Key` (`{:?}` of `encode()`); `expiration.rs` `Expires`, `ExpirationTime` (`{:?}` of the enum; the constructor takes the `Display` text) |
| B0: `'{:?}'` around a text form with no text constructor | `query.rs` `Query` (`{:?}` of `encode()`; `Query::new` takes no argument) |
| B1: `'{:?}'` around an enum that has no text constructor | `query.rs` `QuerySource`; `dependencies.rs` `DependencyRelation`; `metadata.rs` `LogEntryKind` (`'Info'`) |
| B2: bare `format!("{:?}", …)` | `query.rs` `ActionParameter`, `ResourceName`, `ActionRequest`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment`; `command_metadata.rs` `EnumArgument`, `ArgumentType`, `CommandKey`, `ArgumentInfo`, `CommandDefinition`, `CommandPreset`, `CommandMetadata`; `metadata.rs` `DependencyRecord`, `Metadata`, `MetadataRecord`, `LogEntry`; `recipes.rs` `Recipe`; `dependencies.rs` `PlanDependency` |
| Already Python-style | `Version`, `DependencyKey`, `Status`, `AssetRef`, `Issue`, `Position` |

Constructors of the text-form classes (`query.rs`): `ActionParameter::new(parameter, position)`
and `ResourceName::new(name, position)` take a `Position`; `ActionRequest::new(name)` takes a name
only; `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment` and `Query`
take no argument; `QuerySource` and `DependencyRelation` have no `#[new]`. So almost none of them
can be rebuilt from `Name('<text>')`. `QuerySegment` is not registered in `lib.rs` or `__all__`;
it is reached only through `Query.segments`.

No repository document sets a `repr` convention. `PYTHON_INTEGRATION.md` §1 says only that default
reprs read `liquers.X`. `PYTHON_GUIDE.md` and `liquers-py/README.md` say nothing about it.
`LANGUAGE-INTEGRATION_GUIDE.md` OBJECT lists "`repr`/`toString`" among the conventions each
integration's design must answer, and `design/liquers-py/` did not answer it.

### Scope and Acceptance Criteria

- **AC-1** ~~Key and Query are quoted once~~ Retired 2026-10-27: `Key` moved to `py-encoded-reprs` (its AC-4); `Query` is AC-6.
  WHEN ~~`repr` is taken of `lq.parse("-R/data/a.txt")` and of `lq.parse_key("data/a.txt")`~~
  THEN ~~it is `Query('-R/data/a.txt')` and `Key('data/a.txt')`, and
  `eval(repr(k), vars(lq)) == k` for the key~~
- **AC-2** ~~Expiration reprs use their text form~~ Retired 2026-10-27: moved to `py-encoded-reprs` (its AC-2).
  WHEN ~~`repr` is taken of `lq.Expires("in 5 min")` and of `lq.ExpirationTime("never")`~~
  THEN ~~it is `Expires('in 5 min')` and `ExpirationTime('never')`, and each evaluates back to an
  equal object~~
- **AC-3** Escaped query text is shown encoded (reinstated 2026-10-27 with `Query`)
  WHEN `repr` is taken of `lq.parse("hello-a~_b")`, whose parameter decodes to `a-b`
  THEN it is `Query('hello-a~_b')`, the `encode()` text, quoted as Python's `repr(str)` quotes it
- **AC-4** No Debug rendering is left (part B)
  WHEN `repr` is taken of an instance of each class in rows B1 and B2
  THEN it follows the convention decided in Design Readiness, contains no Rust field syntax
  (`{ `, `: Some(`, `None,`), and starts with the class name or `<liquers.`
- **AC-5** Enum-like kinds read as Python enums (part B)
  WHEN `repr(lq.LogEntryKind.Info)` is taken
  THEN it is `LogEntryKind.Info`
- **AC-6** Query is quoted once (part B)
  WHEN `repr` is taken of `lq.parse("-R/data/a.txt")`
  THEN it is `Query('-R/data/a.txt')` under the recommended convention, with no doubled quotes

Non-goals: `Key`, `Expires`, `ExpirationTime` (`py-encoded-reprs`); `__str__` (it keeps its current text); classes in `plan.rs`, which have no `__repr__`
and show PyO3's default `<liquers.Plan object at …>`; any change to equality, hashing or
constructors.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. Part B introduces a `repr` format that no document defines,
  so it does not restore documented behaviour (rule 2), and its convention is an open design
  question (rule 5). The eligible part A was split off into `py-encoded-reprs`; `Query` stays
  here because its repr depends on question 1.
- **Leading issue:** the repr convention for the B classes.
- **Explanation:** part B is a working plan under the recommended convention, but the result is
  visible in every session and test message, so the maintainer should choose it.
- **Open questions:**
  1. **Open design: repr convention (B0, B1, B2).** Recommended:
     (a) a class with a text form (`encode()`) shows `Name('<text>')`, which **names the text form
     and is not guaranteed to evaluate**: of the (a) classes only `ActionRequest` without
     parameters could be rebuilt that way (constructors above);
     (b) `LogEntryKind` shows `LogEntryKind.<Variant>`;
     (c) the rest show `<liquers.Name …>` with the fields listed in Phase 2.
     It reads like the query text users write, and `Key` / `Expires` (`py-encoded-reprs`) have
     the same shape and do evaluate. Alternative, Python's strict rule (`object.__repr__`): only a
     rebuilding expression may look like a call, so every (a) class moves to (c) as
     `<liquers.Name '<text>'>`. A third, `Name(field=value, …)` for every class, looks like a
     constructor call that most of these classes do not have.
  2. **Proposed resolution: which fields (c) shows.** Phase 2's table: at most three short
     fields per class, with each value as its Python `repr`.

### Design Dependencies

- `py-encoded-reprs` (ready, eligible): **requires** (soft). It fixes the three part A reprs and
  adds `pub(crate) fn quoted` in `query.rs`, which rule (a) and `Query` reuse; landing it first
  keeps the two diffs from both adding the helper. If this design lands first, it adds the helper
  and `py-encoded-reprs` reuses it.
- `py-metadata-typed-errors`, `py-metadata-fields-as-properties`: **overlaps** (weak). They edit
  other methods of the same classes (`metadata.rs`). There is no order.
- `py-docstrings` (split from `py-api-reference`): **overlaps** (weak). Docstrings could document
  the repr, but there is no shared contract, and `__repr__` is outside its coverage set.

### Scope Changes

- **2026-10-27, part A split off.** The four reprs that fill their own quotes with `{:?}` (`Key`,
  `Query`, `Expires`, `ExpirationTime`; former AC-1 to AC-3, Phase 2 "Part A", Phase 4 steps 1-2)
  moved to `py-encoded-reprs` and its issue `PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING`, so that the
  plain bug can be fixed automatically without waiting for the convention (overlap exclusion E1
  in reverse: an eligible part kept inside an ineligible design loses its eligibility). AC-1 to
  AC-3 are retired, not renumbered. Size stays `M` (23 reprs in five files). Readiness and
  eligibility are unchanged; no phase needs re-approval, since only part A's text left.
- **2026-10-27, `Query` back** (review): `Query` cannot be rebuilt from its repr, which is what
  question 1 decides, so it returned from `py-encoded-reprs` as row B0, rule (a). AC-3 is
  reinstated unchanged and the `Query` half of retired AC-1 is AC-6. Size stays `M` (24 reprs).

### Consolidated Findings

- To quote as Python does (AC-3, AC-6), use `py-encoded-reprs`' `quoted` helper
  (`PyString::new(py, s).repr()`, with a plain-quote fallback) rather than hand-escaping. A
  `__repr__` may take `py: Python<'_>`, as `asset.rs` `AssetRef::__repr__` already does. Encoded
  query text has no quotes or non-ASCII characters (`-R/it's` and `-R/data/café.txt` do not parse;
  they need tilde entities), so the quoting never has to escape anything in practice.
- The `Expires` time-zone finding moved to `py-encoded-reprs` with the expiration classes.
- Part A was split off as `py-encoded-reprs` (Scope Changes); part B waits for decision 1.

## Phase 2: Architecture

### Solution

Part A (`Key`, `Expires`, `ExpirationTime`) is `py-encoded-reprs`. Its helper
`pub(crate) fn quoted(py, text) -> String` in `query.rs` is reused by rule (a) below.

**Part B**, under the recommended convention:

| Rule | Classes | Example |
|---|---|---|
| (a) `Name('<text>')`, naming the text form, not guaranteed to evaluate | `Query`, `ActionParameter`, `ResourceName`, `ActionRequest`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySegment`, `QuerySource`, `DependencyRelation` | `ActionRequest('to_text')` |
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

`liquers-py/src/query.rs`, `command_metadata.rs`,
`metadata.rs`, `recipes.rs`, `dependencies.rs` (part B): only the `__repr__` bodies and private
helpers. A new test file is `liquers-py/tests/test_OBJECT_reprs.py`. The documents gain one
sentence in `PYTHON_INTEGRATION.md` §2 stating the convention (with a History row and a
`reviewed:` bump), and §8's "`repr` of several classes is Rust's debug format" is removed. No
commands, signatures visible from Python, or registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test or doctest compares a repr | `grep -rn "repr(" liquers-py/tests specs/guides/PYTHON_GUIDE.md` finds only a `print` and commented-out lines | high |
| `PyString::repr` fails | It cannot fail for a valid `str`; `quoted` falls back to plain quotes and returns `String` | high |
| A (c) field accessor fails | Leave the field out (Solution) | high |
| The convention is rejected | Part B is redone from the table | medium |

Recovery: revert. Nothing is persisted.

## Phase 3: Examples and Tests

### Examples

```python
assert repr(lq.LogEntryKind.Info) == "LogEntryKind.Info"
assert repr(lq.ActionRequest("to_text")) == "ActionRequest('to_text')"
```

### Tests

`liquers-py/tests/test_OBJECT_reprs.py`:

- AC-1 and AC-2 are retired here; the `Key` and expiration tests are in `py-encoded-reprs`
  (`liquers-py/tests/test_OBJECT_encoded_reprs.py`)
- `test_OBJECT_repr_query_quoted_once`: AC-6
- `test_OBJECT_repr_query_shows_encoded_text`: `repr(lq.parse("hello-a~_b")) == "Query('hello-a~_b')"`
  and equals `"Query(" + repr(q.encode()) + ")"`: AC-3
- `test_OBJECT_repr_no_debug_rendering`: parametrized over one instance of every B1/B2 class
  (built with its constructor or from `lq.parse("hello/to_text")`, `env.evaluate_state("hello")`
  with a `hello` command; `QuerySegment`, which is not registered, as
  `lq.parse("hello/to_text").segments[0]`), asserting the AC-4 prefix and the absence of `{ `,
  `: Some(`, `None,`: AC-4
- `test_OBJECT_repr_log_entry_kind_is_enum_style`: AC-5

`hello/to_text` and `hello-a~_b` validate (with `--command hello`); the first plans the actions
`hello` and `to_text`. Run:
`bash scripts/test-liquers-py.sh -k repr`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. After decision 1: `liquers-py/tests/test_OBJECT_reprs.py` with the AC-3, AC-4, AC-5 and AC-6 tests. Proof: they fail today
- [ ] 2. If `py-encoded-reprs` has not landed, add its `quoted` helper to `query.rs`. Proof: `cargo check -p liquers-py --lib --tests`
- [ ] 3. `Query::__repr__` and the B1/B2 `__repr__`s in `query.rs`, `dependencies.rs`, `metadata.rs`, `command_metadata.rs` and `recipes.rs`, per the Phase 2 table. Proof: `bash scripts/test-liquers-py.sh` passes in full
- [ ] 4. `PYTHON_INTEGRATION.md` §2 convention sentence and §8 line, History row and `reviewed:`; issue `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

Part A's former steps 1-2 are `py-encoded-reprs` Phase 4. Rollback: revert per step.

### Validation

`bash scripts/test-liquers-py.sh`, `cargo fmt -p liquers-py --check`,
`python3 scripts/docs_index.py --check`. The diff should touch only `__repr__` bodies, helpers, the
new test file and the documents.
