---
id: RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED
kind: issue
title: Resource plan steps always report non-volatile, behind a TODO that prints at runtime
status: closed
priority: P3
complexity: S
area: [core/plan]
design: step-volatility-impl-removal
created: 2026-10-12
github:
---

## Problem

`liquers-core/src/interpreter.rs`, `impl IsVolatile<E> for Step`: the `Step::GetResource` and
`Step::GetResourceMetadata` arms return `Ok(false)` unconditionally, under
`// TODO: support for resource volatility check`, and each first prints
`ADD SUPPORT FOR RESOURCE VOLATILITY CHECK! (A)` / `(B)` with `eprintln!`. The neighbouring
`Step::GetAssetDirectory` arm answers `Ok(true)` with `// TODO: Is asset directory volatilile?`, and
`Step::GetAssetRecipe` carries `// TODO: not sure when a recipe itself is volatile`.

Not verified: whether `Step::is_volatile` is reached on any supported path. `Plan::is_volatile`
returns the value cached by plan building and does not consult its steps, so the arms may be
reachable only through a caller that asks a single step directly.

## Problem Example

`Step::GetResourceDirectory(parse_key("data")?).is_volatile(envref)` returns `Ok(true)`, while the
plan of `-R-sdir/data` has `is_volatile == false`; `Step::GetResource(..).is_volatile(..)` prints
`ADD SUPPORT FOR RESOURCE VOLATILITY CHECK! (A)` to stderr. Checked 2026-10-17: nothing outside the
unit test `step_is_volatile_when_alias_is_volatile` calls the step-level impl (`IsVolatile` is
`pub(crate)`; `Query`, `Recipe` and links answer from `Plan::is_volatile`). There should be one
authority, the plan, and no runtime print.

## Impact

If the arms are reachable, a resource whose store entry is marked volatile (or whose metadata says
so) is treated as cacheable. If they are not reachable, the code is dead and the runtime line is
noise that `CORE-TRACING-MIGRATION` would otherwise have to carry over as a `trace!` event.

## Expected behaviour

Either the resource arms decide volatility from the resource (its metadata, or the store's
declared behaviour), or the step-level `IsVolatile` impl is removed in favour of the plan-level
cached value, with the decision documented in `specs/reference/ASSETS.md` or the volatility
reference. The `eprintln!` lines go in both cases.

## Discovery

Found 2026-10-12 while inventorying `eprintln!` sites for `specs/design/core-tracing-migration/`.
That design maps the two lines to `tracing::trace!` and leaves the behaviour to this issue.

## Resolution

2026-10-17, by `design/step-volatility-impl-removal/`. The step-level impl was unreachable, so it
was removed rather than taught to read resource metadata: `liquers-core/src/interpreter.rs` no
longer has `impl IsVolatile for Step`, `for ResolvedParameterValues` or `for ParameterValue`, and
the two `ADD SUPPORT FOR RESOURCE VOLATILITY CHECK!` prints went with them. `Query`, `Recipe` and
`Plan` still answer from `Plan::is_volatile`, now documented as the only authority.
`specs/reference/api/DOC_08_RECIPES_PLANS.md` (Planning contract) records that resource steps never
make a plan volatile. The removed unit test `step_is_volatile_when_alias_is_volatile` is replaced
at plan level by `plan::tests::volatile_alias_of_stable_target_makes_plan_volatile`;
`plan::tests::alias_planning_inherits_target_volatility` still passes.
