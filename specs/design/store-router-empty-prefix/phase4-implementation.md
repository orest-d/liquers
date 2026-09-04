# Phase 4: Implementation Plan - Empty File-Store Directories

## Overview

Make an absent, addressable file-store directory list as empty in both file-store variants, then
prove router enumeration no longer needs its artificial prefix-directory setup.

## Implementation Steps

| Step | Files and exact action | Validation | Agent specification / rollback |
|---|---|---|---|
| 1 | Read `AsyncFileStore::listdir`, `FileStore::listdir`, C3, and their callers. Confirm both methods still return `Error::key_not_found(key)` only after successful `try_exists == false`. | `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF c3_async_store_router` establishes the currently masked baseline. | Haiku; skills: rust-best-practices; context: Phase 2 and the four source locations. No edit, so no rollback. |
| 2 | In `liquers-core/src/store.rs`, replace only each absent-path return in the two `listdir` bodies with `Ok(vec![])`. Do not alter path validation, reserved-name filtering, or any `map_err` branch. | Focused unit tests from step 3. | Sonnet; skills: rust-best-practices; context: Phase 2 exact behavior and `STORE_SEMANTICS` §4. Revert the two return statements if the contract proof fails. |
| 3 | Add `filestore01` and `filestore02` beside the existing store unit tests. Amend C3 to omit creation of `root/files`; retain cleanup and the complete conformance report. | All four Phase 3 commands; C3 must run one test, never zero. | Sonnet; skills: rust-best-practices; context: Phase 3 and conformance fixture conventions. Revert only the named tests and C3 setup. |
| 4 | Update `STORE_SEMANTICS` §4 and `STORE_IMPLEMENTATION_GUIDE` with the approved contract and command. At Phase 5, add History rows and `reviewed: 2026-09-04` after checking the final code. | `py -3 scripts/docs_index.py --check` after regeneration. | Haiku; context: Phase 2 documentation architecture and `DOCS_STRUCTURE_GUIDE` §9. Revert the two documentation edits if implementation is reverted. |
| 5 | With passing proof, set the source issue to `closed` with a concise resolution, create the Phase 5 summary, and regenerate indexes. Do not claim completion before Phase 5 review and user approval. | `py -3 scripts/docs_index.py`; `py -3 scripts/docs_index.py --check`; `git diff --check`. | Sonnet; context: all phases and `DOCS_STRUCTURE_GUIDE` §4.3, §9. Revert documentation/index changes together; code remains independently reversible. |

## Testing Plan

Run the focused unit tests before C3, then the whole feature-gated conformance target. No full
workspace build is needed for this isolated core change. The change is safe to pause after any
step: source behavior and tests are committed together, and Phase 5 remains outstanding until code
and current documentation agree.

## Agent Assignment

Steps 1 and 4 are bounded review/documentation work assigned to Haiku. Steps 2, 3, and 5 require
cross-checking the contract, test harness, and phase closure, so they are assigned to Sonnet with
the named context. No agent starts implementation until this Phase 4 revision is approved.

## Rollback Plan

The code rollback is exactly the two `listdir` return statements. The test rollback removes only
`filestore01`, `filestore02`, and the C3 setup change. Documentation and index changes roll back
together; no schema, persisted data, or migration exists.

## Phase 5 Entry Criteria

Enter Phase 5 only after both direct tests and the feature-gated C3 target pass, the source issue
has resolution evidence, and the two `affects_docs` documents have been checked against the final
behavior. User approval is still required before marking the design complete.

## Final Review

Rust review finds no ownership, object-safety, async, error-construction, or dependency-flow risk:
this is a two-branch behavioral change using existing `Result` paths. The remaining approval
criterion is behavioral: the C3 reproduction must run with an uncreated `files` directory and
complete successfully while a real filesystem error is not swallowed.
