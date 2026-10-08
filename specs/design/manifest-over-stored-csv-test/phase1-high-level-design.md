# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — test and guide change in `liquers-lib` (already implemented)
- **Leading issue:** None
- **Explanation:** Test-only work, plus a guide update that quotes the test. The shape is already
  specified by `RECORD_STREAM_GUIDE.md` §3.2 ("A directory of CSV files is the same shape…") and
  every piece is individually tested.
- **Open questions:** None

## Problem

The motivating case of record-streams, a folder of CSV files read as one table through a
manifest, has no end-to-end test. Pieces are covered (`to_record_reads_labelled_csv_bytes`,
`manifest_source_reads_an_unkeyed_chunk_by_evaluating_its_query`), and the guide says so in prose.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

A new integration test that:

1. stores `data/raw/jan.csv` (2 rows) and `data/raw/feb.csv` (3 rows) with the same columns;
2. stores `data/raw/all.manifest.yaml` with two explicit chunks
   `-R/data/raw/jan.csv/-/ns-rec/to_record-csv` and `-R/data/raw/feb.csv/-/ns-rec/to_record-csv`,
   and a `uniform_schema`;
3. evaluates `-R/data/raw/all.manifest.yaml/-/ns-rec/materialize` and asserts 5 rows in file order;
4. asserts that each chunk is identified by its query (unkeyed: the chunk queries end in an
   action, not a filename), so no chunk key is written to the store;
5. asserts that a file violating `uniform_schema` (a non-nullable column with an empty cell, in a
   third variant) fails with an error naming the chunk.

The guide's §3.2 paragraph is then rewritten to quote the test.

## Scope

Test and guide. No production code. If the test exposes a defect, file it (§4.8) and mark the
test `#[ignore = "<ISSUE-ID>"]` only if the defect is outside this design's scope. Never silently
skip it.

## Design Dependencies

- `csv-physical-lines-short-rows` — **overlaps**. If implemented first, CSV error messages change.
  The test asserts error type and chunk naming, not CSV message text.

## Documentation assessment

- Guide: `specs/guides/RECORD_STREAM_GUIDE.md` §3.2, "A directory of CSV files" paragraph, quoting
  the test.

## Consolidated Findings

- All three queries were checked with `liquers-validate` (registry at HEAD):
  `-R/data/raw/jan.csv/-/ns-rec/to_record-csv` and
  `-R/data/raw/all.manifest.yaml/-/ns-rec/materialize` validate.
- The test is `records`-gated (`#![cfg(feature = "records")]`), like the other record tests.
