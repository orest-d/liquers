---
id: SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT
kind: issue
title: set_state logs a spurious media-type warning for a refined data format such as csv:comma
status: closed
priority: P3
complexity: S
area: [core/assets]
design: soft-consistency-refined-format
created: 2026-10-26
github:
---

## Problem

`add_soft_consistency_warnings_enum` and `add_soft_consistency_warnings`
(`liquers-core/src/assets.rs`) compute the expected media type as
`file_extension_to_media_type(&effective_data_format)` without stripping the refinement. For
`csv:comma` that lookup misses and returns `application/octet-stream`. The enum form then compares
it with `Metadata::get_media_type()`, which *does* strip the refinement
(`MetadataRecord::effective_media_type` → `base_data_format`) and answers `text/csv`, so the two
differ and a warning is logged.

## Problem Example

`AssetManager::set_state` on `DefaultAssetManager` with a state whose metadata declares
`data_format: Some("csv:comma")` and no media type. Today the stored log holds
`media_type 'text/csv' differs from expected 'application/octet-stream' for data_format 'csv:comma'`.
Should be: no warning. The record form (`set_binary` with a `MetadataRecord`) shows the same
defect whenever a media type is declared.

## Impact

Noise in the metadata log of every value stored with a refined format; it will become common once
a query can declare one (`design/query-data-format-command/`). No data is wrong.

## Expected behaviour

Both helpers map the *base* format (`base_data_format`, or the `TypeInfo`/vocabulary helper once
`data-format-vocabulary` exports one) before looking up the expected media type; a unit test with
`csv:comma` asserts no warning.

## Discovery

Bulk design of `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT` (2026-10-26), reading the media-type derivation
for a refined format. Found by reading `assets.rs` and `metadata.rs`; not run.

## Resolution

Fixed 2026-10-27 by `design/soft-consistency-refined-format/`. The soft-consistency check now looks up the expected media type of a refined data format by its
base (`txt:utf8` → `txt` → `text/plain`), in both `add_soft_consistency_warnings` and
`add_soft_consistency_warnings_enum`. Tests in `liquers-core/tests/type_consistency.rs`:
`refined_format_is_consistent_through_set_state`,
`refined_format_with_consistent_media_type_is_silent`,
`refined_format_with_divergent_media_type_still_warns`.
