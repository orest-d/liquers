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
- [ ] Phase 4: Implementation Plan (in progress)
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

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
