# Phase 1: High-Level Design — liquers-web-records-bridge

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds exported JavaScript classes (`RecordSource`,
  `RecordStream`) and changes the JavaScript binding (rules 3 and 4); complexity `L`.
- **Leading issue:** None
- **Decided 2026-10-17 (maintainer):** there is one environment, the web (client) environment; a
  multi-environment system is not defined yet. Traversal therefore uses the web environment and takes
  no environment argument. `ns-rec` is registered in the web environment.
- **Explanation:** Every part has a working solution over existing public API (`RecordSource::chunks`,
  `stream`, `materialize`, `liquers_records::EnvResolver`, `register_records_commands!`), and the
  tests run under the Node loop. The JS→Rust direction below was accepted as proposed by the maintainer on 2026-10-18.
- **Open questions:** None. Decided:
  1. **JS→Rust direction (accepted 2026-10-18):** an unmodified `RecordBatch` or `RecordSource`
     handle crosses back as the same `Arc` (`ExtValue::RecordView` / `RecordSource`), found through a
     handle registry keyed by a marker property. A JavaScript-built table is **not** converted by the
     bridge: it crosses structurally and `ns-rec/from_json` makes it a record. A freed handle is
     refused with a `ConversionError` naming it.

Resolved: `ns-rec` is registered in the web environment whenever the `records` feature is on
(maintainer, 2026-10-17); Step 2 records the `.wasm` size before and after for information.

## Purpose

A page can read a materialized batch, but cannot stream a chunked `RecordSource` (the case
`record-streams` exists for), cannot hand records back to a command, and cannot produce a record
from a query because the web environment registers no `ns-rec` commands. The `VALUE`/RECORDS
sub-feature is PARTIAL in the 2026-10-17 audit (`specs/archive/2026-10-17-liquers-web-guide-conformance.md`).

## Problem Example

```js
await liquers.init();
const src = await liquers.evaluate("-R/data/sales.manifest/-/ns-rec/to_record_source");
```

Today: `ns-rec/to_record_source` is not a registered command in the browser (`builtins.rs` registers
only `register_core_commands!`). Even when a Rust command returns `ExtValue::RecordSource`,
`default_value.rs` `to_js_custom` returns `Ok(None)` and the standard mapping fails with a generic
JSON `ConversionError` that does not name `RecordSource`.

Should be:

```js
for await (const batch of src.stream()) { total += batch.numRows; batch.free(); }
src.chunks();                        // [{ kind: "key", query: "-R/data/sales/day_0001.csv" }, …]
const all = await src.materialize(10000);  // RecordBatch, or an error past 10 000 rows
// a JavaScript command that returns a RecordBatch it received hands Rust back the same batch
```

## Scope and Acceptance Criteria

- **AC-1** A source reaches JavaScript as a `RecordSource`
  - WHEN a query's value is an `ExtValue::RecordSource`
  - THEN the page receives a `RecordSource` handle with `chunks()`, `stream()`,
    `materialize(maxRows)`, `schemaJson()`, `manifestJson()`, `bounded` and `free()`
- **AC-2** The async iterator yields the source's rows (`RECORDS08`)
  - WHEN a page runs `for await (const b of src.stream())`
  - THEN it receives one `RecordBatch` per view, in order, with the same rows as `materialize`
- **AC-3** Chunk ids are data (`RECORDS07`, route 1)
  - WHEN a page calls `src.chunks()` on a manifest-backed source
  - THEN it receives every chunk's kind and an evaluable query, without evaluating any chunk; and
    evaluating them one at a time yields the stream's rows
- **AC-4** Bounded materialization
  - WHEN `src.materialize(n)` is called on a source of more than `n` rows
  - THEN the promise rejects with the `max_rows` error, and with at most `n` rows it resolves to a
    `RecordBatch`
- **AC-5** Handles cross back unchanged (`RECORDS01`)
  - WHEN a JavaScript command returns a live `RecordBatch` or `RecordSource` handle (for example the
    record state it was given)
  - THEN Rust receives the same `Arc` as `ExtValue::RecordView` / `RecordSource`, with schema, field
    roles, `chunk_id`, `rows` and `sources` intact
- **AC-6** Freed handles are refused clearly
  - WHEN a freed handle is returned or passed
  - THEN conversion fails with a `ConversionError` naming the freed `RecordBatch`/`RecordSource`
- **AC-7** Record commands run in the browser
  - WHEN a page registers a JavaScript command `rows` returning an array of row objects and
    evaluates `rows/ns-rec/from_json-records/nrows`
  - THEN it gets the row count (the query validates with `liquers-validate --command rows`)
- **AC-8** Columns and buffers are tested at the JS boundary (`RECORDS02`, `RECORDS04`)
  - WHEN a column is read through `column(i)`'s typed-array view and through `columnCopy(i)`
  - THEN both give the same values, and the copy is detached so writing to it leaves the batch
    unchanged
- **AC-9** Every RECORDS ID has a disposition
  - WHEN the `## Conformance` section of `liquers-web/README.md` is read
  - THEN `RECORDS01`, `02`, `04`-`08`, `10` name their tests, and `03`, `09`, `11` are `NA` with
    their reversing conditions

Non-goals: an Arrow route (`RECORDS03`); JavaScript-implemented `RecordView`/`RecordSource`
(`RECORDS11`); route 2 (blocking), impossible in Wasm; converting a JavaScript-built table inside
the bridge; the single-cell scalar (`web-records-single-cell-scalar`).

## Core Interactions

Value bindings (`ExtValue::RecordView`/`RecordSource` across `JsExtensionBridge`); commands (`ns-rec`
in the web environment); evaluation (`EnvResolver` over the web environment's `EnvRef`).

## Crate Placement

`liquers-web` only; everything used is public in `liquers-lib` (re-exporting `liquers-records`).

## Documentation Intent

- Reference: none.
- Guide: extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §RECORDS "What `liquers-web` does".
- Other documents: `liquers-web/README.md` (usage and the RECORDS dispositions).
- Documents to update: the two above; `liquers-web/src/typescript.rs` declarations.

## Design Dependencies

- `web-records-single-cell-scalar` — **requires**: lands first; this design edits the same
  `to_js_custom` arm and builds on its `records10_*` tests. Kept separate (overlap E1).
- `web-store-conformance-tests` — **overlaps** (weak): both write `NA` rows into the `## Conformance`
  section of `liquers-web/README.md` (seeded from the audit's §1 matrix); whichever lands first
  creates the section.
- `web-outbound-int64-precision` — **overlaps** (weak): an `Int` column copy and an `Int` cell leave
  through paths that design pins; no shared change site.

## Consolidated Findings

- No page query yields a record value today (no `ns-rec` in the browser), so AC-7 precedes every
  page-level example; its `.wasm` size cost is the one measurable risk.
- `to_js_custom(&self)` has no environment; chunks are evaluated in the web environment, resolved
  at each call (`with_global()`), so a rebuild between calls is harmless.
- A wasm-bindgen handle cannot be borrowed back from a `JsValue` without consuming it; a `u32 → Arc`
  registry filled in `From` and emptied in an unconditional `Drop` is the safe route (decision 1).
- `BoxRecordStream` must not be borrowed across an `await`: `nextBatch` takes it out of its
  `RefCell` and puts it back; a concurrent call is refused.
- Order: refusal message → `ns-rec` → registry → handles → way back → records. Step 1 alone meets
  the issue's "clear refusal meanwhile".
