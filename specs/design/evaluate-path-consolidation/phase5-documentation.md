---
id: EVALUATE-PATH-CONSOLIDATION-PHASE5
kind: design
title: "Phase 5: What was built, and the six issues it left behind"
status: draft
phase: documentation
area: [core/assets, core/plan, core/context]
created: 2026-09-04
---
# Phase 5: Documentation — Evaluation Path Consolidation

## Completion Preconditions

| Precondition | Status |
|---|---|
| Steps 1–7 implemented and committed | yes — seven commits |
| `cargo test -p liquers-core --lib --tests` green | yes — 1010 passed, 0 failed |
| Build matrix | clean except three pre-existing `liquers-lib --tests` rows (`BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`) |
| wasm | `liquers-core` and `liquers-web` compile for `wasm32-unknown-unknown`, and the `liquers-web` conformance suites **run green** — 146 tests |
| User and review comments answered | yes — three corrections recorded below |
| No new `TODO`/`FIXME`/`todo!()` introduced | yes |

## Implementation Summary

`AssetRef` had two independent evaluation bodies reached through four run entry points and six
manager entry points, diverging on delegation, payload admission, status finalization, persistence
and dependency recording. It now has **one private `evaluate(payload)`**, reached by two run
harnesses (spawning and spawn-free — a platform split, not duplication), behind **four** manager
entry-point implementations.

Delivered in seven commits, each independently revertable:

| Step | Change |
|---|---|
| 1 | The plan's payload requirement is recorded at `apply_plan`, beside the gate that already read it, and reaches `MetadataRecord` and `AssetInfo` |
| 2 | Each asset records whether it is a **keyed asset**, and which key, at construction |
| 3 | Only a keyed asset is written to the store; the write and invalidation targets are unified |
| 4 | One evaluation body; `run(payload)` / `run_inline(payload)` |
| 5 | `apply` absorbs `apply_immediately`; `Context::apply` loses its pre-check; the key/payload boundary is named |
| 6 | `InlineRunClaim` — execute-once on the inline path |
| 7 | Entry-point equivalence test, build matrix, wasm |

## Validation

1010 `liquers-core` tests pass. The build matrix is clean except three
`liquers-lib --tests` rows that fail on `rustc 1.94.1` versus an `egui` stack requiring 1.95 —
pre-existing, confirmed by checking out the dependency-upgrade commit directly, and recorded on
`BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`. `liquers-core` and `liquers-web` both compile for
`wasm32-unknown-unknown` — the check that no `tokio` primitive leaked into the shared body — and
the `liquers-web` conformance suites run green under Node (146 tests), which exercises the
consolidated body and `InlineRunClaim` on the target where the inline manager is the only manager.

Tests written to fail first, and observed failing before their step: the payload-requirement
recording (verified by disabling the projection and watching the exact assertion go red), both
`persist_apply_writes_nothing` scenarios, and the yielding-command execute-once test.

## Conformance and Remaining Work

### Conformance to the approved design

Conforms, with three corrections made during the work and recorded where they were made.

1. **The write predicate.** Phase 1 proposed `bound_owner_key()`. That returns `None` for a
   volatile keyed asset, which the manager deliberately never registers — so it would have silently
   stopped storing volatile results, which the project owner had explicitly asked to keep. Replaced
   by the **keyed-asset model**: the key is recorded at construction, `stored ⟹ keyed` and
   `persistent ⟹ stored`.
2. **No duplicated field.** Phase 1 sketched an `AssetData.payload_required` mirroring
   `is_volatile`. Rejected: `is_volatile`'s existing duplication is what forces two-source reads in
   `try_to_set_ready`. The requirement lives in metadata, read through an accessor.
3. **The entry-point equivalence test is a regression guard, not a fix-encoder.** Phase 3 marked it
   "fails today"; it does not, because `get_asset` and a payload-free `apply` already shared
   `evaluate_and_store`. The asymmetry was with the payload path. Corrected rather than left
   claiming an unearned win.

### What was omitted

- **`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`** — out of scope, and still open. The design states
  the ordering invariant and made the violation visible; the stale-dependency rule lives in the
  harness the consolidation keeps.
- **The registration contract** — `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`, filed at the owner's
  request. Ownership is approximated by keyedness meanwhile, with a metadata warning when a
  non-volatile keyed asset writes while not the registered owner.
- Nothing else. The `liquers-web` wasm test loop initially could not run for want of a
  `wasm-bindgen-test-runner` matching the lockfile's `wasm-bindgen 0.2.127`; installing it let the
  suites run, and they pass.

## Issues filed

Six, all linked to this design:

| Issue | P | What it records |
|---|---|---|
| `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` | P2 | **Closed here.** The fields existed and round-tripped; nothing ever set them |
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | P2 | Registration is a manager convenience nothing can rely on; blocks assets-as-a-channel |
| `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` | P2 | An asset using a stale dependency is stored `Ready`, then labelled `Expired` in memory only |
| `ASSET-FINISHED-PROGRESS-CONTRACT-UNDEFINED` | P3 | What progress a finished asset reports was decided by a race |
| `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` | P2 | `payload: required` works, is tested, and appears in no document |
| `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` | P2 | Updated: the breakage now reaches the library target and the default test loop |

## Important learning

**Duplication here is semantic, not textual.** The same question — *which key does this asset own?*
— was answered by two independent derivations **with opposite precedence**: `save_to_store` tried
`key()` then `store_to_key()`, `bound_owner_key` the reverse. Nobody chose that; it is what
independent local reasoning produces. They disagree for exactly the volatile keyed asset, so an
asset could be **written under a key it could never invalidate**. Latent, because
`mark_expired_status` refuses `Volatile` outright — one change away from mattering. Four reviewers
and the author missed it; it surfaced only by asking "does Step 3 cover *every* write site?".

**A test written to confirm an assumption is worth more than one written to confirm a change.**
Writing the test for "status is finalized before persistence" showed HEAD does not honour it.
Writing the execute-once test with a command that *yields* showed the existing test could never
have caught the gap, because its command never suspends.

**Compile errors are not test failures.** A test naming a field that does not exist yet cannot
"fail first" in any meaningful sense. The fail-first discipline applies to behaviour tests; for
new-API tests the equivalent is inverting the assertion once after implementing.

**Two `apply` calls are not a concurrency test.** Ad-hoc assets are unshared by construction, so
execute-once must converge two callers on one *mapped* asset. "One evaluation path" never meant
"every entry point is interchangeable" — they are thin in evaluation logic, not in construction.

## Documentation Delivered

| Question | Document |
|---|---|
| What the public surface is; how the surviving methods relate; the step-by-step flow; why flows differ | [`reference/ASSET_LIFECYCLE.md`](../../reference/ASSET_LIFECYCLE.md), rewritten |
| The high-level public API, at module level | the `//!` rustdoc of `liquers-core/src/assets.rs` |
| The API-level contract and the narrowed public surface | [`reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`](../../reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md) |
| What the duplication was, before it was removed | [`archive/2026-09-04-asset-lifecycle-duplication-audit.md`](../../archive/2026-09-04-asset-lifecycle-duplication-audit.md) |

`ASSET_LIFECYCLE.md` was **rewritten rather than extended**: its stated purpose was to catalogue
this duplication "as a basis for potential refactoring", so completing that refactoring left most
of its body false at HEAD, which a reference may not be.
