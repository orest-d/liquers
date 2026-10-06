# Phase 5: Documentation - `Context::set_title` and `Context::set_description`

**Status: executed 2026-10-06**, pending approval. Written on 2026-10-05, when the design adopted the five-phase `liquers-project`
contract; followed
`.claude/skills/liquers-project/references/phase5-documentation.md`. The sections the skill requires
after implementation are present and marked *pending*.

## Completion Preconditions

- [ ] Implementation is finished and validated (Phase 4)
- [ ] All user comments are answered or incorporated
- [ ] All review comments are answered or incorporated
- [ ] Documentation is consistent with the implemented and tested behavior
- [ ] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | beside `set_filename` (≈423): `set_title` / `set_description`, the recipe-wins-per-field rule, `Ok(())` when the recipe's value is kept, persisted with keyed metadata, never part of `version`, `NotSupported` on legacy metadata |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | a short example near the context usage (the Phase 3 `summarize` snippet), stating that a recipe's title takes precedence |
| `reference/ASSETS.md` | §Remove Semantics, the `set_description(key, …)` bullet (≈1008): one sentence pointing to `Context::set_title` / `set_description` as the command-side counterpart and its precedence rule |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`core/context`): `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `ENVIRONMENT_CONFIG.md`, `PAYLOAD_GUIDE.md`, `DOC_08_RECIPES_PLANS.md` — no statement about metadata a command writes. By content: `VALUE_TYPE_SYSTEM.md` ≈212 (`set_filename` as a data-format level; unaffected), `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` ≈223 (keyed mutations; a context setter is not one).

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION` → `closed`, resolution recording the recipe-wins decision (2026-10-04) and naming the tests.

## Implementation Summary

`Context::set_title` / `set_description` added (`context.rs`), backed by
`AssetRef::set_description_fields_from_command` and two private `AssetData` flags
(`recipe_sets_title`, `recipe_sets_description`) set when the recipe is adopted and cleared by
`reset`. Recipe wins per field; otherwise the command's text is written to the live metadata and
persisted with it; `version` is unchanged. Seven integration tests
(`liquers-core/tests/context_title_description.rs`, most run on both asset managers) and three unit
tests (`assets.rs`) pass; `cargo test -p liquers-core --lib --tests` is green (949 lib tests).

## Documentation Delivered

`DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`, `COMMAND_REGISTRATION_GUIDE.md` and `ASSETS.md`
updated as planned, each with a History row and `reviewed:` bump; the issue is `closed`; the docs
index was regenerated (`docs_index.py --check`: 0 errors).

## Issues Filed

`CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY` (P3).

## Important Learning

The Phase 3 assumption that every step of a query shares one `Context` is false for plain
evaluation: the cacheable prefix becomes a predecessor asset (`finalize_plan`). Chain tests that
need a shared context must `apply` with an input state.

## Conformance and Remaining Work

Matches the approved design. Deviation: the chained-step test uses `apply` with a state (see
above). Not run here: `liquers-lib` tests and the wasm32 check (no `liquers-lib`/`liquers-py`/
`liquers-web` code changed).

## Validation

`cargo test -p liquers-core --lib --tests`; `python3 scripts/docs_index.py --check`.
