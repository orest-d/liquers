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
2. Expire-then-evict only on valid expiration outcome:
   - Evict from maps only if expiration succeeded or asset is already expired.
   - Do not evict when `expire()` fails due to non-expirable current status.
3. Replacement-safe untracking:
   - Any operation replacing/removing a keyed asset (`set_state`, `set_binary`, `remove`) must untrack prior asset id.
4. `get(key)` stale-expired guard:
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
   - Add stale-expired guard in keyed `get`.
5. Tests and regression coverage:
   - Add focused timing/race tests.

## Acceptance criteria
1. Repeated `schedule_expiration` for same asset id does not create multiple effective timers.
2. `set_state` replacement does not leave active timer for old asset id.
3. Monitor does not evict map entry when `expire()` fails on non-expirable status.
4. `get(&key)` does not return a stale expired cached entry after expiration has fired.
5. Existing expiration tests keep passing and new timing tests pass reliably.

## Suggested test cases
1. `duplicate_track_updates_deadline_not_duplicate_fire`
2. `set_state_untracks_previous_asset_timer`
3. `failed_expire_does_not_evict_from_map`
4. `get_key_skips_stale_expired_cached_asset`
5. `reschedule_to_later_prevents_early_fire`
