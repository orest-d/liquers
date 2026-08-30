---
id: CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC
kind: issue
title: SimpleEnvironmentWithPayload panics when no recipe provider is configured
status: closed
priority: P1
complexity: S
area: [core/context]
design: payload-env-recipe-provider-fallback
created: 2026-08-27
github:
---
## Problem

`SimpleEnvironmentWithPayload::get_recipe_provider` panics instead of falling back:

```rust
fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
    if let Some(provider) = &self.recipe_provider {
        return provider.clone();
    }
    panic!("No recipe provider configured in SimpleEnvironmentWithPayload");
}
```

Its three sibling environments in the same file do not. `SimpleEnvironment` logs to stderr and
returns `TrivialRecipeProvider`; `ImmediateEnvironment` and `ImmediateEnvironmentWithPayload`
return `TrivialRecipeProvider` silently. The struct's own doc comment claims the
`TrivialRecipeProvider` fallback that the other three implement, so the documented behavior and the
code disagree.

`recipe_provider` is `Option<...>` and defaults to `None`, so the panic is on the default path: any
evaluation reaching `get_recipe_provider` on a freshly constructed `SimpleEnvironmentWithPayload`
aborts the process.

This is the same defect as `LIB-RECIPE-PROVIDER-PANIC` (closed, P0), which fixed
`liquers-lib`'s `DefaultEnvironment`. The `liquers-core` payload environment was not covered by
that fix.

## Impact

A panic on a supported path, which is `DOCS_STRUCTURE_GUIDE.md` §4.4 P0 territory. Priority is
recorded P1 rather than P0 only because the affected environment is the least used of the four —
reachable from `liquers-core/tests/payload_inheritance.rs` and the payload documentation, but not
from `liquers-lib`, `liquers-axum` or `liquers-web`, which use their own environments. Raise to P0
if a shipped path is found to depend on it.

## Expected behavior

Match the siblings: return `TrivialRecipeProvider` when none is configured, with the same
`eprintln!` diagnostic `SimpleEnvironment` uses, or none at all — but consistently across all four.

## Fix direction

One line, replacing the `panic!` with the sibling fallback. Recorded against the
`environment-builder` design because that project consolidates the four environments
and gives the recipe provider a builder-supplied default, which removes this divergence by
construction. If that project does not land, fix it directly.

## Verification

1. `SimpleEnvironmentWithPayload::new().to_ref()` followed by an evaluation that consults the
   recipe provider does not panic.
2. All four core environments return the same provider for the unconfigured case.
3. A test asserting the fallback, alongside `liquers-lib`'s equivalent from
   `LIB-RECIPE-PROVIDER-PANIC`.

## Resolution

Closed on 2026-08-30 by changing `SimpleEnvironmentWithPayload::get_recipe_provider` to return
`TrivialRecipeProvider` with the same stderr diagnostic used by `SimpleEnvironment`, and by updating
the struct Rustdoc to match. The focused regression
`context::tests::unconfigured_core_environments_return_trivial_recipe_provider` verifies that all
four core environments return a trivial provider when unconfigured.
