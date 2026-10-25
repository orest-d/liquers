---
id: QUERY-CACHE-DIRECTIVES
kind: design
title: Positional cache directives that mark which intermediates of a query are kept
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/assets, core/query]
issues: [QUERY-CANNOT-MARK-CACHED-INTERMEDIATES]
affects_docs: [specs/reference/RECIPES_AND_PLANS.md, specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/ASSETS.md]
created: 2026-10-20
---
# Positional cache directives that mark which intermediates of a query are kept

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. The maintainer deferred the source "until it becomes important"
(`plan-policy`, 2026-10-10); this records a plan for when it does.

## Phase 1: High-Level Design

### Purpose

Every cut boundary of a long chain becomes a cached asset (DOC-08 §"Cutting is recursive"), so a
large value flowing through `n` steps is kept `n` times. A query author should be able to say, in
the query, which prefixes are worth keeping — without a recipe per reusable point and without a
command author's `cached: false` (VISION: queries as the unit of reuse and caching).

### Problem Example

`-R/data/big.csv/-/ns-pl/from_csv/sort-a/head-100/describe` (validates `Ok`) registers four assets
at HEAD: the prefixes ending at `from_csv`, `sort-a` and `head-100`, and the result — one copy of
the frame per step. There is no way to write "keep the parsed frame and the result, nothing in
between" in the query. Should:

```
-R/data/big.csv/-/ns-pl/from_csv/cache-off/sort-a/head-100/cache-on/describe
```

keeps `…/from_csv` and the result only; `sort-a` and `head-100` run inline after the kept
prefix. `cache-off` is the action `cache` with parameter `off`; it validates with
`liquers-validate --command cache` and fails today with `ActionNotRegistered`.

### Scope and Acceptance Criteria

- **AC-1** Off/on window
  WHEN a query contains `cache-off` … `cache-on`
  THEN prefixes whose last command lies in the window are not cut as boundaries (they run inline after the nearest kept prefix), and the value equals the query without directives
- **AC-2** Directives are not identity
  WHEN `x/cache-this/y` and `x/y` are evaluated in one environment
  THEN the prefix `x` is computed once: the query asset is registered under the query with directives removed
- **AC-3** Keep this point
  WHEN `cache-this` follows a command inside a `cache-off` window
  THEN the prefix ending there is cut as a boundary and kept
- **AC-4** Final result follows the mode
  WHEN the last command of a top-level query lies in a `cache-off` window
  THEN the result is not registered for reuse, with a "Not cached for reuse: …cache-off…" log line, as for a `cached: false` command
- **AC-5** Links and recipes are separate
  WHEN a link parameter's query or a keyed recipe's query is evaluated
  THEN it starts in mode `on`, whatever mode its consumer was in
- **AC-6** Ineffective directive warns
  WHEN `cache-this` lies in a volatile region (after `v` or a volatile command)
  THEN a planning warning says it has no effect and no boundary is cut there
- **AC-7** Malformed directive
  WHEN `cache` has no parameter, more than one, or one other than `on`, `off`, `this`
  THEN planning fails with a positioned `ParameterError`

Non-goals: per-step boolean directives (`cached-<bool>`) and `inline`/`qinline` command flags
(rejected in `plan-policy`); a cache size limit (`CORE-ASSET-GC`); directives inside recipe YAML
fields (a recipe's `cached:` strategy stays); changing the default (everything is kept unless
marked).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — query syntax gains a reserved instruction and a pub field
  of `Plan` changes type (rule 4).
- **Leading issue:** question 1 (final result follows the mode).
- **Explanation:** a working solution is fully described in `liquers-core` under the recommended
  answers; the remaining questions change user-visible caching and are left visible.
- **Open questions:**
  1. **Open design - final result:** follows the mode in force at its last command (recommended:
     the same rule as `cached: false`, and `x/cache-off/y` then means "keep nothing after x"), or
     always follows the query cache strategy.
  2. **Open design - `cache-this` versus `cached: false`:** the directive wins (recommended: the
     author asked for this point explicitly) or the command's declaration wins.
  3. **Proposed resolution - spelling and reservation:** `cache-on|off|this`, i.e. a reserved
     instruction `cache` like `v`, `q`, `ns`; a registered command named `cache` becomes unreachable
     (none exists in `specs/command_registry.yaml`).
  4. **Proposed resolution - `cache-this` in a volatile region:** a planning warning (AC-6), not an
     error.

### Design Dependencies

