For [`issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md`](../../issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md). Nothing here is implemented.

# Phase 1 — High-level design

## Problem and evidence

`SimpleEnvironmentWithPayload::get_recipe_provider` (`liquers-core/src/context.rs:1954`) aborts the
process instead of falling back:

```rust
fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
    if let Some(provider) = &self.recipe_provider {
        return provider.clone();
    }
    panic!("No recipe provider configured in SimpleEnvironmentWithPayload");
}
```

Its three siblings in the same file do not:

| Environment | `get_recipe_provider` with none configured | Site |
|---|---|---|
| `SimpleEnvironment` | `eprintln!` then `TrivialRecipeProvider` | `context.rs:1105` |
| `ImmediateEnvironment` | `TrivialRecipeProvider`, silently | `context.rs:1242` |
| `ImmediateEnvironmentWithPayload` | `TrivialRecipeProvider`, silently | `context.rs:1384` |
| `SimpleEnvironmentWithPayload` | **panics** | `context.rs:1954` |

`recipe_provider` is `Option<Arc<dyn AsyncRecipeProvider<Self>>>` (`context.rs:1819`) and
`SimpleEnvironmentWithPayload::new()` leaves it `None`, so the panic is on the **default** path: any
evaluation that reaches `get_recipe_provider` on a freshly constructed environment aborts.

**One correction to the issue text.** It states that "the struct's own doc comment claims the
`TrivialRecipeProvider` fallback that the other three implement", and that documentation and code
disagree. That is not what is at `HEAD`. The doc comment (`context.rs:1804-1808`) reads:

> A recipe provider must be configured before a keyed recipe lookup; otherwise
> [`Environment::get_recipe_provider`] panics.

The three siblings' doc comments (`:964`, `:1129`, `:1265`) each promise the fallback. So the
divergence is real and documented on both sides — the defect is the **inconsistency between four
environments that are otherwise interchangeable**, not a lie in a doc comment. This matters for
Phase 2: the fix must update that paragraph too, and the "documented behaviour" argument cannot
carry the decision on its own.

This is the same defect class as `LIB-RECIPE-PROVIDER-PANIC` (closed, P0), which fixed
`liquers-lib`'s `DefaultEnvironment` by making the field non-optional
(`liquers-lib/src/environment.rs:171` now returns `self.recipe_provider.clone()`). The
`liquers-core` payload environment was not covered.

## Expected behaviour and acceptance criteria

1. `SimpleEnvironmentWithPayload::new().to_ref()` followed by an evaluation that consults the recipe
   provider does not panic.
2. All four core environments return a provider — never abort — for the unconfigured case, and a
   test asserts that for all four rather than for the one being fixed.
3. The doc comment on `SimpleEnvironmentWithPayload` states what the code does.
4. No behaviour change for a configured provider.

## Affected users, workflows and systems

`core/context`. Reachable from `liquers-core/tests/payload_inheritance.rs` and the payload
documentation. `liquers-lib`, `liquers-axum` and `liquers-web` use their own environments and are
unaffected. `liquers-py` has its own `get_recipe_provider` (`liquers-py/src/context.rs:113`) and is
unaffected.

## Scope and non-goals

In scope: the fallback, the doc comment, and a test covering all four environments.

Not in scope: harmonising the `eprintln!` diagnostic across all four (see Q1 — this is the one open
choice), changing `Option<Arc<…>>` to `Arc<…>` as `liquers-lib` did, consolidating the four
environments (that is `environment-builder`), and `RECIPE-PROVIDER-BY-NAME`.

## Compatibility constraints

Behaviour changes on exactly one path: a process that used to abort now proceeds with a provider
that resolves no recipes, so a `-R/` query returns an error instead of killing the process. That is
the point of the fix. Nothing that works today changes.

## Known questions and assumptions

- **Q1** — diagnostic consistency: `SimpleEnvironment` prints, the two immediate environments do
  not. The issue asks for "the same `eprintln!` … or none at all — but consistently across all
  four", which is an instruction to pick. See Phase 2.
- **Q2** — overlap with `environment-builder`, whose Phase 2 says this issue is "fixed by
  construction" once the builder resolves the default in `build()`. Fixing it now duplicates work
  that design will delete. See Phase 2 §Relationship to `environment-builder`.

## Documentation assessment

Small maintenance only: the struct doc comment in `context.rs`, and a check of
`specs/reference/PAYLOAD_GUIDE.md` for any sentence describing the panic. No new document.
`environment-builder`'s Phase 2 table
([`phase2-architecture.md`](../environment-builder/phase2-architecture.md):220) records the panic as current
behaviour; if this lands first, that row becomes stale and needs one word changed — but that
document is an approved phase artifact of a live design, so the change belongs to that design's
owner, not to this issue.
