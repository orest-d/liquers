---
id: MANIFEST-CHUNK-ERROR-IDENTITY
kind: design
title: A manifest chunk read error names the chunk
form: compact
status: complete
readiness: ready
autofix: not-eligible
area: [records]
issues: [MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK]
created: 2026-10-08
---
# A manifest chunk read error names the chunk

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-08 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

When a chunk of a manifest is refused, say which chunk, so a manifest over a directory of files
does not have to be bisected by hand.

### Problem Example

A manifest over `data/raw/` declares `uniform_schema` with `amount` not null; its third chunk,
`data/raw/bad.csv`, holds one empty `amount`. The query
`-R/data/raw/with_bad.manifest.yaml/-/ns-rec/materialize` fails today with

```text
ManifestSource: chunk field 'amount' holds 1 null(s), but uniform_schema declares it not null
```

It should fail with the same error type and a message that starts with the chunk's identity:

```text
chunk 2 (data/raw/bad.csv): ManifestSource: chunk field 'amount' holds 1 null(s), …
```

### Scope and Acceptance Criteria

- **AC-1** A schema mismatch names a keyed chunk
  - WHEN a traversal reads a keyed chunk whose view violates `uniform_schema` (field count, field
    name/type, or a null in a not-null field)
  - THEN the error message starts with `chunk <global index> (<key>)`, and the error type is
    unchanged
- **AC-2** An unkeyed chunk is named by its query
  - WHEN the refused chunk is unkeyed
  - THEN the message names its encoded query instead of a key
- **AC-3** The rest of the message is preserved
  - WHEN any chunk read fails during a traversal
  - THEN the original message follows the prefix verbatim, so existing assertions on the field
    wording still hold

Out of scope: a single chunk read through `view_from_chunk_value` by `ns-rec/rowid`
(`liquers-lib/src/records/commands.rs`). That caller already knows which chunk it asked for, and
changing the `pub` function's signature would be an interface change.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — the fix is in `liquers-records`, but its proof includes
  un-ignoring a `liquers-lib` integration test, so the fix and its tests are not in one crate
  (rule 6). Re-labeled on 2026-10-08 after review; the change is small and is reviewed as an ordinary
  PR rather than landing unattended.
- **Leading issue:** None
- **Explanation:** The traversal already holds both the global chunk index and the `ChunkId` at the
  point where it calls `read_chunk`; prefixing the message there covers every reader path (stored
  bytes, evaluated key, evaluated query) at once.
- **Open questions:** None

### Design Dependencies

- `overlaps` `ERROR-WITH-KEY-SETS-QUERY-FIELD` (weak: structured error context). This design adds
  only message text and does not touch `Error::with_key` / `with_query`.

## Phase 2: Architecture

### Solution

In `ManifestSource::advance`, map the error of `source.read_chunk(&id, &resolver)` to one whose
`message` is prefixed with `chunk {chunk_index} ({identity}): `, where `identity` is
`key.encode()` for `ChunkId::Key` and `query.encode()` for `ChunkId::Query`. `Error` derefs
mutably to `ErrorPayload`, whose `message` is a public field, so the error type, key, query and
position fields are kept as they are.

Rejected: threading the chunk identity into `check_view_matches_schema` (would change the `pub`
`view_from_chunk_value` signature, rule 4 of auto-fix); setting `Error::with_key` (the key field
means "the key that failed", and a chunk key would overwrite an evaluation's own key).

### Changes

- `liquers-records/src/sources.rs` `ManifestSource::advance`: `.map_err(|e| name_chunk(e,
  chunk_index, &id))` before the `place_chunk` map.
- New private helper `fn name_chunk(error: Error, index: u64, id: &ChunkId) -> Error` in the same
  file, with an explicit two-arm `match` on `ChunkId`.
- No commands, no async change (the helper is sync), no documents beyond the issue resolution.

### Risks

Tests that compare a chunk error message for equality would break; a grep for
`"ManifestSource: chunk"` in `liquers-records/` and `liquers-lib/tests/` finds only `contains`
assertions. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary example. Secondary: an unkeyed template chunk
`-R/data/raw/bad.csv/-/ns-rec/to_record-csv` is reported as `chunk 2
(-R/data/raw/bad.csv/-/ns-rec/to_record-csv): …`.

### Tests

- `manifest_csv_chunk_schema_error_names_the_chunk`
  (`liquers-lib/tests/records_manifest_over_csv_files.rs`, today `#[ignore]`d with this issue's ID):
  remove the `#[ignore]`. Proves AC-1.
- `chunk_error_names_unkeyed_chunk_by_query` (new, `liquers-records/src/sources.rs` tests): a
  manifest with one unkeyed chunk whose resolver returns a view with the wrong field count; asserts
  the message starts with `chunk 0 (` and contains the encoded query and the original
  `uniform_schema declares` wording. Proves AC-2, AC-3.
- `chunk_error_keeps_error_type` (new, same module): the refused chunk's error type equals the
  unprefixed one. Proves AC-1, AC-3.

Command: `cargo test -p liquers-records --all-features --lib --tests` and
`cargo test -p liquers-lib --test records_manifest_over_csv_files`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. (`20a282c`) `liquers-records/src/sources.rs` — add `name_chunk` and apply it in `advance` —
  `cargo check -p liquers-records --all-features`
- [x] 2. (`2af80b3`) `liquers-records/src/sources.rs` tests — add `chunk_error_names_unkeyed_chunk_by_query`
  and `chunk_error_keeps_error_type` — `cargo test -p liquers-records --all-features --lib`
- [x] 3. (`679138b`) `liquers-lib/tests/records_manifest_over_csv_files.rs` — remove the `#[ignore]` on
  `manifest_csv_chunk_schema_error_names_the_chunk` — `cargo test -p liquers-lib --test
  records_manifest_over_csv_files`
- [x] 4. (the commit closing the issue) Issue resolution and `status: closed`; Phase 5 note; `python3 scripts/docs_index.py` and
  `--check` — index check passes

### Validation

`cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-records --lib
--tests` (no formats); the liquers-lib test above. Rollback: revert the single commit.

## Phase 5: Documentation

Implemented 2026-10-08 on `claude/manifest-chunk-error-identity`. A chunk refused while
`ManifestSource` walks its chunks now fails with `chunk <global index> (<key or encoded query>): `
followed by the original message, error type unchanged; `ns-rec/rowid`'s single-chunk read is not
affected. Small maintenance: one sentence in [`reference/RECORD_STREAMS.md`](../../reference/RECORD_STREAMS.md)
(the `uniform_schema` row of the manifest fields table), and the issue's resolution. No new
reference or guide is needed.
