---
id: COMBINED-VALUE-IDENTIFIER-DISPATCH
kind: design
title: CombinedValue reads an identifier the extension declares through the extension
form: compact
gh_pr: [97]
workflow: liquers-project
phase: documentation
readiness: ready
autofix: eligible
area: [lib/value]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
issues: [COMBINED-VALUE-DISCRIMINATION]
created: 2026-10-09
---
# CombinedValue reads an identifier the extension declares through the extension

## Phase 1: High-Level Design

### Purpose

`CombinedValue::deserialize_from_bytes` (`liquers-lib/src/value/extended.rs`) asks the base value
first and the extension only when the base refuses. The base accepts some formats whatever the type
identifier, so an extended value written in those formats reads back as a *base* value of another
type. The declared type identifier must decide which half reads the bytes.

### Problem Example

```rust
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
use liquers_lib::value::{ExtValueInterface, Value};

let value = Value::from_record_view(batch);          // a RecordBatch with 3 rows
let bytes = value.as_bytes("json")?;                 // a JSON array of 3 records
let back = Value::deserialize_from_bytes(&bytes, "RecordView", "json")?;
back.identifier()
```

**Today:** `"Array"`. `SimpleValue::deserialize_from_bytes` (`liquers-lib/src/value/simple.rs`)
reads every `json` identifier it does not name as plain JSON, so `CombinedValue` never asks
`ExtValue`. The same happens to a `RecordView` written as `html` (read as `Text`; `txt`, `html` and
`toml` read as `Text` for any identifier) and to a `RecordSource` manifest written as `yaml` or
`json` (read as `Object`). Every other `RecordView` format reads back correctly only because the
base happens to refuse it. A stored or cached record view written as JSON comes back from the asset
layer as an array, and a command expecting a `RecordView` fails on it.

**Expected:** `"RecordView"`, with 3 rows, because `ExtValue::type_descriptions()` declares
`RecordView`. An identifier the extension declares is the extension's to read, and only the
extension's.

### Scope and Acceptance Criteria

- **AC-1** Extended JSON reads as the extended type
  WHEN a `RecordView` is written as `json` and read back through `Value` with identifier `RecordView`
  THEN the result is a `RecordView` with the same number of rows, not an `Array`
- **AC-2** A manifest reads as a source
  WHEN a `RecordSource` with a manifest is written as `yaml` or `json` and read back through `Value`
  with identifier `RecordSource`
  THEN the result is a `RecordSource`, not an `Object`
- **AC-3** A write-only extended format refuses instead of changing type
  WHEN bytes are read through `Value` with an identifier the extension declares, in a format the
  extension cannot read (`RecordView` as `html`)
  THEN the read fails with the extension's error, and no `Text` value is produced
- **AC-4** Extension identifiers never become base values
  WHEN any identifier in `ExtValue::type_descriptions()` is read through `Value` in any format it
  declares
  THEN the result is either an extended value with that identifier or an error, never a base value
- **AC-5** Base and untyped reads are unchanged
  WHEN a base identifier (`Text`, `I32`, `Array`, `Bytes`, `Query`, …) or the empty identifier is
  read through `Value` in any format
  THEN the result is what it is at HEAD: the existing `SimpleValue` round-trip tests, the untyped
  `csv` → `Bytes` fallback and an extension's format inference for `""` all still pass
- **AC-6** The refusal comes from the side that owns the identifier
  WHEN neither half reads a named identifier
  THEN the error is the base's if the base declares the identifier, and the extension's otherwise
  (added in Phase 3 from Phase 2 rule 3; see Scope Changes)

**Non-goals.** `liquers_core::value::Value` has no extension and nothing to discriminate; its `json`
identifier dispatch already exists. `liquers-py`'s `Value` is `PY-VALUE-SERIALIZER-IS-A-STUB`.
Making `html` readable for `RecordView`, adding formats, and migrating identifiers older stores
wrote are out of scope. Identifiers neither half declares keep today's base-first order; the asset
layer already degrades them before deserialization (`liquers-core/src/assets.rs`
`deserialize_stored_value`).

