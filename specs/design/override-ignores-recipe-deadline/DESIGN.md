---
id: OVERRIDE-IGNORES-RECIPE-DEADLINE
kind: design
title: A value pinned with to_override does not expire at its recipe's deadline
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE]
created: 2026-10-26
---
# A value pinned with to_override does not expire at its recipe's deadline

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`to_override` "pins the current value" (`WEB_API_SPECIFICATION.md` §5.4, `key/override`). On the
queued manager the pin silently ends at the deadline the recipe's `expires` scheduled before the
pin, and the next `get` recomputes. The same `Override` status installed by `set_state` never
expires by time, and on the inline manager the lazy deadline check applies to `Ready` only. One
status should have one lifetime.

### Problem Example

Verified by reading HEAD (`cd6d5da`): `AssetRef::run` schedules a finite deadline after a run
(`schedule_expiration` → `DefaultAssetManager::track_expiration`); `DefaultAssetManager::to_override`
and `AssetRef::to_override` never call `untrack_expiration`; when the deadline fires the monitor
calls `expire_with_reason(Direct { Deadline })` and `mark_expired_status` flips `Override` to
`Expired`, persists it, and the monitor evicts the entry. The inline manager's lazy check
(`status == Status::Ready && expiration_time().is_expired()`) never fires for `Override`.

Recipe `data/t.txt` = `make_text/t.txt` with `expires: in 2 s`; queued manager:

1. `get(data/t.txt)` → `Ready`, deadline now + 2 s scheduled;
2. `to_override(data/t.txt)` → `Override`;
3. after 2 s, `get(data/t.txt)`.

Today: the asset was expired at the deadline, evicted, and the recipe runs again (a non-serializable
pinned value is lost outright). On the inline manager the same sequence keeps the pin. Should be:
the pinned value is served on both managers until an explicit `expire`, `remove`, a write, or a
dependency cascade.

### Scope and Acceptance Criteria

- **AC-1** The pin survives the recipe's deadline (queued)
  WHEN a `Ready` asset with a pending deadline is pinned with `AssetManager::to_override` and the deadline passes
  THEN the asset is still `Override` with its value, and `get` serves it without evaluating
- **AC-2** Directly on the asset
  WHEN `AssetRef::to_override` is called instead and the monitor fires
  THEN the asset stays `Override` and is not evicted
- **AC-3** Explicit expiry still works
  WHEN `expire(key)` is called on the pinned key
  THEN it becomes `Expired` and its dependents expire, as today
- **AC-4** A dependency cascade still works
  WHEN a dependency recorded in the pinned value's metadata changes
  THEN the pinned key expires (`Cascaded`), as an `Override` installed by `set_state` does
- **AC-5** `Ready` deadlines unchanged
  WHEN an unpinned `Ready` asset reaches its deadline
  THEN it expires and is evicted, as today (existing expiration tests)
- **AC-6** Both managers agree
  WHEN AC-1 runs under `ImmediateEnvironment`
  THEN the same outcome (already true; the test pins it)

Non-goals: clearing the recipe's `expires` from the pinned metadata (kept as provenance; nothing
schedules a deadline for a loaded `Override`); deadlines on `Source` values.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — rule 5: the expected lifetime of a pinned value is a
  behaviour choice the issue explicitly leaves to the maintainer. Once decided as recommended, it
  is an S bug fix in `liquers-core/src/assets.rs` with no `pub` change and becomes eligible.
- **Leading issue:** **Open design question - does a recipe deadline apply to an `Override`?**
  Recommended: no. A pin is a user decision to stop recomputing; `set_state`'s `Override`, the
  inline manager and the `key/override` route all already behave or read that way. Alternative:
  deadlines apply to every `Override` — then `set_state` would have to track a deadline from the
  supplied metadata, the inline lazy check would include `Override`, and `ASSETS.md` would say a pin
  is temporary.
- **Explanation:** under the recommended answer the fix is two guards: untrack on pin, and a
  deadline that finds an `Override` refuses without evicting.
- **Open questions:** 1. the leading question.

### Design Dependencies

- `expiration-monitor-assetref` (complete, E4) — the monitor's fire path.
- `assets-api-admin-operations` — **overlaps** (weak): uses `to_override` from the HTTP API; no
  change there.
- `ASSETS-IMPROVEMENTS` (sticky eviction of `Source` / `Override`, where this issue was found) —
  **overlaps** (weak): a pinned value that is never deadline-expired is also never deadline-evicted.

