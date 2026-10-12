---
id: RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED
kind: issue
title: Resource plan steps always report non-volatile, behind a TODO that prints at runtime
status: draft
priority: P3
complexity: S
area: [core/plan]
design:
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