**Systems touched.** `liquers-lib` only: `CombinedValue::deserialize_from_bytes`
(`liquers-lib/src/value/extended.rs`), and the comments in `SimpleValue::deserialize_from_bytes`
that defer to this issue. No `liquers-core` change, no command, no query syntax, no stored format.
The issue's `core/value` area is not touched.

**Documentation intent.**
- Reference: `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading gains the dispatch rule (which half
  reads which identifier, and what an unknown or empty identifier does). History row and
  `reviewed:` bump.
- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md` §Adding a value type step 4 says that the `TypeInfo`
  also routes reading: an extension identifier missing from `type_descriptions()` is offered to the
  base first.
- No new documents. `specs/README.md` links this design in place of the issue.

### Scope Changes

- 2026-10-09, Phase 3: **AC-6** added. Phase 2 resolved Q4 by generalising the `polars.DataFrame`
  error choice (rule 3); that rule is observable, so it gets a scenario and a test. No scope added
  beyond Phase 2.

### Design Readiness

Pre-approved after Phase 2 on 2026-10-09 (`proceed all`).

- **Readiness:** `ready` — Phases 1-4 present and reviewed; no blocking or open design question.
- **Automatic fixing:** eligible by the rule (M, a bug fix restoring the declared identifier's
  meaning, one private helper, no `pub` change, one crate), assessed after the decisions were made.
- **Decision log** (`proceed all`):
  - *Blocking:* none.
  - *Needs decision:* none.
  - *Proposed resolution, taken:* AC-6 added for Phase 2 rule 3 (above). The AC-4 test also
    asserts the base and extension identifier lists are disjoint (Phase 2 review finding).
  - *Implementation detail:* the test extensions in `extended.rs` gain one that declares its
    identifier; `RefusingExtension` and `ScalarExtension` keep declaring nothing, so the tests that
    use them keep pinning the undeclared path.

### Design Dependencies

Overlap triage (`references/overlap.md`), all weak; none is merged into this design:

