# Phase 2: Solution and Architecture

## Change

1. Add to `impl<E: Environment> AssetRef<E>` (`liquers-core/src/assets.rs`):

   ```rust
   /// This asset's subject in messages: key, else query, never the runtime id.
   pub(crate) async fn expiry_subject(&self) -> String {
       self.data.read().await.expiry_subject()
   }
   ```

2. In `DefaultAssetManager::wait_for_dependency`:
   - `Status::Error | Status::Cancelled`, no stored error: change the message to
     `format!("Dependency {} did not produce a value (status {:?})", dependency.expiry_subject().await, status)`.
   - `Status::Expired`, evicted: change it to
     `format!("Dependency {} expired and was evicted before its value could be used", dependency.expiry_subject().await)`.

   No lock is held at either site when it is called (only `status()` / `poll_state_any_status()`
   were awaited and released).

3. Optionally simplify `note_expired_dependency` to use the wrapper for its fallback. This is
   behaviour-neutral. Leave it unless the diff stays trivial.

## Errors

The type is still `Error::general_error`. Callers that match on `ErrorType::General` are
unaffected.

## Known-issue preflight

None.

## Relevant commands

None.

## Documentation architecture

None beyond the issue resolution.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` |
| Existing tests | Substring test on "expired and was evicted…" still matches. Search for `"Dependency asset "` in tests: none outside `assets.rs`. |
| New validation | One unit test per message (Phase 3) |
| Compatibility | The message text changes in persisted logs. Nothing parses it. |
| Concurrency | A read lock on the dependency only, with no parent lock held |
| Recovery | Revert |
| Certainty | High |
