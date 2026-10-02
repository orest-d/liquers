---
id: DEPENDENCY-AUDIT-AND-EXPIRY-PROVENANCE
kind: design
title: Dependency audit correctness, audit policy, expiry provenance, outside-change detection and external asset managers
workflow: liquers-project
phase: documentation
area: [core/assets]
gh_pr: [75]
affects_docs: [DEPENDENCIES_STATUS, ASSETS, ASSET_LIFECYCLE, DOC_03_ASSETS_EXECUTION_LIFECYCLE, DOC_04_ENVIRONMENT_CONTEXT_EVALUATION, ENVIRONMENT_CONFIG, COMMAND_REGISTRATION_GUIDE, ENVIRONMENT_CONSTRUCTION_GUIDE, STORE_IMPLEMENTATION_GUIDE, UNITTEST_GUIDE]
issues: [AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION, DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE, EXPIRY-RECORDS-NO-REASON, DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED, STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST, IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES, ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE, STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS]
created: 2026-09-28
superseded_by:
---
# dependency-audit-and-expiry-provenance Design Tracking

**Created:** 2026-09-28

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-09-28)
- [x] Phase 2: Solution & Architecture (approved 2026-09-29)
- [x] Phase 3: Examples & Testing (approved 2026-10-02)
- [x] Phase 4: Implementation Plan (approved 2026-10-02)
- [ ] Phase 5: Documentation
- [x] Implementation Complete (2026-10-02, orest-d/liquers#75)

## Notes

Groups five open `core/assets` issues around dependency verification and expiry. Three of them
(`EXPIRY-RECORDS-NO-REASON`, `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`,
`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`) were handed off explicitly by
`stale-dependency-status-finalization`. `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`
is a candidate sixth (Phase 1 open question 5).

**Phase 1 drafted 2026-09-28.** Cited code sites were checked against HEAD before drafting.

**Phase 2 drafted 2026-09-28.** All five Phase 1 questions are settled: membership-only
listing versions; a per-environment `DependencyAuditPolicy { Explicit, OnLoad }`; the reachability
test through a public `Context::schedule_dependency`; log levels per reason; the uncached race
excluded. It found and filed `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES` (the immediate
manager's deadline check compares status with status) and proposes it for this design's scope. The
multi-agent review could not run because of a spend limit, so both review passes were done inline
(see Phase 2 §"Phase 2 review").

**Phase 2 gate, first round (2026-09-28).** Part E was revised to `Context::submit` plus a public
`Context::wait_for_dependency`, with no new handle type, as the owner asked. The immediate-manager
deadline bug was accepted into scope. `#[non_exhaustive]` is applied to `AuditReport` and
`AuditFinding`, with public constructors, so a future asset manager outside core can still build
them. That check found the `AssetManager` trait is sealed today, filed as
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE`.

**Phase 2 gate, second round (2026-09-28).** The owner brought
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` into scope and said
`DependencyManagerAccess` may be public. That became Part F. The graph type is made public but
opaque, with its methods narrowed to `pub(crate)`. Five lifecycle primitives are made public with
documented contracts. `refresh_command_versions` gets a default body. A from-scratch external
manager in `tests/` runs the shared manager scenarios. This widens the design beyond the
Phase 1 scope, and that is recorded here rather than by editing the approved Phase 1.

**Workflow switched to `liquers-project` (2026-09-29)**, at the owner's request, while Phase 2 was
awaiting approval. The design was started under `liquers-designer`. The switch added what the
new workflow requires and the old one did not: a Documentation Intent section in Phase 1 (added
after that phase's approval and marked as such), a Known-Issue Preflight and a Documentation
Architecture in Phase 2, `affects_docs`, and a mandatory Phase 5. The empty Phase 3 and 4 templates
were replaced with the `liquers-project` templates. Nothing already written was changed in meaning.

**Phase 2 gate, third round (2026-09-29).** A glossary and worked examples were added to Phase 1,
with code-level traces in Phase 2. `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` was brought
in as Part G, following the owner's solution: content-hash versions flagged by bit 127, verified on
read and on demand; a `Source` or `Override` always taken as input; a recipe-backed value converted to
`Override` by default, or deleted under the opt-in `Corrupted` policy. Only content hashes are
flagged, so command versions do not change on upgrade.

**Phase 3 drafted 2026-09-29.** Conceptual code, by the owner's choice. Five parallel drafters
(two examples, pitfalls, unit and integration test plans) were merged by a synthesizer, which
corrected the drafts against Phase 2. In particular, an audit *expires* edges recorded as unknown,
because those are the attribution edges of results reached through intermediate queries. The
synthesis found four gaps in Phase 2, which were settled and recorded there: policy accessors,
`external_change_action` returning `Option`, `on_load` treating a recorded unknown as compatible,
and direct-vs-transitive audit reasons. Three reviewers found nothing blocking. They raised three
fixes, all applied: `VersionVerification` was not declared in Phase 2; the decision-table tests
ignored the `Option`; and problem 6 had no example, now pitfall 11. The reviewer that ran the code
check validated all five example queries with `liquers-validate`.

**Revision 2 (2026-10-02).** `main` was merged (the `store-conformance-backlog` and
`axum-assets-endpoints` work, which rewrote much of `assets.rs`), and every `assets.rs` citation was
re-pointed. The owner corrected the version kinds (hash flag; any non-matching recorded version is
a mismatch), required 0 instead of `None` for current versions, made a mismatch on load bump the
version and default to an override, redesigned expiry reasons as `Direct` / `Cascaded` with root
cause, root key and `via`, written to every expired asset's log through one overridable
asset-manager method (`record_expiry`), and set the directory version to the hash of the ordered
listing. Phases 1 to 3 were updated. The reviews ran in full this time: Phase 2 with 2 reviewers,
which found 3 call sites missing from the cause table; Phase 3 with 3 reviewers, which found
nothing blocking, and one advisory test was added. The Phase 3 update also found 7
underspecifications in Phase 2, settled in its "Revision 2, clarifications" section. The open owner question was settled: a file with no metadata and no recipe is adopted in memory
only, with no sidecar; under a recipe a sidecar is written.

**Phases 2 and 3 approved 2026-10-02.**

**Phase 4 drafted and reviewed (2026-10-02).** Four parallel reviewers checked it against Phases 1,
2 and 3 and against the code. Three found nothing; the Phase 3 reviewer found that the plan's tests
used invented names, so most Phase 3 tests were unassigned. The final (opus) reviewer re-aligned
every one of the 136 Phase 3 tests to exactly one step and added a test assignment index. It also
fixed what would have failed in practice:
- the hash switch moved to Step 1, because later tests expect `from_content` versions;
- a new Step 3b moves the shared scenarios into `tests/common/` first;
- Step 5 depends on Step 4;
- a second sealing supertrait, `KeyMutationAccess`, must also be made public;
- the stale-dependency branch sets `Expired` directly, and its dependents expire through
  `track_keyed_asset`;
- two deadlock rules: `record_expiry` takes the manager and the subject from the lock guard, and
  outside changes are applied only after the asset's data lock is dropped;
- the `on_load` check must not use `Version::matches`;
- the file store writes a sidecar for a bare file on first read, so "no metadata" is defined as a
  stored `Source`/`None` with version 0;
- `expiry_reason` is cleared when the status leaves `Expired`.

The estimate was raised to 8–11 days. The log wording is fixed in Step 2, and Phases 2 and 3 now
match it. Three questions remained for the owner (Phase 4 §"Phase 4 review").

**Phase 4 approved 2026-10-02.** The owner agreed to all three:
- `KeyMutationAccess` becomes public, with the contract "one lock per manager, never taken while an
  asset's data lock is held";
- "no metadata" means a stored `Source`/`None` with version 0, kept in memory only; the file store's
  own sidecar-on-first-read stays out of scope (`STORE-NO-READ-ONLY-ADAPTER`);
- the log wording table of Step 2 is confirmed.

**Implementation started 2026-10-02** (Step 0): the eight issues are set to `in_progress`.

**Implementation evidence for Phase 5 (Steps 1–4).**
- **Step 1.** The toolchain had to be updated (rustc 1.94 to 1.99, because `egui` 0.36 needs
  1.95). Review caught `new_unique` masking bit 127 *before* the shift, so the bit was not cleared.
- **Step 2.** Review rewrote `log_entry` to remove `unwrap` and catch-all arms; the level is now a
  typed `LogEntryKind`.
- **Step 3.** `DependencyManager::expire(key)` lists the root itself in the expired set.
- **Step 4.** The root gets `Direct { cause }`, and an asset already `Expired` keeps its reason,
  because only a real transition records. The `enter_dependencies` log line now names keys, not ids.
- **Correction to Phase 3 Example 1's variant.** A recipe that reads its dependency through its
  plan (`-R/data/b.txt/-/upper`) also records that dependency's own dependency as a direct record.
  A cascade from `a` then reaches `report` directly, with `via == root`. A real second hop needs
  the command to read `b` itself (`ctx.get_dependency_state`), which is what the test fixture does.
- **Step 6.** Filed `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW` (5 links 0.15 s,
  20 links 6 s, 40 links 80 s; each link records every upstream link). Not caused by this design.
- **Step 8, deviation from Phase 2 §"Directory listing".** `-R-dir/data` is an `Evaluate` boundary,
  so `GetAssetDirectory` runs in its own query asset with no owner key, and the planned
  `context.add_dependency` / `dm.add_dependency(owner, …)` never reach the dependent. The step
  therefore also sets the listing version on its own asset's metadata. From there
  `wait_for_dependency_recording` copies it into the dependent's record, and `track_asset` /
  `load_from_records` upgrade the edge. The edge is added before `register_version`, so a
  re-evaluating index is not expired by its own registration. Refresh calls run after
  `key_mutation_lock` is dropped (for `set_binary` and `set_state`), and a `listdir` error is logged,
  not fatal.
- **Step 9, deviations from Phase 2 Part G.**
  - `apply_external_change` cascades through `audit_version`, not `register_version`. After a
    restart, the new hash is the first version the map sees, and `register_version` treats a first
    observation as "no change".
  - `try_fast_track` only *decides*. `AssetRef::fast_track` applies the change after the data lock
    is dropped, so `key_mutation_lock` is never taken under an asset lock.
  - Empty bytes under a *timestamp* version count as "no bytes", because `AsyncMemoryStore` answers
    a metadata-only entry with empty bytes. Filed as `MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES`.
  - The signature gained `actual`, because `Delete` carries no version.
  - Under `Off`, `verify_stored_versions` returns an empty report.
  - In the whole `liquers-lib` suite only raw-seeded files without metadata mismatched (adopted in
    memory, nothing written); every value Liquers stored verified.
  - Also filed: `ANY-STATUS-READ-MISSES-STORED-VALUE-OF-UNLOADED-LIVE-ASSET`.
- **Step 10, deviations from Phase 2 Part E.**
  - `submit` is not lazy on either manager: the queued manager starts the job at once, and the inline
    manager runs it to completion inside `submit`. Phase 2's "the inline manager runs it on the
    first wait" was wrong. Filed as `SUBMIT-IS-NOT-LAZY-ON-ANY-MANAGER`. The planned test
    `submit_does_not_run_on_inline_manager_until_waited` was replaced by two tests that pin the
    actual behaviour (a submitted dependency is recorded once, and waiting returns its state).
  - The trait-default `wait_for_dependency`, used by the inline manager, now applies the stale
    dependency policy, as the queued manager already did. Without it, `submit` + wait on an inline
    environment bypassed `OnLoad`.
  - Mutation check: reverting `submit` to an immediate `evaluate` fails the recording test.
- **Step 11, primitives beyond Phase 2 F2.** A from-scratch external manager
  (`tests/common/minimal_manager.rs`, public API only) passes all 22 shared scenario runs. It
  needed two more provided trait methods:
  - `AssetManager::publish_version`, because `DependencyManager::register_version` is now
    crate-private;
  - `AssetManager::is_volatile_query`, because `IsVolatile` is crate-private.
  Its limits: its `set_state` goes through bytes, and it cannot notify a replaced asset. These are
  filed as `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET`. Three `DependencyManager` methods that
  only in-crate tests use are now `#[cfg(test)]`.
- **Step 12.** Every native suite, `check-build-matrix.sh` (32 configurations) and
  `check-stubs.sh --build` passed. `liquers-web`'s wasm suite passed except
  `object06_every_enum_variant_roundtrips`, which fails on `main` too
  (`WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT`). The PR is orest-d/liquers#75.
- **Filed during Step 4:** `SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON` and
  `DEPENDENCY-FAILURE-ERRORS-NAME-ASSET-IDS`.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
