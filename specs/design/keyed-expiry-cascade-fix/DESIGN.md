---
id: KEYED-EXPIRY-CASCADE-FIX
kind: design
title: Versions for computed keyed assets, so keyed expiry cascades
workflow: liquers-project
status: in_review
phase: high-level
area: [core/assets]
issues: [KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS]
gh_pr: []
affects_docs: [DEPENDENCIES_STATUS, ASSETS, ASSET_LIFECYCLE]
created: 2026-09-05
superseded_by:
---
# keyed-expiry-cascade-fix Design Tracking

**Created:** 2026-09-05

## Phase Status

- [x] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Designs the fix for `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS` (P1, L), under the project
owner's decision of 2026-09-05 recorded on that issue: **option 2 — computed assets get real
versions**, hashed from the bytes they serialize to, with a fallback for those that do not
serialize. It unblocks `stale-dependency-status-finalization`, which is parked on this gap.

### Verified live at HEAD before drafting Phase 1

- `MetadataRecord.version` is written in exactly four places, all hand-in paths:
  `DefaultAssetManager::set_binary` (`assets.rs:5136`), `set_state` (`:5244`), and the immediate
  manager's pair (`:6316`, `:6363`). `evaluate` (`:2507`), `try_to_set_ready` (`:1803`),
  `serialize_to_binary` (`:2697`) and `save_to_store` (`:2583`) never set it. The issue's line
  numbers (5203/5313/6379/6429) predate a later edit; the four sites are the same four.
- `track_asset` therefore registers `mr.version.unwrap_or(Version::new(0))` — unknown — for every
  computed keyed asset (`dependencies.rs:302`), and `expire_internal`'s `skip_cascade`
  (`:591-599`) is taken on the first iteration for all of them.
- The condition guarding that check, `include_root || current != *key` (`:592`), is **vacuously
  true on both call paths**: with `include_root` the root is the first queue entry and the
  left disjunct holds; without it the root is never enqueued and the right one does. Its comment
  claims an exemption "(except for the root key)" that the code does not implement.
- Baseline is green before any change: `liquers-core` 793 lib tests, `expiration_integration` 34,
  `dependency_manager_integration` 5, `dependency_scheduling` 4 — 0 failures
  (2026-09-05, `CARGO_INCREMENTAL=0`).

### Two consequences found while drafting, beyond the issue's list

Both are Phase 2 material and both are named as Phase 1 open questions rather than assumed away:

1. **Real versions make `add_dependency`'s consistency check reachable, and its answer for an
   unregistered dependency is "mismatch".** `version_consistent` returns `false` when the manager
   holds no version for the key (`dependencies.rs:215`), and `add_dependency` turns that into
   `expire(dependent)` (`:242`). Today a computed dependency's recorded version is unknown, so the
   check is skipped. With real versions, `load_from_records` on a cold start — from
   `try_fast_track` or `track_asset` — would expire a persisted dependent loaded before its
   dependency, which is the ordinary order. That would trade stale reads for a cold cross-process
   cache rather than fix anything. `load_from_records`'s own doc comment ("Ignores
   `DependencyVersionMismatch` errors") already misdescribes this: `add_dependency` returns
   `Ok(expired)`, not an error, so nothing is ignored.
