# Phase 5: Documentation - stale-dependency-status-finalization

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

An asset whose evaluation consumed a stale dependency was written to the store as `Ready` and only
then relabelled `Expired` in memory. The runtime and the store disagreed, and the disagreement was
invisible until a process boundary: `try_fast_track`'s recorded-version guard is vacuous in a fresh
process, which holds no versions to compare against, so a restart served the stale value without
recomputing it.

The fix moves the rule into the status authority. `AssetRef::finalize_status_with_version` now
decides `Expired` for such an evaluation, under the same write lock that installs the version and
**before** the notification and the store write; the relabel in `finish_run_with_result` is deleted,
leaving exactly one reader of the `stale_dependency` flag. Two consequences were designed rather
than inherited:

- **The version is still registered.** `DependencyManager::track_asset` refuses an `Expired` asset,
  so finalizing earlier would have left the graph asserting the key still holds its previous
  content. A stale-dependency *keyed* asset registers directly through the extracted
  `track_keyed_asset`; a delegating one registers nothing; a volatile one is not a node at all.
- **The reading half.** `try_fast_track` declines a stored asset when a recorded dependency is in a
  status it would not itself reuse — asking the manager first, the store only as a fallback — and
  treats a dependency it cannot address as inconclusive rather than expired.

This conforms to the approved design. Three deliberate differences from the plan: the
`try_to_set_ready` → `finalize_status` rename was not taken (Phase 2's approved decision, left
available, and now complicated by the sibling `finalize_status_with_version` the prerequisite work
added); test I2 became a unit test because the stale-dependency path cannot be driven end to end
from a command; and R2 shipped as the "dependency vanished" case rather than the "version moved"
case, because the planned form exposed a defect rather than failing as a test mistake. All three are
recorded below with issues where one is owed.

Current behaviour lives in [`reference/ASSETS.md`](../../reference/ASSETS.md) and
[`reference/ASSET_LIFECYCLE.md`](../../reference/ASSET_LIFECYCLE.md); the testing rules in
[`guides/UNITTEST_GUIDE.md`](../../guides/UNITTEST_GUIDE.md) §Testing Assets.

## Documentation Delivered

### New Reference Documents

None. The behaviour is an ordering rule and a reuse rule inside subsystems that already have
reference documents; a new one would have split the asset model across two files. Phases 1–2
planned no new reference document and that decision was reconsidered and kept.

### New Guide Documents

None, for the same reason: the testing material belongs in the existing unit-testing guide, whose
promotion condition `CROSS-PROCESS-RELOAD-IS-UNTESTED` set — a third recurrence — was met.

### Existing Documents Reviewed or Updated

`affects_docs` is authoritative and was widened from three to five during this phase. Every listed
document was reviewed against the shipped code and tests, not against the plan.

| Document | Change | `reviewed:` |
|---|---|---|
| `reference/ASSETS.md` | New §The one meaning of `Expired` — one meaning (the data is stale), two provenances, and why a distinct `Stale` status is not the answer. New §Who decides status — the manager is authoritative, every keyed expiry writes through to the store, ask the manager before the store, and two environments over one live store is not supported. §Status Descriptions no longer glosses `Expired` as "was ready but invalidated". | 2026-09-15 |
| `reference/ASSET_LIFECYCLE.md` | New §Reusing a stored asset: what the fast track verifies. Step 6 names the four outcomes of the status authority; the two rows both numbered 9 are merged into one. | 2026-09-15 |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | The dependency-status check is a numbered step of the fast-track sequence, with a note on why the two checks are independent. | 2026-09-15 |
| `guides/UNITTEST_GUIDE.md` | New §Testing Assets; `area` widened to include `core/assets`. | 2026-09-15 |
| `guides/STORE_IMPLEMENTATION_GUIDE.md` | §1 "A wrapper is not two methods". | 2026-09-15 |

Also outside `affects_docs`: four loose uses of "sidecar" in `assets.rs` and
`keyed_version_cascade.rs` became "stored metadata". `reference/STORE_SEMANTICS.md` §8 reserves the
word for a specific companion-key layout, which a memory store does not have.

### Links and Capability Map

`specs/README.md` gains **Status authority, and reuse across a restart** under *Assets and their
lifecycle*, pointing at `reference/ASSETS.md` §Who decides status and `reference/ASSET_LIFECYCLE.md`
§Reusing a stored asset, with the design folder as parenthetical provenance rather than as the
destination. `specs/index.csv` and `specs/index.md` regenerated.

## Issues Filed

| Issue | Why |
|---|---|
| `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST` (P2, M) | Reaching `wait_for_dependency`'s expired arm from a command needs the dependency to expire between being scheduled and being waited on. Scheduling evicts and recomputes an already-expired dependency, and one that is `Ready` when waited on returns immediately, so that window is a race rather than a state a test can arrange. |
| `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` (P2, S) | `audit_gaps` resolves a gap through `register_version`, which compares only against a version this manager previously held. A fresh process holds none, so the `Vacant` arm inserts and expires nothing — the cross-process case the audit exists for. The withdrawn R2 is its reproduction. |

