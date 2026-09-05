# Phase 1: High-Level Design - Stale-Dependency Status Finalization

## Feature Name

Stale-dependency status finalization

## Purpose

An asset whose evaluation consumed a dependency that expired mid-run is written to the store as
`Ready` and only afterwards labelled `Expired` in memory, so the store and the runtime disagree
about the one status whose purpose is to force recomputation. This design makes the
stale-dependency rule part of *finalizing* the status — settled before the value is persisted — so
that what a later process loads is what the producing run concluded.

## Core Interactions

### Query System

None. No query syntax, parsing, planning or `Key` encoding changes.

### Store System

Changes **what** is written for one asset shape, not how. The value is written once, as today; only
the status carried in its sidecar metadata changes from `Ready` to `Expired`. The existing load
gate already honours this: `AssetRef::try_fast_track` (`assets.rs:1048`) refuses a stored status
outside `{Ready, Source, Override}`, so an `Expired` sidecar becomes a cache miss with no further
work.

### Command System

None. No `register_command!` signature changes, so `specs/command_registry.yaml` is untouched.

### Asset System

The whole of the change. `AssetRef::evaluate` (`assets.rs:2528`) is the single evaluation body;
step 5 finalizes status (`try_to_set_ready`, `:1818`) and step 7 persists (`:2572`). The
stale-dependency rule instead runs in `finish_run_with_result` (`:2251`), after both, on **both**
harnesses (`run_with_future :2287`, `run_with_future_inline :2326`). Nothing re-persists afterwards:
`save_metadata_to_store` (`:971`) is driven only by `process_service_messages`, whose loop has
already ended on `JobFinishing`.

### Value Types

None. No `ExtValue` variant, no `TypeInfo`, no serializer change.

### Web/API

No route or handler change. `AssetInfo` served from a stored asset stops reporting `Ready` for a
run that concluded `Expired` — a correction of what is reported, not of the API.

### UI

Not applicable.

## Crate Placement

**liquers-core** only — `liquers-core/src/assets.rs`, with a possible read of
`liquers-core/src/dependencies.rs` for the `track_asset` consequence below. The rule, the status
authority and both run harnesses all live in that one file. No other crate implements
`AssetManager` or calls the affected internals, so `liquers-lib`, `liquers-axum`, `liquers-web` and
`liquers-py` are unaffected at compile level.

## Documentation Intent

**Reference:** Extend, do not create. `specs/reference/ASSET_LIFECYCLE.md` already states the
invariant this restores (§"the one evaluation path", row 6: status finalization "must run **before**
the notification and **before** persistence"); it needs the stale-dependency rule named as part of
that step. `specs/reference/ASSETS.md` §Expiry (`:241-244`) and
`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (`:246-248`) both attribute the relabel
to `finish_run_with_result` and need retargeting. Each gets a `## History` row and a `reviewed:`
bump in the same commit (§9.2).

**Guide:** Neither. There is no repeatable task a developer performs here — the behaviour is
internal to evaluation and reached through no API a user calls. Reconsider only if Phase 2 chooses
an architecture that changes what a *caller* must do to recover such an asset, which would make it
guide material alongside the `*_any_status` recovery reads.

**Other documents to create:** None. If Phase 2 confirms the `track_asset` or notification gaps
below are separate defects rather than consequences of this one, they are filed as issues under
`DOCS_STRUCTURE_GUIDE.md` §4.8, not designed here.

**Specific documents to update:** `specs/reference/ASSET_LIFECYCLE.md`,
`specs/reference/ASSETS.md`, `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md` (its four file:line citations predate
the consolidation and are all stale), `specs/README.md` and `specs/index.csv` for the new design
folder.

**Audience:** a future maintainer asking "when is an asset's status final, and what does the store
hold?" must get the answer from `ASSET_LIFECYCLE.md` without opening this folder.

## Open Questions

1. **Where does the rule move to?** Into `try_to_set_ready` (which then owns every status outcome,
   and applies at both its call sites, including the `finish_run_with_result` fallback at `:2224`),
   or into `evaluate` between finalization and persistence (narrower, leaves `try_to_set_ready`
   meaning only "ready"). → Phase 2, from the call graph.
2. **What happens to dependency-manager registration?** `DependencyManager::track_asset`
   (`dependencies.rs:282`) early-returns for `Expired`. Today the relabel lands *after*
   `evaluate` step 8, so such an asset **is** registered; finalizing earlier would stop that.
   Is losing that registration correct, an acceptable cost, or a reason to reorder differently?
   → Phase 2; this is the change's one non-local consequence.
3. **Should the relabel go through `expire()` instead?** `mark_expired_status` (`:2920`) already
   persists `Expired` to the store for a keyed asset (the WP-3 rule), sends
   `AssetNotificationMessage::Expired`, and cascades to dependents. The relabel does none of these.
   Reusing it is a second architecture — and the missing notification and cascade may be a distinct
   defect. → Phase 2 decides; if distinct, file rather than absorb.
4. **Does a volatile keyed asset need the rule?** `try_to_set_ready` sends volatile results to
   `Status::Volatile`, which the `== Ready` guard skips. Volatile assets are never reused, so this
   is probably correct and merely undocumented. → Phase 2, one line either way.
5. **Does the fix disturb the accepted regression in `expired-binary-read-safety`?** That design's
   owner-decided position — a stale-dependency completion is uniformly `Expired` and recoverable
   only through `to_override()` or a `*_any_status` read — is a constraint on this one. Phase 3
   must show its test `I5` still holds. → Phase 2 confirms, Phase 3 proves.

## References

- `specs/issues/ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY.md` — the issue, P2/M
- `specs/design/evaluate-path-consolidation/` — states the invariant (Phase 3 C8/C10, Phase 5
  §"What was omitted"); names the unwritten test `status_is_final_before_persistence`
- `specs/design/expired-binary-read-safety/` — §"Expiry is an error" and the cross-phase B1
  resolution: the two meanings of `Expired` are deliberately collapsed
- `specs/design/dependency-scheduling/` — the execution-time expiry policy the rule implements
- `specs/issues/DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE.md` — closed; it is what makes this
  path reachable in production, and its test is the harness Phase 3 extends
