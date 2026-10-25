---
id: TEMPORARY-ASSET-NEVER-RUN-LEAK
kind: design
title: Coverage record for a temporary asset kept alive by its service loop
form: compact
status: complete
readiness: covered
autofix: not-eligible
area: [core/assets]
issues: [TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP]
created: 2026-10-20
---
# Coverage record for a temporary asset kept alive by its service loop

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-2 only. The work is
covered by `environment-manager-reference-cycle`, so no independent Phase 3 or 4 is written.
Completed 2026-10-22 with that design, which implemented its step 5 and closed this issue.

## Phase 1: High-Level Design

### Purpose

A temporary asset that is never run must be freed when its last outside reference is dropped, and
its service-message task must end.

### Problem Example

Verified at HEAD in `liquers-core/src/assets.rs`: `AssetRef::new_temporary` spawns
`process_service_messages` on a strong clone (`assetref1`). The loop waits on the asset's own
service channel, whose sender lives in the same `AssetData`, and returns only on `JobFinishing`,
`JobFinished` or `ErrorOccurred`, which no one sends to an asset that is never run.

```rust
let asset = AssetRef::new_temporary(envref.clone());   // what interpreter::evaluate does
let weak = asset.downgrade();
drop(asset);
assert!(weak.upgrade().is_none());   // today: Some — the task holds it, forever
```

Should be: `None`, and the task has ended. `interpreter::evaluate` creates one such asset per call,
so a long-running process leaks one task and one asset per call.

### Scope and Acceptance Criteria

- **AC-1** Unreachable temporary asset is freed
  WHEN the last outside reference to a never-run temporary asset is dropped
  THEN a weak reference to it no longer upgrades and its service-loop task has ended

This is `environment-manager-reference-cycle` AC-7 ("WHEN `interpreter::evaluate` returns and its
temporary asset is unreachable THEN no service-loop task for it remains"), tested there by
`teardown08_temporary_asset_loop_exits`.

### Design Readiness

- **Readiness:** covered
- **Automatic fixing:** not-eligible — the covering design is not eligible (it adds `pub`
  `try_get_envref`, rule 4); this record carries no plan of its own.
- **Leading issue:** None
- **Explanation:** `environment-manager-reference-cycle` (in_review, ready) Phase 2 replaces the
  strong loop with one holding a `WeakAssetRef` and the receiver only, which ends when `recv()`
  returns `None` once the asset (and with it `service_tx`) is dropped (its step 5). That is this
  issue's first proposed direction, and its AC-7 is this issue's AC-1.
- **Open questions:** None. Proposed to the maintainer: add this issue to that design's `issues:`
  (triage case 2, T4 subsumption), which this pass could not do because that design is not
  assigned to it.

### Design Dependencies

- `environment-manager-reference-cycle` — **covered-by** (in_review, ready, step 5).
- `temporary-asset-unique-id` — **overlaps**: the same constructor family; the id fix changes
  `AssetData::new_temporary`, not the loop. No order.

## Phase 2: Architecture

### Solution

No separate change. The covering design's step 5 moves the per-message body of
`process_service_messages` into a private `handle_service_message` and spawns, from
`AssetRef::new_temporary`, a loop that holds a `WeakAssetRef` and the service receiver, upgrades
per message, and exits when the channel closes. `run_with_future` keeps its strong, terminated
loop.

Rejected here (as there): starting the loop only on first run (the inline path already drives its
own loop, and a queued run spawns one, so the temporary loop would become dead code anyway — a
valid alternative the covering design may still choose, with the same AC).

The stderr line "Starting to process service messages for asset 0" names id `0` today; after
`temporary-asset-unique-id` it names the asset's real id.

### Changes

None in this record. If the maintainer prefers to keep the issue independent of
`environment-manager-reference-cycle`, its step 5 can be lifted out unchanged as a separate `S`
fix (it needs neither the weak `envref` nor `try_get_envref`).

### Risks

Covered by the covering design's risk table (splitting `process_service_messages`; suites
`asset_cancellation`, `asset_failure_contract`). Certainty that the coverage is complete: high —
the never-run temporary asset is exactly the one `interpreter::evaluate` creates.
