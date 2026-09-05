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
parent, not an internal detail. The owner has decided that point — as early as the version can be
final, never provisionally — under "Owner decisions" below.

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

## Owner decisions

### Assignment point: as early as possible, but never provisional (2026-09-05)

> "The version should become available as soon as possible (even before `track_asset`), as long as
> it is stable — e.g. version would be created immediately on asset creation from a timestamp, but
> that would be not stable since the hash-based version would overwrite it."

This settles what was Phase 1's first open question and gives the rule that decides it:

**A version is published once and never revised.** Earliness is the objective; stability is the
constraint that bounds it. The earliest point at which a *final* version can be known is the
earliest legal assignment point, and anything earlier than that would have to be provisional, which
is forbidden — a parent that has already recorded a provisional version in its own
`DependencyRecord` would hold a version the child no longer has, and the next comparison would read
as staleness that never happened.

For a hash-based version, "final" means "the value exists and its bytes can be computed", which is
after the value is installed and before the `ValueProduced` notification. That places the
assignment in status finalization (`try_to_set_ready`), not in persistence — persistence is too
late, because `record_dependency_on_asset` may already have read the child's metadata and recorded
`Version::unknown()` — and not at asset creation, which is what the timestamp counter-example rules
out.

### One serialization, at finalization, reused by persistence (2026-09-05)

> "One possible is to prepare a binary on finalization, calculate hash and then simply use the same
> binary. This needs to only be done on non-volatile keyed assets. The binary may be disposed later
> based on the policy (out of scope, but make a note)."

Confirmed as the mechanism. Status finalization serializes the value once, hashes those bytes into
`MetadataRecord.version`, and leaves them in `AssetData::binary`; `save_to_store` then finds them
through `binary_unchecked()` and writes them without serializing again. The version and the stored
bytes are the same bytes by construction, which is a stronger guarantee than serializing twice and
trusting the two runs to agree — a value whose serialization is not byte-deterministic (map
iteration order, a float format, an embedded timestamp) would otherwise produce a version that does
not describe what the store holds.

**Only non-volatile keyed assets.** A volatile asset is never registered with the dependency
manager and needs no version; a non-keyed query asset is not a graph node and is never persisted,
so serializing one would be pure cost on the commonest path in the system. Both exclusions are
about avoiding work, not just about correctness.

**Binary disposal is out of scope**, and is now recorded as
`SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` (P2, M). The retention it describes already
exists at HEAD — `serialize_to_binary` has always cached its bytes and nothing has ever released
them — and this design does not enlarge the retained set, since the assets it serializes at
finalization are the same ones that persist today. What changes is that the retention becomes a
deliberate part of the design rather than a side effect of one function, which is why it is worth a
policy rather than silence.

### Delegation carries the version across the hand-off (2026-09-05)

> "The delegation should end up with an identical asset (perhaps different in metadata — whether it
> knows the key) — both delegating and delegated asset must have the same version."

This answers what would otherwise have been a Phase 2 question about the delegated case, and it
answers it the strong way: the delegating asset does **not** compute its own version. It takes the
delegate's, transferred across the hand-off, so the two are equal by construction rather than by
two hashes agreeing.

The rule is required for correctness, not only for tidiness. Both assets resolve to the **same
key**, which is the same dependency-graph node — that is the established hand-off rule
(`DEPENDENCIES_STATUS.md`, "Delegation is a hand-off, not a dependency"). A parent depending on the
delegating asset records `DependencyRecord { key, version }` with the key taken from that shared
node and the version read out of the *delegating* asset's live metadata
(`record_dependency_on_asset`). The dependency manager, meanwhile, holds the version registered by
the **delegate**, because `track_asset` uses `bound_owner_key`, which returns `None` for a keyed
non-owner — so the delegating asset registers nothing. Two ways to get this wrong, both live:

- the delegating asset carries no version, and the parent records `Version::unknown()` — the
  original defect, reintroduced through the delegation path;
- the delegating asset carries an *independently computed* version that differs by a byte, and the
  parent records a version the manager will never hold, so `add_dependency`'s consistency check
  reads it as staleness and expires a parent that is perfectly fresh.

Only equality avoids both. Since `wait_for_dependency` already returns the delegate's `State`, the
delegate's version is in hand at the delegation site; today the hand-off deliberately discards the
owner's metadata record ("A hand-off transfers the value, not the owner's metadata record",
`assets.rs:2412`). The version becomes the one field that must cross it. Whether the delegate's
*serialized bytes* cross too is a Phase 2 optimization, not a requirement — the delegating asset
never persists.

Two invariants Phase 2 states rather than assumes:

- **A delegated evaluation still must not persist.** The `is_keyed && !delegated` guard already
  enforces it; it now has a second reason. The hand-off transfers no dependency records, so a
  persisted delegating asset would put a record in the store claiming a real version with an empty
  dependency list — which `try_fast_track` would later read as "nothing to check" and serve as
  fresh.
- **A delegating asset does not serialize.** It takes a version instead of computing one, so the
  finalization serialization is skipped for it — which also disposes of the redundant-work concern
  raised when the one-serialization decision was made.

Consequences carried into Phase 2:
- Serializing at finalization requires the ungated read. `serialize_to_binary` calls `poll_state`,
  which returns `None` at statuses this path must still handle — this is
  `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE`, already filed, and the same correction the blocked
  `stale-dependency-status-finalization` design needs as its C1.
- `evaluate` installs a value without clearing a stale `lock.binary`
  (`EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY`, already filed). Latent today; with finalization writing
  the cache on the same path, Phase 2 states the invariant explicitly rather than leaving it to
  luck.
- "Available before `track_asset`" is stronger than "assigned before `track_asset`": the dependency
  manager's `versions` map is what `add_dependency` consults, and it is written by `track_asset` at
  the very end of `evaluate`. Whether the metadata assignment alone is enough, or the manager
  registration must move earlier with it, is Phase 2's to settle — it is the same window that open
  question 2 below is about.
- The fallback for a non-serializable keyed asset (open question 3) is bound by the same rule: a
  timestamp taken at finalization is final for that evaluation, whereas one taken at creation is
  not.

A third possibility this rule admits and Phase 2 should record as considered: a version derived
from the recipe and the dependency versions would be knowable *before* evaluation and would be
stable. It is rejected here as a different versioning model — it would make a version describe how
a value was produced rather than what it is, losing the "recomputed to identical bytes does not
invalidate dependents" property that is the reason for hashing.

## Open Questions

1. *(decided — see "Owner decisions" above)*
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
