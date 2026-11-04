---
id: WEB-PIPELINES
kind: design
title: Pipelines from JavaScript - evaluate, convert, edit and register
form: compact
workflow: liquers-project
status: complete
readiness: ready
autofix: not-eligible
area: [web]
issues: [PIPELINES-NOT-IN-LANGUAGE-BINDINGS]
gh_pr: [256]
affects_docs: [specs/reference/JAVASCRIPT_INTEGRATION.md, specs/guides/JAVASCRIPT_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-11-04
---
# Pipelines from JavaScript - evaluate, convert, edit and register

The `liquers-web` half of `PIPELINES-NOT-IN-LANGUAGE-BINDINGS`; `liquers-py` stays with the issue.
Written and implemented together on 2026-11-04 (accepted by the owner the same day) at the owner's request ("design and implement the
pipeline support"), so Phases 1-4 record what was built and why, and Phase 5 what it changed.

## Phase 1: High-Level Design

### Purpose

`LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE asks a binding to evaluate a pipeline, convert between
it and a query, edit it with core's checks, and register a pipeline command, with tests
`PIPELINE01`–`05`. `liquers-web` did none of it: a command's `Value::Pipeline` result reached a page
as its JSON object, and nothing went the other way.

### Problem Example

```js
// Before: none of these exist, and a structured argument cannot be passed at all.
await liquers.evaluatePipeline([{ action: "describe", arguments: { options: { value: { count: 3 } } } }]);
liquers.registerCommand({ name: "loud_greet", pipeline: [/* … */], arguments: [{ name: "who" }] });
```

### Scope and Acceptance Criteria

- **AC-1** Evaluate
  WHEN a page evaluates a pipeline (object or array form) THEN the result equals evaluating its
  query, and a failure rejects with a `LiquersError`.
- **AC-2** Structured argument
  WHEN a pipeline gives an `any` argument a map THEN the command receives the map whole, numbers
  keeping their JSON types; a value with no JSON form (a `Map`) is a `conversion_error`.
- **AC-3** Conversions
  WHEN a page converts a query to a pipeline and back THEN it gets plain objects and the same text;
  a pipeline with no query form throws `parameter_error` naming the rule.
- **AC-4** Pipeline command
  WHEN a declaration has `pipeline` in place of `run` THEN the command plans and runs as the long
  query, with defaults, after a rebuild, and in any order within its batch.
- **AC-5** Refused pipeline command
  WHEN its step is unknown, a placeholder is undeclared, or `run` is given beside `pipeline` THEN
  the batch is refused whole with a report naming each problem.
- **AC-6** Editors
  WHEN an edit of a `PipelineEditor` or `QueryEditor` is refused THEN it throws and the pipeline is
  unchanged; a `QueryEditor` refuses an edit that leaves no query form.

Out of scope: a pipeline passed *as a value* to a command (`WEB-PAGE-CANNOT-PASS-A-PIPELINE-VALUE`),
a definition-mode editor (`WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`), `liquers-py`.

### Documentation Intent

`JAVASCRIPT_INTEGRATION.md` (contract), `JAVASCRIPT_GUIDE.md` §6 (runnable examples), the guide's
§PIPELINE status line, `liquers-web/README.md` conformance row.
- API docs: `liquers-web/src/pipeline.rs` (new module doc); `liquers-web/src/lib.rs` (main
  components list); `liquers-web/src/command/spec.rs` (the declaration gains `pipeline`).

### Design Readiness

Ready: core has every piece (`Pipeline::from_json`/`to_json`, `interpreter::evaluate_pipeline`,
`to_query_with`, `PipelineEditor`/`QueryEditor`, `complete_pipeline_command`,
`check_pipeline_command`). Decisions applied from project memory: new calls are `Environment`
methods as well as module functions; registration stays the existing `registerCommands`
(refused on `new Environment()`); reports are core's `IssueReport`.

## Phase 2: Architecture

### Solution

The guide's four questions, answered:

1. **Type.** No pipeline class; the native structure is the JSON form, typed `LiquersPipeline` in
   the `.d.ts`. Rejected: a class (a handle to free, and a second shape beside the serialized one).
2. **Editors.** Wrapped, not reimplemented. One method set for both classes (`editor_methods!` over
   an `Editing` trait). Core's `ActionEditor` borrows the editor, which a JavaScript object cannot
   hold, so `action()` returns the action's index and the argument edits take it. Rejected: a
   JavaScript `ActionEditor` object (a borrow across calls).
3. **Not representable.** Core's `not_representable` error as a `LiquersError` of type
   `parameter_error`; no new error type.
4. **Placeholder values.** JSON only, by the value bridge's structural rules
   (`bridge::js_value_to_json`); a `Map`, class instance or opaque value is a `ConversionError`.

Pipeline commands reuse the declaration path: `pipeline` in place of `run`, so `validateCommands`
reports (`ENVIRON08`), the rebuild replay and `unregisterCommand` all apply unchanged.

### Changes

- `src/pipeline.rs` (new): `pipeline_from_js`, `pipeline_to_js`, `evaluate_pipeline`,
  `evaluate_pipeline_to_promise`, `pipeline_to_query`, `query_to_pipeline`;
  `LiquersPipelineEditor` (`PipelineEditor`), `LiquersQueryEditor` (`QueryEditor`).
- `src/lib.rs`: `evaluatePipeline`, `pipelineToQuery`, `queryToPipeline`.
- `src/environment.rs`: `current_metadata_registry` (reads the pending registry without sharing
  it); `Environment.evaluatePipeline`, `pipelineToQuery`, `pipelineEditor`, `queryEditor`,
  `metadata_snapshot`; `validate_batch` checks pipeline commands against the environment's registry
  plus the batch; `register_commands_on` registers function commands first (stable sort).
- `src/command/spec.rs`: `JsCommandSpec::run` becomes `Option`; `is_pipeline`;
  `from_pipeline_metadata`; `check_inner` reads `pipeline`, refuses `run`/`state`/`async` beside it.
- `src/command/import.rs`: imported `!Pipeline` metadata needs no implementation.
- `src/command/adapter.rs`: `register_pipeline_metadata` (complete, unregister, add; no re-check).
- `src/bridge.rs`: `js_value_to_json`. `src/typescript.rs`: `LiquersPipeline`,
  `LiquersPipelineElement`, `LiquersPipelineSource`, `LiquersPipelineCommandSpec`.

### Risks

Registration does not re-check a pipeline command: validation did, against the batch, so replay
cannot fail on order. The cost is that a pipeline command whose step is later unregistered fails
when planned, not when registered, as in core. `current_metadata_registry` clones the pending
registry on each call; fine for editors, not for a hot loop.

## Phase 3: Examples and Tests

### Examples

The three examples of `JAVASCRIPT_GUIDE.md` §6 "Pipelines", run under Node by
`scripts/test-guide-examples.sh`.

### Tests

`liquers-web/tests/pipeline_PIPELINE.rs`, under Node:
`pipeline01_evaluating_a_pipeline_equals_its_query` (AC-1, AC-3),
`pipeline02_map_argument_reaches_any_argument_whole` (AC-2),
`pipeline03_non_representable_pipeline_names_the_rule` (AC-3, AC-6),
`pipeline04_pipeline_command_runs_as_the_long_query` (AC-4),
`pipeline04_invalid_pipeline_commands_are_refused_with_a_report` (AC-5),
`pipeline05_refused_edit_leaves_the_pipeline_unchanged` (AC-6). `tests/stubs/valid_usage.ts`
type-checks the new declarations (`STUBS02`).

```
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test pipeline_PIPELINE
```

## Phase 4: Implementation Plan

### Steps

- [x] 1. `src/bridge.rs` `js_value_to_json`; `src/pipeline.rs` conversions and evaluation —
  `pipeline01`, `pipeline02`, `pipeline03`
- [x] 2. `src/command/{spec,import,adapter}.rs`, `src/environment.rs` batch check and order —
  `pipeline04_*`
- [x] 3. `src/pipeline.rs` editor classes; `Environment` methods — `pipeline05`
- [x] 4. `src/typescript.rs`, `tests/stubs/valid_usage.ts` — `check-stubs.sh` (see Phase 5)
- [x] 5. Documents, README row, issues — `docs_index.py --check`, `check_api_docs.py`

### Validation

Both Node loops (`--features debug-handles`, and `debug-handles,records`), the guide examples,
`check_api_docs.py` on the touched modules, `docs_index.py --check`.

## Phase 5: Documentation

Built as designed. Results are in the PR. Documents changed: `reference/JAVASCRIPT_INTEGRATION.md`
(§2, §4 pipeline commands, new "Pipelines", limitations), `guides/JAVASCRIPT_GUIDE.md` §6
"Pipelines", `guides/LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE status line, `liquers-web/README.md`
conformance row (`PIPELINE` complete). The user guide chapter `docs/guides/web/03-queries.md`
keeps its outline line, now without "Not yet available in this binding", for whoever writes the
chapter. Filed: `WEB-PAGE-CANNOT-PASS-A-PIPELINE-VALUE`, `WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`.
`PIPELINES-NOT-IN-LANGUAGE-BINDINGS` is `in_progress`, narrowed to `liquers-py`.
