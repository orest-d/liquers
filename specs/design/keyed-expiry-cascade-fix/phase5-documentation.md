---
title: "Phase 5: Documentation — Versions for computed keyed assets"
kind: design
audience: internal
area: [core/assets]
---
# Phase 5: Documentation

## Completion Preconditions

- Implementation complete and validated: the whole `liquers-core` suite is green (26 test binaries,
  zero failures), `liquers-py` compiles, and `liquers-core` compiles for
  `wasm32-unknown-unknown` — which was the one claim earlier phases recorded as unproven.
- All review findings answered; the corrections they produced are in Phase 2 Revisions 2.1–2.6.
- `cargo test -p liquers-lib --lib --tests` **cannot be run here**, blocked by
  `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` (rustc 1.94.1 against dependencies requiring 1.95). The
  `--no-default-features` loop runs green (221 lib tests plus integration targets); that issue now
  records the workaround.

## Implementation Summary

**What was asked:** make expiring a computed keyed asset invalidate the keyed assets that depend on
it.

**What was found first, and it changed the work:** the problem statement in the issue, in Phase 1
and in Phase 2 was wrong, and only measurement caught it. Invalidation *did* reach direct
dependents — through the weak-reference route `expire_internal` collects outside the version guard.
What never happened was the second hop. So a two-asset test passes either way, which is why 34
expiration tests were green over a P1 defect, and why the regression fixture has three links.

**What shipped**, in the order it landed:

| | Change |
|---|---|
| A | `Version::from_time_now`/`new_unique` take wall time from `chrono` — `SystemTime::now()` is not a supported clock on `wasm32-unknown-unknown`, and `liquers-web` is wasm32-only. `serialize_to_binary` reads through the ungated accessor. |
| B | Edges carry the version the dependent observed. `expire_internal` splits into seeding and one shared traversal. `add_dependency` records without verifying. `register_version` expires only what a change provably affects; `report_no_version` is its audit-flow companion. `missing_versions` reports gaps, synchronously. `AssetManager::version` is the authority. Dependency records carry the dependency's settled version, and plan records stop hard-coding zero. |
| C | Computed keyed assets get a content version, assigned in the same write transaction as their status. `ValueOrigin` replaces `delegated: bool` and carries the delegate's version across a hand-off. |
| D | `version_for_tracking` is the last-resort net; `trigger_dependency_audit*` are the verification seam, defaulting to never. |

**The measured outcome:**

```
before: expire(a) -> a=Expired b=Expired c=Ready
after:  expire(a) -> a=Expired b=Expired c=Expired
```

## Deviations from the approved design

1. **`add_dependency_fails_stale_version` was replaced, and Phase 2 said it would survive.** That
   was true under Revision 2, which kept an inline check behind a resolver; the record/verify split
   in Revision 2.2 removed the check entirely. Phase 2's "Existing Tests This Changes" table was
   right about six of seven rows.
2. **`trigger_dependency_audit` takes no depth parameter.** Phase 4 specified `AuditDepth`. It has
   no purpose once the audit stopped traversing: transitive behaviour is the existing cascade,
   reached by pushing versions back in. Dropping it removed a knob with one possible value.
3. **The shared-store fixture and I8/I9 were not built.** They test cross-process reload, which
   needs two environments over one store, and `AsyncMemoryStore` is neither `Clone` nor shareable.
   The audit tests that *did* land cover the mechanism they were meant to exercise
   (`nothing_audits_by_default`, `metadata_kept_data_deleted_still_verifies_clean`); what remains
   untested is specifically reload. **Filed as `CROSS-PROCESS-RELOAD-IS-UNTESTED`.**

## Documentation Delivered

| Path | Change |
|---|---|
| `reference/DEPENDENCIES_STATUS.md` | Extended: the version contract, that a version is a fact about metadata and never the value, that `add_dependency` no longer verifies and the graph does no I/O, the opt-in audit, per-edge expiry precision and its invariant, `Version(0)` meaning only "unknown", and the caller-trusted expectation. Flow A step 3 and Flow B step 4 corrected. `## History` row added, `reviewed:` bumped. |
| `issues/…` (6) | Closed with evidence: the leading issue plus `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE`, `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY`, `DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES`, `PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO`, `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE`. |
| `issues/BUILD-SYSINFO-REQUIRES-NEWER-RUSTC.md` | Records the `--no-default-features` workaround, found while needing a `liquers-lib` run. |
| `design/stale-dependency-status-finalization/DESIGN.md` | Blocker discharged; its C2 decision is now revisitable. |
| `README.md` | Capability line advanced. |

`ASSETS.md` and `ASSET_LIFECYCLE.md` were reviewed and need no change: neither describes version
assignment or the dependency-verification contract, which is where every change landed.

## Issues Filed

| Issue | P | Why |
|---|---|---|
| `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE` | P2 feature | The policy vocabulary. This design built the seam, not the policy. |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | P2 | Scoped out by the owner. Pre-existing; this design makes the retention deliberate rather than incidental. |
| `CROSS-PROCESS-RELOAD-IS-UNTESTED` | P2 | Deviation 3 above. |

## Important Learning

1. **Measure before writing the sentence down.** Three documents and two reviewers repeated "no
   keyed dependent is ever reached" because it was plausible and the code reads that way. A
   ten-minute probe disproved it. `assets.rs` is 9,500 lines with two overlapping invalidation
   routes; reading one of them is not knowing what happens.
2. **A probe answers only the question you point it at.** The first probe printed versions, bytes
   and statuses — and not `metadata.get_dependencies()`, which is exactly where the *next* wrong
   premise lived. Every recorded version was zero, invalidating two phases of cold-start reasoning.
3. **A test suite can be careful, cross-checked, and about the wrong mechanism.** U1–U4 and U9 were
   reviewed twice; neither pass asked whether the thing under test should exist, because each review
   was scoped to conformity with the phase above it and the wrong premise was three phases up.
4. **Composing an existing primitive N times is not reusing it.** `expire()` per differing dependent
   looked conservative; each call carries its own `visited` set, so a shared descendant is expired
   twice. Verified by temporarily implementing the wrong version and watching the test fail.
5. **A test that cannot fail is not evidence.** Both discriminating tests here were checked by
   reverting the fix and confirming they go red.
6. **The best simplification came from the owner inverting the direction of control.** Every
   complication in the resolver design — the trait, the `&dyn` discipline, the leak rule no test can
   enforce, the supertrait, the `maybe_send` shape — existed to make a low-level structure safely
   call the layer above it. Having it *report* instead deleted all of them.

## Conformance and Remaining Work

The requested scope is delivered and tested. Remaining, all recorded: the audit policy vocabulary,
cross-process reload testing, binary disposal, and — for whoever picks it up —
`stale-dependency-status-finalization`, now unblocked.

## Validation

```
cargo test -p liquers-core                              # 26 binaries, 0 failures
cargo check -p liquers-core --target wasm32-unknown-unknown   # clean
cargo check -p liquers-py                               # clean
cargo test -p liquers-lib --no-default-features --lib --tests # 221 + integration, 0 failures
```
