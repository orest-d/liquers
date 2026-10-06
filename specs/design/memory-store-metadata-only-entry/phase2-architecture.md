# Phase 2: Solution and Architecture

## Memory store

`liquers-core/src/store.rs`, `AsyncMemoryStore`:

- Change the entry type from `(Arc<[u8]>, Metadata)` to `(Option<Arc<[u8]>>, Metadata)`.
- `get`: `Some(data)` → `Ok((data.to_vec(), metadata))`, `None` → `Err(Error::key_not_found(key))`.
- `get_bytes`: same rule.
- `set`: stores `Some(Arc::from(data))`.
- `set_metadata`: inserts `(None, metadata)` for a new key and updates the metadata only for an
  existing one (as today).
- `contains`, `listdir*`, `remove`, `get_metadata`: unchanged semantics. Check every
  `read_async`/`update_async` closure that destructures the tuple.

## Conformance rule

`liquers-core/src/store_conformance/`: add `sidecar05` ("a metadata-only key has no data object")
next to `sidecar01`–`sidecar04`, gated on the fixture's capability to accept `set_metadata` on a
new key (the existing capability that `sidecar` rules use). Assertions: acceptance 1 and 2 from
Phase 1. A store that refuses metadata-only writes with `KeyNotFound` is consistent and passes
(STORE_SEMANTICS allows it).

## Part G

`liquers-core/src/assets.rs`:

- Delete `no_bytes_by_design`.
- At its two use sites, the store read of bytes already matches on `Ok(bytes)` / `Err`. A
  `KeyNotFound` error means no bytes, so skip (the existing "missing bytes are skipped" branch).
  `Ok(bytes)` with empty bytes is content and is checked.
- Update the tests `missing_bytes_are_skipped` and `empty_data_object_is_checked_not_skipped` so
  their fixtures produce a real metadata-only entry (now possible on the memory store).

## Rejected alternatives

- A metadata marker (`stored_bytes: false`). It duplicates what the store already knows, and
  STORE_SEMANTICS already chose the store-level answer.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | Must land with this (fast-track `?` on `KeyNotFound`) | Ordering constraint, not a blocker |
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | Unrelated router listing | No |

## Relevant commands

None.

## Documentation architecture

`STORE_SEMANTICS.md` §2 (sidecar paragraph): normative sentence and `sidecar05`, with History and
`reviewed:`. `STORE_IMPLEMENTATION_GUIDE.md` §9: counts.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/store.rs`; `store_conformance/{rules,mod}.rs`; `assets.rs`; `liquers-core/tests/store_conformance_CONF.rs` (expected counts) |
| Workflows | Every in-memory deployment (tests, the browser default store if memory-backed) |
| Existing tests | The memory-store metadata-only assertion flips. Part G tests are rewritten. Any test that wrote metadata-only and then read empty bytes. |
| Compatibility | Callers that read a metadata-only memory key now get `KeyNotFound`, as on files |
| Concurrency | No change (same `scc` map operations) |
| Performance | None |
| Recovery | Revert the store change. The rule can stay as an allowed failure. |
| Certainty | High. The other stores' results are discovered by the new rule. |
