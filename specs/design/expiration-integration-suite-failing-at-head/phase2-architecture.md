# Phase 2: Solution & Architecture - Expiration Integration Suite Triage

## Overview

This is a no-code closure design. Phase 5 will rerun the targeted integration suite and, only if
it passes, update the issue from `draft` to `closed` with the closing revision and complete Cargo
output. The completed `expiration-safety` design may be linked as related historical context; it
must not be described as the cause of the passing result unless git history verifies that claim.
A failing rerun aborts closure: this design does not expand into a runtime fix. The issue remains
open and the failure must be triaged as a separately scoped runtime-remediation effort.

## Known-Issue Preflight

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD` | draft | P1 | Subject of the design; current reproduction passes. | no | no | Close only after final clean rerun. | Retain P1 until closure. |
| `DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE` | draft | P1 | Similar expired-read gate, but covers an execution-time dependency race not exercised as a suite failure. | no | no | Keep as an independent follow-up. | Keep P1. |
| `EXPIRATION-RECOVERY-WEB-API` | draft | P2 | Depends on the established core recovery APIs, not this triage. | no | no | Keep independent. | Keep P2. |

### Blocking and Priority Decision

No unresolved issue invalidates the closure evidence or requires a priority change. The existing
P1 is appropriate until the repro is confirmed green at the closing revision; P0 is not warranted
because no current incorrect result, panic, or broken documented behavior was reproduced.

### Cross-Reference Verification

No other issue references `EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD`. Two designs reference
the same historical failure: `keyed-recipe-ownership/DESIGN.md` calls it pre-existing and unchanged,
and `liquers-web-store/DESIGN.md` calls it pre-existing and unrelated. Their referenced problem
does not persist at `9293ad322a75b88be601049b7d19b3c71af71b17`: the exact targeted suite passes
32 tests with 0 failures. These historical records are not edited because they describe the state
observed during those designs.

## Data Structures

None. The design creates no runtime state or serialized data.

## Trait Implementations

None. No `AssetManager`, store, command, or value trait changes are proposed.

## Generic Parameters & Bounds

None; no Rust API is added or changed.

## Sync vs Async Decisions

The existing `#[tokio::test]` integration cases retain their async execution. The only planned
action is the existing Cargo test command; no blocking I/O or new runtime task is introduced.

## Function Signatures

None. The solution deliberately preserves all public and crate-private signatures.

## Integration Points

| Location | Role | Planned change |
|---|---|---|
| `liquers-core/tests/expiration_integration.rs` | Runtime evidence | No source change; execute its 32 cases. |
| `specs/issues/EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD.md` | Issue authority | In Phase 5, record the passing evidence and set `status: closed`. |
| `specs/index.csv` | Generated index | Regenerate after a passing closing update, or after recording a failed rerun. |
| `specs/README.md` | Live map | Regenerate its generated blocks; retain any entry linking this non-superseded design. Do not replace a design link with a closed-issue link. |

## Documentation Architecture

### Reference Plan

None. `ASSETS.md`, `ASSET_LIFECYCLE.md`, and the API references were reviewed as area candidates;
the current contract is unchanged, so a History/review-date update would misrepresent a content
review.

### Guide Plan

None. `CLAUDE.md` already gives the exact targeted command and `UNITTEST_GUIDE.md` need not change
for one historical-suite closure.

### Other Documents to Create

`specs/design/expiration-integration-suite-failing-at-head/` (design, internal, `core/assets`)
preserves the triage reasoning and Phase 5 closure summary.

### Existing Documents to Review or Update

The authoritative `affects_docs` set remains empty: this no-code triage does not change any
reference or guide claim. Operational documentation changes are instead limited to:

| Path | Kind / audience / area | Exact change and required links |
|---|---|---|
| `specs/issues/EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD.md` | issue / internal / `core/assets` | On a passing final rerun, record the exact command, date, closing git revision, and complete Cargo result (including test count), link this design and optionally the related `expiration-safety` design, then set `status: closed`. On failure, retain its non-terminal status and record or file the separately scoped remediation work. |
| `specs/README.md` | capability map / internal / `docs` | Regenerate generated blocks. If the generated unplaced block lists `design/expiration-integration-suite-failing-at-head`, retain that exact design entry while it is not superseded; no closed-issue substitution is permitted. No new capability row is needed because this design adds no capability. |
| `specs/index.csv` | generated index / internal / `docs` | Regenerate from the final front matter after either outcome; never hand-edit. |

The asset references listed above are discarded candidates because no present-tense claim changes.

### Design and Capability Links

`specs/README.md` has no new capability link for this triage. Its generated coverage entry, if
present, continues to point at this design folder because it is not superseded; closing the related
issue does not change that link target.

## Relevant Commands

### New Commands

None.

### Relevant Existing Namespaces

None. Tests register local commands solely as fixtures; no command namespace is part of the
solution, so no command-namespace decision is needed from the user.

## Web Endpoints (if applicable)

None.

## Error Handling

No new errors are introduced. A non-zero `cargo test` result is closure-blocking evidence, not an
error-handling API change.

## Serialization Strategy

None. Existing tests cover persisted `Status::Expired` metadata without changing its format.

## Concurrency Considerations

No new concurrency mechanism is proposed. The suite continues to exercise the existing async
expiration monitor and dependency paths.

## Compilation Validation

`cargo test -p liquers-core --test expiration_integration` completed successfully at the observed
HEAD: 32 passed, 0 failed, 0 ignored. Phase 5 must rerun it and capture the exact closing revision
and full Cargo result before closure. The design adds no code that could alter compilation.

## References to liquers-patterns.md

Aligned: no new ownership, traits, errors, enum matches, async I/O, or dependency edges are
introduced. Rust-best-practices review found no blocking or advisory Rust findings for this
no-code architecture.
