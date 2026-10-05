# Phase 2: Solution and Architecture - Skipped Store Writes Are Not Persists

## Chosen Solution

In `liquers-core/src/assets.rs`:

```rust
/// What `AssetRef::save_to_store` did, when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveOutcome {
    /// `store.set` completed.
    Written,
    /// Nothing was written: the asset was cancelled (before or after serialization), or its
    /// metadata says `stored: false`. Recorded as `PersistenceStatus::None`, never `Persisted`.
    Skipped,
}
```

- `async fn save_to_store(&self) -> Result<SaveOutcome, Error>`: the three early
  `return Ok(())` become `return Ok(SaveOutcome::Skipped)`; the successful `store.set` path maps
  `written.map(|()| SaveOutcome::Written)` (keeping the `refresh_listing_version` call on
  success).
- `async fn record_persistence_result(&self, result: Result<SaveOutcome, Error>)`:

  ```rust
  match result {
      Ok(SaveOutcome::Written) => self.set_persistence_status(PersistenceStatus::Persisted, None).await,
      Ok(SaveOutcome::Skipped) => self.set_persistence_status(PersistenceStatus::None, None).await,
      Err(error) => { /* unchanged */ }
  }
  ```

- `persist_with_status_tracking` is unchanged textually (it forwards the result).
- `PersistenceStatus::None` doc comment widened (Phase 1).

A skip keeps its existing `eprintln!` diagnostics (stderr, allowed).

## Rejected Alternatives

- **Return `Err(Error::cancelled…)`** — recorded as `NotPersisted` with an error; a skip is not a
  failure (issue's own note). Rejected.
- **New `PersistenceStatus::Skipped` variant** — public enum change; every exhaustive `match`
  (three `to_override` sites, `set_persistence_status`) would need an arm, and `None` already
  has the needed meaning at the caller. Rejected as larger than the defect.
- **`Result<bool, Error>`** — works, but a named enum documents the two cases and keeps future
  skip reasons expressible.

## Files and Symbols

| Symbol | Change |
|---|---|
| new private `enum SaveOutcome` | near `PersistenceStatus` |
| `AssetRef::save_to_store` | return type and four return sites |
| `AssetRef::record_persistence_result` | parameter type, new arm |
| `PersistenceStatus::None` | doc comment |
| test ≈11743 (`assetref.save_to_store().await?`) | compiles unchanged; may assert `== SaveOutcome::Written` |

## Errors, Ownership, Sync/Async

No new errors. All async paths unchanged; the background `tokio::spawn` closure calls the same
two methods. wasm path identical. No lock held across the new code.

## Compatibility

Private API only. Public observable: `persistence_status()` for skipped writes. `to_override`
sites need no edit: they already route `None` to `persist_with_status_tracking`.

## Questions

- **Implementation detail — enum vs bool:** enum.
- **Implementation detail — testing the post-serialization check:** structural, no hook
  (Phase 1).

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` |
| Affected workflows/crates | keyed persistence; `to_override` recovery |
| Existing-test impact | tests asserting `Persisted` after a cancellation, if any, were asserting the bug — none found; `stored_cached_flags.rs` tests stay green (caller path unchanged) |
| New validation | cancelled-before-save test; `to_override` after cancellation test; unchanged success/failure tests |
| Compatibility/data | fewer spurious metadata-only entries; no format change |
| Concurrency | none added; same checks, same order |
| Security | none |
| Recovery | revert the enum and two signatures |
| Certainty | high |

## Review

Against Phase 1: criteria 1-2 → `record_persistence_result` arms; 3 → existing `to_override`
routing; 4 → no other code touched. Against code: `save_to_store`, `record_persistence_result`,
`persist_with_status_tracking`, `classify_persistence_error`, the three `to_override` matches,
and `AssetData::set_cancelled` (≈1506) were read at HEAD.
