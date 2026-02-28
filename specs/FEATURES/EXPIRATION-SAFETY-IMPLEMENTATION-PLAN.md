# Implementation Plan: EXPIRATION-SAFETY

Status: Draft  
Related feature: `specs/FEATURES/EXPIRATION-SAFETY.md`

## Objective
Implement expiration handling that is deterministic, order-robust, and safe under replacement/race conditions while preserving current asset lifecycle semantics.

## Fixed Decisions
1. Track merge policy for same `asset_id`: **earliest-deadline-wins**.
2. `schedule_expiration` and `track_expiration` are crate-internal (`pub(crate)`).
3. Timers for same key but different `asset_id`s are independent and should both fire.
4. On timer fire with missing/replaced map entry, emit `eprintln!` and continue.
5. On `expire()` failure: evict non-running/stale statuses, preserve in-flight statuses.
6. Stale-expired read guard applies to both keyed and query-cached paths.
7. Timing-sensitive tests must be deterministic and include a brief explanatory comment.

## Scope
In scope:
1. Expiration monitor tracking data model and fire behavior.
2. Asset replacement hygiene (`set_state` untrack gap).
3. Read-path stale-expired guards in manager cache lookups.
4. Deterministic race/timing regression tests.

Out of scope:
1. Scheduler redesign beyond expiration safety.
2. Command execution priority/queue-classification policies.
3. Store-level data deletion policy changes.

## Implementation Phases

### Phase 1: Remove deterministic duplicate scheduling path
1. Remove duplicate scheduling in `AssetManager::apply_immediately`:
   - Keep scheduling in `finish_run_with_result` path only.
   - Remove second explicit `schedule_expiration` block after `run_immediately`.
2. Add a regression test proving one effective timer for `apply_immediately`.

### Phase 2: Monitor state normalization (earliest-deadline-wins)
1. Replace current `heap + cancelled-set` one-shot semantics with canonical per-id state:
   - `active_deadline_by_id: HashMap<u64, DateTime<Utc>>`
   - optional metadata map for debug (`asset_ref` weak/strong strategy per current ownership approach).
2. On `Track(asset_id, dt)`:
   - if no active deadline: insert.
   - if active deadline exists: keep `min(existing, dt)` (earliest-deadline-wins).
3. Heap entries become advisory; on pop, validate against canonical map before firing.
4. On `Untrack(asset_id)`:
   - delete canonical active deadline for id.
   - stale heap entries are ignored during pop validation.

### Phase 3: Order-robustness and status-aware eviction fallback
1. Ensure stale/out-of-order `Track`/`Untrack` cannot revive logically removed timers:
   - canonical-map validation at fire time is authoritative.
2. At timer fire:
   - attempt `asset_ref.expire().await`.
   - apply status-aware fallback if `expire()` fails:
     - evict on failure for `None`, `Recipe`, `Source`, `Error`, `Cancelled`, `Directory`, `Volatile`;
     - do not evict on failure for `Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing`.
3. Keep map eviction strictly `asset_id`-scoped (`remove_expired_from_maps` with id checks).
4. If map entry not found/replaced for the fired `asset_id`, emit `eprintln!` diagnostic.

### Phase 4: Replacement hygiene
1. Add missing `untrack_expiration(old_asset_id)` in `set_state` replacement path.
2. Re-verify existing `remove` and `set_binary` untrack behavior remains intact.
3. Add regression test that `set_state` replacement does not leave active timer for old id.

### Phase 5: Read-path stale-expired guard
1. Keyed path:
   - In `get(&key)`, if cached asset is already `Expired`, remove it (id-checked) and continue resolution instead of returning stale entry.
2. Query-cached path:
   - Apply equivalent stale-expired guard in query asset resolution path (`get_query_asset` internals).
3. Add tests:
   - `get_key_skips_stale_expired_cached_asset`
   - `get_query_skips_stale_expired_cached_asset`

### Phase 6: Deterministic tests and documentation
1. Implement timing-sensitive tests with deterministic clock control (`tokio::time::pause/advance` or equivalent hook).
2. Add brief inline comment in each timing-sensitive test:
   - what race/timing issue is validated,
   - why extra timing-control code is needed.
3. Update feature status/checklist after passing validation.

## Planned Test Matrix
1. `duplicate_track_updates_deadline_not_duplicate_fire`
2. `set_state_untracks_previous_asset_timer`
3. `stale_track_after_untrack_does_not_reactivate_timer`
4. `failed_expire_inflight_does_not_evict_from_map`
5. `failed_expire_nonrunning_evicts_from_map`
6. `get_key_skips_stale_expired_cached_asset`
7. `get_query_skips_stale_expired_cached_asset`
8. `reschedule_to_later_prevents_early_fire`

## File-Level Change Map (Planned)
1. `liquers-core/src/assets.rs`
   - monitor data model + fire logic
   - `apply_immediately` duplicate scheduling removal
   - `set_state` untrack fix
   - keyed/query stale-expired guard logic
2. `liquers-core/tests/expiration_integration.rs` and/or `liquers-core/src/assets.rs` tests
   - deterministic expiration/race tests
3. `specs/FEATURES/EXPIRATION-SAFETY.md`
   - progress/status updates when implementation lands

## Validation Commands
1. `cargo check -p liquers-core --tests`
2. `cargo test -p liquers-core --test expiration_integration`
3. `cargo test -p liquers-core`

## Risks
1. Over-retention of stale heap entries if canonical-map pruning is incomplete.
2. Test flakiness if real-time sleeps remain in timing-sensitive tests.
3. Behavioral drift between keyed and query cache paths if guard logic is not mirrored.

## Mitigations
1. Canonical-map validation on every timer pop and regular stale-entry cleanup.
2. Deterministic time control and strict “no wall-clock sleep” for race tests.
3. Shared helper for stale-expired guard to keep keyed/query behavior consistent.
