---
id: EXPIRED-BINARY-READ-SAFETY
kind: design
title: Expired-safe binary reads
status: draft
phase: implementation
area: [core/assets, core/store]
gh_pr: []
issues: [ASSET-EXPIRED-CACHED-BINARY-READ]
created: 2026-08-08
superseded_by:
---
# expired-binary-read-safety Design Tracking

**Created:** 2026-08-08

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (approved)
- [x] Phase 3: Examples & Testing (approved)
- [ ] Phase 4: Implementation Plan (revised after cross-phase review; awaiting approval)
- [ ] Implementation Complete

## Notes

Filed from `ASSET-EXPIRED-CACHED-BINARY-READ` (P0, carried forward from the 2026-08-08 migration
triage with a "needs verification against PR #11" caveat). **Verified still live at HEAD** during
Phase 1: PR #11 gated `poll_state` and added `poll_state_any_status`, but left
`AssetData::poll_binary` status-blind. See Phase 1 §"Verification of the issue at HEAD".

**Phase 1 feedback (user):** every `get`/`poll` value-read method must have an analogous `*_binary`
counterpart. Recorded as the design's governing principle (Phase 1 §"Read-API symmetry"), which
widens scope from "add one status check" to "complete and align the binary read family" — five
methods added, four brought under the state contract — and closes the original open question 3.

**Phase 1 feedback (user), second round:** `Error`/`Cancelled`/`Directory` have no valid binary and
must report absence in whatever the signature allows — `None` for `Option` returns, `Err` for
`get_binary`, `Ok(None)` for the manager's `Result<Option<_>>`. Recorded as Phase 1 §"Statuses with
no valid binary"; closes original open question 2 and replaces today's accidental behaviour (which
depends on `State::as_bytes` checking `value_error` first, and so differs between `Error` and
`Cancelled`).

**Phase 1 feedback (user), third round:** (a) construct a suitable error where none is recorded —
`Error` reuses the asset's own failure, `Cancelled`/`Directory` get a purpose-built one; (b) expiry
is an error. (b) settles both the `get_binary` hard case (already-`Expired` returns `Err` rather
than falling through to `get()` and hanging) and the HTTP question (axum surfaces expiry as an
error response; it does not re-request from the manager). Recorded as Phase 1 §"Expiry is an error".
All four original open questions are now closed; the one live question is whether `AssetRef::get()`
owes itself the same pre-wait check — recommended yes, since it hangs identically today.

**Phase 2 multi-agent review.** Reviewer A (Phase 1 conformity) — clean: all eight rows of the
symmetry table accounted for, every Phase 1 decision honoured, `ReadExposure` judged an
implementation of the symmetry rule rather than scope creep. Reviewer B (codebase alignment) —
three findings, all verified against source and applied:
1. Axum catch-all arms are at `handlers.rs:109` (GET) and `:216` (POST), not `:107`.
2. `AssetManager` is reached via the associated type `Arc<E::AssetManager>`; there is no
   `dyn AssetManager` in the workspace, so the object-safety claim was misjustified (corrected —
   object safety is preserved but is not a binding constraint).
3. The "status is always `Value`-exposure at persist time" claim was too strong.
   `AssetRef::set_state` persists with whatever status the supplied state carries, reachable in
   production via `Context::set_state`. This upgrades `binary_unchecked` from a semantic nicety to
   a correctness requirement.
Reviewer B verified clean: signatures, the 15-variant `ReadExposure` table against `poll_state`,
the `has_data()` unsuitability claim, trait-default feasibility for both implementors, and that
`liquers-axum` is the only consumer needing changes.

**Phase 2 approved** by the user, who confirmed no commands are in scope — no `register_command!`,
no `specs/command_registry.yaml` regeneration, and no query-reachable recovery path.

