# Phase 5: Documentation - record-streams

## Completion Preconditions

- [x] Implementation is finished and validated — Milestones 0–8, one commit per step
  ([`phase5-evidence.md`](phase5-evidence.md) rows 0.1–8.3)
- [x] All user comments are answered or incorporated — the user's decisions are recorded in
  Phases 2–4
- [x] All review comments are answered or incorporated — the implemented diff was reviewed by three
  reviewers, and every finding was fixed, decided or filed (evidence row "Review")
- [x] Documentation is consistent with the implemented and tested behavior — every document below
  was reviewed against the code at HEAD
- [x] Documentation is included in the implementation branch (`claude/stoic-mayer-y5y1hi`)

## Implementation Summary

Liquers now has a **tabular value family**, which Phase 1 asked for.

- **`liquers-records`** is a new crate that depends on `liquers-core` only, so it builds on
  wasm32. It holds:
  - an Arrow-layout columnar model: 64-byte-aligned buffers, validity bitmaps, and `Column`,
    which is validated on construction;
  - `RecordSchema` with roles and exactly one optional `Id`;
  - three traits: `RecordSource` (asked repeatedly, async), `RecordStream` (one traversal) and
    `RecordView` (a finite table, sync);
  - lazy views, and `RecordBatch`, the materialized view;
  - manifests with explicit and template chunks, keyed by `<name>_{n:04}.<ext>` when the manifest
    is stored as `<name>.manifest.yaml`;
  - `ManifestRecipeProvider`, which serves those chunk keys;
  - the table formats: CSV/TSV, NDJSON and seven JSON orients, Markdown, HTML (write-only), Arrow
    IPC (read and write, validating untrusted input), and Parquet (written here, read through
    polars).
- **`liquers-lib`** adds, behind the `records` feature and on by default:
  - `ExtValue::RecordView` and `RecordSource` with their `TypeInfo`s;
  - the 13 `ns-rec` commands;
  - scalar reading of a single cell;
  - a polars bridge.
- **`liquers-core`** adds:
  - `stored` and `cached` on recipes and metadata, honoured by both asset managers. A
    `cached: false` asset is not reused, but it stays its key's node in the dependency graph;
  - `RecipeProviderChain`;
  - an interpreter rule: a state produced by fetching a key carries that key to the next action.
- **`liquers-web`** adds a `RecordBatch` handle that shares column memory with JavaScript, with
  `columnCopy` as the fallback. The wasm cost is +7.25% on the quickstart build.

Current behaviour: [`RECORD_STREAMS.md`](../../reference/RECORD_STREAMS.md). How to produce
records: [`RECORD_STREAM_GUIDE.md`](../../guides/RECORD_STREAM_GUIDE.md).

**Conformance.** The implementation follows the approved Phase 2 design. Where it differs, Phase 2
itself was corrected, with a changelog row each time:
- every `ns-rec` command is `async fn … context`;
- `schema` is a `String` argument, because `Option<Value>` cannot bind;
- Float inference is a spelling rule, not a canonical round trip;
- the `table` orient carries a `liquers` field property and declares `tz: UTC`;
- a keyless manifest refuses every per-chunk and shared argument.

**Added beyond the plan:**
- Fixes to defects found on the way, some of which predate the project:
  - recipe providers failed on a key the store refused as unsupported, which broke three
    `liquers-web` e2e tests;
  - `SimpleValue` could not read JSON;
  - a `stored: false` write was reported as `Persisted`, so `to_override` then wrote metadata to
    the store.
- The review's fixes (7 blocking), described in the evidence log.

**Omitted, and filed:**
- the Arrow C Data zero-copy export and any `liquers-py` path (`RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`);
- wrapping and filtering sources (`RECORD-SOURCE-WRAPPERS-UNSPECIFIED`);
- incremental serialization (`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`, unchanged);
- serving records over HTTP. `liquers-axum` was never touched, so `axum` is dropped from this
  design's `area`.

## Documentation Delivered

