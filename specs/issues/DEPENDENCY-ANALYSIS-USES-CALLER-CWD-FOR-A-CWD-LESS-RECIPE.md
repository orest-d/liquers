---
id: DEPENDENCY-ANALYSIS-USES-CALLER-CWD-FOR-A-CWD-LESS-RECIPE
kind: issue
title: Dependency analysis resolves a cwd-less recipe against its caller's cursor, but its run resolves against the root
status: draft
priority: P3
complexity: S
area: [core/plan]
design: 
created: 2026-10-13
github:
---
## Problem

The dependency walk (`liquers-core/src/plan.rs` `DependencyWalk`, behind
`analyze_plan_dependencies`) analyses a recipe that has no `cwd` of its own "against the caller's
cursor". Its memo is keyed `(key, caller CWD)`, and `visit` scans the recipe's plan with the
reading plan's cursor. At run time, though, the keyed asset of that recipe runs in a `Context` of
its own whose CWD starts unset (`Context::new`), so `finalize_plan_expanded` freezes its relative
operands against the logical root. No code passes a caller's CWD into a dependency's run (the only
`set_cwd_key` callers outside tests are the legacy `interpreter::evaluate` and `Step::SetCwd`).

So, for a recipe without `cwd` that uses a relative operand (`-R/./s.txt`) and is read from a plan
with a CWD:
- the analysis records dependencies, volatility and expiry of `<caller CWD>/s.txt`;
- the run reads `s.txt` at the root.

## Impact

- **Affected:** recipes without a `cwd` that use relative operands, served by a provider other
  than `DefaultRecipeProvider`, which gives every recipe its folder as `cwd`.
- **Severity:** wrong dependency metadata: a recorded `PlanDependency`, a volatility or expiry
  inherited from a key the run does not read, and missing ones for the key it does. No wrong value.
- **Workaround:** give such recipes a `cwd`.

## Expected behaviour

The analysis and the run agree. Either the walk analyses a keyed recipe without `cwd` against the
root, as its run does, or the run inherits the caller's CWD, as the analysis assumes. Which one is
a contract decision.

## Discovery

Found on 2026-10-13 while answering a Codex review comment on orest-d/liquers#127
(`dependency-resolved-once-per-evaluation`). The inline manager's priming used the walk's recorded
graph. It now records only caller-independent recipes (those with their own `cwd`, or reached with
no caller CWD), so priming is not affected; the analysis itself still is.
