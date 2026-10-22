# Phase 1: High-Level Design — recipe-override-addressing

## Purpose

A recipe fills holes in its query from `Recipe.arguments`, but today a hole can only be an argument
of the **last** action. This design lets one recipe argument name address any command of the query
and the key of a resource segment, so a template, a tool or an HTTP request is a `Recipe` with named
holes instead of query text assembled by its caller (VISION: queries as the one addressable unit;
agent memory and templates as consumers).

## Problem Example

`reports/recipes.yaml`, at HEAD:

```yaml
recipes:
  - query: -R/data/sales.csv/-/ns-pl/from_csv/is_in-region/head-10/eu.csv
    arguments:
      values: [EU, US]          # the list is for is_in, not for head
```

`liquers-validate --recipes reports/recipes.yaml --cwd reports` reports
`ParameterError: Argument 'values' is not an argument of the last action` (verified 2026-10-20), and
`1.values` fails the same way. The only workaround is to reorder the query or split it into two
recipes. Should: `1.values` (or `is_in.values`) fills `values` of the second command, and the plan
runs `is_in-region-EU-US` before `head-10`.

The key case, from `liquers-mcp`: a tool declared as `-R/notes/-/ns-mem/read` with a key argument
cannot be a recipe today; `liquers-mcp/src/server.rs` `fill_key` edits the parsed query itself.
Should: the recipe argument `"@key": "notes/idea.md"` appends a validated relative key to the
first resource segment, so the tool is a plain `Recipe { query, arguments }`.

## Scope and Acceptance Criteria

- **AC-1** Earlier command by ordinal
  WHEN a recipe argument is named `<n>.<name>` (or `<n>.<k>`, the `k`-th written parameter slot) and the query's `n`-th command (0-based, counting only actions that call a command) declares that argument
  THEN that action's argument is overridden, converted as a last-action override is, and the result equals the query with the value written in place
- **AC-2** Earlier command by action name
  WHEN a recipe argument is named `<action>.<name>` and `<action>` occurs exactly once among the query's commands
  THEN it addresses that command as AC-1 does; when it occurs more than once THEN `ParameterError` naming the ordinals to use
- **AC-3** Last action unchanged
  WHEN a recipe argument has an unqualified name
  THEN it addresses the last action exactly as today (same errors, same warnings, same order)
- **AC-4** No boundary over an overridden action
  WHEN an action other than the last is overridden and the plan is finalized
  THEN no predecessor boundary includes that action, and a planning `Info` says why
- **AC-5** Key hole
  WHEN `arguments` holds `@key` (or `@key.<j>` for the `j`-th resource segment) with a string relative key
  THEN that key is appended to the segment's key before planning, and `Recipe::key`, `filename` and `to_plan` all see the filled query
- **AC-6** Key hole refuses injection
  WHEN the `@key` value is absolute, has an empty, `.` or `..` element, an element starting with `-`, is not a string, or does not survive an encode–parse round trip
  THEN the recipe fails to plan with an error naming the argument, and nothing is evaluated
- **AC-7** Consumers use it
  WHEN a `liquers-template` non-scalar placeholder fills a parameter of an earlier action, or a `liquers-mcp` tool takes a key argument
  THEN the template produces a qualified override instead of an error, and the tool builds its recipe with `@key` (`fill_key` is gone)
- **AC-8** Unknown targets are errors
  WHEN an ordinal is out of range, an action name is absent, a segment index has no resource segment, or the addressed argument is injected
  THEN `ParameterError` with the recipe's query, as for an unknown last-action name today

Non-goals: a hole in the middle of a key or a text syntax for holes inside the query; holes in a
link's own query; changing `Recipe`'s serialized fields (names stay `HashMap<String, Value>` keys);
the `liquers-template` per-segment hygienic key placeholder (unchanged); Python and JavaScript
binding changes (the recipe struct is unchanged).

## Core Interactions

