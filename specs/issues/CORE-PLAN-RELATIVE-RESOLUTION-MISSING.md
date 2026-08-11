---
id: CORE-PLAN-RELATIVE-RESOLUTION-MISSING
kind: issue
title: Queries are not resolved relative to the current working directory
status: draft
priority: P1
complexity: M
area: [core/plan]
design: plan-relative-resolution
created: 2026-08-08
github:
---
## Problem

`Recipe::cwd` is not propagated into the plan or interpreter when a recipe is applied. The default
provider assigns the directory containing `recipes.yaml` and rejects a `cwd` authored in that file;
programmatically created recipes may set `cwd`, but `Recipe::to_plan` ignores it. Although
`Query::to_absolute` and `Step::SetCwd` exist, `find_dependencies` still contains the unresolved-query
TODO and runtime pre-scheduling consumes query/key identities without consulting `Context::cwd_key`.

## Impact

A provider-loaded or programmatic recipe that uses a relative resource reference can resolve it
against the wrong base. Planning, dependency records, pre-scheduling, and execution can consequently
disagree with the recipe's storage key or select a different asset without reporting an error.

## Expected behaviour

The provider-derived working key is authoritative for recipes loaded from `recipes.yaml`, and an
explicit `cwd` is respected for programmatically constructed recipes. Planning, dependency identity,
scheduling, and interpretation apply that working key consistently to every query-bearing path. The
plan records the recipe-derived working key as an executable `SetCwd` prefix and an informational
planning diagnostic. Resolution is ordered: a later `-R-cwd/<key>` first resolves its possibly
relative key against the current CWD, then overrides the base for every subsequent key, embedded
query, and nested plan.

`PlanBuilder` preserves source-relative operands. The interpreter is the semantic authority for
CWD; dependency discovery and pre-scheduling simulate its ordered cursor without mutating live
context or rewriting the plan. A future optimizer may make static operands absolute, but may remove
a `SetCwd` only when later commands and nested plans cannot observe its context effect. The shared
cursor is a pure analysis helper; it is not a second runtime CWD.

If a leading `.` or `..` needs a missing CWD, interpretation establishes logical root `/`,
resolves successfully, and emits a warning. Ordinary keys and absolute queries are unaffected and
silent.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #7, work package WP-7. Reverified
2026-08-10 against `recipes.rs`, built-in `Environment::apply_recipe` implementations, plan
finalization, and interpreter scheduling; `validate::tests::cwd_changes_key_not_plan` confirms that
changing recipe `cwd` currently leaves the plan unchanged. That pre-fix assertion was replaced
during implementation by `validate::tests::cwd_changes_key_and_plan_prefix_only`, which verifies
the current contract: the recipe prefix and reported key change while the query-derived plan tail
remains source-relative. See
`specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Resolution

Implemented and validated on 2026-08-11 by the linked `plan-relative-resolution` design. Recipe
CWD is represented by a raw executable `SetCwd` prefix and planning `Info`; dependency analysis,
pre-scheduling, and runtime execution resolve ordered relative operands through the same
Context-owned semantics.

Executable evidence includes 507 library tests, the 16-test `manager_parametric` suite, and the
eight-scenario `recipe_cwd_resolution` integration suite. Absolute outer resource paths are aligned
back to their query-derived plan step during analysis and execution, so they remain rooted without
changing the live CWD used by relative child links.
