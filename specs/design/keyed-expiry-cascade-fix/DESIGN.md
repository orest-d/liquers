---
id: KEYED-EXPIRY-CASCADE-FIX
kind: design
title: Versions for computed keyed assets, so keyed expiry cascades
workflow: liquers-project
status: in_review
phase: architecture
area: [core/assets]
issues: [KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS, DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE, DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES, PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO]
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
- [x] Phase 3: Examples & Testing (approved 2026-09-05; **partly superseded by Phase 2 Revision 2**)
- [x] Phase 4: Implementation Plan (drafted; **returned to Phase 2** — Step 3 and Group B are rewritten by Revision 2)
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

## Phase 3 review (2026-09-05), and a correction to the problem statement

**The measurement matters more than the reviews.** Before writing the tests, a throwaway probe
built the three-link chain fixture and ran it against HEAD. It disproves a sentence the issue,
Phase 1 and Phase 2 all carry:

```
statuses AFTER expire(a) : a=Expired  b=Expired  c=Ready
after recompute, expire(b): b=Expired  c=Expired
```

Invalidation reaches **direct** dependents today and never propagates. One level per expiry, always
exactly one. A keyed asset sits in both maps — `Context::evaluate` calls `add_dependent_asset`
whenever the current asset is keyed — and `expire_internal` collects `dependent_assets` outside the
`skip_cascade` guard while traversing `keyed_dependents` inside it. So the weak-reference route
always fires once, and the graph route, the only one that enqueues a node, never runs.

Nothing about the fix changes. What changes is the test: **a two-asset test passes at HEAD**, which
is why 34 expiration tests are green over a P1 defect, and why the regression test needs three
links. The issue, Phase 1 and Phase 2 are corrected.