### New Reference Documents
- [`specs/reference/RECORD_STREAMS.md`](../../reference/RECORD_STREAMS.md) — the contracts: the
  traits and conversions, views and their costs, row identity, schema, manifests and keyed chunks,
  memory layout, the Arrow mapping, browser sharing, the format table, the command table,
  serialization, and limits.
  - The "field naming" section states that the `meta.`/`attr.`/`key.` qualification is not
    wired: only an unused `resolve_field` exists.

### New Guide Documents
- [`specs/guides/RECORD_STREAM_GUIDE.md`](../../guides/RECORD_STREAM_GUIDE.md) — a
  "produce records from a new source" workflow: a shape decision table, a command walkthrough, a
  manifest walkthrough, batch size, views as a DataFrame, writing a view or source, handing data to
  polars or JavaScript, pitfalls and testing. Every snippet is taken from a passing test, named
  under the snippet.
  - The pyo3 and zero-copy routes are replaced by the three routes that exist, all of which copy.

### Existing Documents Reviewed or Updated
The authoritative `affects_docs` in `DESIGN.md` names 19 documents: the two new ones, the 11 that
Phases 2 and 4 named, and 6 added by area overlap with the review fixes.

Every existing document was reviewed against the code and changed, with `reviewed: 2026-09-27` and
a `phase-5` History row:
- `VALUE_TYPE_SYSTEM`, `TYPE_SYSTEM_GUIDE` (a gated-variant worked example),
  `COMMAND_REGISTRATION_GUIDE`, `LANGUAGE-INTEGRATION_GUIDE`, `ASSETS`, `ASSET_LIFECYCLE`,
  `ASSET_SET_OPERATION`, `DEPENDENCIES_STATUS`, `ENVIRONMENT_CONFIG`,
  `ENVIRONMENT_CONSTRUCTION_GUIDE`, `PROJECT_OVERVIEW`, `REGISTER_COMMAND_FSD`, and `api/DOC_03`,
  `DOC_04`, `DOC_08`.

Claims found wrong on review and corrected:
- a nonexistent `ChunkKeys` type;
- `set()` for `set_binary()`;
- a completeness test's reach (now filed);
- persistence of `store_to`-keyed assets;
- a malformed History table.

`CLAUDE.md` gained the crate, its dependency flow, its test loop and its features (Step 8.2).

**Discarded candidates:**
- `STORE_SEMANTICS`, `STORE_IMPLEMENTATION_GUIDE`, `COMMAND_DECLARATION` and
  `WEB_API_SPECIFICATION`: no store trait, JavaScript declaration or HTTP route changed;
- `POLARS_COMMAND_LIBRARY`: the bridge is not a polars command;
- `DOC_01`, `PAYLOAD_GUIDE` and `UNITTEST_GUIDE`: not affected.

### Links and Capability Map
- `specs/README.md`: "Record streams" moved from designing to built. It now links the reference,
  the guide and the design.
- Search's entry now notes its prerequisite is built, and the narrative's "being stabilized first"
  is past tense.
- `NO-RECORD-STREAM-ABSTRACTION` links the reference and guide from its Resolution.

## Issues Filed

**Closed by this work:**
- `NO-RECORD-STREAM-ABSTRACTION`
- `RECORD-SELECTION-IS-EAGER-NOT-A-VIEW`
- `NO-RECIPE-PROVIDER-CHAIN`
- `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`
- `EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS`
- `SIMPLE-VALUE-CANNOT-READ-JSON`
- `RECIPE-PROVIDERS-FAIL-ON-A-KEY-THE-STORE-DOES-NOT-SUPPORT`

**Left open, each with a dated line saying why:**
- `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`: the manifest provider's case is resolved, but
  the trait default is not;
- `NO-RELATIONAL-DATABASE-ACCESS-LAYER`, a future consumer;
- the eight issues named in Step 8.3.

