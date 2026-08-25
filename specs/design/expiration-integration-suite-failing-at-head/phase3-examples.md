# Phase 3: Examples & Use-cases - Expiration Integration Suite Triage

## High-Level Introduction

These conceptual scenarios establish whether the historical integration-suite failure remains
live, without changing the asset lifecycle. They progress from the decisive closing rerun, through
what the suite covers, to the failure branch that prevents an invalid closure.

## Example Type

**User choice:** Conceptual evidence. No prototype or new test code is appropriate because this
design introduces no API or runtime behavior.

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|---|---|---|---|
| 1 | Example | Clean closing rerun | Establish reproducible 32/0 evidence at a recorded revision. | Haiku Agent 1 |
| 2 | Example | Coverage interpretation | Relate existing cases to the historical report without inferring causality. | Sonnet synthesis |
| 3 | Example | Failed or unreliable rerun | Define the rule that blocks closure. | Haiku Agent 2 |
| 4 | Unit tests | No new unit tests | No functions, types, or branches are added. | liquers-unittest review |
| 5 | Integration tests | Existing expiration suite | Retain one authoritative end-to-end evidence command. | Haiku Agent 3 |

## Example 1: Reproduce and Close the Reported Failure

### Connection to the High-Level Design

This distinguishes a live expiration regression from a stale report, exercising the approved
asset, dependency, persistence, and query-recomputation interactions without changing them.

### Scenario and Sequence of Steps

1. Record `git rev-parse HEAD` and confirm source/test files are clean (documentation-only changes
   are acceptable when explicitly identified).
2. Run `cargo test -p liquers-core --test expiration_integration` from the workspace root.
3. Preserve the complete Cargo result and test counts.
4. On success, Phase 5 records the revision, date, command, and output in the issue before closing.

### Core Evidence

```text
git rev-parse HEAD
cargo test -p liquers-core --test expiration_integration
```

**Expected output:** `test result: ok. 32 passed; 0 failed; 0 ignored`.

### Guide and Executable Example

No guide or prototype is needed. `liquers-core/tests/expiration_integration.rs` is the canonical
executable evidence; this design records the targeted command as its closure procedure.

## Example 2: Interpret What the Green Suite Establishes

The suite's existing async cases cover plan-expiration propagation, timed expiration, dependency
invalidation, keyed persistence/recovery, normal and binary read gates, re-request recomputation,
and fast-track prevention. A green run disproves the issue's present-tense claim at that revision;
it does not prove which historical change made the suite green or validate unrelated design work.

## Example 3: Failed or Unreliable Rerun

A non-zero exit, timeout, or inconsistent timing-sensitive failure blocks closure. Record the
revision, full output, and named test; leave the issue non-terminal and create separately scoped
runtime or flakiness remediation. Do not hide the failure by selecting individual passing tests.

## Corner Cases

| Risk | Required treatment |
|---|---|
| Historical references | `keyed-recipe-ownership` and `liquers-web-store` are time-scoped records; do not edit them or infer causality. |
| Dirty checkout | Do not close from source/test changes that could affect compilation or behavior. |
| Async timing | Any flaky result is a failure signal; preserve its output rather than retrying it away. |
| Scope dilution | A broad/default test command cannot replace the exact integration target. |
| Memory/serialization | This triage mutates neither; existing cases already exercise persistence and serialized expired status. |

## Documentation and Learning Log

No guide candidate emerged. Preserve the distinction between "not reproducible at the closing
revision" and an unproven causal explanation, plus the exact command/revision/output, in Phase 5.

## Test Plan

### Unit Tests

None: no new function, data structure, error path, or branch exists. The liquers-unittest review
confirms that generated unit-test templates are inapplicable.

### Integration Tests

Run the whole existing `liquers-core/tests/expiration_integration.rs` target once as the closure
gate. Its `#[tokio::test]` cases are the appropriate end-to-end coverage; passing individual tests
is insufficient.

### Manual Validation

Capture `git rev-parse HEAD`, `git status --short`, and the complete output of the exact Cargo
command. Success requires a clean applicable source/test checkout and 32 passed, 0 failed. A
failure prevents closure and becomes evidence for separately scoped remediation.
