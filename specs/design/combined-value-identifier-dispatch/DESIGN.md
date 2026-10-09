---
id: COMBINED-VALUE-IDENTIFIER-DISPATCH
kind: design
title: CombinedValue reads an identifier the extension declares through the extension
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [lib/value]
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

Decided at the Phase 1 gate (2026-10-09), all on the recommendation:

1. **Resolved — where the dispatch lives.** In `CombinedValue`: an identifier that
   `E::type_descriptions()` declares is read by the extension only; every other identifier keeps
   today's base-first order. The `SimpleValue` arms stay as they are.
2. **Resolved — extension declared, extension fails.** The extension's error is returned; there is
   no fallback to the base (AC-3).
3. **Resolved — cost.** `type_descriptions()` is called per read; no new trait method.
4. **Resolved in Phase 2 — the `polars.DataFrame` special case.** Generalised rather than kept:
   when neither half reads a named identifier, the base's error is returned if the base declares
   the identifier, and the extension's otherwise (Phase 2 §Changes).

No question is open.

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
/// Whether `descriptions` declare `type_identifier`. The empty identifier ("not known") is never
/// declared.
fn declares(descriptions: &[liquers_core::type_system::TypeInfo], type_identifier: &str) -> bool {
    !type_identifier.is_empty()
        && descriptions.iter().any(|info| info.type_identifier == type_identifier)
}

impl<B: ValueInterface + Default, E: ValueExtension> DefaultValueSerializer for CombinedValue<B, E> {
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error>;
    //   1. declares(&<E as ValueExtension>::type_descriptions(), id) → E only
    //   2. B, then E, then (id empty) B::from_bytes
    //   3. both refuse a named id → base_err if declares(&<B as ValueInterface>::type_descriptions(), id),
    //      ext_err otherwise
}
```

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
