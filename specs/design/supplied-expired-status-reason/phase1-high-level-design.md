# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — accept a supplied `Expired` with a reason, or refuse
  it.** This changes what `set_binary` / `set_state` accept, a public `AssetManager` contract.
- **Explanation:** Both answers are small and implementable. Phases 3–4 specify the recommended
  one: accept and record `Direct { Explicit }`.
- **Open questions:**
  1. **Proposed resolution — accept, record `Direct { cause: Explicit }`:** writing a value as
     `Expired` is a caller explicitly saying "this is stale", which is what `Explicit` means
     ("Someone asked"). It preserves the current accepted inputs, so no caller breaks, and it
     closes the invariant "every `Expired` carries a reason". Rejected: a new cause variant
     (`Supplied`). It would be a serialized enum addition for a distinction no consumer reads
     (reasons are "recorded, never consulted"). Rejected: refusing `Expired`. That breaks any
     caller that restores a stale record verbatim (e.g. copying an asset between stores), with
     nothing gained.
  2. **Implementation detail:** if the supplied metadata already carries an `expiry_reason`,
     keep it rather than overwrite. A copied record keeps its history.

## Problem

`set_binary` and `set_state` (both managers) keep a supplied `Status::Expired` and write it
without going through `AssetManager::record_expiry`. The stored record has `status: Expired` and
no `expiry_reason`, which breaks the invariant introduced by Step 4 of
`dependency-audit-and-expiry-provenance`.

## Expected behaviour and acceptance

1. `set_state(key, state)` with `state.metadata.status() == Expired` and no reason: the stored
   and live metadata have `expiry_reason() == Some(Direct { cause: Explicit })`, and a log entry
   from `record_expiry`.
2. The same for `set_binary`.
3. A supplied `Expired` that already has a reason keeps it unchanged.
4. Non-`Expired` inputs are unaffected.

## Scope

The four write sites. No dependent cascade: writing a stale value does not expire dependents
beyond what `publish_version` already does for the written version.

## Design Dependencies

- `immediate-set-state-status-match` — **requires**. This design changes the `Expired` row of its
  shared `written_status` site (or the code right after it). Recommended merge candidate: both
  touch the same four matches, and together they are one small PR.

## Documentation assessment

- Reference: `specs/reference/ASSETS.md` or `DEPENDENCIES_STATUS.md` (wherever the "every
  `Expired` carries a reason" invariant is stated): add "including a value written as
  `Expired`".
- No guide.

## Consolidated Findings

- `record_expiry` is a manager method `(&self, metadata: &mut Metadata, subject: &str, reason)`,
  so it can be applied to the metadata being written before the store write, with `subject =
  key.to_string()` (the keyed form of `expiry_subject`).
- Legacy metadata is left untouched by `record_expiry` by design. Acceptance case 1 applies to
  `MetadataRecord` only.
