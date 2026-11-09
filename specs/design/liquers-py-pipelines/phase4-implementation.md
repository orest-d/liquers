# Phase 4: Implementation Plan — liquers-py-pipelines

## Overview

The last M6 design: it lands after `liquers-py-templates` (the declaration slot, `declare`, the
`CommandDefinition` accessors) and `liquers-py-data-nodes` (`closure.rs`). Order inside the PR:
1. `lq.Pipeline` and the JSON walker, which everything else reads through.
2. The value bridge, which changes a type existing tests see.
3. Evaluation entry points.
4. Editors.
5. Pipeline commands.

Every new Python name is exported with a docstring and declared in the stubs.

## Progress

- [x] Step 1: `lq.Pipeline`, `pipeline_from_py`, `json_slot`
- [x] Step 2: `Pipeline` across the value bridge, `Recipe.pipeline`, `ArgumentInfo.default`
- [x] Step 3: `evaluate*` accept pipelines (`apply_queued`, `wait_for_asset`)
- [x] Step 4: `Context.evaluate` accepts pipelines (query or `apply_ad_hoc`, relative-key check)
- [x] Step 5: `PipelineEditor`, `QueryEditor`, `ActionEditor`
- [x] Step 6: `pipeline_command` and `CommandDefinition.pipeline`
- [x] Step 7: conformance row, docs, module docs

## Implementation Steps

### Step 1: `lq.Pipeline`
- Files / symbols: `liquers-py/src/pipeline.rs` (new) `Pipeline`, `pipeline_from_py`,
  `json_slot`, `JsonPath`; `lib.rs`.
- Change: the class members of Phase 2; NaN and ±inf as the reference's strings; paths in errors.
- Depends on: none.
- Proof: `test_PIPELINE_convert.py` (all five tests).
- Rollback: delete the module.

### Step 2: the value bridge
- Files / symbols: `value.rs` `simple_to_py`, `wrapper_to_simple`; `recipes.rs` `Recipe.pipeline`;
  `command_metadata.rs` `ArgumentInfo.default`.
- Depends on: Step 1.
- Proof: `test_PIPELINE_evaluate.py::test_pipeline_values_keep_their_type`; the existing suite,
  with tests that read a pipeline as a `dict` updated and listed in the commit message.
- Rollback: revert the commit (the `dict` form returns).

### Step 3: top-level evaluation
- Files / symbols: `environment.rs` `Evaluable`, `to_evaluable`, `pipeline_recipe` (all new),
  `evaluate`, `evaluate_state`, `evaluate_async`, `evaluate_state_async`, `get_asset` (refusal);
  `closure.rs` `wait_for_asset` (new).
- Setup check: `cargo run -p liquers-core --features cli --bin liquers-validate -- --pipeline
  <Example 1 as YAML>` gives `Ok` and the problem query as `encoded`.
- Change: `apply_queued(pipeline_recipe(p), State::new())`, then `get`; an interrupted sync wait
  cancels the asset.
- Depends on: Steps 1-2.
- Proof: `test_PIPELINE_evaluate.py::test_pipeline_evaluates_as_its_query`,
  `test_failing_step_raises_like_its_query`, `test_structured_argument_arrives_whole`,
  `test_get_asset_refuses_a_pipeline`, `test_relative_key_pipeline`,
  `test_interrupt_cancels_the_pipeline`.
- Rollback: `to_evaluable` refuses pipelines again.

### Step 4: in-command evaluation
- Files / symbols: `context.rs` `evaluate`, `evaluate_async`; `pipeline.rs`
  `pipeline_has_relative_key` (new).
- Change: representable → its query; otherwise `apply_ad_hoc`, after the relative-key check.
- Depends on: Step 3.
- Proof: `test_PIPELINE_evaluate.py::test_non_representable_in_a_command`, and
  `test_relative_key_in_a_command_is_refused`.
- Rollback: refuse non-representable pipelines with `NotSupported` (Phase 1's first answer).

### Step 5: editors
- Files / symbols: `pipeline.rs` `PipelineEditor`, `QueryEditor`, `ActionEditor`, `Owner` (all
  new); `environment.rs` `pipeline_editor`, `query_editor` (new); `aio.rs` `lock_py_attached`
  (new, factored out of the private `lock` there, so `aio.rs` and the editors share it).
- Change: convert arguments, then `lock_py_attached`, clone, edit, commit.
- Depends on: Step 1 (parallel with Steps 2-4).
- Proof: `test_PIPELINE_editors.py` (all four tests, among them `test_stale_action_editor` and
  `test_editor_threads`).
- Rollback: remove the classes.

### Step 6: pipeline commands
- Files / symbols: `environment.rs` `DeclaredCommand::Pipeline` (new variant of the
  `liquers-py-templates` enum), its arm in `configured_builder`
  and `declare`, `Environment.pipeline_command`; `command_metadata.rs` `CommandDefinition.pipeline`.
- Depends on: Step 1; `liquers-py-templates` merged.
- Proof: `test_PIPELINE_commands.py` (both tests, the first also after a rebuild).
- Rollback: revert the step's commit (variant, arm, `pipeline_command`, the getter).

### Step 7: records and docs
- Files / symbols: `liquers-py/README.md` PIPELINE row; the documents below; module docs.
- Depends on: Steps 1-6.
- Proof: `python3 scripts/check_api_docs.py` on each touched module; `python3 scripts/docs_index.py --check`.
- Rollback: documentation only.

## Testing Plan

Each step's proof as it lands; at the end `bash scripts/test-liquers-py.sh` (CI: 3.10, 3.14,
3.14t). No Rust crate outside `liquers-py` changes.

## Rollback Plan

The PR reverts as a whole. Steps 1-3 alone give pipelines as values and top-level evaluation;
Step 5 (editors) and Step 6 (commands) are independent of each other.

## Documentation Updates

- `specs/reference/PYTHON_INTEGRATION.md` §Pipelines and §5's conversion rows;
  `specs/guides/PYTHON_GUIDE.md` "Pipelines" chapter; `specs/guides/PIPELINE_GUIDE.md` (the Python
  form); `specs/reference/PIPELINES.md` §Related; `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`
  §PIPELINE, §6 Python. Each gets a History row and a `reviewed:` bump.
- `liquers-py/README.md` conformance row; `specs/README.md` M6 links; `specs/index.csv` regenerated.
- Module docs per Phase 2; the user guide chapters `docs_index.py chapters-for` lists, in Phase 5.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated (`bash scripts/test-liquers-py.sh`: 498 passed, 23
  skipped; the 20 `test_PIPELINE_*` tests among them)
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