- Query/Plan: recipe override application and the predecessor cut (`liquers-core`).
- Assets: none beyond the cut; override recipes are already never keyed by their arguments.
- Web/API: `liquers-axum` named arguments may use qualified names (text conversion by the addressed type).
- Consumers: `liquers-template` (earlier-action overrides), `liquers-mcp` (`@key`).

## Crate Placement

`liquers-core` (`recipes.rs`, `plan.rs`) owns addressing and filling: it is where `Recipe` lives and
the only crate every consumer shares. `liquers-template`, `liquers-mcp` and `liquers-axum` adopt it in
later steps; none needs `liquers-lib`.

## Documentation Intent

- Reference: extend `specs/reference/RECIPES_AND_PLANS.md` §Recipe contract and §Planning
  contract (addressing, the cut floor; strike the P1 gap "Recipe override scope is narrower…");
  `specs/reference/TEMPLATE_LANGUAGE.md` line 73 (overrides reach any action); `specs/reference/AGENT_MEMORY_SERVICE.md` (key argument through `@key`).
- Guide: none — `TEMPLATE_GUIDE.md` does not describe the last-action limit (checked); commands
  are unchanged, so no command guide change.
- Other documents: none. `PROJECT_OVERVIEW.md` is not touched: query encoding does not change.
- Documents to update: the three references above, with History rows and `reviewed:` bumps.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` items in `liquers-core` and changes what recipe
  argument names mean (rule 4); crosses four crates (rule 6); size `L` (rule 1).
- **Leading issue:** the address grammar (question 1).
- **Explanation:** a working solution is fully described under the recommended grammar; it is
  additive for existing recipes (an unqualified name keeps its meaning). The decisions change how
  recipes and tools are written, so they stay visible.
- **Open questions:**
  1. **Open design - address grammar:** qualified names inside `arguments` (`1.values`,
     `1.1` for the second parameter slot, `is_in.values`, `@key`, `@key.1`) — recommended: every transport (YAML, HTTP named arguments,
     WebSocket, Python, MCP) already carries `HashMap<String, Value>`, so no schema changes; argument
     names are identifiers and can contain neither `.` nor `@`. Alternative: new structured fields
     (`action_arguments: [{action, name, value}]`, `key_arguments`) — explicit, but every transport and
     binding must learn them.
  2. **Open design - ordinal base:** 0-based (recommended: matches positions and the template's
     indices) or 1-based (reads naturally in hand-written YAML).
  3. **Proposed resolution - key hole shape:** append a relative key (one or more elements) to the
     end of a segment's key, exactly `fill_key`'s contract (recommended); a hole in the middle of a
     key is a non-goal.
  4. **Proposed resolution - cut floor:** decline every boundary that would contain an overridden
     action (recommended: a prefix query does not describe a patched action, so caching it under
     that query would serve a wrong value). Alternative: rewrite the boundary query to carry the
     override as text — impossible for a list, map or long text, which is the reason overrides exist.

## Design Dependencies

- `liquers-mustache`, `axum-query-named-arguments`, `agent-memory-mvp` (complete) — origins; E4.
- `template-non-scalar-text-form` — **overlaps**: other template slot widening; no order.
- `cwd-less-recipe-analysed-at-root` — **overlaps**: `DependencyWalk` sees overrides as parameters.
- `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` (issue) — **overlaps**: calls `to_plan`.
- `query-cache-directives` — **overlaps**: each adds a decline reason to `Plan::cut_predecessor`;
  either order, the second rebases one arm.

## Consolidated Findings

- **Correctness constraint (Phase 2):** `Plan::cut_predecessor` builds boundaries from the recorded
  prefix *query*, which lacks an override; without the floor (AC-4) a boundary would cache the
  un-overridden prefix — a silent wrong value. Steps 4 and 5 ship together.
- An override clears `Step::Action::query` for the patched action and every later one.
- Ordinals count the plan's `Step::Action`s (aliases once; `ns`, `v`, `q` never); the template's
  count is pinned to it by T-10.
- Key holes are filled on the parsed query before planning, so every derived value sees them and
  the result stays cacheable; no query-encoding, `PROJECT_OVERVIEW.md` or registry change.