- `plan-policy` (complete) — origin; E4, not extended.
- `recipe-override-addressing` — **overlaps**: both add a decline reason to
  `Plan::cut_predecessor`; either order. Its ordinal count must also skip `cache` (noted there).
- `context-title-predecessor-inheritance` — **overlaps**: which prefix becomes a boundary decides
  which asset a command-set title lands on; no order.
- `CORE-ASSET-GC` (issue) — **overlaps**: the other instrument for memory; independent.

### Consolidated Findings

- The candidate walk already builds each candidate's own plan from its prefix *text*, so a
  directive in that text is seen by the candidate's builder: the mode needs no new walk state, only
  a builder flag and one more "uncached" reason.
- Identity is the main cost: the query asset map (both managers) and `DependencyKey::from(&Query)`
  must use the directive-free query while the asset's recipe keeps the full text, so the creator's
  directives govern its own boundaries. Two spellings share one asset, which is sound because the
  values are identical (AC-2).
- `Plan::uncached_by` becomes `Option<UncachedReason>`; its two readers are in `liquers-core` only
  (`cut_predecessor`, `QueryRetention::decide`). `Plan` serialization has no compatibility contract
  (DOC-08 P1 gap), and the field is skipped when `None`.
- Query syntax changes: `PROJECT_OVERVIEW.md` must be updated (CLAUDE.md constraint).

## Phase 2: Architecture

### Solution

`PlanBuilder` intercepts `cache` in `process_action` exactly where it intercepts `v`: it validates
one parameter, emits no step, and updates a private `cache_mode: bool` (default on). After each
command's flags are applied, an action processed while the mode is off gets
`uncached_by = Some(UncachedReason::Directive)` unless the command already set one; `cache-this`
sets `uncached_by = None` on the plan as built so far (it names the prefix ending at the previous
command), and warns instead when the plan is already volatile. `cache-on`/`cache-off` themselves
never change `uncached_by` (it describes the last command). Because `Query::predecessor` treats
`cache-…` as an action, a candidate can end at a directive, which is how `cache-this` and
`…/from_csv/cache-off` become boundaries; the empty-tail rule already stops a boundary from covering
a whole stable plan (`a/b/cache-this` is not cut into itself).

Identity: `Query::without_cache_directives(&self) -> Query` removes every action named `cache`
(any parameters) from transform segments. `DefaultAssetManager` and the immediate manager key
`query_assets` by it (lookups and inserts in `get_query_asset_for`, `get_nonvolatile_query_asset`,
`get_volatile_query_asset`, the `read_sync` path), while the asset's recipe keeps the full query;
`impl From<&Query> for DependencyKey` encodes the stripped query.

Rejected: directives as part of identity (prefixes would be shared only between identical
spellings); a separate pass after building (the candidate walk already rebuilds each prefix); a
new step variant (every exhaustive `Step` match would change for no runtime effect).

Known-issue preflight: none blocks. `CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY` is affected only
in where boundaries fall. Namespaces: none (examples use `ns-pl`).

### Changes

