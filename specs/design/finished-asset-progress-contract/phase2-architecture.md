# Phase 2: Solution and Architecture

## Ordering (`liquers-core/src/assets.rs`)

- `run_with_future` (native):

  ```rust
  self.service_sender().await.send(AssetServiceMessage::JobFinishing).ok();
  let psm_result = psm.await;
  self.finalize_primary_progress().await;
  self.finish_run_with_result(result, psm_result).await
  ```

- `run_inline_with_future` (inline/wasm): remove the call from `eval_side` and call it after
  `futures::join!(eval_side, psm_side)`, before `finish_run_with_result`.

## Finalization

```rust
/// Leaves a finished run's progress drawable: started progress becomes done, unstarted progress
/// stays absent, secondary progress is cleared. See ASSETS.md "Progress after completion".
async fn finalize_primary_progress(&self) {
    {
        let mut lock = self.data.write().await;
        let last = lock.metadata.primary_progress();
        let terminal = finished_progress(&last);
        lock.metadata.remove_progress();
        if let Some(entry) = terminal {
            lock.metadata.set_primary_progress(&entry);
        }
    }
    let _ = self.save_metadata_to_store().await;
}

/// The primary progress a finished run leaves: `None` when progress never started.
fn finished_progress(last: &ProgressEntry) -> Option<ProgressEntry> {
    if last.is_off() {
        None
    } else if last.is_done() {
        Some(last.clone())
    } else {
        Some(ProgressEntry::done(last.message.clone()))
    }
}
```

The function does not depend on the final status, which is decided later in
`finish_run_with_result`. The decision makes "finished" alone sufficient, so the outcome is not
needed. Check `ProgressEntry`'s `message` field visibility (it is set by `with_message`). If it is
private, add a `message()` accessor. Check also that `Metadata` (the enum) exposes
`primary_progress` / `remove_progress` / `set_primary_progress`. `metadata.rs` has both the
`MetadataRecord` methods and the `Metadata` wrappers (~2539, ~2580).

The cancel handler in the service loop keeps writing `done("Cancelled")`. The decision allows a
final done when it simplifies the flow. Finalization then sees `is_done()` and keeps it.

## Rejected alternatives

- Finalize inside the loop's `JobFinishing` arm. The loop has three exits, so the call would be
  needed three times.
- A status-dependent entry ("Failed" for `Error`). The decision treats every finish alike, and
  the status already carries the outcome.

## Known-issue preflight

None blocking.

## Relevant commands

None. Commands keep calling `context.progress(..)`.

## Documentation architecture

`ASSETS.md` "Progress after completion" (table + "status is authoritative"), with a History row and
a `reviewed:` bump.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs`; `liquers-core/src/interpreter.rs` (test may re-assert `is_done()`); `ASSETS.md` |
| Workflows | Every evaluation; UI progress display |
| Existing tests | Tests asserting `primary_progress().is_off()` after a run that reported progress change. Search `primary_progress()` in tests first. |
| Compatibility | Stored metadata of finished assets may carry one `done` entry. Additive, same serde shape. |
| Concurrency | Removes a race and adds none |
| Performance | One metadata store write per keyed run |
| Recovery | Revert both changes together |
| Certainty | High |
