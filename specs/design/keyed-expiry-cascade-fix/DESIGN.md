---
id: KEYED-EXPIRY-CASCADE-FIX
kind: design
title: Versions for computed keyed assets, so keyed expiry cascades
workflow: liquers-project
status: in_review
phase: architecture
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

- [x] Phase 1: High-Level Design (approved 2026-09-05)
- [x] Phase 2: Solution & Architecture (approved 2026-09-05)
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

   **Sequencing, decided the same day: the approximation ships and the new issue is follow-up.**
   Provisional registration is deferred verification, not absent verification — a persisted asset's
   version enters the manager the moment anything loads it (`try_fast_track` registers
   `metadata.version()` then replays the record's edges; `track_asset` does the same after an
   evaluation), and `register_version` compares it against the provisional entry and cascades on a
   difference. It is also continuous with what `add_dependency` already does for an unknown
   version. The one residual case — a dependent persisted against a dependency that was never
   persisted, so nothing ever loads it and the correction never arrives — is exactly the new
   issue's *absent → expire* row, and falls inside the owner's stated exception for an inconsistent
   persisted state. Phase 3 pins the accepted behaviour with a test so the follow-up has something
   to change.

### Not a duplicate

The completed `dependency-management` design already *intended* this: its Phase 4 Step 3 documents
`version` as "the content-hash version of the asset computed at save time
(`Version::from_bytes(content)`)", and the field carries that sentence as its doc comment in
`metadata.rs:938` today. The field and both constructors landed; the evaluate-path call never did.
This folder is the omission, not a second design of the same thing. No open design covers it —
`refresh-command-metadata-versions` (complete) is about `ns-dep/command_*` versions, which are
already real.

## Open questions at the Phase 1 gate

| # | State |
|---|---|
| 1 Assignment point | Decided — as early as it can be final, never provisional |
| 2 Unregistered dependency key | Decided — provisional registration, with `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` as follow-up |
| 3 Fallback clock | Open — `chrono::Utc::now()` (wasm-safe, used everywhere else) vs `SystemTime` (what `Version::from_time_now` uses, unsupported on `wasm32-unknown-unknown`). Not blocking |
| 4 `expire_internal` root guard | Open with a leading answer — keep `skip_cascade` as the zero-version policy mechanism, correct the comment. Not blocking |
| 5 `try_fast_track` version ordering | Open — largely absorbed by decision 2; Phase 2 confirms |
| 6 Does this discharge the `stale-dependency-status-finalization` blocker | Deferred to Phase 5 by design |

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
- **Open questions** — six as drafted; the owner settled 1 and 2 during the Phase 1 gate, and both
  are recorded as decisions with their reasoning rather than as questions. Nothing outstanding
  blocks Phase 2 — see the table above.

One checklist item is deliberately not met: the document exceeds 30 lines. The scope is a
three-line guard whose *consequences* span four mechanisms, and compressing the consequence list is
what let the previous design mis-scope this work.

## Issues this work has produced

| Issue | P | Cx | Relationship |
|---|---|---|---|
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | P2 | M | Filed 2026-09-05 at the owner's request when scoping the one-serialization decision. Pre-existing at HEAD; this design makes the retention deliberate rather than incidental, and explicitly does not fix it |
| `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` | P2 | — | Already filed. Serializing at finalization needs the ungated read; the same correction `stale-dependency-status-finalization` needs as its C1 |
| `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` | P2 | S | Already filed. Latent today; finalization writing the binary cache on the same path makes the invariant worth stating |
| `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` | P1 | M | Filed 2026-09-05 at the owner's request. **Follow-up, not a prerequisite** — this design ships provisional registration as the approximation, and that issue replaces it with real verification |

## Phase 2 review (2026-09-05)

Two reviewers in parallel, then the fix applied directly.

**Reviewer A (Phase 1 conformity) — no findings.** All four owner decisions are implemented, all
six Phase 1 questions are decided, carried to the gate, or deferred to Phase 5, the two invariants
Phase 1 asked Phase 2 to state are stated, and the documentation plan covers everything Phase 1
promised. No scope drift: the two absorbed issues were already named by Phase 1 as consequences.

**Reviewer B (codebase alignment) — one blocking finding, and every other claim verified.** It
checked thirteen factual claims against source and confirmed all of them, including the two that
carry load-bearing arguments: `load_command_versions_sync` really registers every command at
`start()` and skips `is_unknown()` versions, and `register_plan_dependencies` only records an edge
when `get_version` returns `Some` — which is what makes the command-key exception sound. It also
confirmed no entry guard is held across an `.await` in the proposed `add_dependency` change.

The blocking finding is a test the architecture invalidates and the document had not listed:
`add_dependency_fails_unregistered_dep` (`dependencies.rs:835`) registers `-R/a`, leaves `-R/b`
unregistered, and asserts the dependent expires. Those are asset keys, so under the provisional
rule `a` must **not** expire. Phase 2 now carries an "Existing Tests This Changes" table covering
it and six neighbours — including the one it pairs with, a new
`add_dependency_expires_on_unregistered_command_dep`, so the two branches cannot be collapsed
later. Its old name is worth noting: `..._fails_unregistered_dep` records a behaviour nobody chose,
which is how the conflation of "not loaded" with "changed" survived this long.

**No fixer agent was launched.** The workflow calls for one when reviewers surface issues; a single
finding whose fix was already fully understood did not warrant a cold agent re-deriving the
context, and the correction was applied directly. Reviewer B's two "advisory" items were the
absorbed issues restated as code changes, already in the document.

Also added after the review: the `wasmbind` evidence for the clock recommendation, and a section
recording the two in-code doc comments this change makes load-bearing and must correct
(`load_from_records`'s non-existent `DependencyVersionMismatch`, and `expire_internal`'s
non-existent root exemption).

## Phase 2 gate decisions (2026-09-05)

- **Clock: chrono, and the purpose is uniqueness rather than time.** Cross-platform confirmed —
  `chrono`'s default `wasmbind` routes `Utc::now()` through `js_sys::Date` on
  `wasm32-unknown-unknown`, and the repository already relies on it (every metadata timestamp and
  expiry comparison), while `std::time::SystemTime::now()` is the unsupported one. The reframing
  narrows the build: the clock's only job is separating *processes* — a counter alone restarts at
  zero and could re-issue a version another process handed out, which is the case the durability
  decision needs to expire — so `Version::new_unique()` is reimplemented on chrono, keeping its
  atomic counter, and the fallback path calls it rather than `from_time_now()`.
- **Scope addition accepted at the gate:** `from_time_now` also moves to chrono and the two
  existing non-serializable fallbacks (`set_state` on both managers) call `new_unique()`, removing
  `SystemTime::now()` from `liquers-core` and closing a reachable wasm hazard rather than fixing
  the new path and leaving the old one. Three lines.
- **Test surface: no new value type, no new code.** `Value::as_bytes` already refuses an integer
  for the `bin` data format and accepts a string, and `data_format` is seeded from the key's
  extension — so `count.bin` returning `I32` is the non-serializable case and `greeting.txt`
  returning `Text` the serializable control, differing in one character of a filename.
- **`expire_internal` root guard: delete the vacuous condition** — confirmed. The comment's claimed
  root exemption is deleted rather than implemented, because an asset opted out of version-based
  invalidation should stay out even as the root of an explicit expiry.
- **Not verified by compilation:** the wasm claims are reasoning, not evidence — the target is not
  installed here. Phase 4 adds it and runs the existing build-matrix wasm rows.

**Phase 2 approved 2026-09-05.** No open question carried into Phase 3.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