- `liquers-core/src/plan.rs`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub enum UncachedReason { Command(CommandKey), Directive }
  pub uncached_by: Option<UncachedReason>,   // was Option<CommandKey>
  ```
  `PlanBuilder::process_action` (interception, `cache_mode`), `apply_command_flags` (wrap in
  `Command`), `Plan::cut_predecessor` (reason text for `Directive`: "is not cached (cache-off)").
  `Plan::split` (line ~2761) keeps clearing it.
- `liquers-core/src/query.rs`: `Query::without_cache_directives`; a `CACHE_DIRECTIVE` name constant.
- `liquers-core/src/assets.rs`: `QueryRetention::decide` (`Directive` → "Not cached for reuse:
  cache-off"), the two managers' `query_assets` keying (Solution).
- `liquers-core/src/metadata.rs`: `From<&Query> for DependencyKey` strips directives.
- Errors: `Error::too_many_parameters` (as `v`) for surplus parameters; `Error::from_error(ErrorType::ParameterError, …)`
  with the action position for a missing or unknown mode. All synchronous except the asset-manager
  lookups, which are already async.
- No command, no registry change. Documents: DOC-02 §Planner instructions (`cache`), DOC-08
  §Planning contract and §What is kept (a third instrument), `ASSETS.md` §When an asset is kept for
  reuse, `PROJECT_OVERVIEW.md` (query syntax); History rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A lookup site keyed by the full query is missed, so two spellings make two assets | AC-2 test through both managers; grep `query_assets` | medium |
| A user command named `cache` is shadowed | documented as reserved; none in the registry | high |
| The candidate step-count check rejects a candidate ending at a directive | directives emit no step, as `v`; covered by T-3 | high |
| Changing `uncached_by`'s type breaks an outside reader | none outside `liquers-core` (grepped) | high |

Existing tests: `uncached_command_sets_uncached_by_and_is_not_contagious` and `QueryRetention`
tests adapt to `UncachedReason::Command`. Recovery: revert; plans without directives are unchanged.

## Phase 3: Examples and Tests

### Examples

Problem example (validated with `--command cache`):
`-R/data/big.csv/-/ns-pl/from_csv/cache-off/sort-a/head-100/cache-on/describe` → plan
`Evaluate(-R/data/big.csv/-/ns-pl/from_csv/cache-off)`, `sort-a`, `head-100`, `describe`; the
boundary is registered as `-R/data/big.csv/-/ns-pl/from_csv`. Keeping one middle point:
`…/from_csv/cache-off/sort-a/cache-this/head-100/describe` keeps `…/sort-a` as well.

### Tests

Unit, `liquers-core/src/plan.rs` `mod tests` (registry with `fetch`, `expensive`, `wide`
(`cached: false`), `render`, as in the existing `uncached_by_of` tests):

- `cache_directive_window_marks_commands_uncached` — `uncached_by` per prefix of
  `fetch/cache-off/expensive/cache-on/render`: AC-1
- `cache_this_clears_uncached_and_wins_over_command_flag` — `fetch/cache-off/expensive/cache-this`
  and `fetch/wide/cache-this`: AC-3 (and Q2)
- `cut_skips_a_cache_off_window` — finalized `fetch/cache-off/expensive/render` cuts at
  `fetch/cache-off`; `Info` line names cache-off: AC-1
- `cache_this_in_volatile_region_warns` — `fetch/v/expensive/cache-this/render`: AC-6
- `malformed_cache_directive_is_positioned_error` — `cache`, `cache-x`, `cache-on-1`: AC-7

Unit, `liquers-core/src/query.rs`: `without_cache_directives_strips_only_cache_actions`: AC-2.

Integration, new `liquers-core/tests/cache_directives.rs` (memory store, counting commands as in
`tests/cache_strategy.rs`):

- `window_keeps_only_marked_assets` — count registered query assets and command runs for the
  problem shape: AC-1, AC-4
- `spellings_share_one_prefix_asset` — `seed/add-1/cache-this/double` then `seed/add-1/triple`:
  `add` runs once; run under `DefaultAssetManager` and the immediate kind: AC-2
- `link_starts_in_mode_on` — a link `~X~seed/add-1~E` inside a `cache-off` window is cut and kept: AC-5
- `value_is_unchanged_by_directives` — same results with and without directives: AC-1

Run: `cargo test -p liquers-core --lib cache_` and `cargo test -p liquers-core --test cache_directives`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `plan.rs` `UncachedReason`, `uncached_by` type; `assets.rs` `QueryRetention::decide` — adapt existing tests — `cargo test -p liquers-core --lib uncached`. Rollback: revert.
- [ ] 2. `plan.rs` `PlanBuilder::process_action` interception and `cache_mode`; `cut_predecessor` reason text — the five plan unit tests — `cargo test -p liquers-core --lib cache_`. Rollback: revert step 2.
- [ ] 3. `query.rs` `without_cache_directives`; `metadata.rs` `DependencyKey`; `assets.rs` query-asset keying in both managers — query unit test and `tests/cache_directives.rs` — `cargo test -p liquers-core --test cache_directives`. Depends on 2. Rollback: revert step 3 (directives then still work but are identity).
- [ ] 4. Full core and lib loops — `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`.
- [ ] 5. Documents: DOC-02, DOC-08, `ASSETS.md`, `PROJECT_OVERVIEW.md`, `.claude/skills/liquers-validate/SKILL.md` (a `cache` directive validates without `--command`) with History rows and `reviewed:`; close `QUERY-CANNOT-MARK-CACHED-INTERMEDIATES` — `python3 scripts/docs_index.py --check`.

### Validation

`liquers-validate -- '<problem query>'` returns `Ok` without `--command cache` (the validator uses
the planner, so the interception covers it). No `cfg(feature)` change; `specs/command_registry.yaml`
unchanged.
