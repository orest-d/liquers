# Phase 4: Implementation Plan — default namespaces for plan building

## Overview

Core first (field and rule, then planner, then builder and configuration), then the other
resolvers, then validation tooling and documentation. Each step compiles and passes on its own.
No prerequisite issue.

## Progress

- [ ] Step 1: `lookup_namespaces`, chain checks
- [ ] Step 2: Planner uses `lookup_namespaces`; `append_action` stays portable
- [ ] Step 3: Builder setter, configuration key, `Environment::default_namespaces`, Python builder
- [ ] Step 4: Template and lib resolvers use `lookup_namespaces`
- [ ] Step 5: `liquers-validate` flag and overlay merge
- [ ] Step 6: Documentation

## Implementation Steps

### Step 1: Registry field, `lookup_namespaces`, checks
- Files / symbols: `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry`, `new`, `check`
- Change: add `lookup_namespaces`; extend `check` with the empty-entry error and
  unknown-namespace warning.
- Depends on: none
- Proof: `cargo test -p liquers-core --lib command_metadata` — tests 11, 12
- Rollback: revert the file; nothing calls the method yet

### Step 2: Planner
- Files / symbols: `liquers-core/src/plan.rs` `namespaces_for_query`, `append_action`
- Change: `namespaces_for_query` parses `ns` as today and calls
  `lookup_namespaces`; `append_action` returns the plain query only when it resolves to the
  requested namespace with the environment's chain and with `["", "root"]`; otherwise `ns-<ns>`, or
  `ns-root` when a configured namespace shadows a requested root command.
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib plan::tests` — tests 3-8, existing plan tests green
- Rollback: revert; Step 1 alone is inert

### Step 3: Builder, configuration, accessor
- Files / symbols: `environment_builder.rs` `EnvironmentBuilder::{default_namespaces, with_default_namespaces, build}`;
  `environment_config.rs` `EnvironmentConfig::default_namespaces`, `with_config`;
  `context.rs` `Environment::default_namespaces`; `liquers-py/src/environment.rs` `EnvCell::configured_builder`
- Change: as Phase 2; `build()` replaces the main chain (root appended when not listed) before
  `validate()`; the Python builder applies the key.
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib environment_config` (tests 9, 10) and
  `cargo test -p liquers-core --test plan_default_namespaces` (test 13),
  `bash scripts/test-liquers-py.sh` (test 18)
- Rollback: revert; the registry chains can still be set by hand

### Step 4: Template and lib resolvers (parallel with Step 5)
- Files / symbols: `liquers-template/src/diagnostics.rs` `lookup_namespaces`,
  `liquers-template/src/query/instantiate.rs` override lookup, `liquers-lib/src/utils.rs` `namespaces_from`
- Change: delegate to `CommandMetadataRegistry::lookup_namespaces` ; keep each site's own
  handling of link parameters.
- Depends on: Step 1
- Proof: `cargo test -p liquers-template` (test 14), `cargo test -p liquers-lib --lib --tests` (test 15, 17)
- Rollback: revert per file

### Step 5: Validation tooling (parallel with Step 4)
- Files / symbols: `liquers-core/src/bin/liquers_validate.rs` `Cli`
- Change: repeatable `--default-namespace` sets the merged registry's chain by the builder's
  rule.
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --features cli --lib validate` (test 16) and
  `cargo run -p liquers-core --features cli --bin liquers-validate -- --default-namespace pl -- '-R/data/sales.csv/-/head-3'`
- Rollback: revert; the planner change is unaffected

### Step 6: Documentation
- Files: the four documents of Phase 2's documentation architecture; `specs/README.md`;
  `python3 scripts/docs_index.py`
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert

## Testing Plan

Each step's proof as it lands; at the end `cargo test -p liquers-core --lib --tests`,
`cargo test -p liquers-template`, `cargo test -p liquers-lib --lib --tests`. No `cfg(feature)` or
optional dependency changes, so the build matrix is not required. `bash scripts/test-liquers-py.sh`
runs for Step 3, since the Python builder changes.

## Rollback Plan

Steps 1-3 are the feature; 4-5 make other resolvers agree. If only 1-3 land, template diagnostics
and `next` presets would disagree with the planner in a configured environment — file that as an
issue rather than ship it silently.

## Documentation Updates

As Step 6, with History rows and `reviewed:` bumps on each reference and guide document.
`specs/command_registry.yaml` is not regenerated (no command changes; field skipped when empty).

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
