# Phase 5: Documentation

**Status: executed and approved 2026-10-08** (maintainer approval in the backlog compaction
follow-up). This design follows the four-phase bulk-design contract; this record was added at the
maintainer's request.

## Summary

Implemented in commit `8c018e7` (2026-10-06): `try_fast_track` (`liquers-core/src/assets.rs`)
treats a `KeyNotFound` from `store.get` as a metadata-only entry — clears the payload and falls
through to evaluation — instead of propagating it or reporting the entry as corrupted. The memory
store half was completed by `design/memory-store-metadata-only-entry/` (`b10774e`), after which every
in-tree store answers a metadata-only key with `KeyNotFound`.

## Conformance and deviations

As designed. The design rejected a metadata marker or a new status; none was added. Tests (Phase 3
T1, T2, T4 and acceptance 3) live in `liquers-core/tests/metadata_only_entry_reload.rs`:
`metadata_only_entry_on_file_store_is_recomputed`, `metadata_only_entry_on_memory_store_is_recomputed`,
`metadata_only_entry_without_recipe_answers_like_an_absent_key`.

## Documentation

- `reference/ASSET_LIFECYCLE.md` §Reusing a stored asset: the metadata-only rule (Phase 4 step 5's
  planned sentence, which had not been written). History row.
- `reference/ASSETS.md` §Content changed outside Liquers: corrected a sentence that still said the
  memory store answers a metadata-only entry with empty bytes. History row.

## Issues

`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` closed 2026-10-08 with a resolution. None filed.

## Validation

`cargo test -p liquers-core --test metadata_only_entry_reload` → 3 passed (2026-10-08);
`python3 scripts/docs_index.py --check`.
