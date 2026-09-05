---
title: "Phase 1: High-Level Design — Versions for computed keyed assets"
kind: design
audience: internal
area: [core/assets]
---
# Phase 1: High-Level Design - Versions for computed keyed assets

## Feature Name

Versions for computed keyed assets (fix for `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`)

## Purpose

A keyed asset produced by *evaluation* never records a `MetadataRecord.version`, so the dependency
manager registers it as `Version::unknown()`. Every keyed-to-keyed invalidation path is gated on a
known version, so expiring a computed asset invalidates no keyed dependent — dependency-driven
invalidation, one of the system's central promises, is inert for exactly the assets the system
computes for itself. This design gives computed keyed assets a real version, derived from the hash
of the bytes they serialize to, and settles what the resulting live cascade must and must not do.

The scope is deliberately the *version*, not the cascade rule: the owner's decision on the issue is
option 2. The `expire_internal` comment/condition discrepancy the issue also records is in scope
because it is the same three lines, and is decided rather than left ambiguous.

## Core Interactions

### Query System

None. No query, key or plan syntax changes.

### Store System

`MetadataRecord.version` is already persisted in the sidecar and already `Option<Version>`, so
records written before this change deserialize unchanged. Nothing new is written to a store; an
existing field stops being empty for computed assets. Confirming the round-trip rather than
assuming it is Phase 2 work.

### Command System

None. Command metadata and implementation versions (`ns-dep/command_*`) already carry real versions
registered at startup and are untouched.

### Asset System

This is the whole of the change. Four mechanisms currently see `Version::unknown()` for a computed
keyed asset and start seeing a concrete one:

1. `DependencyManager::expire_internal` — its `skip_cascade` guard stops being always-taken, so
   keyed dependents are reached for the first time.
2. `DependencyManager::register_version` — a recomputation with different bytes cascades; with
   identical bytes it does not, which is the property that makes hashing preferable to a timestamp.
3. `DependencyManager::add_dependency` / `load_from_records` — their version-consistency check
   stops being skipped, in-process and on reload.
4. `AssetRef::try_fast_track` — its recorded-dependency check becomes non-vacuous, changing
   cache-hit behaviour including across a restart.

Items 3 and 4 are where the risk is, not item 1. Their present behaviour on an *unregistered*
dependency key is `version_consistent → false → expire the dependent`, which today is unreachable
for computed assets only because the recorded version is unknown. Turning versions on makes it
reachable on every cold start, which would expire persisted dependents on first load rather than
serve them. Establishing what an unregistered dependency means is therefore part of this design,
not a consequence to discover afterwards.

The version is also read by `AssetRef::record_dependency_on_asset` out of the *child's live
metadata*, so **when** in the evaluation sequence the version is set is observable by a concurrent
parent, not an internal detail.

### Value Types

None. `Version` and both of its constructors (`Version::from_bytes`, `Version::new_unique`) already
exist and are already used by the hand-in paths; what is missing is calling them on the evaluate
path.

### Web/API

No endpoint changes. `liquers-web` is wasm32-only, and the timestamp fallback route reaches
`std::time::SystemTime::now()`, which is not a supported clock on `wasm32-unknown-unknown` — the
rest of the codebase uses `chrono::Utc::now()` for wall time. Whether the fallback is reachable on
wasm, and which clock it should use, is an open question below.

### UI

None.

## Crate Placement

**liquers-core**, entirely: `src/assets.rs` (where the version is computed and installed on the
evaluation path) and `src/dependencies.rs` (the unregistered-dependency rule and the
`expire_internal` guard). No other crate changes; `liquers-lib`, `liquers-axum`, `liquers-web` and
`liquers-py` consume the behaviour without touching the API, since `MetadataRecord.version` is
already public and already `Option`.

## Documentation Intent