**New, with the problem each names:**
- **Records:**
  - `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` (P2)
  - `RECORDS-PARQUET-POLARS-READ-IGNORES-DECLARED-SCHEMA` (P2)
  - `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED`
  - `RECORD-TIMESTAMP-TIME-ZONE-DIFFERS-BY-FORMAT` (a design question)
  - `SCHEMA-LESS-JSON-READS-SORT-COLUMNS-ALPHABETICALLY`
  - `MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT`
  - `CSV-ROW-NUMBERS-COUNT-RECORDS-AND-SHORT-ROWS-READ-AS-NULL`
  - `COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE`
  - `COLUMNMUT-VALIDITY-AND-VARIABLE-LENGTH-SET-ALWAYS-COPY`
  - `RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION`
  - `RECORD-SOURCE-WRAPPERS-UNSPECIFIED`
  - `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT`
  - `REC-ID-PARSES-DATE-AND-TIMESTAMP-IDS-AS-RAW-NUMBERS`
  - `NULL-CELL-READS-AS-THE-TEXT-NONE`
  - `NO-END-TO-END-TEST-OF-A-MANIFEST-OVER-STORED-CSV-FILES`
- **Core and lib:**
  - `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` (P2)
  - `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES` (P2)
  - `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`
  - `REGISTER-COMMAND-OPTION-VALUE-CANNOT-BIND`
  - `METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ`
  - `SIMPLE-VALUE-WRITES-FEWER-FORMATS-THAN-DECLARED`
  - `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`
  - `DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK`
  - `EXT-VALUE-DESCRIPTION-COMPLETENESS-TEST-SAMPLES-TWO-VARIANTS`
- **Found by the first full web run** (these predate this design):
  - `HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE`
  - `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE`
  - `LOCAL-STORAGE-STORE-FAILS-CONFORMANCE-IN-A-BROWSER`
  - `STUBS01-GREP-MISSES-DERIVE-INTERLEAVED-CLASSES`
- `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` gained the `stored: false` trigger, which
  is now fixed.

## Important Learning

- **The review after implementation was worth more than any single milestone.**
  - All tests were green when it began, and it still found 7 blocking defects.
  - Two of them made headline queries of the design silently wrong: keyless manifests, and chunks
    streamed with no chunk index.
  - The tests passed because each asserted too little: one column, one chunk, or command-built
    chunks only.
  - Reviewers verified their findings with probes, including pyarrow and pandas.
  - Every fix was written test-first.
- **Run every test loop in full at least once.** The first full run of the three `liquers-web`
  loops found four defects that predate this design. Some browser tests can run only through the
  `NO_HEADLESS=1` + Playwright route, because the container's chromedriver and Chromium differ.
- **The interpreter's state metadata is the query asset's, not the input's.** Any command that
  needs its input's key relies on the carry rule now in `apply_plan`.
- **A parallel agent in a separate worktree needs its own disk.** Three `target/` directories
  filled the 30 GB allowance once. Remove a finished worktree promptly.

## Conformance and Remaining Work

The requested capability (Phase 1's five requirements) is delivered, except where filed:
- **Interoperable:** through Arrow IPC, Parquet and the polars bridge, all of which copy. The
  zero-copy C Data export is filed.
- **A `Value` variant:** delivered.
- **A polars-free DataFrame that works on wasm:** delivered through views, and measured in
  `liquers-web`.
- **Lazy and chunked:** delivered through sources and manifests. A single chunk is still read and
  written whole; the serialization issues cover that.
- **Provenance and validity per chunk:** delivered through per-chunk `Metadata` and the dependency
  graph.

No part of the approved design is partially done without an issue.

## Validation

| Check | Result |
|---|---|
| core | 1112 passed |
| records, all features | 399 passed |
| records, no features | 360 passed |
| lib | 515 passed, 1 ignored (a fixture generator) |
| build matrix | 32/32 configurations |
| `liquers-axum`, `liquers-py`, `liquers-store` check | clean (warnings only in untouched files) |
| wasm Node loop | 149 passed |
| quickstart build and `check-stubs.sh` | pass |
| browser loop | 26/27 (the failure is filed) |
| e2e | 14/16 (both failures predate this design and are filed) |
| `python3 scripts/docs_index.py --check` | 0 errors; 30 warnings, all about other, older documents |
