# Phase 5: Documentation - Pipelines

## Completion Preconditions

- [x] Implementation is finished and validated — Steps 1–13 of Phase 4 (commits 02032d7 … d8d8533);
  core, lib, mcp and records suites pass after the merge of main (57e5bcd); build matrix and
  liquers-web Node loop: see §Validation
- [x] All user comments are answered or incorporated — reviews 1–8 and the 2026-11-03 decision log
  in Phase 1; "pull fresh from main before phase 5" done in 57e5bcd
- [x] All review comments are answered or incorporated — PR #238's one review thread (on Phase 2, now outdated) was answered in f293a49
- [x] Documentation is consistent with the implemented and tested behavior — every claim below was
  checked against the code at HEAD, not the design
- [x] Documentation is included in the implementation PR — PR #238

## Implementation Summary

A **pipeline** (`liquers-core/src/pipeline/`) is the structured, serializable form of a query: a
list of elements in one realm, where an action's argument is a value of any JSON shape, a
placeholder, a link, a nested pipeline or the input state, and a value can become the state. It
converts to and from a query under explicit representability rules (P/E/S/N ids) and plans
through `PipelinePlanBuilder`, which plans every query exactly as `PlanBuilder` does (checked
over the parser corpus in both modes) and is now the planner behind `make_plan` and
`Recipe::to_plan`. On top of it:

- **Pipeline commands** (`CommandDefinition::Pipeline`, `register_pipeline`): expanded at planning
  into one `Step::PipelineCommand`, run in a lexical `PipelineFrame`, cached under the calling query,
  invalidated when the body changes; cycles and depth over 16 are `pipeline_cycle`.
- **Recipe pipelines** (`Recipe::pipeline`): run before the query, literal values only; this is how
  an earlier action gets a structured argument (`RECIPE-OVERRIDE-ONLY-LAST-ACTION`).
- **Editors**: `PipelineEditor` and `QueryEditor`, checked against a registry snapshot; the query
  builder of `QUERY-BUILDER-TOOLING`.
- **Values and validation**: `Value::Pipeline`; `liquers-validate --pipeline`, `--build`,
  `--emit query|pipeline|plan`.

**Conformance.** The implementation matches the approved Phase 1 (AC-1–AC-41) and Phase 2
contracts. The deviations are implementation decisions recorded in Phase 4 §Implementation Notes;
none changes an approved contract. Two are visible to users: `encodeParam` in liquers-web no longer
throws (every string is representable), and a recipe override of a query whose last action is a
pipeline command is not applied yet (filed). The web API part was moved out at Phase 1
(`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`), and so were the language bindings
(`PIPELINES-NOT-IN-LANGUAGE-BINDINGS`); liquers-py only exposes `Recipe.pipeline` read-only.

## Documentation Delivered

### New Reference Documents

- `specs/reference/PIPELINES.md` — the model and serialized form, conversions and
  representability rules, planning, pipeline commands, recipe pipelines, added types, validation,
  editors.

### New Guide Documents

- `specs/guides/PIPELINE_GUIDE.md` — choosing between alias, pipeline command and Rust; writing,
  evaluating, converting and editing pipelines; defining a pipeline command; recipe pipelines;
  validating; testing.

### Existing Documents Reviewed or Updated

`affects_docs` is authoritative. Each document below was checked against the code, updated, and
given a History row and `reviewed: 2026-11-04`:

