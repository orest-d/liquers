# Phase 3: Examples & Use-cases — asset-cancellation-outcome

## Overview Table
| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A command that stops when asked | `check_cancelled()?` in a long loop | AC-5, AC-6 |
| 2 | Example | Cascade with a named root cause | a dependent cancelled by its dependency | AC-9, AC-10, AC-11 |
| 3 | Test | `liquers-core/tests/asset_cancellation.rs::ac01_completed_sync_run_wins_over_late_cancel` | completed sync run ends `Ready`, stored, request dropped | AC-1, AC-5 |
| 4 | Test | `…::ac02_cancel_suspended_async_command` | suspended async command dropped, nothing stored | AC-2, AC-10 |
| 5 | Test | `…::ac03_cancel_while_waiting_for_dependency` | dependent cancelled, dependency finishes | AC-3 |
| 6 | Test | `…::ac04_cancel_before_the_job_starts` | `Submitted` → `Cancelled`, command never runs | AC-4 |
| 7 | Test | `…::ac05_async_command_checks_cancellation` | `is_cancelled` / `check_cancelled` from async code | AC-5, AC-6 |
| 8 | Test | `…::ac06_cancellation_error_ends_cancelled` | command's own `Error::cancelled` | AC-6, AC-10 |
| 9 | Test | `…::ac07_one_terminal_status_per_run` | ten races of cancel against completion | AC-7 |
| 10 | Test | `…::ac08_replacement_is_not_overwritten`, `…::ac08_to_override_discards_the_late_result` | late result discarded | AC-8 |
| 11 | Test | `…::ac09_cascade_names_the_root_cause`, `…::ac09_handled_cascade_finishes_ready` | three-level cascade; handled cascade | AC-9, AC-10, AC-11 |
| 12 | Test | `…::ac12_cancelled_keyed_asset_info_then_reevaluated` | info reports cause; next `get` re-evaluates | AC-12 |
| 13 | Test | `…::cancelled_state_returns_recorded_cause` | `State::value_error` returns the stored cause | AC-10 |
| 14 | Test | `liquers-core/src/assets.rs::tests::cancellation_request_first_cause_wins_and_clear_resets` | lock-free request, wake-up, `clear` | AC-5 |
| 15 | Test | `liquers-core/src/assets.rs::tests::dependency_failure_error_names_key` (tightened) | cascade cause type and `query` | AC-9, AC-10 |
| 16 | Test | `liquers-axum/tests/assets_api_endpoints.rs::aae92_…`, `aae38_…` (tightened) | HTTP cancel of a blocking command ends `Ready` only | AC-1, AC-7 |

## Example 1: A command that stops when asked
A synchronous command that loops over many items cannot be interrupted, so it checks between items.
`check_cancelled()?` returns the request's cause; returned unchanged, it ends the asset `Cancelled`.

```rust
fn process_rows(state: &State<Value>, context: Context<E>) -> Result<Value, Error> {
    let rows = state.try_into_string()?;
    let mut out = String::new();
    for line in rows.lines() {
        context.check_cancelled()?;          // Err(cause) once cancel() was requested
        out.push_str(&expensive(line));
    }
    Ok(Value::from(out))                     // finished anyway: the asset ends Ready and is stored
}
register_command!(cr, fn process_rows(state, context) -> result)?;
```
Expected: cancelled mid-loop → `Status::Cancelled`, `value_error()` is `ErrorType::Cancelled` with
`query` = this asset's key; not cancelled, or cancelled after the loop → `Ready` and persisted.

## Example 2: Cascade with a named root cause
`top` evaluates `middle` (`context.get_dependency_state`), which evaluates `root_dep`. A client
cancels `root_dep`. `middle`'s wait returns the cancellation `root_dep` recorded; `?` returns it, so
`middle` ends `Cancelled`, and so does `top`. Each logs one warning,
`Dependency root_dep was cancelled; root cause: root_dep`, and each `value_error().query` is
`root_dep`. A command that matches `Err(e) if e.is_cancelled()` and returns `Ok` ends `Ready`.

## Edge and Error Cases
| Symptom | Expected | Test |
|---|---|---|
| Cancel lands while a sync command blocks | it completes; `Ready`, stored | `ac01_…`, `aae92_…` |
| Cancel lands between the command returning and finalization | completion wins (biased race; finalization outside it) | `ac07_…` |
| Cancel of a queued asset that a runner is claiming | the claim and the cancel take the same write lock; one wins | `ac04_…` |
| Replacement while running | replaced asset `Cancelled` at once; late result discarded | `ac08_…` |
| Asset with legacy metadata | `with_cancellation` sets status and message only | covered by type, not tested |
| wasm / inline manager | same harness via `select_biased!`; cancel effective only at suspension points | build matrix (wasm row) |

## Test Plan
- Integration: `liquers-core/tests/asset_cancellation.rs` — 13 tests above (AC-1 to AC-12).
- Unit: `liquers-core/src/assets.rs` — `cancellation_request_first_cause_wins_and_clear_resets`
  (AC-5); `dependency_failure_error_names_key` (AC-9, AC-10); existing save-skip tests now set
  `Status::Cancelled`; `test_get_binary_error_identity` fails an in-flight asset;
  `test_wait_for_evicted_expired_dependency_fails_parent` asserts the wait leaves the parent's status
  to its run (D1).
- Integration (axum): `aae92_cancel_while_processing_reports_cancelled_deterministically`,
  `aae38_get_q_cancel_with_destructive_gets` assert `Ready` (AC-1, AC-7).
- Commands: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum --test assets_api_endpoints`;
  `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm row).

## Learning Log
- A synchronous command blocks its worker thread for its whole run, so tests that cancel one need
  `#[tokio::test(flavor = "multi_thread")]`; the request is made from another thread and seen by
  `Context::is_cancelled()` without a lock.
- "Command returned `Ok`" is the point of no return only up to the next genuine suspension point in
  the rest of the plan: a cancel requested while a later step of the same plan awaits still cancels.
- `AssetManager::get_asset_info` already read the live asset, so AC-12 needed only the cause in
  `error_data`.
- `fail_asset` runs after `finalize_primary_progress`, so it must not overwrite a finalized bar
  (`finished_run_progress_contract_*`).