**Reference:** Extend `specs/reference/DEPENDENCIES_STATUS.md`. It already owns the
`Version::unknown()` contract ("unknown versions may record edges, but they must not replace an
already-known dependency version") and the flow descriptions that say a computed dependency's
version is unknown. Those statements become wrong the moment this ships, so the document must
change in the same commit. A new reference is not warranted: the versioning rule is one paragraph
of an existing contract, not a subsystem.

**Guide:** Neither. Nothing here is a repeatable task a contributor performs; it is behaviour the
runtime has. Reconsider only if Phase 3 produces a reusable recipe for asserting cascade behaviour
in tests that is worth lifting into a testing guide.

**Other documents to create:** None. Issues found in passing are filed under `specs/issues/` by the
normal procedure rather than as design documents.

**Specific documents to update:**

- `specs/reference/DEPENDENCIES_STATUS.md` — the version contract, the unregistered-dependency
  rule, and the Flow A/B statements about unknown computed versions (`## History` row, `reviewed:`).
- `specs/reference/ASSETS.md` and `specs/reference/ASSET_LIFECYCLE.md` — reviewed for the
  evaluation-sequence description; updated only if they state or imply the ordering this change
  touches. Neither mentions `version` today, so this may be a no-op confirmed in Phase 5.
- `specs/issues/KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS.md` — status and resolution note.
- `specs/design/stale-dependency-status-finalization/DESIGN.md` — that design records itself as
  blocked on this one and asks for its C2 decision to be revisited once versions are real; this
  design's Phase 5 says whether the blocker is discharged.
- `specs/README.md` — capability-map line for this design folder.

Audience: contributors and coding agents working on `core/assets`. What must be understandable
without reading this folder: *what a version means for a computed asset, when it is assigned, and
what an unregistered dependency version implies.*

## Open Questions

1. **Where in the evaluation sequence is the version assigned?** `serialize_to_binary` is where the
   bytes exist but runs only during persistence, so a keyed asset whose persist is skipped or fails
   gets none; status finalization (`try_to_set_ready`) is more consistent and runs before the
   `ValueProduced` notification a parent can observe, but has no bytes yet. Leading answer:
   serialize once during finalization for keyed non-volatile assets, cache the bytes, and let
   persistence reuse them.
2. **What does an unregistered dependency key mean?** Today `version_consistent` answers `false`
   ("mismatch") for a key the manager has never seen, and `add_dependency` expires the dependent on
   that answer. Options: treat absence as unverifiable and record the edge; or register the
   recorded version provisionally so a later real registration cascades if it differs. This is the
   question that decides whether cross-process caching survives.
3. **Timestamp fallback, or `Version::unknown()`, for a keyed asset that does not serialize?** The
   owner's rule says a fallback, and `set_state` already uses one — but a timestamp makes such an
   asset invalidate all its dependents on every evaluation. If a fallback, which clock: `chrono`
   (wasm-safe, used everywhere else) or `SystemTime` (what `Version::from_time_now` uses today)?
4. **Does the `expire_internal` root guard change?** `include_root || current != *key` is
   vacuously true on both call paths, and its comment claims an exemption the code does not
   implement. Decide between implementing the comment (an explicit expiry's root always cascades)
   and correcting the comment, and pin the choice with a test.
5. **Does `try_fast_track` need a version-ordering rule?** A dependent fast-tracked before its
   dependency is loaded skips the check that the dependency was later found to have changed. Real
   versions make this the difference between a warm cross-process cache and a stale one.
6. **Is the `stale-dependency-status-finalization` blocker discharged by this design alone**, or
   does its C2 decision need a change here to be revisitable? Answered in Phase 5, not assumed now.

## References

- `specs/issues/KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS.md` — the issue and the owner's
  decision (option 2) of 2026-09-05.
- `specs/design/stale-dependency-status-finalization/` — the design this one unblocks; its Phase 4
  review is where the gap was traced.
- `specs/reference/DEPENDENCIES_STATUS.md` — the current version and dependency-edge contract.
- `liquers-core/src/dependencies.rs` — `expire_internal`, `register_version`, `add_dependency`,
  `version_consistent`, `track_asset`.
- `liquers-core/src/assets.rs` — `evaluate`, `try_to_set_ready`, `serialize_to_binary`,
  `save_to_store`, `try_fast_track`, `record_dependency_on_asset`, and the `set_binary`/`set_state`
  pair that already versions hand-in values.
