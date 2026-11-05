# Phase 5: Documentation - liquers-py-pipelines

## Completion Preconditions

- [x] Implementation is finished and validated (commit `44ce399d`; `bash scripts/test-liquers-py.sh`:
  500 passed, 23 skipped, Rust unit tests pass)
- [x] All user comments are answered or incorporated (Orest 2026-11-04: Q1, Q2, Q5, Q6 agreed;
  `PY-HAS-NO-VALIDATING-QUERY-EDITOR` merged into this design)
- [x] All review comments are answered or incorporated (two reviewers per phase, Phases 1-4)
- [x] Documentation is consistent with the implemented and tested behavior (commit `572531f0`)
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Python passes, evaluates, converts, edits and registers pipelines, as approved.

- **`lq.Pipeline`** (`liquers-py/src/pipeline.rs`): frozen, compared by value; built from a list,
  a dict in the JSON form, or `Pipeline.from_query` / `from_json`; `to_query(env)`, `to_json`,
  `to_list`. Every Python value bound for a JSON slot goes through one walker (`json_slot`) that
  refuses what JSON cannot hold and names the path.
- **Evaluation.** `Environment.evaluate` / `evaluate_state` and their `_async` forms take a
  pipeline and run it as a pipeline-only recipe through `apply_queued` (Decision 3): an asset of its
  own, a job-queue slot, and on Ctrl-C the asset is cancelled (`closure.rs` `wait_for_asset`).
  `Context.evaluate` evaluates a representable pipeline as its query and any other through
  `apply_ad_hoc`, after liquers-py's relative-key check. `get_asset` refuses a pipeline.
- **Editors.** `env.pipeline_editor()`, `env.query_editor()` and `lq.ActionEditor` wrap the core
  editors; each call edits a clone and keeps it only on success.
- **Pipeline commands.** `env.pipeline_command(name, body, arguments=[…])` is a
  `DeclaredCommand::Pipeline`, kept across rebuilds like a template command.
- **Value bridge.** A pipeline value, `Recipe.pipeline`, a pipeline `ArgumentInfo.default` and
  `CommandDefinition.pipeline` are `lq.Pipeline` (they were `dict`s; no existing test read them).

Nothing was omitted. Added after the Phase 5 review: `evaluate` takes a tuple, as `lq.Pipeline`
does; the `environment.rs` and `context.rs` module docs now describe the pipeline entry points and
which waits cancel. Corrections against Phase 3 (recorded in its Learning Log): a relative key is
`./x.csv`, not `x.csv`; the top-level relative case reads under a `cwd` element; `to_query` writes
the canonical query, so the round trip checks that the canonical form is stable.

## Documentation Delivered

### New Reference Documents
None: the contract is §16 of the existing Python reference.

### New Guide Documents
None: the usage is a section of the existing Python guide, and `PIPELINE_GUIDE.md` §8.

### Existing Documents Reviewed or Updated
`affects_docs`, each reviewed against the code and tests, with a History row and `reviewed:`
2026-11-04:
- `specs/reference/PYTHON_INTEGRATION.md` §16 Pipelines; §2, §3 and §5 (the `Pipeline` rows).
- `specs/guides/PYTHON_GUIDE.md` §6 Pipelines (two examples, run by `test_GUIDE_examples.py`).
- `specs/guides/PIPELINE_GUIDE.md` §8 From Python; `specs/reference/PIPELINES.md` §Related.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE (liquers-py implements it) and §6 Python
  *Pipelines*.
- `liquers-py/README.md`: `PIPELINE` row, `COMPLETE`, tests `PIPELINE01`-`05` mapped.
- `specs/README.md`: see Links.

### Module Docs
New: `liquers-py/src/pipeline.rs`. Changed: `environment.rs`, `context.rs`, `closure.rs`,
`declared.rs`, `command_metadata.rs`, `lib.rs`; `value.rs` and `recipes.rs` brought up to the
guide. `check_api_docs.py` on each: 0 errors, 0 warnings.

### User-Guide Chapters
`docs_index.py chapters-for` lists chapters of the three guides (among them `04-values.md`,
`05-commands.md`, `a-query-syntax.md`); all are `status: outline`, so no chapter review was due.

### Links and Capability Map
`specs/README.md`: the M6 entry now reads *built* and links `PYTHON_INTEGRATION.md` §14-§16,
`PYTHON_GUIDE.md`, `ASSETS.md` §Closure assets and `TEMPLATE_GUIDE.md`, with the designs in
parentheses.

## Issues Filed

- `TOP-LEVEL-RELATIVE-RESOURCE-QUERY-LOOKS-UP-A-RELATIVE-RECIPES-KEY`: with no working directory,
  `-R/./x.txt` fails on `./recipes.yaml` while the same pipeline resolves against the root.
- Filed in Phase 2 and linked: `CONTEXT-APPLY-AD-HOC-SKIPS-RELATIVE-PIPELINE-KEYS` (worked around
  here), `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED`.

## Important Learning

- A pipeline-only recipe is the existing way to run a pipeline through the asset harness, at the
  top level (`apply_queued`) and in a command (`apply_ad_hoc`); `evaluate_pipeline` would have
  bypassed status, cancellation and the job queue.
- The core editors are `Clone`, so clone-edit-commit makes a multi-step Python call atomic.
- Inherent methods win over trait methods on a concrete type: inside the generic editor helpers
  the trait method must be called by path (`CoreEditor::push(e, …)`).

## Conformance and Remaining Work

Requested: pipelines in liquers-py (`LANGUAGE-INTEGRATION_GUIDE.md` §PIPELINE) and a validating
query editor. Approved: Phases 1-4 with Decision 3. Implemented: AC-1 to AC-10. Nothing remains
outside the linked issues; `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` and
`PY-HAS-NO-VALIDATING-QUERY-EDITOR` are closed.

## Validation

`bash scripts/test-liquers-py.sh` (500 passed); `check_api_docs.py` on the touched modules;
`python3 scripts/docs_index.py --check` (0 errors).
