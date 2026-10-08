---
id: EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK
kind: issue
title: Evaluating the last link of a deep recipe chain overflows the thread stack
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-10-08
github:
---

## Problem

Evaluating the last link of a chain of keyed recipes (`l{i} = -R/data/l{i-1}.txt/-/upper`), with
nothing evaluated before it, aborts with a stack overflow between 20 and 30 links. The run was a
debug build on the default 2 MB test-thread stack, with `SimpleEnvironment`. Each link's
evaluation waits on its dependency's evaluation inside the same task, so the poll recursion
depth grows with the chain. Evaluating the links one at a time, bottom-up, works for any length.

The dependency *analysis* is not involved: since `dependency-chain-analysis-cost` it is iterative
(a 2 000-link analysis runs on the default stack), and so is the stored-records walk.

## Impact

- **Affected:** any deployment whose recipe chains are deeper than a few dozen links, on a cold
  evaluation of the end of the chain.
- **Severity:** the process aborts, which cannot be caught as an error.
- **Workaround:** evaluate upstream links first, or run with a larger stack (`RUST_MIN_STACK`,
  or a runtime with bigger worker stacks).

## Expected behaviour

Evaluation depth bounded independently of chain length. For example, a dependency is evaluated
as a separate task that the dependent awaits, rather than inline in the dependent's poll; or
evaluation is scheduled bottom-up.

## Discovery

Found on 2026-10-08 while implementing `dependency-chain-analysis-cost` (Phase 4, Step 5): a test
evaluating a 50-link chain top-down overflowed. A probe over a fresh chain environment passed at 10
and 20 links and overflowed before 30. The tests in `liquers-core/tests/dependency_audit_integration.rs`
evaluate link by link for this reason.
