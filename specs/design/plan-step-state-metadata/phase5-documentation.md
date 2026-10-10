# Phase 5: Documentation - plan-step-state-metadata

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 steps 1-6 ticked, commits listed there)
- [x] All user comments are answered or incorporated (Decision 7 → `is_applied`)
- [x] All review comments are answered or incorporated (no PR review yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

A command's input state now describes **its input**. The cut plan is the reference: a predecessor
boundary (`Step::Evaluate`) hands on the state of its asset unchanged, and the expanded plan
approximates it. `interpreter::do_step_state` builds the state each step hands on — fetched state
for `Evaluate`/`GetAsset`/`GetAssetBinary`, stored metadata for `GetResource`, the input unchanged
for `Info`/`Warning`/`Error`/`SetCwd`/`Filename`, and for an action's result a copy of the asset's
record corrected to describe the prefix the action completes. `apply_plan` and `do_step` keep their
signatures; `value_origin_key` and `fetched_key` are gone.

Supporting changes, all in `liquers-core`:

- `Step::Action::query` records the prefix query each action completes, promoted and frozen like
  `Plan::predecessor`; cleared on the action a recipe override patches.
- `Plan::cut_predecessor` declines a boundary over a bare key read, so
  `-R/data/x.manifest.yaml/-/ns-rec/materialize` stays `GetAsset, Action`.
- `Recipe::data_format` is `Option<String>`: an unnamed query declares no format instead of `bin`.
- `is_applied` on `MetadataRecord` / `AssetInfo` marks an asset built by applying a plan to an
  input state, and with it every step state of that plan.

Conforms to the approved design (Phases 1-4, Decision 7 as decided). Two deviations, both
narrower than approved: Decision 6 cleared prefix queries for *every* action of a recipe with
overrides; overrides patch only the last action, so only its query is cleared and earlier prefixes
stay usable. And `GetResource` with legacy stored metadata hands on the key through `with_key`,
which also seeds the format from the key's extension (`legacy_stored_metadata_degrades_to_a_warning`).

## Documentation Delivered

### New Reference Documents
None. The ownership model is a section of the existing context reference, which owns the contract.

### New Guide Documents
None. One section added to the command guide.

### Existing Documents Reviewed or Updated
`affects_docs`, each with a 2026-10-10 History row and `reviewed:` bump:
- `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` — new §Metadata ownership during
  evaluation (roles, reference rule, per-step table, `is_applied`); §Context lifetime rewritten.
- `reference/api/DOC_08_RECIPES_PLANS.md` — recipe `data_format`, override clears the patched
  action's query, bare-key-read exception to cutting, `Step::Action::query`, execution paragraph.
- `reference/VALUE_TYPE_SYSTEM.md` — §Seeding: an unnamed query seeds no format.
- `guides/COMMAND_REGISTRATION_GUIDE.md` — new §1 *Reading the input's description*.

### Links and Capability Map
`specs/README.md` §Query and evaluation: the capability line now points to DOC-04's section, with
the design in parentheses.

## Issues Filed

None new. Closed: `CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA`,
`FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`. Updated: `CORE-PLAN-POLICY-AND-DEFAULTS` (new cut
policy) and `CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY` (the predecessor's title now reaches
the next command's input, not the final asset; its design's change site moved to
`do_step_state`).

## Important Learning

- The question "which metadata is authoritative" had a clean answer in the code (the asset); the
  defect was that intermediate states were built from it. Separating *the record of what is being
  built* from *the description of a value* resolved the issue and three related symptoms (fetched
  formats lost, output filename labelling the input, the `bin` default) with one rule.
- The cut plan is a better reference than the expanded plan: most evaluations use it, and its
  boundary hands on a real asset's state.
- A bare-key-read boundary existed only to be worked around (`value_origin_key`); removing it
  removed the workaround.
- `liquers-core`'s `Value` cannot load a stored entry declared `csv`; core tests use `txt`.

## Conformance and Remaining Work

Requested: explain metadata roles, root cause, solutions (done in Phase 1, now DOC-04). Approved:
AC-1..AC-12. Implemented: all twelve, each cited by a passing test (Phase 3 table). Nothing
remains.

## Validation

- `cargo test -p liquers-core --lib --tests`: 1508 passed, 0 failed.
- `cargo test -p liquers-lib --lib --tests`: 560 passed, 0 failed (new tests 22-24 confirmed by
  name; `registry_export` unchanged — no command signature or body changed).
- `cargo test -p liquers-lib --no-default-features --features records --lib --tests`: 439 passed.
- `bash scripts/check-build-matrix.sh`: every native row passed; the seven wasm32 rows failed only
  because the target was not installed, and all seven passed after
  `rustup target add wasm32-unknown-unknown`.
- `python3 scripts/docs_index.py --check`: 0 errors.
- `validate_phase.py` 1-4: pass (Phase 1 over the size guideline).