- **requires (landed)** `simple-value-untyped-and-scalar-reads` (PR #91, merged): set the current
  fallback (an untyped file neither half reads is `Bytes`), which AC-5 preserves. Same function
  (T3), but in implementation and merged (E3).
- **overlaps** `simple-value-serializer-parity` (in documentation): deferred the `txt`/`html`/`toml`
  "any identifier" rule to this issue.
- **overlaps** `DATA-FORMAT-CONSTANTS-AND-TOOLING`: the format side of the same serializers.
- `value-type-system` and `text-value-markdown-format` (complete) supplied `type_descriptions()`
  and the evidence; frozen (E4).

### Open Questions

None open. Decided at the Phase 1 gate (2026-10-09) on the recommendation: (1) the dispatch lives in
`CombinedValue`, `SimpleValue`'s arms stay; (2) an extension's refusal of its own identifier is
final; (3) `type_descriptions()` is called per read, no new trait method; (4) the `polars.DataFrame`
special case is generalised (Phase 2 rule 3, AC-6).

## Phase 2: Architecture

### Solution

`CombinedValue::deserialize_from_bytes` routes by the declared type identifier before it tries
anything:

1. **The extension declares it** (non-empty, in `<E as ValueExtension>::type_descriptions()`): ask
   `E` only. Its value is `CombinedValue::Extended`, its error is returned unchanged.
2. **Otherwise** (empty, base-declared, or declared by nobody): today's chain, unchanged in what it
   accepts. `B` first; then `E`; then, for an empty identifier only, the bytes as a base `Bytes`.
3. **Error choice when both refuse a named identifier:** the base's error if
   `<B as ValueInterface>::type_descriptions()` declares the identifier, the extension's otherwise.
   This replaces the hard-coded `type_identifier == "polars.DataFrame"` branch: with `polars` on that
   identifier never reaches here (rule 1), and with it off the extension's "unsupported type
   identifier" is the more accurate message, which is what the special case existed to give.

The identifier spaces are meant to be disjoint: `CombinedValue::type_descriptions` concatenates
both lists, and `TypeRegistry::from_value_type` (`liquers-core/src/type_system.rs`) reports a
duplicate on stderr and keeps the first (the base's). The rule does not enforce it: an identifier
declared by both would be read by the extension. That is a bug in the two lists, not a case to
design for; the Phase 3 test for AC-4 asserts the lists are disjoint for `Value`, so it cannot
happen silently.

**Rejected.** *Strict base arms* (every `SimpleValue` arm refuses identifiers it does not own): fixes
one base type only, spreads the rule over every arm, and makes `liquers-web`'s combinations rely on
the same discipline (Phase 1 Q1). *Extension first for every identifier*: an extension may infer a
type from the format for `""` (`ScalarExtension` in the tests), so asking it first would change
untyped reads (AC-5). *A cached or overridable `declares_identifier` hook on `ValueExtension`*: a new
trait method for a saving nobody has measured (Q3). *Consulting the asset layer's `TypeRegistry`*:
`DefaultValueSerializer::deserialize_from_bytes` is a static function with no registry argument, and
the cache path (`liquers-core/src/cache.rs`) calls it without one.

**Known-issue preflight** (open items in `lib/value` / `core/value` and the integration points):

| Issue | Relevance | Fix first? | Blocks? |
|---|---|---|---|
| `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` (P3) | `RecordView` declares `html` but cannot read it. With AC-3 a stored `html` view now fails to load and is recomputed instead of loading as `Text`, which is that issue's documented consequence | no | no |
| `PY-VALUE-SERIALIZER-IS-A-STUB` (P2) | `liquers-py`'s `Value` is not a `CombinedValue`; untouched | no | no |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (P2, L) | Format vocabulary of the same serializers; independent | no | no |
| `TYPE-REGISTRY-NOT-REALM-AWARE` (P2, L) | Identifiers are compared in the default realm only, as everywhere else in `type_descriptions` | no | no |

No blocker. **Command namespaces involved:** none.

### Changes

**`liquers-lib/src/value/extended.rs`** — one private helper and the body of one existing method.
No `pub` item changes.

```rust
/// Whether `descriptions` declare `type_identifier`. The empty identifier is never declared.
fn declares(descriptions: &[liquers_core::type_system::TypeInfo], type_identifier: &str) -> bool;
```

`CombinedValue::deserialize_from_bytes` keeps its signature; its body applies rules 1-3.

The `type_descriptions` calls are fully qualified: `ValueExtension` and `ValueInterface` both
define one, and `B` and `E` are bounded by different traits. The base's list is built only on the
double-refusal path.

**`liquers-lib/src/value/simple.rs`** — comments only. The `txt` / `html` / `toml` arm's comment
says the lenient read is kept for identifiers no half declares, and that extension identifiers no
longer reach it through `CombinedValue`. The `md` arm's comment loses "so `CombinedValue` asks the
extension" as the reason (the `md` refusal is kept: it is correct for `SimpleValue` used alone).

**Errors:** existing constructors only; no message text changes except that the double-refusal path
may now return the extension's message (rule 3). **Sync/async:** synchronous, CPU-only, as today.
**Commands:** none; `specs/command_registry.yaml` does not change. **Features:** no new `cfg`.
Every identifier `ExtValue::type_descriptions()` declares under a feature has its read arm under the
same feature (`RecordView`, `RecordSource` under `records`; `polars.DataFrame` under `polars`).
`UIElement`, `egui.Command` and `egui.Widget` are declared with no read arm, so they now fail in the
extension instead of in the base; they have no byte form and are persisted as metadata only.

**Documents** (`affects_docs`): `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading — a paragraph
stating the three rules. `specs/guides/TYPE_SYSTEM_GUIDE.md` step 4 — a sentence: an extension's
`TypeInfo` also routes reading, so an identifier it omits is offered to the base value first. Both
get a `## History` row and a `reviewed:` bump. `specs/README.md` moves the entry to "built" in
Phase 5.

### Risks

| Category | Risk | Mitigation |
|---|---|---|
| Files | Two files in `liquers-lib/src/value/` | — |
| Tests likely to change | None should; `record_typeinfo.rs` and the `extended.rs` tests already pin the paths kept. A test that relied on an extension identifier reading as a base value would fail, and that is the bug | Run `cargo test -p liquers-lib --lib --tests` and the `records` / no-default feature rows |
| Compatibility | A stored or cached `RecordView` in `html`, or an extension identifier stored in a format only the base could read (an `Image` as `txt`), now fails to load and is recomputed instead of loading as a base value of the wrong type | Intended (AC-3); noted in the reference |
| Performance | One `Vec<TypeInfo>` allocation per read through `Value` | Reads already go through a store and allocate the whole payload; accepted at the gate |
| Feature matrix | A declared identifier without a read arm in some configuration would now error instead of falling back | Checked above; `scripts/check-build-matrix.sh` in Phase 4 |
| Recovery | Revert the one method body | — |
| Certainty | High: every path was read at HEAD; the change is local to one generic function | — |

## Phase 3: Examples and Tests

### Examples

**Primary (AC-1).** The problem example: a 3-row `RecordView` written as `json` and read back
through `Value` with identifier `RecordView` is a `RecordView` of 3 rows.

**Secondary (AC-3).** The same view written as `html` and read back with identifier `RecordView`
is an error whose message comes from the `RecordView` reader; at HEAD it is `Text`.

**Edge and error cases**, matching Phase 2's risks: a declared identifier in a format only the base
reads (`RecordView` as `txt`) refuses (AC-4); the empty identifier still reads `json` as plain JSON
and `csv` as bytes (AC-5); an identifier nobody declares, which both halves refuse, reports the
extension's error, and a base identifier reports the base's (AC-6).

No Liquers query is involved; the tests call `DefaultValueSerializer::deserialize_from_bytes`
directly, as the existing serializer tests do.

### Tests

Unit tests in `liquers-lib/src/value/extended.rs` `mod tests`, over a new test-local
`DeclaringExtension` (declares `test.Declared` in `json`; reads any `json` as itself, refuses every
other format) combined with `SimpleValue`. No feature gate.

| Test | Proves |
|---|---|
| `declared_identifier_is_read_by_the_extension_even_when_the_base_could` | AC-1, AC-4 — `test.Declared` as `json` is `Extended` (the base would read plain JSON) |
| `declared_identifier_keeps_the_extension_refusal` | AC-3, AC-4 — `test.Declared` as `txt` is `Err` from the extension, not `Text` |
| `undeclared_and_base_identifiers_read_as_before` | AC-5 — `""` as `json` and `Text` as `txt` are base values; `""` as `csv` is `Bytes` |
| `undeclared_identifier_refusal_comes_from_the_extension` | AC-6 — `test.Unknown` as `csv` through `RefusingValue` fails with the extension's message |
| `base_identifier_refusal_comes_from_the_base` | AC-6 — `I32` as `csv` through `RefusingValue` fails with the base's message |

The existing `untyped_file_neither_half_reads_is_bytes`, `untyped_file_goes_to_an_inferring_extension_first`
and `named_identifier_neither_half_reads_still_refuses` stay unchanged and also prove AC-5.

Integration tests with the real `Value`:

| File (gate) | Test | Proves |
|---|---|---|
| `liquers-lib/tests/record_typeinfo.rs` (`records`) | `record_view_json_reads_back_as_a_record_view` | AC-1 |
| same | `record_source_manifest_reads_back_as_a_record_source` (`yaml` and `json`) | AC-2 |
| same | `record_view_html_refuses_instead_of_reading_as_text` | AC-3 |
| `liquers-lib/tests/value_type_system.rs` (none) | `extension_identifiers_never_read_as_base_values` — every `ExtValue::type_descriptions()` identifier × each of its formats plus `txt`, `json`, `yaml`, over one JSON-array payload: `Err` or `Extended` with that identifier; and the base and extension identifier lists are disjoint | AC-4 |

Run: `cargo test -p liquers-lib --lib --tests`, then the `--no-default-features` rows for `records`
and none from `CLAUDE.md`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-lib/src/value/extended.rs` — add `declares` and route
  `CombinedValue::deserialize_from_bytes` by it (Phase 2 rules 1-3, dropping the `polars.DataFrame`
  branch); add `DeclaringExtension` and the five unit tests — `cargo test -p liquers-lib --lib value::extended`.
  Rollback: revert the file. Commit `cebdcd8`.
- [x] 2. `liquers-lib/tests/record_typeinfo.rs`, `liquers-lib/tests/value_type_system.rs` — the four
  integration tests — `cargo test -p liquers-lib --test record_typeinfo --test value_type_system`.
  Rollback: revert the two files. Commit `cebdcd8`.
- [x] 3. `liquers-lib/src/value/simple.rs` — the `txt`/`html`/`toml` and `md` comments (no code) —
  `cargo test -p liquers-lib --lib --tests`. Rollback: revert the comments. Commit `cebdcd8`.
- [ ] 4. Feature rows — `cargo test -p liquers-lib --no-default-features --lib --tests` and
  `--no-default-features --features records`; `bash scripts/check-build-matrix.sh` if disk allows,
  otherwise the two rows plus `--features polars`.

### Validation

`cargo test -p liquers-lib --lib --tests` green; the feature rows of step 4 green; no
`specs/command_registry.yaml` change (no command touched). Documents (Phase 5):
`specs/reference/VALUE_TYPE_SYSTEM.md` §Reading and `specs/guides/TYPE_SYSTEM_GUIDE.md` step 4, each
with a History row and `reviewed:` bump; the issue closed with a resolution note; `specs/README.md`
entry moved to built; `python3 scripts/docs_index.py --check`.

## Phase 5: Documentation

**Built versus approved.** As approved in Phases 2-4: one private helper (`declares`) and the body
of `CombinedValue::deserialize_from_bytes` in `liquers-lib/src/value/extended.rs`; comments in
`liquers-lib/src/value/simple.rs`; five unit tests over a new test-local `DeclaringExtension`; four
integration tests in `liquers-lib/tests/record_typeinfo.rs` (`records`) and
`liquers-lib/tests/value_type_system.rs`. No `pub` item, command, feature or stored format changed.
**Added:** AC-6 in Phase 3 (Scope Changes). **Omitted:** nothing.

**Validation.** `cargo test -p liquers-lib --lib --tests` green (385 unit tests and every integration
suite); the `--no-default-features` rows for none, `records` and `polars` per step 4.

**Documents reviewed against the code** (`affects_docs`): `specs/reference/VALUE_TYPE_SYSTEM.md`
§Reading gains the three dispatch rules; `specs/guides/TYPE_SYSTEM_GUIDE.md` §4 says the `TypeInfo`
also routes reading. Each has a History row and `reviewed: 2026-10-09`. `specs/README.md` lists the
work as built.

**Issues.** `COMBINED-VALUE-DISCRIMINATION` closed with a resolution note. No new problem found.
`TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` (open, P3) gained evidence: a `RecordView` stored as
`html` now fails to load instead of loading as `Text`, as AC-3 intends.

**Learning.** A two-step "ask one half, then the other" read is only as precise as the more lenient
half. The identifier already says who owns the bytes; asking the owner first removes the need for
every base arm to remember to refuse.
