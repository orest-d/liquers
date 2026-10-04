# Phase 3: Examples and Tests - Skipped Store Writes Are Not Persists

## Example

A keyed asset `gate/cancelled.txt` holds a binary; it is cancelled; persistence runs.
Before: `persistence_status() == Persisted`, store empty. After: `None`, store empty.

## Tests to Add (`liquers-core/src/assets.rs`, `mod tests`)

Fixture pattern: the existing test near ≈11725 (`SimpleEnvironment<Value>`,
`AsyncMemoryStore::new(&Key::new())`, `AssetData::new(id, key.into(), Some(key), envref)`,
`d.binary = Some(Arc::new(..))`, `d.to_ref()`).

| Test | Steps | Asserts | Criterion |
|---|---|---|---|
| `cancelled_asset_save_is_recorded_as_no_attempt` | build `AssetData` with binary and a `Ready`-like status, `d.set_cancelled(true)`, `d.save_in_background = false`; `assetref.persist_with_status_tracking(false, false).await` (passing `cancelled = false` so the check *inside* `save_to_store` is what fires) | `persistence_status() == PersistenceStatus::None`; `!store.contains(&key)`; `last_persistence_error` is `None` | 1 |
| `save_to_store_reports_skipped_when_cancelled` | as above, call `save_to_store()` directly | `Ok(SaveOutcome::Skipped)` | 1 |
| `save_to_store_reports_written_on_success` | ≈11743's fixture, uncancelled | `Ok(SaveOutcome::Written)` and store contains the bytes; via `persist_with_status_tracking` the status is `Persisted` | 2 |
| `stored_false_metadata_snapshot_is_skipped` | metadata with `stored = false` placed only in the binary's metadata snapshot (set `lock.metadata` stored false and call `save_to_store` directly) | `Ok(SaveOutcome::Skipped)`, store empty | 1 |
| `to_override_after_cancelled_save_writes_no_metadata_only_entry` | default manager: register the cancelled asset as in the `stored_false_survives_to_override_default` test (`liquers-core/tests/stored_cached_flags.rs`), run persistence, call `to_override(key)` | store has no metadata-only entry for the key (`store.contains` false or entry has data) | 3 |

Existing tests to keep green: `stored_false_survives_to_override_{default,immediate}`
(`liquers-core/tests/stored_cached_flags.rs`), every `persistence_status` assertion in
`assets.rs` (e.g. ≈10814 `NotPersisted`, ≈10901 `Persisted`).

## Edge Cases

- Error path unchanged: a store refusing the key still yields `NotPersisted` with the error.
- Post-serialization cancellation: covered structurally (same return value), see Phase 1.

## Coverage Review

Criteria 1-3 covered; criterion 4 by the existing status/log assertions remaining green.
