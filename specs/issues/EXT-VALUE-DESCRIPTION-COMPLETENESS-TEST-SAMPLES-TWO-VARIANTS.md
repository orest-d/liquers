---
id: EXT-VALUE-DESCRIPTION-COMPLETENESS-TEST-SAMPLES-TWO-VARIANTS
kind: issue
title: The ExtValue description-completeness test samples only two of the variants
status: closed
priority: P3
complexity: S
area: [lib/value]
design: ext-value-description-completeness
created: 2026-09-27
github:
---
## Problem

`ext_value_type_descriptions_complete` (`liquers-lib/tests/value_type_system.rs:60`) is described in
`specs/guides/TYPE_SYSTEM_GUIDE.md` as "the check for step 4" — it should fail when an `ExtValue`
variant has no `TypeInfo`. It only checks the variants it builds a sample of: `Image`, and
`polars.DataFrame` under `polars`. `UIElement`, `egui.Command`, `egui.Widget`, `RecordView` and
`RecordSource` are never sampled, so removing any of their `TypeInfo`s leaves the test green.

The record variants are covered separately by `liquers-lib/tests/record_typeinfo.rs`; the others
have no equivalent that asserts their description exists.

## Impact

A new variant added by following the guide gets no protection from the test the guide names: its
missing `TypeInfo` surfaces only when a value of that type is first written, as "Type identifier
'X' is not registered in this build." Low severity — the failure is loud at run time and each
variant can add its own assertion — but the test's name promises more than it checks.

## Expected behaviour

The completeness test samples every variant enabled in the build (with its `#[cfg]` gate), so a
variant whose `TypeInfo` is missing fails it. Alternatively, since the samples must be constructed
by hand, the test asserts one-to-one agreement between `identifier()` over a sample of every variant
and `type_descriptions()`, which would also catch a stale description.

## Discovery

Found on 2026-09-27 while updating `TYPE_SYSTEM_GUIDE.md` for the `record-streams` Phase 5
documentation review: the guide's worked example for the gated record variants needed to name the
test that pins their descriptions, and it was not this one.

## Resolution (2026-10-07)

Fixed by design `ext-value-description-completeness`. `liquers-lib/tests/value_type_system.rs`
samples every `ExtValue` variant compiled into the build except `Foreign` (integration-owned,
deliberately absent from the static list): `Image`, `UIElement`, `polars.DataFrame`,
`egui.Command`, `egui.Widget`, `RecordView`, `RecordSource`. A helper, `statically_described`,
matches every variant with no default arm and is called on each sample, so a new variant fails to
compile until it is sampled. The new `ext_value_type_descriptions_have_no_stale_entries` checks
the reverse direction.

Evidence: renaming `RecordView`'s `TypeInfo` identifier made both tests fail (checked by hand and
reverted); both pass in every feature configuration of `scripts/check-build-matrix.sh` and the
per-feature `cargo test` runs in `CLAUDE.md`.
