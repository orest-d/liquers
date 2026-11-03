# Phase 4: Implementation Plan — Pipelines

Written on 2026-11-03 under the pre-approval given after Phase 2. Steps follow the five slices of
Phase 2 gate decision 6, split further so that each step builds and tests on its own.

## Overview

The order is: leaf utilities first, then data types, conversions and editors, then values and
parameters. After those come the planner, in `Delegate` mode first and then `Native`, the
recipes, the pipeline commands and finally the tooling. Each step leaves `main`'s behaviour
unchanged until a later step uses it, so the work can stop after any step. Prerequisites: none;
`plan-default-namespaces` is at HEAD.

Exhaustive matches that new variants reach outside `liquers-core`:
- `Value`: `liquers-lib` (`commands.rs`, `value/simple.rs`, `search/command_catalog.rs`) and
  `liquers-mcp` (`server.rs`, `schema.rs`);
- `Step` / `ParameterValue`: `liquers-mcp/src/server.rs`, `liquers-py/src/command.rs` and
  `liquers-axum/src/api_core/overrides.rs`;
- `CommandParameterValue`: `liquers-macro/src/registration.rs` and
  `liquers-py/src/command_metadata.rs`.

## Progress

- [ ] Step 1: Token predicates, `check_encodable`, template and web consumers
- [ ] Step 2: Pipeline types and serde
- [ ] Step 3: Query and recipe conversions, representability, canonical form
- [ ] Step 4: Registry snapshot and editors
- [ ] Step 5: `Value::Pipeline`
- [ ] Step 6: Pipeline parameter values and defaults
- [ ] Step 7: Extract `PlanAssembly`
- [ ] Step 8: `PipelinePlanBuilder` in `Delegate` mode, `Step::UseValue`, entry points
- [ ] Step 9: `Native` mode and the dual-mode corpus test
- [ ] Step 10: Recipe pipelines
- [ ] Step 11: Pipeline commands
- [ ] Step 12: `liquers-validate` pipeline support
- [ ] Step 13: Cross-crate arms, registry export, polars integration test, build matrix

## Implementation Steps

### Step 1: Token predicates, `check_encodable`, template and web consumers
- Files / symbols:
  - `liquers-core/src/escape.rs`: new `is_identifier`, `is_realm_name`, `is_resource_name`,
    `is_header_value`, `is_filename`;
  - `parse.rs`: the private `is_identifier_start` and the other character classes, now used
    through them;
  - `query.rs`: `Query::check_encodable`;
  - `error.rs`: `Error::invalid_token`;
  - `liquers-template/src/query/instantiate.rs`;
  - `liquers-web/src/encode.rs`: `encode_param`.
- Depends on: none. Steps 1–3 can run in parallel with Step 5.
- Proof: `predicates_match_parser`, `check_encodable_names_first_bad_token`,
  `validators_use_core_predicates` and `web_encode_param_accepts_the_empty_string`;
  `cargo test -p liquers-core --lib` and `cargo test -p liquers-template`.
- Rollback: revert; the parser behaviour is pinned by its existing tests.

### Step 2: Pipeline types and serde
- Files / symbols:
  - `liquers-core/src/pipeline/mod.rs`: `Pipeline`, `Located`, `ElementKind`, `ActionElement`,
    `Source`, `Operand`, `Read`, `QueryOrPipeline`, `placeholders`, `bind`;
  - `lib.rs`: `pub mod pipeline`;
  - `Cargo.toml`: `serde_json` with `float_roundtrip`;
  - `error.rs`: `invalid_pipeline`.
- Depends on: Step 1.
- Proof: the `pipeline/mod.rs` tests (AC-21, AC-38).
- Rollback: the module is unused until Step 3.

### Step 3: Query and recipe conversions, representability, canonical form
- Files / symbols:
  - `pipeline/convert.rs`: `TryFrom<&Query>`, `to_query_with`, `NotRepresentable` /
    `Error::not_representable`, `ElementPath`, namespace placement, realm merging;
  - `Recipe::to_pipeline`, as an `impl` in `convert.rs`. It is written against today's fields
    and gains the `pipeline` field in Step 10.
