---
id: WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH
kind: issue
title: A single-cell RecordView reaches JavaScript as a 1×1 batch instead of a scalar
status: draft
priority: P2
complexity: S
area: [web, records]
design:
created: 2026-10-17
github:
---

## Problem

`LANGUAGE-INTEGRATION_GUIDE.md` §5 VALUE/RECORDS requires that "a single-cell view reaches the
*language* as its own scalar, and a larger view refuses that conversion with an error naming its
shape" (`RECORDS10`). `liquers-lib` implements the scalar path
(`liquers-lib/src/value/mod.rs` `record_view_is_single_cell`, used around line 527), but
`liquers-web`'s bridge pre-empts it: `to_js_custom` in `liquers-web/src/default_value.rs:49-52`
materializes **every** `ExtValue::RecordView` and wraps it in `LiquersRecordBatch`, with no
single-cell check. A query ending in a `cell` view therefore hands JavaScript a 1×1 `RecordBatch`
handle instead of a number or string.

## Impact

JavaScript callers must unwrap a batch (descriptor, typed-array view or `columnCopy`) to read one
scalar, and code written against the guide's contract breaks. `RECORDS10` has no test in
`liquers-web/tests/records_RECORDS.rs`, so the divergence is silent.

## Expected behaviour

`to_js_custom` checks for a single-cell view before materializing and returns the cell as the
corresponding JavaScript scalar, matching `liquers-lib`'s own scalar path; a larger view continues
to cross as a batch. Add a `records10_*` test covering both halves (scalar for 1×1, batch handle
with its shape otherwise).

## Discovery

Bidirectional conformance audit of `liquers-web` against `LANGUAGE-INTEGRATION_GUIDE.md`
(2026-10-17); confirmed by reading `liquers-web/src/default_value.rs` against
`liquers-lib/src/value/mod.rs`.
