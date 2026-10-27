---
id: QUERY-TOKEN-CLASSES-ASCII
kind: design
title: Action names, filenames, segment headers and header values are tested as ASCII
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/query]
issues: [QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII]
created: 2026-10-26
---
# Action names, filenames, segment headers and header values are tested as ASCII

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

Five parser productions test a character by truncating it to its low byte (`c as u8`). A code point
whose low byte happens to be an ASCII letter or digit is accepted, and any other non-ASCII character
is refused. The documented grammar (`DOC_02` §Lexical forms) is ASCII. The parser should test
characters as ASCII, as the template validators already do, so that the accepted set is predictable
and every tool agrees with the parser.

### Problem Example

Verified at HEAD (cd6d5da) with `liquers-validate --no-registry`:

| Query | Production | Today | Should be |
|---|---|---|---|
| `Łoad-x` | `identifier` | `Ok`, action `Łoad` (Ł is U+0141, low byte `0x41` = `A`) | `ParseError` at offset 0 |
| `load/ŁŁ.txt` | `filename` | `Ok`, filename `ŁŁ.txt` | `ParseError` |
| `-RŁ/data/x.csv` | `resource_segment_header` | `Ok`, header name `Ł` | `ParseError` |
| `-R-Ł/data/x.csv` | `header_parameter` | `Ok`, header parameter `Ł` | `ParseError` |
| `-aŁ/x` | `full_transform_segment_header` | `Ok`, realm `aŁ` | `ParseError` |
| `éoad-x` | `identifier` | `ParseError` (low byte `0xE9`) | unchanged |

The test `parse.rs` `a15_non_ascii_is_not_part_of_the_grammar` says "a multi-byte char can never
begin or continue an action name". That holds for `é`, the case it checks, and is false for `Ł`.

### Scope and Acceptance Criteria

- **AC-1** Action names are ASCII
  WHEN `Łoad-x` is parsed
  THEN `parse_query` returns `ParseError` positioned at offset 0
- **AC-2** Filenames are ASCII
  WHEN `load/ŁŁ.txt` is parsed
  THEN `parse_query` returns `ParseError`
- **AC-3** Header names, header parameters and realm names are ASCII
  WHEN `-RŁ/data/x.csv`, `-R-Ł/data/x.csv` or `-aŁ/x` is parsed
  THEN `parse_query` returns `ParseError`
- **AC-4** ASCII input is unchanged
  WHEN every query in the existing parser tests and the `query_backward_compatibility.rs` corpus is
  parsed
  THEN each parses and encodes as before
- **AC-5** Unicode in string parameters still works
  WHEN `f-café` or `f-日本` is parsed
  THEN it parses as today: `is_unescaped_parameter_char` is untouched

Non-goals: resource names (already ASCII, `is_resource_name_char`; Unicode addressing is
`resource-name-unicode`); making the predicates public (that is step 1 of `query-builder`); any
change to what the encoder emits.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-core/src/parse.rs` that narrows the parser
  to the grammar `DOC_02` already documents. It adds private predicates only and leaves `pub` items,
  the encoder and the registry unchanged. The set that stops parsing (non-ASCII code points whose low byte is
  alphanumeric) was never documented, so this is not an interface change (rule 4).
- **Leading issue:** None
- **Explanation:** each truncating closure is replaced by the matching `char::is_ascii_*` test. The
  query-builder design later moves the same predicates to `liquers_core::escape` and makes them
  public. Doing the bug fix first, with private helpers, keeps it eligible for automatic fixing
  (overlap exclusion E1).
- **Open questions:** None.

### Design Dependencies

- `query-builder` — **required-by** (soft; finished Phase 4, not edited). Its Phase 4 step 1 says
  "token predicates in `escape.rs`; parser uses them … fix `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII`
  if it is still open". After this lands, that step only moves these private helpers to
  `escape.rs` and makes them `pub`. The behaviour is already in place.
- `resource-name-unicode` — **overlaps**: changes `is_resource_name_char`, which this design does
  not touch. No order.
- `parameter-entity-escaping` (complete) removed the truncation from string parameters and resource
  names. This design finishes the job for the remaining productions.

### Consolidated Findings

- The source issue named four productions. There are five: `full_transform_segment_header` also
  truncates (`-aŁ/x` parses with realm `aŁ`). Its lead character also needs `is_lowercase`, which
  becomes `is_ascii_lowercase`.
- The issue labelled `-R-Ł/…` as a header *name*. It is a header parameter. The name form is
  `-RŁ/…`. The issue has been corrected.
- `parse.rs` is performance-sensitive. `char::is_ascii_alphanumeric` is a range test on the
  `char`, at most as costly as truncating and then testing, so the hot path does not get slower.
- `DOC_02` §Lexical forms does not cover realm names. One line is added.

## Phase 2: Architecture

### Solution

In `liquers-core/src/parse.rs`, add private `#[inline]` predicates next to `identifier`:

