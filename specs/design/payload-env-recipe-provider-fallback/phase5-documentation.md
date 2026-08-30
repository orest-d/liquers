# Phase 5: Documentation - Recipe-provider fallback for the payload environment

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation change

## Implementation Summary

`SimpleEnvironmentWithPayload::get_recipe_provider` now follows the same fallback pattern as
`SimpleEnvironment`: a configured provider is returned unchanged; otherwise the method writes a
stderr diagnostic and returns `TrivialRecipeProvider`. The struct Rustdoc was updated so it no
longer claims that missing configuration panics.

This conforms to the approved Phase 2 recommendation. No public signatures, trait bounds, stores,
payload semantics, or command behavior changed. The only behavior change is the intended one: an
unconfigured native queued payload environment now reaches the same no-recipe error behavior as
the other core environments instead of aborting the process.

The focused regression
`context::tests::unconfigured_core_environments_return_trivial_recipe_provider` checks all four
core environments and asks the returned provider for observable trivial-provider behavior.

## Documentation Delivered

### New Reference Documents

None. The current behavior belongs in the existing environment/context reference.

### New Guide Documents

None. There is no new repeatable workflow beyond the already documented environment selection and
provider configuration behavior.

### Existing Documents Reviewed or Updated

Authoritative `affects_docs`: `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`.
The built-in environment comparison now states that `SimpleEnvironmentWithPayload` falls back to
`TrivialRecipeProvider` with a stderr notice. The same table also corrected the older
`liquers_lib::DefaultEnvironment` row, whose recipe provider has been non-optional since
`LIB-RECIPE-PROVIDER-PANIC`.

### Links and Capability Map

`specs/README.md` now treats the payload-environment fallback as built and points readers to the
current environment/context reference instead of leaving the design as active work.

## Issues Filed

None.

## Important Learning

The issue text's original doc-comment claim was stale: at intake, `SimpleEnvironmentWithPayload`
explicitly documented the panic. The defect was the cross-environment divergence and supported-path
panic, not a mismatch between that local Rustdoc and its implementation.

## Conformance and Remaining Work

Nothing remains for `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC`. Diagnostic behavior is still split by
environment family: the two native queued environments write stderr when falling back, while the
immediate environments stay silent. That is the Phase 2 choice, not deferred scope.

## Validation

- `cargo fmt -p liquers-core`
- `cargo test -p liquers-core --lib unconfigured_core_environments_return_trivial_recipe_provider`
