---
id: WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED
kind: feature
title: The records bridge is one-way and a RecordSource has no JavaScript representation
status: draft
priority: P2
complexity: L
area: [web, records]
design: liquers-web-records-bridge
created: 2026-10-17
github:
---

## Problem

The JavaScript records bridge covers only the outbound batch case, and only partially:

- **`ExtValue::RecordSource` is unmapped.** `to_js_custom`
  (`liquers-web/src/default_value.rs:57`) returns `Ok(None)` for it, so the value falls through to
  `try_into_json_value` and the page receives a generic `ConversionError` about JSON — not even a
  message naming `RecordSource`. The guide records the absence ("A `RecordSource` has no JavaScript
  representation yet") and prescribes the traversal contract: iterate `chunks()` (route 1) and
  expose the async iterator (`RECORDS07`/`RECORDS08`); route 2 (blocking) is impossible in Wasm.
- **The bridge is one-way.** `from_js_custom` (`default_value.rs:28-32`) always returns `Ok(None)`.
  A `RecordBatch` handle passed back into a query or command is refused under the structural
  policy, and `opaque()` turns it into an opaque `js.Value`, not an `ExtValue::RecordView`. Nothing
  turns a JavaScript batch back into a record value.
- **Most RECORDS conformance tests have no JavaScript-side disposition.** Only `RECORDS05`/`06`
  exist in `liquers-web/tests/records_RECORDS.rs`. `RECORDS01`, `02`, `04`, `07`, `08` are
  satisfied only by Rust counterparts in `liquers-records`, which do not exercise the JS boundary;
  `RECORDS03` (no Arrow route), `RECORDS09` (JS has an async model) and `RECORDS11` (JS cannot
  implement the traits today) need explicit `NA` entries with reversing conditions, recorded
  nowhere.

## Impact

A page can read a materialized batch but cannot stream a chunked source (the primary case the
record-streams design exists for), cannot feed records back into commands, and the integration
cannot claim the RECORDS sub-feature beyond PARTIAL. The missing `NA` records violate the guide's
"absence is not an `NA` decision".

## Expected behaviour

A design (suggested slug `liquers-web-records-bridge`) answering the guide's five RECORDS
questions for JavaScript: a `RecordSource` wrapper exposing `chunks()` and an async iterator
(`Symbol.asyncIterator`), bounded `materialize(maxRows)`; a decision on the JS→Rust direction
(retain the `Arc` in the handle so an unmodified batch can cross back, or document the refusal);
a clear refusal message naming `RecordSource` until then; JS-side `RECORDS01/02/04/07/08/10`
tests and recorded `NA` dispositions for `03`/`09`/`11` with reversing conditions.

## Discovery

Bidirectional conformance audit of `liquers-web` against `LANGUAGE-INTEGRATION_GUIDE.md`
(2026-10-17). The guide itself already records the `RecordSource` absence as "yet".
