# EXPIRATION-SAFETY

Status: Draft

## Summary
`EXPIRATION-SAFETY` addresses timing and consistency issues in asset expiration handling.
Scope includes monitor scheduling correctness, expiration/eviction ordering, and stale-timer behavior when assets are replaced.

## Motivation
Current expiration behavior has race windows and stale scheduling paths that can produce incorrect timing outcomes or transiently inconsistent reads.
These are runtime-correctness problems and should be handled as a dedicated feature, not a side note in other asset fixes.

## Verified failure modes
1. Expire/remove is non-atomic:
   - Monitor sets status to `Expired`, then removes from manager maps in a separate step.
   - `get(&key)` can return the stale expired cached asset in this window.
2. Duplicate scheduling for one asset id:
   - `track_expiration` pushes to heap with no dedup/update.
   - Multiple `schedule_expiration` calls can create conflicting timers; earlier one may fire unexpectedly.
3. Remove-after-failed-expire path:
   - Monitor currently attempts map cleanup even when `expire()` fails.
   - This can evict assets that were not actually expired.
4. Stale timer after `set_state()` replacement:
   - `set_state()` replaces map asset but does not untrack old asset id.
   - Old timer can still fire on detached ref, creating noisy or ambiguous lifecycle behavior.

## Goals
1. Ensure expiration is timing-safe and deterministic for a given asset id.
2. Prevent stale or duplicate timer entries from expiring assets at wrong moments.
3. Guarantee map-eviction semantics are consistent with successful expiration.
4. Keep behavior compatible with existing status model (`Ready`, `Override`, `Expired`, `Volatile`).

## Non-goals
1. Redesigning the whole scheduler/job-queue architecture.
2. Changing command execution classification (`fast/slow/default`) handled by `EXTENDED-FAST-TRACK`.
3. Changing business semantics of `Expired` vs `Volatile`.

## Proposed design
1. Single active timer per asset id:
   - Maintain canonical monitor state `asset_id -> scheduled_expiration`.
   - `Track` updates the active expiration (replace/merge), not append blind duplicates.
   - Tracking must be order-robust: stale/out-of-order `Track`/`Untrack` messages must not revive invalid timers for an asset id that was logically untracked/replaced.
   - Eviction/cleanup decisions must remain strictly `asset_id`-scoped (never key-only).
2. Expire-then-evict only on valid expiration outcome:
   - If `expire()` succeeds (or asset is already `Expired`), evict as normal.
   - If `expire()` fails, apply status-aware fallback:
     - Evict on failure for clearly non-running/stale statuses (`None`, `Recipe`, `Source`, `Error`, `Cancelled`, `Directory`, `Volatile`).
     - Do NOT evict on failure for in-flight statuses (`Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing`).
   - Rationale (resolved policy):
     - `Source`: eviction on failure is acceptable/desirable because value is typically re-fetchable from store and deserializable.
     - `Volatile`: eviction on failure is redundant but safe; volatile assets are not intended to be manager-cached in keyed/query maps.
3. Replacement-safe untracking:
   - Any operation replacing/removing a keyed asset (`set_state`, `set_binary`, `remove`) must untrack prior asset id.
4. Stale-expired guard on read paths:
   - Apply stale-expired guard to both keyed (`assets`) and query-cached (`query_assets`) paths.
   - If cached entry is already `Expired`, remove it (id-checked) and continue to resolve fresh asset path.
   - This closes the read window where expired cached object is returned after timer fire.
5. Monitor observability:
   - Add structured diagnostics for track/update/untrack/fire outcomes including `asset_id`, key/query (if available), and decision reason.

## Suggested implementation phases
1. Monitor state normalization:
   - Add canonical active-schedule map and dedup policy.
2. Expiration outcome gating:
   - Gate map removal on successful/valid expiration outcome.
3. Replacement hygiene:
   - Add missing `untrack_expiration` in `set_state` replacement path.
4. Read-path safety:
   - Add stale-expired guard in both keyed and query-cached get paths.
5. Tests and regression coverage:
   - Add focused timing/race tests.

## Acceptance criteria
1. Repeated `schedule_expiration` for same asset id does not create multiple effective timers.
2. `set_state` replacement does not leave active timer for old asset id.
3. Out-of-order `Track`/`Untrack` messages do not re-activate stale timers after logical untrack/replacement.
4. On `expire()` failure, monitor eviction is status-aware:
   - in-flight statuses are preserved in maps;
   - non-running/stale statuses are eligible for eviction cleanup.
5. Keyed and query-cached `get` paths do not return stale expired cached entries after expiration has fired.
6. Existing expiration tests keep passing and new timing tests pass reliably.
7. Timing-sensitive tests are deterministic and include a brief inline comment explaining:
   - the race/timing issue being validated, and
   - why timing-control scaffolding (pause/advance/hooks) is required.

## Suggested test cases
1. `duplicate_track_updates_deadline_not_duplicate_fire`
2. `set_state_untracks_previous_asset_timer`
3. `stale_track_after_untrack_does_not_reactivate_timer`
4. `failed_expire_inflight_does_not_evict_from_map`
5. `failed_expire_nonrunning_evicts_from_map`
6. `get_key_skips_stale_expired_cached_asset`
7. `get_query_skips_stale_expired_cached_asset`
8. `reschedule_to_later_prevents_early_fire`

## Decisions and remaining open questions
1. **Track merge policy (high priority, decided)**:
   - Decision: use **earliest-deadline-wins** for repeated `Track` on the same `asset_id`.
   - Additional policy:
     - `schedule_expiration` and `track_expiration` are crate-internal (`pub(crate)`), not public API.
     - Timers for the same key but different `asset_id`s are independent and should both fire (each asset variant expires on its own lifecycle).
     - If a timer fires but corresponding map entry is no longer present/replaced, emit `eprintln!` diagnostic and continue.
   - Known drawback:
     - A later attempt to extend TTL for the same `asset_id` will not take effect if an earlier deadline already exists.
     - Consequence: conservative early expiration (extra recomputation, lower cache lifetime), but no stale-overstay risk.
   - Observed multi-deadline paths in current code:
     1. Deterministic duplicate for same `asset_id` in `apply_immediately`:
        - `run_immediately()` path already schedules in `finish_run_with_result`.
        - `AssetManager::apply_immediately()` schedules again after return.
        - Result: two `Track` events for the same `asset_id` and usually same deadline.
     2. Public API repeatability:
        - `AssetRef::schedule_expiration()` and manager `track_expiration()` are crate-internal but still repeatable from internal paths; repeated calls can enqueue multiple deadlines for same `asset_id`.
     3. Replacement path creates concurrent deadlines for same key but different `asset_id`s:
        - `set_state()` replaces map entry with a new `asset_id` but currently does not `untrack` old one.
        - Old timer + new timer can coexist for the same key.
     4. `Untrack` is one-shot in current monitor logic:
        - `cancelled` is a set and `cancelled.remove(asset_id)` is consumed when first due entry is popped.
        - With duplicate heap entries for one `asset_id`, one `Untrack` may skip only one due entry, leaving later duplicate entries still active.

2. **Timing test determinism (decided)**:
   - Timing-sensitive tests must be deterministic (paused Tokio time and/or deterministic clock hooks).
   - Each timing-sensitive test must include a brief explanatory comment describing the targeted issue and the reason for additional timing-control code.