- Depends on: Step 2.
- Proof: the `pipeline/convert.rs` tests (AC-25–AC-30, AC-40). The corpus is the parser's test
  queries plus `liquers-core/tests/query_backward_compatibility.rs`.
- Rollback: unused by planning until Step 8.

### Step 4: Registry snapshot and editors
- Files / symbols:
  - `commands.rs`: `CommandRegistry` caches an `Arc<CommandMetadataRegistry>` and invalidates it
    on every registration;
  - `context.rs`: `Environment::command_metadata_snapshot`, with a default implementation built
    from `get_command_metadata_registry().clone()`;
  - `pipeline/editor.rs`: `PipelineEditor`, `ActionEditor`, `QueryEditor`, `ArgRef`.
- Depends on: Step 3.
- Proof: the `pipeline/editor.rs` tests (AC-31, AC-32, AC-36, AC-37, AC-41).
- Rollback: additive.

### Step 5: `Value::Pipeline`
- Files / symbols:
  - `value.rs`: the variant, `TypeInfo`, `identifier`, `type_name`, `default_extension`,
    `default_filename`, `default_media_type`, serialization and deserialization;
  - `ValueInterface::try_into_pipeline` and `from_pipeline`, with defaults;
  - `liquers-lib/src/value/mod.rs`: the `ExtValue` delegation.
