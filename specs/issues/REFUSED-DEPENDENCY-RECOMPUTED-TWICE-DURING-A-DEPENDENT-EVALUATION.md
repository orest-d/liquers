---
id: REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION
kind: issue
title: A dependency refused on the fast track runs its recipe twice while a dependent is evaluated
status: draft
priority: P3
complexity: M
area: [core/assets]
design:
created: 2026-10-08
github:
---

## Problem

The setup: a restarted process under `dependency_audit: on_load`, with a chain
`l0 <- l1 <- l2` whose `l0` must be recomputed. Evaluating `-R/data/l2.txt` refuses the stored `l2`,
`l1` and `l0`, then recomputes them. The `upper` command behind `l1` runs **twice** on the same
input:

```
upper("new")   # l1
upper("NEW")   # l2
upper("new")   # l1 again, with no "Evaluating asset … for key data/l1.txt" line before it
```

The same asset id is reported refused twice (`Asset 1001 stale (on_load): …` appears twice):
the fast track is attempted twice for one asset. The duplicate happens with or without the
first-registration expiry added by `dependency-chain-analysis-cost`.

## Impact

Wasted computation; every recipe-backed command runs twice in this path. No wrong result was
observed.

## Expected behaviour

Each refused dependency is recomputed once per evaluation.

## Discovery

Found on 2026-10-08 in `restart_upstream_command_change_on_load_refuses`
(`liquers-core/tests/dependency_audit_integration.rs`, `dependency-chain-analysis-cost` Phase 4,
Step 5). That test asserts "at least" the expected number of recomputations because of this.