### Consolidated Findings

- `untrack_expiration` alone is racy: the `Untrack` message may reach the monitor after it has
  popped the entry. The guard in `mark_expired_status` makes the outcome independent of that race,
  and returning an error (not `Ok`) is what keeps the monitor from evicting (`should_evict` is
  `false` for an `Override` refusal).
- Only `Direct { Deadline }` is refused; a `Cascaded` reason whose root cause was a dependency's
  deadline still expires the pin (AC-4).

## Phase 2: Architecture

### Solution

1. `AssetRef::mark_expired_status` (`liquers-core/src/assets.rs`): in the `Status::Override` arm,
   when `reason` is `ExpiryReason::Direct { cause: ExpiryCause::Deadline { .. } }`, return
   `Err(expire_refusal(owner_key, Status::Override, "a deadline does not expire a pinned value"))`
   without changing anything. The test is a private `fn is_own_deadline(reason: &ExpiryReason) ->
   bool` with an explicit `match` over every `ExpiryReason` / `ExpiryCause` variant.
2. `DefaultAssetManager::to_override` live branch: `self.untrack_expiration(asset_ref.id())` after
   `asset_ref.to_override()`. The inline manager's `to_override` calls its `untrack_expiration`
   too (a no-op there; kept for symmetry and for external managers reading it as the pattern).
3. The monitor's fire path is unchanged: the `Err` leaves `should_evict` false for `Override`
   (not in its evict-on-failure list), so it keeps the entry and logs once to stderr.

Rejected: untrack only (racy, Consolidated Findings); skip `Override` in the monitor before
expiring (a pin between the check and the expire still flips it); clear `expiration_time` on the
asset (the run's scheduling reads it; not the cause).

### Changes

- `liquers-core/src/assets.rs`: `mark_expired_status`, a private helper,
  `DefaultAssetManager::to_override`, `ImmediateAssetManager::to_override`.
- `specs/reference/ASSETS.md` (§Remove Semantics / `to_override` bullet: a pinned value does not
  expire by its recipe's deadline) and `ASSET_LIFECYCLE.md` route table (deadline → `Ready` only).
  History rows, `reviewed:`.
- No `pub` signature, command, format or binding change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| A test expects a pinned value to expire by deadline | none found in `expiration_integration.rs`; run it | medium |
| Monitor log noise when the race occurs | one `eprintln!` per raced deadline | high |
| An external manager relying on deadline expiry of `Override` | documented change in `ASSETS.md` | medium |

## Phase 3: Examples and Tests

### Examples

The Problem Example, with `expires: in 200 ms` to keep the test short.

### Tests

In `liquers-core/tests/expiration_integration.rs`:

- `to_override_survives_recipe_deadline` — queued manager: evaluate, `to_override`, sleep past the
  deadline (bounded poll), the asset is `Override`, `get` does not run `make_text` again (counter).
  Covers AC-1.
- `to_override_survives_recipe_deadline_inline` — the same under `ImmediateEnvironment`. Covers
  AC-6.
- `asset_to_override_refuses_deadline_expiry` — `AssetRef::to_override` without the manager, then
  `expire_with_reason(Direct { Deadline })` is `Err(StatusConflict)` and the status stays
  `Override`; then `expire()` succeeds. Covers AC-2, AC-3.
- `pinned_value_expires_when_its_dependency_changes` — `data/t.txt` =
  `-R/data/src.txt/-/upper/t.txt`, pin, `set_state(data/src.txt, …)`: `data/t.txt` is `Expired`.
  Covers AC-4.
- Existing deadline tests (`test_*expir*` in the same file) pass. Covers AC-5.

Queries validated: `make_text/t.txt`, `-R/data/src.txt/-/upper/t.txt`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test expiration_integration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `expiration_integration.rs` — the four tests; AC-1, AC-2 fail at HEAD — `cargo test -p liquers-core --test expiration_integration override`
- [ ] 2. `assets.rs` `mark_expired_status` + `is_own_deadline` — AC-2, AC-1 pass — same command.
  Rollback: revert this step.
- [ ] 3. `assets.rs` both `to_override` implementations untrack — `cargo test -p liquers-core --lib --tests`
- [ ] 4. `ASSETS.md`, `ASSET_LIFECYCLE.md` (History, `reviewed:`); issue closed — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum` route tests for
`key/override` if present; diff review (explicit matches, no `_ =>`).
