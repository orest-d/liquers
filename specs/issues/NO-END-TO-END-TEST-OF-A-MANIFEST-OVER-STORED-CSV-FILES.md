---
id: NO-END-TO-END-TEST-OF-A-MANIFEST-OVER-STORED-CSV-FILES
kind: issue
title: No test reads a manifest whose chunks are stored CSV files, the design's motivating case
status: draft
priority: P3
complexity: S
area: [records]
design: manifest-over-stored-csv-test
created: 2026-09-27
github:
---
# No test reads a manifest whose chunks are stored CSV files, the design's motivating case

## Problem

The record-streams design is motivated by a folder of CSV files read as one table through a
manifest: one chunk per file, a `uniform_schema`, and `…/ns-rec/materialize`. The tests cover
manifests whose chunks are *command* results (`ns-fixture/…`), keyed and unkeyed, and
`records_scenario_files_to_csv.rs` lists a folder. No test stores several CSV files, writes a
manifest whose chunks are `-R/<folder>/<file>.csv/-/ns-rec/to_record`, and materializes it. The
guide's CSV-directory walkthrough therefore describes that case in prose and says so.

## Expected behaviour

An integration test in `liquers-lib/tests/` for exactly that shape. It should check the per-file
`uniform_schema` read, that each chunk's key is its file (or the chunk query), and the
materialized row count. The guide's walkthrough would then quote that test.

## Discovery

Found 2026-09-27 while writing `specs/guides/RECORD_STREAM_GUIDE.md` in record-streams Phase 5.
