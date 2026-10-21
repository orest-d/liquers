# Phase 4: Implementation Plan — recipe-override-addressing

## Overview

Core first (grammar, key filling, addressed overrides, the cut floor), then the three consumers,
then documents. Steps 1-5 are in `liquers-core` and each leaves the workspace compiling; steps 6-8
are independent of each other. No prerequisite issue must be fixed first. Decisions Q1-Q2 are taken
as their recommended answers; a different answer changes only Step 1's parser and the encoded
names in Steps 6-8.

## Progress

- [ ] Step 1: `ArgumentAddress` grammar
- [ ] Step 2: `Query::with_key_suffix`
- [ ] Step 3: addressed overrides on `Plan`
- [ ] Step 4: `Recipe` key holes and addressed `to_plan`
- [ ] Step 5: the cut floor and the end-to-end test
- [ ] Step 6: `liquers-template` earlier-action overrides
- [ ] Step 7: `liquers-mcp` drops `fill_key`
- [ ] Step 8: `liquers-axum` converts qualified text arguments
- [ ] Step 9: documents and records

## Implementation Steps

### Step 1: `ArgumentAddress` grammar
- Files / symbols: `liquers-core/src/plan.rs` — `ArgumentAddress`, `ActionSelector`, `ArgumentSelector`, `parse`, `encode`
- Change: new types (Phase 2 signatures). `parse`: `@key` / `@key.<digits>`; `<digits>.<ident|digits>`;
  `<ident>.<ident|digits>`; a bare identifier → `LastAction`; anything else `ParameterError`.
- Depends on: none
- Proof: T-1 — `cargo test -p liquers-core --lib argument_address`
- Rollback: delete the types (unused yet)

### Step 2: `Query::with_key_suffix`
- Files / symbols: `liquers-core/src/query.rs` `impl Query`
- Change: port `liquers-mcp/src/server.rs` `fill_key`'s checks, generalized to resource segment `j`
  (counting `QuerySegment::Resource` only); `Error::parse_error` for refusals, `ParameterError` for a
  missing segment.
- Depends on: none (parallel with Step 1)
- Proof: T-6 — `cargo test -p liquers-core --lib with_key_suffix`
- Rollback: delete the method

### Step 3: addressed overrides on `Plan`
- Files / symbols: `plan.rs` `Plan::{action_steps, action_command, resolve_argument, override_value_at, override_link_at}`; `last_action_command`, `override_value`, `override_link` delegate; private `argument_for_slot`
- Change: `resolve_argument` maps the selector to a step index (ordinal, or the unique matching
  `action_name` as written — for an alias the alias name from `ActionOrigin::Alias`), fetches the
  command with `action_command`, and finds the argument by name or slot; injected and alias-head
  arguments refused with today's wording.
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib plan` (existing `last_action_command_returns_alias_metadata` stays green)
- Rollback: revert; delegations make the old API unchanged

### Step 4: `Recipe` key holes and addressed `to_plan`
- Files / symbols: `liquers-core/src/recipes.rs` `Recipe::{get_query, query_as_written, has_arguments, to_plan}`, `not_in_last_action`, `overridable_argument`
- Change: `get_query` = `query_as_written` + `with_key_suffix` for each `@key*` (value must be a
  JSON string, else `ConversionError`); `has_arguments` skips `@key*`; `to_plan` parses each name,
  resolves, converts with `convert_override`, applies `override_*_at`, and clears
  `Step::Action::query` from the first patched step onward; `@key` in `links` is `ParameterError`.
- Depends on: Steps 2, 3
- Proof: T-2, T-3, T-4, T-5, T-7 — `cargo test -p liquers-core --lib recipes`; then
  `cargo test -p liquers-core --lib --tests` (every existing recipe test unchanged)
- Rollback: revert Step 4; Steps 1-3 are inert without it

### Step 5: the cut floor and the end-to-end test
- Files / symbols: `plan.rs` `Plan::cut_predecessor`, private `first_overridden_step`; new `liquers-core/tests/recipe_override_addressing.rs`
- Change: the walk declines a candidate with `cut_at > first_overridden_step`, logging the Phase 2
  `Info` line, and steps back.
- Depends on: Step 4 (a plan can only hold an earlier override after it)
- Proof: T-8, T-9 — `cargo test -p liquers-core --lib cut_declines_over` and
  `cargo test -p liquers-core --test recipe_override_addressing`
- Rollback: **do not ship Step 4 without Step 5** — revert both together; Step 4 alone can serve a
  cached un-overridden prefix

### Step 6: `liquers-template` earlier-action overrides
- Files / symbols: `liquers-template/src/query/instantiate.rs` `Builder::query` (drop `is_last` refusal, record the action ordinal), `QueryTemplate::instantiate` (names via `ArgumentAddress::Action{Ordinal, Slot}.encode()`, bare name for the last action; trim trailing overridden parameters per action), delete `argument_name` and `last_action`
- Depends on: Step 4
- Proof: T-10 and the two inverted tests — `cargo test -p liquers-template`
- Rollback: revert the crate's change; core is unaffected

### Step 7: `liquers-mcp` drops `fill_key`
- Files / symbols: `liquers-mcp/src/server.rs` tool-call path (line ~745), `fill_key` and its tests; `src/lib.rs` re-export
- Change: insert `"@key"` into the recipe built by `recipe_for`; keep the required-key check.
- Depends on: Step 4
- Proof: T-11 — `cargo test -p liquers-mcp`; then `cargo test -p liquers-agent` (end to end, Rust 1.96)
- Rollback: revert; `fill_key` returns

### Step 8: `liquers-axum` converts qualified text arguments
- Files / symbols: `liquers-axum/src/api_core/overrides.rs` `OverrideSet::to_recipe`
- Change: look the argument up with `ArgumentAddress::parse` + `Plan::resolve_argument` instead of
  `last_action_command` + name; `@key` text passes through as a string.
- Depends on: Step 3
- Proof: T-12 — `cargo test -p liquers-axum --lib overrides`
- Rollback: revert; qualified names still work with a conversion warning

### Step 9: documents and records
- Files: `specs/reference/api/DOC_08_RECIPES_PLANS.md`, `specs/reference/TEMPLATE_LANGUAGE.md`,
  `specs/reference/AGENT_MEMORY_SERVICE.md` (History rows, `reviewed:`), module docs in `plan.rs`
  and `recipes.rs`; close `RECIPE-OVERRIDE-ONLY-LAST-ACTION` and
  `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY` with resolution notes; `DESIGN.md` `affects_docs`.
- Depends on: Steps 1-8
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: n/a

## Testing Plan

Per step as above. Final: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`
(recipes and templates through `liquers-lib`), `cargo test -p liquers-template`, `cargo test -p liquers-mcp`,
`cargo test -p liquers-axum`. No `cfg(feature)` changes, so the build matrix is not required; run
`liquers-validate --recipes` on Example 1 as a smoke check. `specs/command_registry.yaml` is not
regenerated (no command changes).

## Rollback Plan

Steps 4 and 5 are one unit. Everything else reverts independently: without Steps 6-8 the consumers
keep their current behaviour.

## Documentation Updates

DOC-08 (§Recipe contract, §Planning contract, §Where a boundary goes, §Pitfalls row "an override
inside a boundary", §Conflicts), TEMPLATE_LANGUAGE.md line 73, AGENT_MEMORY_SERVICE.md key
argument; `specs/index.csv` regenerated by `python3 scripts/docs_index.py`.

## Phase 5 Entry Criteria

None — four-phase design.