| Document | Change |
|---|---|
| `reference/PROJECT_OVERVIEW.md` | `pipeline/` module, pipelines, pipeline commands, `Recipe::pipeline`, raw-emission caveat, glossary |
| `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | header names as a pipeline's realm; `check_encodable` and `QueryEditor` |
| `reference/COMMAND_ALIASES.md` | `CommandDefinition::Pipeline`; an alias cannot target one |
| `reference/COMMAND_DECLARATION.md` | §4.1 `definition: Pipeline`; `!pipeline` defaults |
| `reference/RECIPES_AND_PLANS.md` | §Recipe pipelines, `PipelinePlanBuilder`, new steps and parameter values, limitation closed |
| `reference/TEMPLATE_LANGUAGE.md` | shared token predicates; structured values for earlier actions |
| `reference/VALUE_TYPE_SYSTEM.md` | the `Pipeline` identifier |
| `reference/REALMS.md` | §Pipelines as realm scopes; `store_config` refusals |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | §Registering a pipeline command |
| `guides/COMMAND_DESIGN_GUIDE.md` | §Alias, pipeline command or Rust |
| `guides/QUERY_ESCAPING_GUIDE.md` | `QueryEditor` first, then `--build`, then `encode()` |
| `guides/TYPE_SYSTEM_GUIDE.md` | `Pipeline` as a worked core variant |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | new feature `PIPELINE` with tests `PIPELINE01`–`05` |

Added beyond the Phase 2 list, because the implementation changed them: `reference/JAVASCRIPT_INTEGRATION.md`
and `guides/JAVASCRIPT_GUIDE.md` (`encodeParam` never throws). Also updated: `CLAUDE.md`
§Validating queries and the `liquers-validate` skill (`SKILL.md`, `references/output-format.md`:
`--pipeline`, `--build`, `--emit`, the `pipeline` field, `NotRepresentable`, the two new steps).

### Module Docs

Brought up to `API_DOCUMENTATION_GUIDE.md` §3 (all five parts, executed example):
`liquers-core/src/pipeline/mod.rs` and `liquers-core/src/validate/mod.rs`. Already conforming:
`pipeline/editor.rs`, `pipeline/convert.rs`, `pipeline/plan.rs`. `check_api_docs.py` reports 0
warnings for these five.

**Not brought up**, a deliberate deviation from §11: the other 32 modules the design touched
(`check_api_docs.py` over them: 82 warnings). They changed by a match arm, a variant or a helper
(`plan.rs`, `interpreter.rs`, `command_metadata.rs`, the `liquers-lib` value modules, liquers-py,
liquers-mcp…); most have no module doc at all, so bringing each up is the backlog
`MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`, not this design.

### User-Guide Chapters

`docs_index.py chapters-for <affects_docs>` lists 25 chapters (rust, python, web: `01-introduction`,
`02-installing` (rust only), `03-queries`, `04-values`, `05-commands`, `07-assets`, `10-templates`
(rust and python), `15-troubleshooting` (rust only), `a-query-syntax`, `c-glossary`). **All are
`status: outline`**, so none is reviewed. As Phase 2 planned, `reference/PIPELINES.md` (and, for
`05-commands`, `guides/PIPELINE_GUIDE.md`) was added to the `sources:` of `03-queries`, `04-values`,
`05-commands`, `07-assets`, `a-query-syntax` and `c-glossary` in all three guides, each with a
`TBD:` line naming the pipeline topic; the Python and web lines say it is not yet available there.
`cargo run -p liquers-docs -- check`: 0 errors, 0 warnings.

### Links and Capability Map

`specs/README.md`: *Pipelines* now links `reference/PIPELINES.md` (how-to and design in brackets);
*Programmatic query builder* moved from planned (the issue) to documented (`PIPELINE_GUIDE.md` §3).
`PIPELINES.md` and `PIPELINE_GUIDE.md` are linked from every document in the table above.

## Issues Filed

- `PIPELINE-COMMAND-OPERAND-TYPES-UNCHECKED` — placeholder operand types and GUI info are not checked
  or inferred beyond `any`.
- `RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND` — recipe overrides do not reach a pipeline command
  ending the query.
- `PLAN-BUILDER-DUPLICATES-PIPELINE-PLANNING` — two builders now plan the same queries.
- `PIPELINES-NOT-IN-LANGUAGE-BINDINGS`, `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS` — scope
  moved out at Phase 1.
- `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`, `QUERY-BUILDER-NOT-IN-BINDINGS`,
  `RECIPES-CANNOT-DESCRIBE-A-SELF-CONTAINED-ASSET-TREE`, `MANIFEST-TEMPLATE-BATCH-ONLY-AT-THE-END-OF-THE-QUERY`
  — found during the design phases.

Closed with resolution notes: `COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE`,
`QUERY-BUILDER-TOOLING`, `RECIPE-OVERRIDE-ONLY-LAST-ACTION`.

## Important Learning

- **Plan equality is the contract.** Making the pipeline the planner's input language was safe only
  because a corpus test plans every query both ways and compares plans, positions included. Every
  divergence it found (resource keys, quoted prefixes, written action requests) was a real one.
- **Representability belongs to encodings, not structures.** Comparing parsed structures refused
  every shorthand query; comparing the encoded text is the rule that holds.
- **A pipeline command is an alias generalized.** Both are commands without an implementation,
  resolved at planning; the pipeline command differs in expanding to a nested plan in its own frame,
  which is why its body is cached only as a whole.

## Conformance and Remaining Work

Requested (gh#237, `QUERY-BUILDER-TOOLING`, `RECIPE-OVERRIDE-ONLY-LAST-ACTION`), approved (Phases 1–2
with the 2026-11-02 extension) and implemented scope agree, except the two items moved out at
Phase 1 and the follow-ups above, each of which is an issue. Nothing remains in this design.

## Validation

- `cargo test -p liquers-core --all-features`, `-p liquers-lib --lib --tests`, `-p liquers-mcp`,
  `-p liquers-records --lib --tests` after the merge of main: all pass.
- `bash scripts/check-build-matrix.sh`: RESULT_MATRIX.
- liquers-web Node loop (`--features debug-handles`, and with `records`): RESULT_WEB.
- `cargo test -p liquers-core --doc` (module doc examples): RESULT_DOC.
- `python3 scripts/check_api_docs.py` on the five pipeline and validate modules: 0 warnings.
- `cargo run -p liquers-docs -- check`: 0 errors. `python3 scripts/docs_index.py --check`: 0 errors.