- Depends on: Step 2.
- Proof: `pipeline_value_type_info_and_conversions` and `pipeline_value_stored_and_read_back`
  (AC-39); `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert the variant; nothing else depends on it.

### Step 6: Pipeline parameter values and defaults
- Files / symbols:
  - `command_metadata.rs`: `CommandParameterValue::Pipeline`, its deserializer for
    `{pipeline: …}`, and `ArgumentInfo` checks;
  - `plan.rs`: `ParameterValue::{ParameterPipeline, OverridePipeline, DefaultPipeline,
    InputState}`, plus every match over `ParameterValue`;
  - `TryFrom<&ParameterValue> for Pipeline`;
  - match arms in `liquers-macro/src/registration.rs` and `liquers-py/src/command_metadata.rs`.
- Depends on: Step 3.
- Proof: `parameter_value_to_pipeline` (AC-18), `registry_export_round_trip` (AC-21); `cargo test
  -p liquers-core --lib --tests`.
- Rollback: the variants are produced only by later steps.

### Step 7: Extract `PlanAssembly`
- Files / symbols: in `plan.rs`, the fields of `PlanBuilder` that hold the plan under
  construction, its flags and `uncached_by`, plus `apply_command_flags` and the step push, move
  into a crate-private `PlanAssembly`.
- Depends on: none (it is a refactor).
- Proof: `cargo test -p liquers-core --lib --tests` unchanged, including `tests/command_alias.rs`
  and `tests/plan_default_namespaces.rs`.
- Rollback: revert; no behaviour changed.

### Step 8: `PipelinePlanBuilder` in `Delegate` mode, `Step::UseValue`, entry points
- Files / symbols:
  - `pipeline/plan.rs`: `PipelinePlanBuilder`, `PlanningMode`, `Pipeline::to_plan`;
  - `plan.rs`: `Step::UseValue`, and every `make_plan` / `Recipe::to_plan` path routed through
    the new builder;
  - `interpreter.rs`: `UseValue`, pipeline arguments as a nested plan or `Evaluate`;
  - match arms in `liquers-core/src/validate/report.rs`, `store/realm.rs`,
    `liquers-mcp/src/server.rs`, `liquers-py/src/command.rs` and
    `liquers-axum/src/api_core/overrides.rs`.
- Depends on: Steps 3, 6 and 7.
- Proof: the `tests/pipeline_planning.rs` tests except the dual-mode one (AC-5, AC-11, AC-16–AC-19,
  AC-23, AC-41); the whole `liquers-core` suite unchanged.
- Rollback: route the entry points back to `PlanBuilder`. That is one function.

### Step 9: `Native` mode and the dual-mode corpus test
- Files / symbols: `pipeline/plan.rs` gains native planning of resources, header instructions,
  `ns`, `q`, `v`, aliases and filenames.
- Depends on: Step 8.
- Proof: `plans_are_equal_in_both_modes` (AC-10). Switch the default to `Native` only when it is
  green on the whole corpus.
- Rollback: the default back to `Delegate`.

### Step 10: Recipe pipelines
- Files / symbols:
  - `recipes.rs`: `Recipe::pipeline`, its validation rules, `filename` / `data_format` / `key`
    from either part, `to_plan` through `PipelinePlanBuilder`;
  - `liquers-py` recipe access: field passthrough.
- Depends on: Step 8.
- Proof: the `tests/recipe_pipelines.rs` tests (AC-15, AC-30); `tests/recipe_*.rs` unchanged.
- Rollback: the field is optional; revert it.

### Step 11: Pipeline commands
- Files / symbols:
  - `command_metadata.rs`: `CommandDefinition::Pipeline` and the checks;
  - `commands.rs`: `register_pipeline`;
  - `plan.rs`: `Step::PipelineCommand`, expansion in both builders, `cut_predecessor`,
    `finalize_plan` and the dependency scan;
  - `interpreter.rs`: the input frame;
  - `error.rs`: `pipeline_cycle`, `with_pipeline`, `invalid_argument`.
- Depends on: Step 8.
- Proof: the `tests/pipeline_commands.rs` tests (AC-1–AC-4, AC-6–AC-8, AC-20, AC-21).
- Rollback: unregistered when absent; revert the variant.

### Step 12: `liquers-validate` pipeline support
- Files / symbols: in `liquers-core/src/validate/`, `--pipeline`, `--emit` and `--build`, plus
  `--recipe` accepting `pipeline`.
- Depends on: Steps 3 and 10.
- Proof: the `tests/validate_pipelines.rs` tests (AC-35).
- Rollback: the flags are additive.

### Step 13: Cross-crate arms, registry export, polars integration test, build matrix
- Files / symbols:
  - the `liquers-lib` exporter;
  - `specs/command_registry.yaml`, regenerated if the export shape changed;
  - `liquers-lib/tests/pipeline_polars.rs`.
- Depends on: Steps 5, 10 and 11.
- Proof: `top_in_matches_long_query` and `recipe_pipeline_eu_csv`; `cargo test -p liquers-lib
  --lib --tests`; `bash scripts/check-build-matrix.sh`; the liquers-web Node loop.
- Rollback: test-only, plus regenerated files.

## Testing Plan

After each step: `cargo test -p liquers-core --lib --tests`. After Steps 5, 8, 11 and 13 also run
`cargo test -p liquers-lib --lib --tests`. After Step 1 run `cargo test -p liquers-template`.
After Steps 5, 6, 8 and 13 run `bash scripts/check-build-matrix.sh`, because new variants reach
feature-gated matches. The liquers-web Node loop runs after `cargo clean` at Steps 1 and 13. The
disk budget limits how much runs together (CLAUDE.md), so run `cargo clean` between the native
and wasm loops.

## Rollback Plan

Each step is its own commit. Up to Step 7 nothing changes behaviour, and any prefix of the steps
can ship. From Step 8 on, the planning entry points are the single switch: routing them back to
`PlanBuilder` restores the old planning, and the pipeline types stay as unused library code. When
only some steps land, the unshipped remainder becomes an issue (§5.6), named in Phase 5.

## Documentation Updates

- Create `reference/PIPELINES.md` and `guides/PIPELINE_GUIDE.md`.
- Update every `affects_docs` document listed in Phase 2 §Documentation Architecture, each with a
  History row and a `reviewed:` bump.
- Update the user-guide chapters' `sources:` and review the chapters that `chapters-for` lists.
- Update CLAUDE.md §Validating queries and the `liquers-validate` skill for `--pipeline`, `--emit`
  and `--build`.
- Regenerate `specs/command_registry.yaml` if the export changed, and `specs/index.csv`.
- Update `specs/README.md`'s capability line.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated, or the remainder filed as issues
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
- [ ] Main merged again before Phase 5 (the maintainer's request, 2026-11-03)