`EXPIRY-RECORDS-NO-REASON` stays open deliberately. This design sets the ordering precedent and
records the recommended shape — a typed reason in metadata rather than a new status — but does not
implement it.

Closed with resolution notes: `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` (P1) and
`CROSS-PROCESS-RELOAD-IS-UNTESTED` (P2).

## Important Learning

**A status decided after the write is a status the store does not have.** The defect is entirely an
ordering one; the values were always correct. What it cost was agreement between two records of the
same fact, and that cost is only payable across a process boundary — which is exactly where nobody
was looking. The general form is worth carrying: when one component is authoritative and another
follows, the follower must be written *after* the decision and *before* anything can observe it.

**Where one direction of a mistake is invisible to the test suite, the code must lean the other way
and a test must assert the lean.** The fast-track dependency check fails open on a dependency it
cannot address. Failing closed instead would have produced results that are still correct, merely
recomputed — the entire suite stays green and the loss arrives months later as a complaint about
speed. Two tests exist only for that asymmetry: F0, the success baseline that did not exist (the
only test naming `try_fast_track` asserted `false`), and F3, the inconclusive case.

**The same shape appeared twice more.** R2 found that no test anywhere asserted an audit expiring
anything, and that it cannot in a fresh process. A path whose *success* is asserted nowhere is a
path that can quietly stop working.

**The manager is the synchronization mechanism; the store is not equipped for it.** This settled two
questions that looked unrelated: the fast-track check asks the manager before the store, and the
proposed shared-store test fixture was not built. Re-hydration — capture the persisted bytes, drop
the environment, replay into a fresh one — is both cheaper and more faithful, because a second
process does not share a live store object either.

**`Expired` means one thing.** The temptation to add a `Stale` status for "expired at birth" was
analysed and declined: it splits one meaning across two values, obliges roughly sixty-five `match`
sites to handle both identically, breaks the language bindings, and breaks store forward
compatibility. Provenance worth recording belongs beside the status, not inside it.

**Corrections made during the work**, kept because each was believed and acted on first: the claim
that `save_to_store` has no status gate (it reaches the gated `poll_state` through
`serialize_to_binary` — later resolved upstream); the claim that the dependency-manager step should
cascade rather than register (it reversed twice, each answer correct for the code as it then stood,
and only "register the version" is correct now that computed assets *have* versions); and the "two
meanings of `Expired`" framing, replaced by the sharper one that version and status answer different
questions — content identity versus freshness.

## Conformance and Remaining Work

| | Scope |
|---|---|
| Requested | Fix `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`: finalize the stale-dependency status before persistence, so the store and the runtime agree. |
| Approved | That, plus the dependency-graph decision (register the version directly), the fast-track verification added at the Phase 2 gate, and the `CROSS-PROCESS-RELOAD-IS-UNTESTED` fixture and its three tests. |
| Implemented | All of it. Tests U1–U8, I2, I3, I4, I7, I8, F0–F4, R1–R3. |

Nothing remains inside this design's scope. Three things it touched are represented by issues rather
than by a partial status: `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`,
`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` and `EXPIRY-RECORDS-NO-REASON`. The
`try_to_set_ready` → `finalize_status` rename remains an approved, untaken decision recorded in
`DESIGN.md`.

## Validation

- `cargo test -p liquers-core --test keyed_version_cascade` — 18 passed, including F0–F4 and R1–R3.
- `cargo test -p liquers-core --lib --tests` — 828 lib tests and every integration binary green.
- `liquers-lib` was checked with `--no-default-features`, the substitute
  `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` records for rustc 1.94.1, which cannot run the default loop.
  This is a substitution, not a clean run of the documented default.
- `liquers-core` compiles for `wasm32-unknown-unknown`; `liquers-py` checks.
- `python3 scripts/docs_index.py --check` — 305 documents, **0 errors**, 26 warnings, all
  pre-existing staleness warnings on documents this design does not touch.
- Both stash checkpoints were observed failing before passing. The keyed-persistence one was run
  against the original behaviour reconstructed exactly and failed at the *store* assertion while the
  in-memory assertion passed — the issue reproduced, then closed.
- `cargo fmt` is **not** run: `cargo fmt --check -p liquers-core` reports drift in some 130 places
  across files this work never touched, so the workspace does not enforce it and reformatting would
  bury the diff. New code follows the surrounding style.
- No rebase or merge conflict has occurred on this branch; no post-merge consistency review is owed
  yet. PR [#71](https://github.com/orest-d/liquers/pull/71) is green.
