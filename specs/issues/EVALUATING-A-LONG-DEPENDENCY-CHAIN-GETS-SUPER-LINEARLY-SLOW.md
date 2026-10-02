---
id: EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW
kind: issue
title: Evaluating a long dependency chain gets super-linearly slow
status: draft
priority: P2
complexity: M
area: [core/assets]
created: 2026-10-02
github:
---
## Problem

Evaluating a chain of keyed recipes, each reading the previous one (`-R/data/l{i-1}.txt/-/upper/l{i}.txt`),
takes time that grows far faster than the chain length. Measured with a test driving
`SimpleEnvironment` and evaluating the links one at a time, in a debug build: 5 links in 0.15 s,
20 links in 6 s, 40 links in 80 s. Roughly the fourth power of the length.

A cascade-expire seen in the same experiment hints at the cause: after the chain was loaded, the
last link's expiry reason named `via` = the *first* link, not its predecessor, so every link records
an edge to every upstream link. The per-link dependency list therefore grows with the depth, and
each evaluation registers and checks all of it. Not confirmed by profiling; the edge growth is
observed, the cost attribution is a guess.

## Impact

Any deployment with deep recipe chains pays for it on every cold evaluation. Chains of a few links
are unaffected. The audit and cascade paths are not the slow part: a 100-link chain written
directly to the store loads and cascades in milliseconds.

## Expected behaviour

Evaluation cost linear (or at worst quadratic with a small constant) in chain length, and a link's
recorded dependencies limited to what it reads directly, or at least a documented reason for the
transitive records.

## Discovery

Found while writing `cascade_over_100_link_chain` in
`liquers-core/tests/dependency_audit_integration.rs` (dependency-audit-and-expiry-provenance,
Step 6). The test now writes the persisted chain by hand to avoid the cost.