```rust
fn is_identifier_start(c: char) -> bool { c.is_ascii_alphabetic() || c == '_' }
fn is_identifier_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
fn is_filename_suffix_char(c: char) -> bool { is_identifier_char(c) || c == '.' || c == '-' }
fn is_header_value_char(c: char) -> bool { is_identifier_char(c) || c == '.' }
```

Replace the closures:

| Function | Today | After |
|---|---|---|
| `identifier` | `AsChar::is_alpha(c as u8) \|\| c == '_'`, then `AsChar::is_alphanum(c as u8) \|\| c == '_'` | `is_identifier_start`, then `is_identifier_char` |
| `filename` | stem `AsChar::is_alphanum(c as u8) \|\| c == '_'`; suffix `… \|\| '.' \|\| '-'` | stem `is_identifier_char`; suffix `is_filename_suffix_char` |
| `header_parameter` | `(c as u8).is_alphanum() \|\| '_' \|\| '.'` | `is_header_value_char` |
| `full_transform_segment_header` | lead `(c as u8).is_alpha() && c.is_lowercase()`; rest `(c as u8).is_alphanum() \|\| '_'` | lead `c.is_ascii_lowercase()`; rest `is_identifier_char` |
| `resource_segment_header` | `(c as u8).is_alphanum() \|\| '_'` | `is_identifier_char` |

Note that the stem of a filename and the rest of a header name accept a leading digit today. The
helpers keep that (`is_identifier_char`, not `is_identifier_start`). The `AsChar` import is removed
if nothing else uses it.

Rejected: public predicates in `escape.rs` now (adds `pub` API and makes this ineligible for
automatic fixing; `query-builder` does it with its own review); accepting Unicode letters (a
grammar change, and action names become command names in Python and JavaScript).

### Changes

- `liquers-core/src/parse.rs`: four private predicates, five productions, test `a15` updated, new
  tests.
- Documents: `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Lexical forms — one line for
  realm names (a lowercase ASCII letter, then ASCII alphanumerics or `_`), one sentence that a
  non-ASCII character in these tokens is a parse error. History row, `reviewed:` bump.

Sync, pure. No `pub` change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `parse.rs`, `DOC_02` |
| Existing tests | `a15_non_ascii_is_not_part_of_the_grammar` keeps passing, and its comment is corrected. `query_backward_compatibility.rs` (parameter-entity-escaping's T8 corpus) has no non-ASCII token in these positions |
| Compatibility | a stored query or recipe with `Ł`-like characters in an action name, filename or header stops parsing. None in the repository: no `recipes.yaml` contains a non-ASCII character |
| Performance | equal or better on the hot path |
| Recovery | revert |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example table, validated with `liquers-validate --no-registry` at HEAD (five `Ok`, one
`Error`). After the fix all six are `Error`.

### Tests

In `liquers-core/src/parse.rs` `mod tests`:

- `a15_non_ascii_is_not_part_of_the_grammar` — keep, fix the comment (the test was `c as u8`, now
  ASCII), and add `parse_query("\u{141}oad-x")` → `ParseError` at offset 0 (AC-1).
- `non_ascii_low_byte_aliases_are_rejected` (AC-1, AC-2, AC-3): for each of `Łoad-x`, `load/ŁŁ.txt`,
  `-RŁ/data/x.csv`, `-R-Ł/data/x.csv`, `-aŁ/x`, `-R-dŁ/data/x.csv`, `parse_query` is `Err` with
  `ErrorType::ParseError`.
- `ascii_token_classes_unchanged` (AC-4): `load-x`, `_a1/b_2`, `load/x_1.tar.gz`, `-R-meta/data/x.csv`,
  `-Rname-meta/data/x.csv`, `-realm1/x`, `-R/data/x.csv/-/a/b.txt` parse, and `encode()` returns the
  input.
- `unicode_string_parameters_still_parse` (AC-5): `f-café` and `f-日本` parse, and the parameter
  decodes to `café` / `日本`.
- Existing corpus `liquers-core/tests/query_backward_compatibility.rs` (AC-4) and
  `liquers-core/src/escape/tests.rs` (the decoder accepts `café`, `日本`, `Ł`; AC-5).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib parse::tests` and
`cargo test -p liquers-core --test query_backward_compatibility`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `parse.rs` tests — add `non_ascii_low_byte_aliases_are_rejected`, `ascii_token_classes_unchanged`, `unicode_string_parameters_still_parse`, the `Ł` case in `a15`; the rejections fail at HEAD — `cargo test -p liquers-core --lib parse::tests`
- [x] 2. `parse.rs` — add the four private predicates; rewrite `identifier`, `filename`, `header_parameter`, `full_transform_segment_header`, `resource_segment_header`; correct `a15`'s comment — depends on 1 — step 1 tests pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert.
- [x] 3. `DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Lexical forms — realm-name line, non-ASCII sentence; History row, `reviewed:` — `python3 scripts/docs_index.py --check`
- [x] 4. Close `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` with a resolution note; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-template` (its placeholder validators already use ASCII and now agree);
`bash scripts/check-build-matrix.sh` wasm32 row for `liquers-core` is unaffected but cheap to
confirm. Diff review: `parse.rs` and `DOC_02` only.
