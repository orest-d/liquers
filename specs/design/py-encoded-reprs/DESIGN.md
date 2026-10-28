---
id: PY-ENCODED-REPRS
kind: design
title: Key, Expires and ExpirationTime reprs quote their text form once
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [py]
issues: [PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING]
created: 2026-10-27
---
# Key, Expires and ExpirationTime reprs quote their text form once

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written. Split
from `design/py-python-style-reprs/` (its former part A), which keeps the repr convention for the
other classes, `Query` included (Scope Changes).

## Phase 1: High-Level Design

### Purpose

Three `liquers-py` reprs write Python quotes and fill them with Rust's `{:?}` output, so they show
quotes inside quotes or a Rust enum variant. Each class has a constructor that takes its text form,
and the code already writes `Name('…')`; only the text inside is wrong. This design puts the text
form there, quoted as Python quotes a `str`, so the repr evaluates back to an equal object.

### Problem Example

```python
import liquers as lq
repr(lq.parse_key("data/a.txt"))    # today: Key('"data/a.txt"')         expected: Key('data/a.txt')
repr(lq.Expires("in 5 min"))        # today: Expires('InDuration(300s)') expected: Expires('in 5 min')
repr(lq.ExpirationTime("never"))    # today: ExpirationTime('Never')     expected: ExpirationTime('never')
```

Verified at HEAD (`04667a80`): `liquers-py/src/query.rs` `Key::__repr__` is
`format!("Key('{:?}')", self.0.encode())`; `liquers-py/src/expiration.rs` `Expires::__repr__` is
`format!("Expires('{:?}')", self.0)` and `ExpirationTime::__repr__` is
`format!("ExpirationTime('{:?}')", self.0)`, where both core enums derive `Debug`
(`liquers-core/src/expiration.rs`). `Key::new(key: &str)`, `Expires::new(spec: &str)` and
`ExpirationTime::new(spec: &str)` take the text form; `Expires::encode` and
`ExpirationTime::encode` return `to_string()`, the core `Display` (`in 5 min`, `end of day`,
`never`). `Query::__repr__` has the same defect, but `Query::new` takes no argument, so whether a
`Query` repr should name text it cannot be rebuilt from is `py-python-style-reprs`' open
convention question.

### Scope and Acceptance Criteria

- **AC-1** ~~Key and Query are quoted once~~ Retired 2026-10-27: `Query` moved to `py-python-style-reprs` (its AC-6); `Key` is AC-4.
  WHEN ~~`repr` is taken of `lq.parse("-R/data/a.txt")` and of `lq.parse_key("data/a.txt")`~~
  THEN ~~it is `Query('-R/data/a.txt')` and `Key('data/a.txt')`, and `eval(repr(k), vars(lq)) == k` for the key~~
- **AC-2** Expiration reprs use their text form
  WHEN `repr` is taken of `lq.Expires("in 5 min")`, `lq.Expires("end of day")` and
  `lq.ExpirationTime("never")`
  THEN it is `Expires('in 5 min')`, `Expires('end of day')` and `ExpirationTime('never')`, and
  each evaluates back to an equal object
- **AC-3** ~~Escaped query text is shown encoded~~ Retired 2026-10-27: moved back to `py-python-style-reprs` (its AC-3) with `Query`.
  WHEN ~~`repr` is taken of `lq.parse("hello-a~_b")`, whose parameter decodes to `a-b`~~
  THEN ~~it is `Query('hello-a~_b')`, which equals `"Query(" + repr(q.encode()) + ")"`~~
- **AC-4** Key is quoted once and evaluates back
  WHEN `repr` is taken of `lq.parse_key("data/a.txt")`
  THEN it is `Key('data/a.txt')`, equals `"Key(" + repr(k.encode()) + ")"`, and
  `eval(repr(k), vars(lq)) == k`

Non-goals: `Query` and the other 23 classes, and any repr convention (`py-python-style-reprs`);
`__str__`, equality, hashing and constructors.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible, re-checked after `Query` moved out. A bug fix that restores
  plainly intended behaviour (rule 2): each method already writes `Name('…')`, and each of the
  three classes has a constructor taking exactly the text that belongs inside, so the fixed repr
  is the standard Python one that evaluates back. Size `S`, two files of one crate (rules 1, 6), no
  new structure (rule 3; one `pub(crate)` helper function). Rule 4: no signature, method, argument
  or class changes; the repr text is debugging output by Python convention, no document defines
  today's text (`PYTHON_INTEGRATION.md` §8 lists it as a limitation), and no test or guide example
  compares it (`grep -rn "repr(" liquers-py/tests specs/guides/PYTHON_GUIDE.md`: one `print` and
  commented-out lines). Only a caller parsing today's repr text would notice.
- **Leading issue:** None
- **Explanation:** for these three classes the intended form is fixed by the existing code and
  constructors; nothing depends on `py-python-style-reprs`' convention question, which was the
  reason `Query` moved out.
