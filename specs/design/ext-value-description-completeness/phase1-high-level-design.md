# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Test-only work. The design adds a compile-time guard (an exhaustive match that
  must name every `ExtValue` variant) so the test cannot fall behind the enum again.
- **Open questions:** None

## Problem

`ext_value_type_descriptions_complete` (`liquers-lib/tests/value_type_system.rs`) is the check
`TYPE_SYSTEM_GUIDE.md` names for step 4 ("add a `TypeInfo`"). It samples only `Image` and, under
`polars`, `PolarsDataFrame`. `UIElement`, `UiCommand`/`Widget` (egui), `RecordView`/`RecordSource`
(records) are never sampled, so deleting their `TypeInfo` leaves the test green.

## Expected behaviour and acceptance

1. The test samples every `ExtValue` variant compiled into the build, except `Foreign`. Its
   identifier belongs to the integration crate and is deliberately absent from the static list
   (CLAUDE.md, "Adding a Value Type"). The exclusion is written in the test with that reason.
2. A helper with an exhaustive `match` over `ExtValue` (no default arm, feature-gated arms) is
   called on every sample, so adding a variant without extending the test is a compile error in
   the test.
3. The test also asserts the reverse direction, using the samples' identifiers. Every
   `type_descriptions()` identifier is the identifier of some sampled variant, which catches a
   stale description left after a variant is removed.
4. It passes in every feature configuration (`check-build-matrix.sh` builds tests).

## Scope

The test and the guide sentence. No production code.

## Design Dependencies

None. `record_typeinfo.rs` keeps its record-specific checks.

## Documentation assessment

- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md`. The sentence naming the test stays true. Add "the
  test fails to compile when a variant is added without a sample".

## Consolidated Findings

- Constructing samples: `UIElement` needs an `Arc<dyn UIElement>`, so reuse a test or simple
  element from `liquers-lib/src/ui` (look for a minimal element used in UI tests). `Widget` needs
  `Arc<Mutex<dyn WidgetValue>>`, so reuse one from egui tests. `RecordView`: a 0-row
  `RecordBatch`. `RecordSource`: reuse the constructor `record_typeinfo.rs` uses. If a variant has
  no cheap constructor, the exhaustive match still forces a deliberate decision. The test may then
  skip the sample with a comment (`ExtValue::X { .. } => { /* not constructible in tests: … */ }`),
  and the guard still holds.
