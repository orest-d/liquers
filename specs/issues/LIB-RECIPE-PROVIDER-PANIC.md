---
id: LIB-RECIPE-PROVIDER-PANIC
kind: issue
title: DefaultEnvironment panics when no recipe provider is configured
status: closed
priority: P0
complexity: S
area: [lib/commands, core/assets, web]
design:
created: 2026-08-09
github:
---

## Problem

`DefaultEnvironment::get_recipe_provider` ends in a bare `panic!`
(`liquers-lib/src/environment.rs:148-153`):

```rust
fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
    if let Some(provider) = &self.recipe_provider {
        return provider.clone();
    }
    panic!("No recipe provider configured in DefaultEnvironment");
}
```

`recipe_provider` is `None` by default (`:56`), and evaluating any keyed (`-R/`) query reaches this
method through `AssetRef::evaluate_recipe`. So the default-constructed environment aborts on the
first resource query rather than reporting anything.

This also breaks the project's own rule that library code does not panic (`CLAUDE.md`): the method
returns a plain `Arc`, so the panic is the only failure channel it currently has.

## Impact

**On native** the panic unwinds and a caller sees a panic message — bad, but visible.

**On wasm it is much worse.** A panic aborts the wasm instance; the `Promise` the page is awaiting
never settles, and the only trace is a `pageerror`. The symptom is an indefinite hang with no error
at the call site, which is how it was found.

The default constructor being the broken configuration is what makes this reachable: an integration
has to know to call `with_default_recipe_provider`, and nothing says so. `liquers-web` did not, and
every `-R/` query in the browser aborted the instance until M6 of
`specs/design/liquers-web-store/` added the call.

## Expected behaviour

Two candidates, and the choice is a real one:

1. **Default to `DefaultRecipeProvider` in `DefaultEnvironment::new`.** Nothing has to be
   remembered, and the behaviour with no recipes in the store is simply "no recipe found", which
   is what a caller expects. This is what `liquers-web` now does by hand.
2. **Return `Result`,** so the absence is reported rather than fatal. Honest, but it changes an
   `Environment` trait method's signature and therefore every implementor, including
   `liquers-py` — which the language-integration guide says must not happen (`§3`: additive only).

Option 1 is additive and fixes the reachable case; option 2 is the more correct shape and costs a
breaking change. A middle path is option 1 plus turning the remaining `panic!` into a
`NotAvailable` error carried on the returned provider, so a genuinely unset provider degrades
rather than aborts.

## Discovery

Found on 2026-08-09 in M6 of `specs/design/liquers-web-store/`: browser end-to-end tests for `-R/`
hung, and the wasm stack trace ended in `get_recipe_provider`. Worked around in `liquers-web` by
calling `with_default_recipe_provider()` in `new_environment()`, with a comment pointing here. The
workaround revealed a second defect immediately behind it —
`CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`.

## Resolution

Resolved on 2026-08-12. `DefaultEnvironment::new` now installs `DefaultRecipeProvider`, and its
private provider field is always populated rather than representing an invalid optional state.
Explicit provider configuration still replaces that default. The focused
`default_environment_has_a_recipe_provider` regression test reproduces the former
default-constructor panic and verifies that provider access now succeeds.