2. **The assignment point is observable.** `record_dependency_on_asset` reads the version out of
   the child's *live metadata* (`assets.rs:1564`), and `try_to_set_ready` publishes `ValueProduced`
   before persistence and before `track_asset`. So a version set during persistence is invisible to
   a parent that has already read the child, and a version set at finalization is visible before
   the dependency manager knows it. Whichever is chosen has to be chosen deliberately.

   **Decided by the owner, 2026-09-05** (Phase 1 §"Owner decisions"): as early as possible, even
   before `track_asset`, subject to the version being **stable** — published once, never revised.
   That rules out a provisional timestamp at asset creation, which a later hash would overwrite,
   and it rules out assignment during persistence, which is after a parent can have read the child.
   The earliest final point for a hash is status finalization, once the value is installed and its
   bytes can be computed. Whether the dependency manager's `versions` registration must move
   earlier alongside the metadata assignment is left to Phase 2.

   **Mechanism confirmed by the owner, same day:** finalization serializes once, hashes those
   bytes, and leaves them in `AssetData::binary` for `save_to_store` to reuse — so the version and
   the stored bytes are the same bytes by construction, not two serializations trusted to agree.
   Only non-volatile keyed assets. Binary *disposal* is explicitly out of scope and is filed as
   `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` (P2, M); the retention it describes already
   exists at HEAD and this design does not enlarge the retained set.

   **Delegation, decided the same day:** a delegating asset takes the delegate's version rather
   than computing one, so the two are equal by construction. Required, not cosmetic — both resolve
   to the same key and therefore the same graph node, a parent reads the version from the
   *delegating* asset's metadata while the manager holds the one registered by the *delegate*
   (`track_asset` uses `bound_owner_key`, `None` for a keyed non-owner), so any inequality makes
   `add_dependency` expire a fresh parent. It also settles the redundant-serialization concern: a
   delegating asset does not serialize at all.

   **Nonzero-version guarantee, proposed by the owner and accepted with a limit:** a fallback at
   the start of tracking replaces `Version(0)` with a time-based version, so `track_asset` — the
   single funnel for a keyed asset's version — is a net under every way the primary path can fail.
   The limit answers the owner's own "when would that be a problem": `track_asset` runs *after*
   persistence, so a version invented there never reaches the store; the parent persists it in its
   `DependencyRecord`, and after a restart the child registers a different one or none, which
   expires the parent. It is therefore an **in-process guarantee, not a durable one**, and the
   fallback is layered: the time-based version is assigned at *finalization* (before persistence,
   so the store carries it) and the tracking-time net catches only the residue, recording in the
   metadata log that it fired so a silently-broken primary path cannot hide behind it.

   **`Version(0)` stays supported and gains a single meaning.** It currently conflates an accident
   ("nobody computed one") with a policy ("does not participate in version-based invalidation").
   Once every path assigns a version, a zero can only have arrived deliberately, so zero becomes
   the policy sentinel by construction — no new type, field or migration, and `matches`,
   `version_consistent`, `add_dependency` and `expire_internal`'s `skip_cascade` branch all keep
   working unchanged as its implementation. That is the reason to keep `skip_cascade` rather than
   delete it, and it simplifies open question 4 to "correct the comment".

   **Durability, decided the same day:** the net does **not** re-persist, and losing a
   fallback-versioned asset's dependents across a restart is the intended outcome. The owner's
   rule — an asset that is not durable and cannot be provably reconstructed with the same value
   should be effectively expired on restart — matches the path exactly: a non-serializable computed
   keyed asset leaves *no* durable trace, because the evaluate path's `save_to_store` propagates
   the `SerializationError` and writes nothing, not even metadata-only (unlike `set_state`, which
   falls back to `store.set_metadata`). Version persistence for such assets is future work if it is
   ever needed, recorded on the issue rather than built here.

   This **sharpens** open question 2 without answering it. "Expire when the version cannot be
   verified" is narrower than what the code does — "expire when the manager has not registered the
   dependency yet" — and they differ precisely in the durable case: `K` on disk carrying `v1`,
   dependent `D` recording `K@v1`, restart, `D` loaded first, `D` expired for a dependency that
   could have been confirmed. Phase 2's leading answer is a provisional registration of the
   recorded version on an unregistered edge, which yields the warm cache in the durable branch and
   the owner's required expiry in the non-durable one, from the existing `register_version`
   comparison and no new structure.

   **Durable versions, decided the same day:** they are correct, and the manager should consult
   them — by loading at startup or verifying dynamically. Scoped out as
   `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` (P1, M), since `DependencyManager` holds
   no store handle at all (five `scc` maps and a mutex; it is generic over `E` but holds no
   environment) and `AssetManager::start` loads command versions and nothing else. Open question 2
   becomes a *sequencing* judgement: this design ships an approximation, or the new issue is a
   prerequisite. Relaxing the check instead is not available — it fixes the durable branch and
   breaks the non-durable one, because fast-track serves a dependent without evaluating it, so the
   dependency is never consulted and the staleness never surfaces.

### Not a duplicate

The completed `dependency-management` design already *intended* this: its Phase 4 Step 3 documents
`version` as "the content-hash version of the asset computed at save time
(`Version::from_bytes(content)`)", and the field carries that sentence as its doc comment in
`metadata.rs:938` today. The field and both constructors landed; the evaluate-path call never did.
This folder is the omission, not a second design of the same thing. No open design covers it —
`refresh-command-metadata-versions` (complete) is about `ns-dep/command_*` versions, which are
already real.

## Phase 1 critical review (2026-09-05)

Against the Phase 1 checklist:

- **Scope clarity** — purpose states one defect and one cause. Interactions are enumerated by
  mechanism (four of them) rather than by file, so the blast radius is legible.
- **No duplication** — checked against `dependency-management`, `refresh-command-metadata-versions`
  and `stale-dependency-status-finalization`; recorded above.
- **Philosophy/layering** — `liquers-core` only, no API change, async paths unchanged.
- **Documentation needs** — all four questions answered with rationale: extend
  `DEPENDENCIES_STATUS.md` (it owns the statements this change falsifies), no guide, no other new
  documents, five specific updates listed.
- **Open questions** — six as drafted, none blocking. Question 1 (assignment point) was decided by
  the owner on 2026-09-05 and is now recorded as a decision rather than a question; questions 2 and
  3 are the two that still change what is built; 4 and 5 are decisions with a leading answer; 6 is
  deferred to Phase 5 by design.

One checklist item is deliberately not met: the document exceeds 30 lines. The scope is a
three-line guard whose *consequences* span four mechanisms, and compressing the consequence list is
what let the previous design mis-scope this work.

## Issues this work has produced

| Issue | P | Cx | Relationship |
|---|---|---|---|
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | P2 | M | Filed 2026-09-05 at the owner's request when scoping the one-serialization decision. Pre-existing at HEAD; this design makes the retention deliberate rather than incidental, and explicitly does not fix it |
| `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` | P2 | — | Already filed. Serializing at finalization needs the ungated read; the same correction `stale-dependency-status-finalization` needs as its C1 |
| `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` | P2 | S | Already filed. Latent today; finalization writing the binary cache on the same path makes the invariant worth stating |
| `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` | P1 | M | Filed 2026-09-05 at the owner's request. The capability the durable branch of open question 2 needs; may be a prerequisite rather than follow-up work |

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
