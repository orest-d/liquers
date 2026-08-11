# Phase 5: Documentation - plan-relative-resolution

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included with the implementation change set

## Implementation Summary

Liquers now treats CWD as ordered interpreter state throughout recipe planning, dependency
analysis, scheduling, and execution. Provider-loaded recipes continue to derive CWD from the
directory containing `recipes.yaml` and reject YAML-authored CWD. Programmatic `Recipe::cwd` is now
honored by `Recipe::to_plan` as one raw executable `SetCwd` prefix plus one planning `Info`; the
query-derived plan remains source-relative and serializes without runtime cursor state.

`Context` owns the live CWD shared by interpreter steps. Relative `SetCwd` values resolve against
the current CWD before replacing it; later resource, asset, key, query, action-link, and nested-plan
operations observe the updated value. Independently evaluated links use scoped cursors, while a
nested `Step::Plan` shares and can update the caller's CWD. A leading `.` or `..` without an entry
CWD installs logical root and emits the exact warning once across Context clones. Absolute outer
resources use root without changing the live CWD used by relative child links.

Dependency discovery, volatility/expiration analysis, pre-scheduling, cycle identity, cache
identity, runtime access, and final owner registration now use resolved identities consistently.
Nested keyed recipes retain provider CWD, overrides, and payload boundaries through
`Recipe::to_plan_for_key`. Multiple action parameters are traversed recursively; top-level linked
values remain lossless, while links nested inside variadic parameter trees use the existing typed
JSON conversion contract with source positions preserved on errors.

The implementation conforms to the request and approved design. The only added mechanism is
`Plan::absolute_query_resource_step_index`, discovered by the end-to-end tests: PlanBuilder must
remain source-relative, so analysis and the interpreter reverse-align the absolute query's own
Resource step after a recipe prefix is inserted. They resolve only the consumed/examined step copy;
raw plans and serialized operands remain unchanged. No requested behavior was omitted.

## Documentation Delivered

### New Reference Documents

None. The capability belongs in the existing query, Context, recipe/plan, lifecycle, and project
references selected during Phases 1-2.

### New Guide Documents

None. Integration guidance was added to the existing language integration guide.

### Existing Documents Reviewed or Updated

The authoritative `affects_docs` set was reviewed against the implementation and executable tests:

- `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`
- `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
- `reference/api/DOC_08_RECIPES_PLANS.md`
- `guides/LANGUAGE-INTEGRATION_GUIDE.md`
- `reference/ASSET_LIFECYCLE.md`
- `reference/PROJECT_OVERVIEW.md`

Each document now describes current behavior at its own level and carries `reviewed: 2026-08-11`
with a matching newest `phase-5` History row.

### Links and Capability Map

`specs/README.md` now routes the plan-relative-resolution capability through the current recipe and
plan reference rather than requiring readers to enter through the design history. Existing
cross-links among query, Context, lifecycle, and integration documents remain the detailed entry
points.

## Issues Filed

None. No requested scope was deferred and implementation review found no remaining defect. The
existing `CORE-PLAN-RELATIVE-RESOLUTION-MISSING` issue records the implemented resolution and test
evidence; its locally owned status was not changed.

## Important Learning

1. `Recipe::cwd` was already public and Serde-backed while `Recipe::to_plan` ignored it. The
   default provider, not Serde, rejects an authored YAML CWD and supplies the containing folder.
2. CWD is observable execution state, not merely a string-rewrite option. Static resolution in
   `PlanBuilder` would make a later `SetCwd` ineffective, so plans remain raw and the interpreter
   owns semantics; analysis simulates the cursor without committing its final value to Context.
3. A nested resource link uses `-R/./hello.txt`; `-R./hello.txt` is not valid link-resource syntax.
4. Plain `-R` means managed `GetAsset` access. Direct store access requires the `stored` selector,
   as in `-R-stored/./hello.txt`.
5. An absolute outer query does not make a relative child link absolute. Raw PlanBuilder output
   cannot identify the flattened source step after a recipe CWD prefix by position alone, so
   `Plan::absolute_query_resource_step_index` reverse-aligns Resource signatures without changing
   serialized Plan schema or the live CWD used by child links.
6. The existing query, Context, recipe/plan, lifecycle, project, and language-integration documents
   are sufficient current-state homes; no additional reference or guide was needed.

Owner identity is a related implementation lesson: it must come from the asset's immutable
construction-time query and be checked against the current recipe and non-evaluating manager
ownership. Provider recipe replacement makes mutable recipe metadata alone insufficient. The
broad integration gate exposed two remaining consumers of the older inference: expiration
persistence and dependency tracking. Both now use the same verified bound-owner helper as Context,
which restored keyed expiration persistence, keyed rebuilds, and dependent expiration cascades
without classifying ad-hoc query assets as keyed.

## Conformance and Remaining Work

All requested and approved semantics are implemented: provider and programmatic recipe CWD,
ordered relative `SetCwd`, action-free `a/b -> .. -> ./c -> key(.) = a/c`, recursive link and nested
plan behavior, root fallback warning, absolute-query isolation, resolved dependency/cache identity,
and initial plan diagnostics. No work remains within the design scope.

The bare `--no-default-features` build remains an existing unsupported Cargo configuration because
the core async traits depend on the `async_store` feature. This project did not alter that feature
architecture; the supported wasm compatibility gate explicitly enables `async_store` and passes.

## Validation

- `cargo fmt --all -- --check`: passed.
- `cargo test -p liquers-core --lib`: 507 passed.
- `cargo test -p liquers-core --test manager_parametric`: 16 passed.
- `cargo test -p liquers-core --test recipe_cwd_resolution -- --test-threads=1`: 8 passed.
- `cargo test -p liquers-core --test expiration_integration -- --test-threads=1`: 32 passed.
- `cargo test -p liquers-core --tests`: passed, including 507 library tests and every integration
  target.
- `cargo check -p liquers-core --features cli`: passed.
- `cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store`: passed.
- Seven-query `liquers-validate` matrix: 7 `Ok`, 0 warnings, 0 errors.
- Phase 5 validation: passed (the checkbox-like-content notice is a reviewed false positive).
- Documentation index: regenerated with 112 documents; check passed with 0 errors and 19 existing
  repository warnings.
- `git diff --check`: passed; Git emitted only working-copy line-ending advisories.
