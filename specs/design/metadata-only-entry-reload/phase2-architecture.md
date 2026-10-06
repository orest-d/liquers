# Phase 2: Solution and Architecture

## Change

In `AssetData::try_fast_track` (`liquers-core/src/assets.rs`), replace

```rust
let (binary, metadata) = store.get(&key).await?;
```

with

```rust
let (binary, metadata) = match store.get(&key).await {
    Ok(entry) => entry,
    // Metadata without a data object (STORE_SEMANTICS §2): a value with no byte form, stored
    // metadata-only by `set_state`. Nothing to load; the recipe re-derives it.
    Err(e) if e.error_type == ErrorType::KeyNotFound => {
        eprintln!("Asset {} at {}: stored without bytes; recomputing", self.id(), key);
        self.clear_fast_track_payload();
        return Ok(false);
    }
    Err(e) => return Err(e),
};
```

The guard tests one `ErrorType` and falls through to propagation, so it is not a default arm over
an enum of ours. Check the field or accessor name for the error type on `Error` before
implementing.

## Rejected alternatives

- **Metadata marker / new `Status`.** That needs a serialized addition and migration semantics
  for old records, and duplicates the store's answer.
- **Read `get_metadata` first and decide from a flag.** One more round trip, and still needs a
  flag.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES` | Required for the memory store | Land together |
| `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` | Adjacent load-path case | No |

## Relevant commands

None.

## Documentation architecture

`ASSETS.md` fast-track sentence, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` (fast track + tests) |
| Workflows | Reload of any non-serializable keyed value; file-store deployments (fixes a failure) |
| Existing tests | Corrupted-payload tests unchanged (bytes exist). Tests that relied on the memory store's empty bytes for metadata-only entries are updated by the companion design. |
| Compatibility | A file-store `get` that used to fail now recomputes |
| Recovery | Revert the match |
| Certainty | High |
