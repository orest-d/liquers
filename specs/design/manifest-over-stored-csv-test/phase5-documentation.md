# Phase 5: Documentation - End-to-end test of a manifest over stored CSV files

**Status: executed 2026-10-07; approved 2026-10-08 (maintainer)**, after implementation (Wave 4 step 28 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `guides/RECORD_STREAM_GUIDE.md` | §3.2 "A directory of CSV files": the test's manifest with a `<sub>` source line, what materialize returns, and the two limits the test found. History row, `reviewed: 2026-10-07` |

### Candidates Considered and Discarded

`reference/RECORD_STREAMS.md`, which states behaviour that this test only confirms.

### Issues to Close

`NO-END-TO-END-TEST-OF-A-MANIFEST-OVER-STORED-CSV-FILES`.

## Implementation Summary

The new test file is `liquers-lib/tests/records_manifest_over_csv_files.rs` (`records`-gated).
It stores `jan.csv`, `feb.csv` and `bad.csv` under `data/raw/`, plus two manifests with
`uniform_schema`.

| Test | Phase 3 | Result |
|---|---|---|
| `manifest_over_stored_csv_files_materializes_in_order` | T1, T4 | passes: 5 rows in file order, `month, amount`; the same again on re-evaluation |
| `manifest_csv_chunks_are_unkeyed` | T2 | passes: `data/raw` lists only the five stored files afterwards |
| `manifest_csv_chunk_violating_uniform_schema_fails` | T3 | passes: refused, naming the field and the not-null constraint |
| `manifest_csv_chunk_schema_error_names_the_chunk` | T3 (chunk naming) | `#[ignore = "MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK"]` |
| `manifest_over_hand_placed_csv_files_materializes` | (added) | `#[ignore = "STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ"]` |

Deviations, per Phase 4 containment ("file an issue and `#[ignore]` with its ID"):
- The CSV files are stored with the `RecordView` type identifier, as Liquers writes them. A
  hand-placed CSV cannot be loaded at all. That case is the added, ignored test.
- `bad.csv` has one empty and one numeric `amount`. If every `amount` were empty, the column would
  be inferred as `Text`, and the refusal would be a type mismatch rather than the null check T3
  means to exercise.
- `AssetRef::get` exposes a failed asset as a no-value state, so the error is read from
  `state.value()` (`evaluation_error`).

## Documentation Delivered

As planned above.

## Issues Filed

- [`MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK`](../../issues/MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK.md) (P3)
- `STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ` (P2) was filed by
  `csv-physical-lines-short-rows` and is confirmed here.

## Important Learning

Before this test, the guide's motivating case existed only as prose. Two defects in it surfaced
once it was run end to end, and neither was visible from the unit tests of its pieces.

## Conformance and Remaining Work

Conforms to Phases 1–4, with the deviations contained as Phase 4 prescribes. The remaining work
is the two filed issues; each has an ignored test waiting for its fix.

## Validation

- `cargo test -p liquers-lib --test records_manifest_over_csv_files`: 3 passed, 2 ignored
  (`--include-ignored`: both ignored tests fail as described)
- Queries validated with `liquers-validate`
- `--no-default-features --features records` run with the wave