- **Open questions:** None. Implementation detail decided in Phase 2: quote with Python's own
  `repr` of a `str` rather than hand-escaping.

### Design Dependencies

- `py-python-style-reprs` (needs decision): **required-by**. It reuses the `pub(crate) fn quoted`
  helper this design adds to `query.rs` for its rule (a) (and `Query`); if it lands first, it adds
  the helper and this design reuses it. The two edit different `__repr__` methods.
- `py-docstrings`: **overlaps** (weak). It adds `///` lines in the same files; `__repr__` is a
  dunder and outside its coverage set. Whichever lands second rebases.

### Scope Changes

- **2026-10-27, `Query` moved out** (review): `Query` cannot be rebuilt from its repr, which is
  exactly what `py-python-style-reprs` question 1 decides, so it went to that design's part B.
  AC-1 and AC-3 are retired (not renumbered); `Key` is AC-4. Size stays `S`; eligibility re-checked
  above and unchanged.

## Phase 2: Architecture

### Solution

Add to `liquers-py/src/query.rs`:

```rust
/// `text` as Python's `repr` of a `str` writes it; never fails.
pub(crate) fn quoted(py: Python<'_>, text: &str) -> String
```

It returns `PyString::new(py, text).repr().map(|r| r.to_string())`, falling back to
`format!("'{text}'")` on error, as `asset.rs` `State::__repr__` already falls back. Then
`Key::__repr__(&self, py: Python<'_>) -> String` returns
`format!("Key({})", quoted(py, &self.0.encode()))`, and in `expiration.rs` (through
`use crate::query::quoted;`) `Expires::__repr__` and `ExpirationTime::__repr__` take `py` and
return `format!("Expires({})", quoted(py, &self.0.to_string()))` and the same for
`ExpirationTime`. The return type stays `String`, so `repr` never raises.

Rejected: `{}` instead of `{:?}` inside hand-written quotes (breaks silently if a text form ever
holds a quote or backslash); keeping `{:?}` and stripping its quotes (Rust escaping is not Python
escaping); a private copy of the helper per file (one helper is shared with `py-python-style-reprs`).

Known-issue preflight: none. `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` covers `Query` and the rest.

### Changes

`liquers-py/src/query.rs` (`quoted`, `Key::__repr__`) and `liquers-py/src/expiration.rs`
(`Expires::__repr__`, `ExpirationTime::__repr__`). New test file
`liquers-py/tests/test_OBJECT_encoded_reprs.py`. No signature visible from Python, no command and
no registry change. `PYTHON_INTEGRATION.md` §8 keeps its "`repr` of several classes is Rust's
debug format" line until `py-python-style-reprs` lands, so no document changes here.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A test compares a repr | grep above finds none | high |
| `Expires` text loses a time zone (`on <day>` drops it in `Display`) | AC-2 uses specs without a zone; the repr is still the text `encode()` gives | high |
| `PyString::repr` fails | Fallback to plain quotes; it cannot fail for a valid `str` | high |

Recovery: revert the two files. Nothing is persisted.

## Phase 3: Examples and Tests

### Examples

```python
k = lq.parse_key("data/a.txt")
assert repr(k) == "Key('data/a.txt')" and eval(repr(k), vars(lq)) == k
assert repr(lq.Expires("in 5 min")) == "Expires('in 5 min')"
```

### Tests

`liquers-py/tests/test_OBJECT_encoded_reprs.py`:

- `test_OBJECT_repr_key_quoted_once`: AC-4
- `test_OBJECT_repr_expiration_uses_text_form`: the three specs, each with an `eval` round trip: AC-2
- AC-1 and AC-3 are retired; the `Query` tests are `py-python-style-reprs`'

Run: `bash scripts/test-liquers-py.sh -k encoded_reprs`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-py/tests/test_OBJECT_encoded_reprs.py` — the two tests — `bash scripts/test-liquers-py.sh -k encoded_reprs` fails as in the Problem Example
- [x] 2. `liquers-py/src/query.rs` `quoted` (unless `py-python-style-reprs` already added it) and `Key::__repr__`; `expiration.rs` `Expires::__repr__`, `ExpirationTime::__repr__` — `bash scripts/test-liquers-py.sh` passes in full; `cargo fmt -p liquers-py --check`
- [ ] 3. `specs/issues/PY-ENCODED-REPRS-QUOTE-THE-DEBUG-STRING.md` `status: closed` with a resolution note; this design `status: complete`; `specs/README.md` row; `python3 scripts/docs_index.py` — `python3 scripts/docs_index.py --check`

Rollback: revert per step.

### Validation

`bash scripts/test-liquers-py.sh`, `cargo fmt -p liquers-py --check`,
`python3 scripts/docs_index.py --check`. The diff touches three `__repr__` bodies, one helper, one
new test file, the issue, the design and the index.
