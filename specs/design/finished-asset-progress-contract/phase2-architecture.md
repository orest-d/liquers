# Phase 2: Solution and Architecture

## Mechanism (independent of the decision)

`liquers-core/src/assets.rs`:

- `run_with_future` (native): move `self.finalize_primary_progress().await` from before the
  `JobFinishing` send to after `psm.await`:

  ```rust
  self.service_sender().await.send(AssetServiceMessage::JobFinishing).ok();
  let psm_result = psm.await;
  self.finalize_primary_progress().await;
  self.finish_run_with_result(result, psm_result).await
  ```

- `run_inline_with_future` (inline/wasm): remove the call from `eval_side`, and call it after
  `futures::join!(eval_side, psm_side)` and before `finish_run_with_result`.

## Contract (recommended resolution)

`finalize_primary_progress(&self)` becomes status-aware:

```rust
async fn finalize_primary_progress(&self) {
    let mut lock = self.data.write().await;
    let status = lock.status;
    match terminal_progress(status, lock.metadata.primary_progress()) {
        Some(entry) => { lock.metadata.remove_progress(); lock.metadata.set_primary_progress(&entry); }
        None => { lock.metadata.remove_progress(); }
    }
}

/// The progress a run leaves behind. `None` means "no progress".
fn terminal_progress(status: Status, last: ProgressEntry) -> Option<ProgressEntry>
```

`terminal_progress` matches every `Status` variant explicitly:

| Status | Result |
|---|---|
| `Ready`, `Source`, `Override`, `Volatile`, `Expired`, `Partial` | `Some(last)` if `last.is_done()`, else `Some(ProgressEntry::done("Finished".into()))` |
| `Cancelled` | `Some(last)` (the cancel handler wrote `done("Cancelled")`) |
| `Error` | `None` |
| `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Storing`, `Directory` | `None` (not a finished run; defensive) |

`Partial` is listed with the data-bearing statuses because the evaluation ended with usable data.
A Phase 4 check confirms that the harness can end in `Partial`. If it cannot, `Partial` moves to
the defensive row.

If the alternative decision (always clear) is taken, `terminal_progress` returns `None` for every
variant and the function collapses to today's body. The ordering mechanism stays.

### Ordering caveat

`finalize_primary_progress` runs before `finish_run_with_result`, which sets the final status. So
`lock.status` at finalize time may still be `Processing`. The contract therefore needs the
outcome. Pass the evaluation result in: `finalize_primary_progress(&self, succeeded: bool)`.
`succeeded = result.is_ok()` maps to "data-bearing" vs. `Error`, and cancellation is read from
`is_cancelled()`. That is simpler and correct regardless of status timing, so the table above is
implemented as:

- cancelled → keep last (the `done("Cancelled")` written by the cancel handler);
- `succeeded` → `last` if `is_done()`, else `done("Finished")`;
- otherwise → clear.

## Persistence

After setting, call the existing `save_metadata_to_store()` in the same way the loop does. It is
already a no-op for `stored: false`, non-keyed assets, and volatile assets. `finish_run_with_result`
persists metadata again when it stores the value, and that write carries the same progress.

## Rejected alternatives

- **Finalize inside the `JobFinishing` arm of the loop.** That is equally ordered, but the loop
  also exits through `Cancel`/`ErrorOccurred` and through the post-finish path, so the call would
  be needed in three places.
- **Drop progress messages until the end.** That loses live progress.

## Known-issue preflight

None relevant beyond `evaluate-path-consolidation` (complete).

## Relevant commands

None. Commands keep calling `context.progress(..)`.

## Documentation architecture

`specs/reference/ASSETS.md`: add a "Progress after completion" paragraph stating the table and
that status is authoritative. Add a History row and bump `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` (harness tail, `finalize_primary_progress`, new `terminal_progress`, tests); `liquers-core/src/interpreter.rs` test may re-add a progress assertion |
| Workflows | Every evaluation; UI progress display |
| Existing tests | Any test asserting `primary_progress().is_off()` after completion changes under the recommended contract. Search `primary_progress()` in tests before Step 2. |
| New validation | Deterministic tests + 100-iteration stress per harness (Phase 3) |
| Compatibility | Stored metadata of finished assets now carries one `done` progress entry. This is additive, and the serde shape is unchanged. |
| Concurrency | Removes a race and adds none. `psm.await` already ran before `finish_run_with_result`. |
| Performance | One more metadata store write per keyed run. Acceptable, but measure the job-queue tests' runtime. |
| Recovery | Revert the move and the body change together |
| Certainty | High for the mechanism; the contract awaits decision |
