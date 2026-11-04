# Phase 4: Implementation Plan — liquers-py-templates

## Overview

This design lands after `ad-hoc-closure-asset`, `template-render-in-env` and
`liquers-py-data-nodes` have merged (each its own PR), and before `liquers-py-pipelines`. Order
inside the PR:
1. The in-command render, which needs only `render_in_context` and the adapter.
2. The top-level render and its waits (`wait_for_run`, the async path through `awaitable_run`).
3. Validation.
4. Declared commands: the slot, `declare`, `template_command`, `CommandDefinition` accessors.
5. Link defaults (`lq.link`), tested through both renders.

Steps 5 and 7 land before Step 3's proof runs, since `test_TEMPLATE_top_level.py` uses a template
command (`show`); a step's proof runs once everything it depends on is in.

Every new Python name is exported from `__init__.py` (and `__all__`) with a docstring and declared
in the stubs, since `test_DOCS_docstrings.py` and stubtest check them.

## Progress

- [x] Step 1: request building and frames (`template.rs`)
- [x] Step 2: `Context.render` / `expand` and `_async`
- [x] Step 3: `closure::wait_for_run`; `Environment.render` / `expand` and `_async` (`test_unstarted_render_is_cancelled` is a Rust unit test, `template.rs` `unstarted_render_is_cancelled`: Python cannot make a run whose asset is cancelled before its claim)
- [x] Step 4: `validate_template` and `lq.Diagnostic`
- [x] Step 5: declared commands and `template_command` (the slot lives in a new `declared.rs`; an alias with no explicit arguments takes the target's remaining ones; `Replaced::Declared` lets the decorator warn `replaces a declared command`)
- [x] Step 6: `lq.link` defaults
- [x] Step 7: `CommandDefinition` accessors
- [x] Step 8: conformance row, docs, module docs

## Implementation Steps

### Step 1: request building and frames
- Files / symbols: `liquers-py/src/template.rs` (new) `template_source`, `frames`, `RenderResult`;
  `lib.rs`.
- Change: Python arguments to `RenderRequest` per Phase 2 (`state` converted or taken from an
  `lq.State`, `data` an adapter node that must be a `Map`).
- Depends on: `template-render-in-env` and `liquers-py-data-nodes` merged.
- Proof: step-local Rust unit tests in `template.rs` `tests` for the frame layouts of `render` and
  `expand` and the `TypeError` cases;
  `cargo test -p liquers-py --no-default-features --features debug-handles --lib template` (the
  form `scripts/test-liquers-py.sh` uses; `extension-module` does not link a test).
- Rollback: delete the module.

### Step 2: in-command render
- Files / symbols: `context.rs` `render`, `expand`, `render_async`, `expand_async`; `aio.rs`
  `ToPy::Rendered`.
- Change: `block_on(render_in_context(core_context, request))` after `check_blocking_call`; the
  async form through `aio::awaitable` (a `render_in_context` future is not a closure run).
- Depends on: Step 1.
- Proof: `test_TEMPLATE_commands.py::test_in_command_render_records_dependencies`,
  `test_nested_python_renders`, `test_sync_form_in_async_command_raises`; the `context` half of
  `test_async_forms_agree` (`render_async` and `expand_async`).
- Rollback: remove the methods.

### Step 3: top-level render
- Files / symbols: `closure.rs` `wait_for_run` (new); `environment.rs` `render`, `expand`, `_async`.
- Change: `render_top_level` + `wait_for_run` (sync) or `awaitable_run` (async); `Ok(None)` and a
  cancellation error raise `lq.CancelledError`; the settle after an interrupt defers signals,
  bounded by `STOP_GRACE`.
- Depends on: Steps 1, 2 (`test_two_frames_at_top_level` compares with `context.render`), 5 and 7.
- Proof: `test_TEMPLATE_top_level.py` (all, including the interrupt and `Ok(None)` cases),
  `test_TEMPLATE_commands.py::test_builtin_commands`.
- Rollback: remove the methods.

### Step 4: validation
- Files / symbols: `template.rs` `Diagnostic` (new); `environment.rs` / `context.rs`
  `validate_template` (new).
- Change: `parse_template` and the core validation against the registry, each diagnostic as an
  `lq.Diagnostic`; nothing is evaluated.
- Depends on: Step 1.
- Proof: `test_TEMPLATE_validate.py::test_validation_runs_nothing`.
- Rollback: remove the class and the methods.

### Step 5: declared commands
- Files / symbols: `environment.rs` `DeclaredCommand::Alias`, `Declarations::declared`,
  `EnvCell::declare` (all new), `configured_builder`, `unregister`, the Python-command registration path
  (symmetric replacement); `Environment.template_command`.
- Change: as Phase 2; the trial `configured_builder` + `validate()` before storing.
- Depends on: none in this design (parallel with Steps 1-4).
- Proof: `test_TEMPLATE_commands.py::test_template_command_needs_no_function` (without its
  metadata assertions, which are Step 7's), `test_template_command_survives_rebuild`,
  `test_template_link_must_be_text`, `test_editing_the_template_invalidates`,
  `test_declare_cost`, `test_declared_and_python_commands_replace_each_other` (last registration
  wins), `test_unregister_declared`, `test_failed_declare_changes_nothing`.
- Rollback: remove the slot; nothing else reads it yet.

### Step 6: `lq.link`
- Files / symbols: `query.rs` `Link`, `link`, `Link._declaration`; `environment.rs` `py_to_json`
  (`default=` hook); `_commands.py` `_DEFAULT_TYPES`.
- Depends on: Steps 2 and 3 (its test renders through both).
- Proof: `test_TEMPLATE_commands.py::test_link_default_cwd`; the existing
  `test_COMMAND_declaration_edges.py` still passes.
- Rollback: revert the step's commit (`Link`, `link`, the `py_to_json` hook, `_DEFAULT_TYPES`).

### Step 7: `CommandDefinition` accessors
- Files / symbols: `command_metadata.rs` `CommandDefinition` `kind`, `alias_of`,
  `head_parameters`, `pipeline` (returns `None` until `liquers-py-pipelines`).
- Depends on: Step 5.
- Proof: the metadata assertions in `test_template_command_needs_no_function`.
- Rollback: remove the getters.

### Step 8: records and docs
- Files / symbols: `liquers-py/README.md` TEMPLATE row; the documents below; module docs.
- Depends on: Steps 1-7.
- Proof: `python3 scripts/check_api_docs.py` on each touched module; `python3 scripts/docs_index.py --check`.
- Rollback: documentation only.

## Testing Plan

Each step's proof as it lands; at the end `bash scripts/test-liquers-py.sh` (3.14 locally; CI adds
3.10 and 3.14t). No Rust crate outside `liquers-py` changes, and no feature flag.

## Rollback Plan

The PR reverts as a whole. Steps 1-4 alone give rendering without template commands; Step 5 is
self-contained and Step 6 independent.

## Documentation Updates

- `specs/reference/PYTHON_INTEGRATION.md` §Templates (escaping difference, declared commands, link
  defaults, the pycall-alias caveat); `specs/guides/PYTHON_GUIDE.md` "Templates" chapter;
  `specs/guides/TEMPLATE_GUIDE.md` §3; `specs/guides/COMMAND_REGISTRATION_GUIDE.md` (link defaults
  from Python); `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE, §6 Python. Each gets a
  History row and a `reviewed:` bump.
- `liquers-py/README.md` conformance row; `specs/README.md` M6 links; `specs/index.csv` regenerated.
- Module docs per Phase 2; the user guide chapters `docs_index.py chapters-for` lists, in Phase 5.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