The probe also settled four facts the tests depend on, none of which were assumptions any more:
`a.txt` stores exactly `b"Hello"` (so I1's expected hash is known); a non-serializable keyed asset
ends `Ready` with `PersistenceStatus::NonSerializable` and **nothing in the store** — the empirical
confirmation of the durability decision's premise; a query asset carries no version; and
`evaluate()` returns an asset that may still be `Processing`, so `get()` must be awaited before
`status()` is read (this cost two probe runs, and is now pitfall P11).

**Reviewer 1 (Phase 1/2 conformity) — two blocking findings, both accepted.**
(a) Phase 2 explicitly asked Phase 3 to record why `version_consistent` and `add_dependency` now
disagree about an unregistered key, and Phase 3 had not. Added as U9, a single test asserting both
halves, because the asymmetry is the point and a reader who sees only one half "fixes" it.
(b) The P3 ordering constraint was left as "Phase 4 should" — it is now a stated Phase 4
precondition, since a code comment is the only artefact that survives a refactor for a constraint
no test can hold. Three advisories also accepted: the test count was wrong and is now derived from
the table; I1 cannot prove the single-serialization property because `Value::Text` encodes
deterministically, which is now said outright; and U5 asserts on the transitive node rather than on
a count, so it forces the root-guard change.

**Reviewer 2 (test realism) — no blocking findings.** It verified seventeen claims against source,
including the one the whole fixture rests on: `Recipe::store_to_key()` derives the target key from
the query's trailing filename, so `Recipe::new("-R/a.txt/-/world/b.txt", …)` really produces
`b.txt`. One over-read to note: it cited the probe as evidence that "expiry cascades", which is
what the probe's own output disproves at the second level — a reminder that a reviewer reading a
transcript is not a substitute for reading the numbers.

## Phase 4 review: one blocking finding, and the design needs a scope decision (2026-09-05)

Two reviewers passed the plan — conformity clean, executability clean, including the item flagged
as highest-risk (`track_asset`'s lock discipline is safe, because the existing `drop(lock)` at
`dependencies.rs:297` precedes the call site). Their advisories were applied: U3/U4 named in Step 3,
R1 in the final test row, and Step 5's placement rules made explicit.

**The holistic pass found something all four phase documents assumed and none had checked.**

### B1 — persisted `DependencyRecord.version` is always zero

Verified independently before acceptance, with a probe over this design's own fixture:

```
--- b.txt own version = None
      dep -R/a.txt                        version = 00000000000000000000000000000000
      dep ns-dep/command_impl---world     version = 00000000000000000000000000000000
stored b.txt deps:
      stored dep -R/a.txt                 version = 00000000000000000000000000000000
```

Two independent causes, both confirmed in source:

1. `Context::schedule_dependency_asset` (`context.rs:553`) reads the dependency's version from the
   manager **at schedule time** — before `get_dependency_asset` at `:582`, therefore before the
   dependency has evaluated, therefore before `track_asset` could have registered it. Nothing
   revisits the record afterwards. `record_dependency_on_asset` is the one function that reads a
   dependency's live metadata version, and its only non-test caller is the delegation branch
   (`assets.rs:2407`), where it returns immediately as a same-node hand-off.
2. `finalize_plan` (`interpreter.rs:71`) writes plan dependency records with a literal
   `Version::new(0)`, although `register_plan_dependencies` looks up the real command versions a
   few lines below.

Filed as `DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES` (P1, M) and
`PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO` (P2, S).

**What survives and what does not.** The core fix is unaffected: graph edges are registered by
`register_scheduled_dependency` regardless of version, `track_asset` will register a real version
for the dependency, `skip_cascade` becomes false, and I2's `c` assertion flips. **In-process
transitive cascade works.**

Everything built on *recorded* versions does not. Because `Version(0)` short-circuits every
comparison, `add_dependency`'s check is never reached from a production caller, so Step 3's
provisional rule and its command-key exception implement a policy nothing can trigger; I9 cannot
pass as written; and Phase 1's central cold-start risk — "turning versions on would expire
persisted dependents on first load" — is not real, because the recorded versions are zero.

### Other findings from the same pass, accepted

- **A1 (architecture-level).** The version window is not closed by placing `assign_version` after
  `try_to_set_ready`. `try_to_set_ready` sets `Ready` with `data` already present, so `poll_state`
  returns `Some` the instant its write lock drops, and both `AssetRef::get` and
  `wait_for_dependency` re-poll at the top of their loops on *any* wake-up, not only on
  `ValueProduced`. A delegate polled inside that window yields
  `ValueOrigin::Delegated { version: None }` — Phase 1's first named delegation failure mode.
  The fix is to serialize *before* the status transaction and install bytes, version and status
  together under one write lock, which also matches the "published once" rule more exactly.
- **A2.** The net's log entry must be written as `lock.metadata.add_log_entry(...)` under the
  asset's own write lock, never through the service channel — `AssetServiceMessage::LogMessage`
  calls `save_metadata_to_store` (`assets.rs:2060`), which would persist the fallback version and
  contradict both the "does not re-persist" decision and I4's assertion that the store holds
  nothing.
- **A3.** Step 1's validation gate (`grep SystemTime::now`) has five pre-existing hits in
  `store.rs` test modules; scope it or an agent will "fix" unrelated code.
- **A5.** A key registered while non-volatile and later resolved volatile keeps its `versions`
  entry — `DependencyManager::remove` has no production caller. Pre-existing; matters because
  Phase 5 was about to publish the exclusion as contract.
- **A6.** `set_state` on a key already evaluated in-process does not expire first, so it will now
  cascade where it could not before. Correct behaviour, but new, and it is the one path where the
  evaluate-path hash and the `set_state` hash must agree. Wants a test.
- **A7, and it is fair.** The Phase 3 probe printed versions, bytes and statuses but not
  `metadata.get_dependencies()` — which is exactly where B1 lives. Phase 3's own learning ("assert
  against a running binary before writing the sentence down") applied to itself: the probe was run
  and the wrong fields were read.

### Decision: Option B, with a version authority (owner, 2026-09-05)

> "I am inclined to Option B — there is more context to work with those issues and it should be
> thus easier to reason about the correctness of the solutions. We probably need some authoritative
> way to obtain a version — perhaps a `version(key)` method on the asset manager? … This may
> eventually be passed as a closure to dependency manager … Limited time use would be desirable to
> prevent dependency manager to create yet another cyclic arc leak."

The design **returns to Phase 2**, which now carries `Revision 2 — the version authority`. Its
seven corrections (C1–C7) are the contract for re-approval.

The proposed architecture turns out to be a net *simplification* despite being a larger change: one
authority replaces one approximation, one special case, and one deferred issue.

- **C1 `AssetManager::version(key)`** — live asset, else store metadata, else `None`. Never
  evaluates, never submits, for the same reason `owned_key_asset` does not
  (`keyed-recipe-ownership`); `Ok(None)` and `Err` stay distinct so a transient store error cannot
  expire dependents.
- **C2 `VersionResolver`, `&dyn`, never stored.** The leak concern is well founded and the shape
  answers it structurally: `DependencyManager` is a *field* of the asset manager, so every caller
  can pass `self` as a borrow. No `Arc`, no field, no third cycle on top of the two
  `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` records.
- **C3** `add_dependency` asks the authority instead of guessing, giving exactly the three outcomes
  `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` defined — so that issue is **absorbed**,
  and provisional registration plus the command-key exception are **deleted**.
- **C4** the record carries the dependency's *post-evaluation* version, upserted in
  `Context::wait_for_dependency` (the single funnel where a dependency's `State` reaches the
  dependent), and `finalize_plan` stops hard-coding zero. **The upgrade transition is gentle:**
  every pre-existing record is zero, zero matches anything, so the first run against an existing
  store invalidates nothing.
- **C5** version assignment merges into the status transaction — the Phase 4 review showed the
  Revision 1 placement does not close the window, because both waiters re-poll on any wake-up.
  This turns Phase 3's untestable P3 constraint into a structural invariant.
- **C6** the fallback's log entry goes under the write lock, not through the service channel, which
  persists metadata.
- **C7** two pre-existing facts corrected so Phase 5 does not publish them as contract:
  `track_asset` is not the single funnel (four of five `register_version` sites are elsewhere, and
  `try_fast_track` never calls it), and `versions` retains an entry for a key that later becomes
  volatile.

One question is open for the re-gate: whether `version(key)` belongs on the public `AssetManager`
trait as a defaulted method (recommended) or only on the concrete managers.

## Phase 2 Revision 2 re-gate (2026-09-05)

Owner confirmed `version(key)` on the trait, defaulted. One reviewer over Revision 2; **two
compile-time blockers**, both verified independently before acceptance, both now answered as
Revision 2.1.

**D1 — the resolver could not reach three of its four callers.** Revision 2 claimed "every caller
is inside the asset manager, which passes `self`". False for the three that carry the load —
`evaluate → track_asset` (`assets.rs:2557`), `try_fast_track → load_from_records` (`:1116`) and
`record_dependency_on_asset → add_dependency` (`:1608`) — all generic code holding
`Arc<E::AssetManager>`, with no `VersionResolver` bound anywhere on `Environment::AssetManager`.
The fix is cheaper than the review supposed, and its stated objection does not apply: `AssetManager`
is **already** sealed by a `pub(crate)` supertrait (`DependencyManagerAccess<E>` under
`#[allow(private_bounds)]`, `:3446`/`:3470`), so adding `VersionResolver` beside it costs nothing
that is not already paid, and every generic call site then gets the coercion for free.

**D2 — `Send + Sync` would break wasm32.** `maybe_send.rs`'s own module doc states the convention
and every async trait in the crate follows it. `VersionResolver` takes the `#[cfg_attr]` pair and
`MaybeSend + MaybeSync`, so an `ImmediateAssetManager` holding `!Send` browser data still
implements it — which is precisely what `liquers-web` is.

**D3/D4, from the same pass.** The hard-coded zero is in `finalize_plan_expanded`, not
`finalize_plan`. `Context::wait_for_dependency` must take the `DependencyKey` as a parameter rather
than deriving one — a derived key differing from the schedule-time key would create a *second*
record instead of upgrading the first, silently, since the upsert matches on key equality. And
C3's safety turns out to rest on an unnamed convention: a concrete version reaches `add_dependency`
only for a dependency that had one, and every spurious-expiry candidate the reviewer traced
(volatile, non-keyed, command, non-serializable, concurrently evaluating) is safe *because* those
paths pass `Version::unknown()`. That invariant is now written down and belongs in the reference.

Confirmed clean: C1's default body, the `contains`-before-`get_metadata` ordering, and
`Context::add_dependency`'s upsert rule. One overclaim corrected — "keeps every implementor
compiling" is trivially true, since the two concrete managers are the only implementors in the
workspace.

**Phases 3 and 4 rebuilt on Revision 2**: seven tests removed (they tested the deleted provisional
mechanism), thirteen added — including the two direct regression tests for the newly filed record
defects, and `a_record_written_before_versions_existed_still_matches`, which pins the property that
makes this deployable against an existing store. Phase 4 is fifteen steps in the same four groups,
and its B-before-C ordering argument is now real rather than hypothetical.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