**Phase 3 approved.** Examples are runnable tests (user's choice). Three reviewers ran; fixes
applied include one factual error of my own (`EnvRef::evaluate` does exist), two missing tests
(U9 `get` on `Expired`, U10 on-demand serialization), `Directory` completed in Example 3, and U2
made runnable without the nonexistent `Status::all()`. The untestable `liquers-axum` handler is
recorded as an explicit Phase 4 decision carrying a file-an-issue obligation.

**Open question 1 resolved at the Phase 3 gate (user, option A):** `AssetRef::get` **is** in scope
and gains the same pre-wait expiry check as `get_binary`. The alternative — binary reads erroring
while state reads hang on the same asset — would have shipped exactly the asymmetry this design
exists to remove.

## Cross-phase review: two blocking findings

The final review read all four phases together and found two problems that pairwise review could
not, both **verified against source**. Phase 4 is not approvable until they are decided.

**B1 — `Status::Expired` carries two different meanings, and the design collapses them.**
`finish_run_with_result` (`assets.rs:1618-1631`) relabels a *successfully completed* asset from
`Ready` to `Expired` when its evaluation consumed a stale dependency — deliberately, tested, and
reached on the normal path. Persistence has already run by then (`serialize_to_binary`, `:2017`),
so the asset holds cached bytes. So `Expired` + cached bytes is not only the bug configuration this
design targets; it is also the ordinary outcome of a query whose dependency expired mid-flight,
whose value is explicitly meant to be kept. Under this design the axum handler would return an
error for a computation that succeeded — and racily, since the 10 ms poll may observe either side
of the relabel. The two meanings are *"stale cached data, do not serve"* (`expire()`) and *"fresh
result, do not cache"* (the relabel). Phase 1 §"Expiry is an error" assumes only the first.

**B2 — a consumer inside `liquers-core` that Phase 2 declared did not exist.**
`Step::GetAssetBinary` (`interpreter.rs:293-299`) calls `AssetRef::get_binary` — a
query-language-level consumer, emitted by the plan builder (`plan.rs:1102`). Phase 2 §Integration
Points says "This is the only consumer change in the workspace", naming only `liquers-axum`. That
is false. Worse, it collides with `liquers-core`'s own execution-time staleness contract
(`wait_for_dependency`, `:4055`), which requires an execution-time expired dependency to remain
*usable* — the opposite of "expiry is an error".

Also recorded: `AssetManager::get_binary_any_status` cannot recover an expired asset whose value was
never serialized (`binary` is populated by two sites and cleared by ten), so the recovery API is
weaker than its state twin — leaving the originating issue's verification item 4 unclosed; and
`AssetRef::get` returning `Err` for a terminal-but-obtainable status contradicts `ASSETS.md`
§Terminal Outcome Contract, which reserves `Err` for delivery failures.

### Resolution (user, at the cross-phase gate)

**Option 2 — accept the regression.** Rationale, recorded because it is the load-bearing judgement
of the whole design: mid-flight dependency expiry ideally causes a restart or an error, but a
restart can loop unboundedly and an error can make an expensive result unachievable — which is why
the relabel rule exists at all. A user may well judge such a result acceptable. But that judgement
is theirs, so they must make it **explicitly**: `to_override()`, or a `*_any_status` read. Absent
that, the asset is simply expired and no different from any other expired asset; the only
distinction is that it was never `Ready`.

Revisions applied across all four phases:
- Phase 1 §"Expiry is an error" gains the uniformity decision and its rationale.
- Phase 2 §Backward Compatibility states the regression; §Integration Points now names
  `interpreter.rs`, `liquers-axum/src/assets/handlers.rs` and `liquers-web` (the last two are
  `get()` consumers that entered scope at the Phase 3 gate, after the original audit).
- **`get_binary_any_status` is no longer an alias**: it serializes on demand, because the sanctioned
  escape hatch must work for an expired asset that retains a value but was never serialized — the
  common case, and the one every Phase 3 setup was blind to. This also closes the originating
  issue's verification item 4. Its return type becomes `Result<Option<_>, Error>`.
- Phase 3 adds I5 (the regression, including that `to_override` recovers it), I6 (the two query
  steps agree) and U11 (recovery with no cached bytes).
- Phase 4 adds Step 7 for `interpreter.rs` and amends `ASSETS.md` §Terminal Outcome Contract, which
  option A contradicts.

**Filed separately:** `DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE` (P1) — `wait_for_dependency`'s
stale-value branch is dead code since PR #11 gated `poll_state`, the same defect class this design
fixes for `poll_binary`. Pre-existing and out of scope here.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
