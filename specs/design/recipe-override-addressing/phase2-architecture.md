# Phase 2: Solution & Architecture — recipe-override-addressing

## Overview

A recipe argument name is parsed into an `ArgumentAddress` before it is applied. An unqualified name
keeps today's meaning (last action). `<n>.<name>` and `<n>.<k>` address the `n`-th command of the
plan by argument name or by written-parameter slot; `<action>.<name>` addresses a command by its
name when that name is unique; `@key` / `@key.<j>` append a relative key to a resource segment.
Key holes are filled on the parsed query **before** planning (so every derived value sees them);
action overrides patch the built plan **after** planning, as today, and set a floor below which no
predecessor boundary may be cut.

Rejected: a hole syntax inside query text (an encoding change reaching every binding); structured
`Recipe` fields (Q1); writing an override into a boundary query as text (impossible for non-scalar
values); searching every action for an unqualified name (silently retargets existing recipes).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft | P3 | calls `to_plan`; inherits it | No | No | none |
| `DEPENDENCY-ANALYSIS-USES-CALLER-CWD-FOR-A-CWD-LESS-RECIPE` | draft | P3 | walks recipe plans; overrides are ordinary parameters to it | No | No | none |
| `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM` | draft | P3 | same template slot classification code (`instantiate.rs`) | No | No | rebase only |

No command namespace is involved; `ns-pl` and `ns-mem` appear only in examples.

## Interfaces

`liquers-core/src/plan.rs` (beside `ParameterValue`; plain data, sync):

```rust
/// What a recipe argument name addresses. Parsed from the `Recipe.arguments` / `links` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentAddress {
    /// `name` — the last action (unchanged meaning).
    LastAction(String),
    /// `<n>.<name>`, `<n>.<k>`, `<action>.<name>`.
    Action { action: ActionSelector, argument: ArgumentSelector },
    /// `@key` (segment 0) or `@key.<j>` — the `j`-th resource segment of the query.
    ResourceKey(usize),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionSelector { Ordinal(usize), Name(String) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentSelector { Name(String), Slot(usize) }

impl ArgumentAddress {
    pub fn parse(text: &str) -> Result<Self, Error>;   // ParameterError on a malformed name
    pub fn encode(&self) -> String;                     // inverse; used by liquers-template
}

impl Plan {
    /// Step indices of the plan's `Step::Action`s, in order: ordinal `n` is `action_steps()[n]`.
    pub fn action_steps(&self) -> Vec<usize>;
    /// `last_action_command` generalized; `last_action_command` delegates to it.
    pub fn action_command(&self, step: usize, cmr: &CommandMetadataRegistry) -> Option<CommandMetadata>;
    /// Resolves an action address to (step index, argument). Errors as AC-2 / AC-8.
    pub fn resolve_argument(&self, address: &ArgumentAddress, cmr: &CommandMetadataRegistry)
        -> Result<(usize, ArgumentInfo), Error>;
    pub fn override_value_at(&mut self, step: usize, name: &str, value: Value) -> bool;
    pub fn override_link_at(&mut self, step: usize, name: &str, query: Query) -> bool;
}
```

`override_value` / `override_link` stay and delegate with the last action's index. `ArgumentSelector::Slot(k)`
maps to the argument that consumes the `k`-th written parameter (injected arguments skipped, a
`multiple` argument takes every slot from its own), the rule `liquers-template`'s private
`argument_name` applies today; it moves into a private `argument_for_slot(&CommandMetadata, usize)`.

`liquers-core/src/query.rs`:

```rust
impl Query {
    /// Appends a relative key to resource segment `segment`: `fill_key`'s contract — refuses an
    /// absolute key, an empty / `.` / `..` element, an element starting with `-`, and a result
    /// that does not survive encode–parse. An empty suffix (or a single `/`) is a no-op.
    pub fn with_key_suffix(&self, segment: usize, suffix: &str) -> Result<Query, Error>;
}
```

`liquers-core/src/recipes.rs` — `Recipe` fields and serde unchanged:

- `get_query()` returns the query **with key holes filled** (`with_key_suffix` per `@key*` entry,
  in segment order); new `query_as_written() -> Result<Query, Error>` returns the text as parsed.
  `key`, `filename`, `data_format`, `store_to_key`, `get_asset_info` and `to_plan` all go through
  `get_query`, so they see the filled query with no further change.
- `has_arguments()` ignores `@key*` entries (they are part of the query, not overrides), so a
  filled key query still has a `key()`.
- `to_plan`: for each remaining name in sorted order, `ArgumentAddress::parse`, then
  `plan.resolve_argument` (`LastAction` keeps `overridable_argument`'s exact errors), then
  `convert_override` against the resolved `ArgumentInfo` and `override_value_at` /
  `override_link_at`. `links` names are parsed the same way (`@key` there is a `ParameterError`).
  After all overrides, clear `Step::Action::query` from the **first patched step to the end**.

`Plan::cut_predecessor` — one more decline reason in the walk, before `break`: if the plan's first
action holding an `OverrideValue` / `OverrideLink` (private `first_overridden_step`, recursing into
`MultipleParameters`) lies inside the candidate (`cut_at > step`), append
`Predecessor boundary expanded at '<q>': it contains an action a recipe argument overrides` and
step back.

## Integration Points

| Crate | File / symbol | Change |
|---|---|---|
| `liquers-core` | `plan.rs` `ArgumentAddress`, `Plan::{action_steps, action_command, resolve_argument, override_*_at, cut_predecessor}` | new / generalized |
| `liquers-core` | `query.rs` `Query::with_key_suffix` | new (from `liquers-mcp` `fill_key`) |
| `liquers-core` | `recipes.rs` `Recipe::{get_query, query_as_written, has_arguments, to_plan}`, `not_in_last_action` | addressing |
| `liquers-template` | `src/query/instantiate.rs` `Builder::query`, `argument_name`, `QueryTemplate::instantiate` | drop the last-action check; emit `ArgumentAddress::Action { Ordinal(n), Slot(k) }.encode()` (or the bare name when `n` is the last action); trim trailing overridden parameters of every action; delete `argument_name` |
| `liquers-mcp` | `src/server.rs` `fill_key`, its call in the tool-call path, `lib.rs` re-export | replaced by `recipe.arguments.insert("@key", key)`; `fill_key` removed (its tests move to `with_key_suffix`) |
| `liquers-axum` | `src/api_core/overrides.rs` `OverrideSet::to_recipe` | resolve the argument through `ArgumentAddress::parse` + `Plan::resolve_argument` so URL text converts by the addressed argument's type |

Ordinal counting is on the plan (`Step::Action`s), which counts aliases once and `ns`, `v`, `q`
(and the proposed `cache` directive) not at all. The template counts its actions minus `ns` / `v`
/ `q` / `cache`; a test pins that both counts agree for a query mixing all of them.

## Error Handling

| Failure | Constructor |
|---|---|
| Malformed address (`1.`, `.x`, `@keys`, `@key.x`) | `Error::from_error(ErrorType::ParameterError, …).with_query(q)` |
| Ordinal out of range, name absent or ambiguous, slot beyond the command's parameters | same, naming the address and the plan's command count / ordinals |
| Addressed argument injected; alias head slot | same text as `overridable_argument` today |
| `@key` value not a string | `Error::from_error(ErrorType::ConversionError, …)` |
| `@key` value refused (AC-6), no such resource segment | `Error::parse_error` (as `fill_key`) / `ParameterError` |
| A value that does not convert | `convert_override` unchanged (`ConversionError`, or a planning warning) |

All synchronous: addressing and filling are pure functions of the query, the registry and the
argument map.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` is untouched.

## Documentation Architecture

DOC-08 §Recipe contract (grammar, key filling, `get_query` / `query_as_written`), §Planning
contract (replace "overrides affect only the last `Step::Action`"), §Where a boundary goes (fifth
decline), §Pitfalls (an override inside a boundary), §Conflicts (strike the P1 gap);
TEMPLATE_LANGUAGE.md line 73; AGENT_MEMORY_SERVICE.md key argument; module docs of `plan.rs` and
`Recipe::to_plan`. `affects_docs` = the three references. No `specs/README.md` link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/{plan,recipes,query}.rs`; `liquers-template/src/query/instantiate.rs`, `tests/render.rs`; `liquers-mcp/src/{server,lib}.rs`; `liquers-axum/src/api_core/overrides.rs` |
| Crates and workflows affected | recipes.yaml, HTTP/WS named arguments, templates, MCP tools; no plain-query path cost |
| Existing tests likely to change | `liquers-template` `instantiate.rs` test at line 453 and `tests/render.rs:67` assert the last-action error — they invert; `liquers-mcp` `fill_key_*` tests move to core |
| New validation | core unit tests T-1…T-9, template T-10, mcp T-11 (Phase 3) |
| Compatibility / data | additive for valid recipes: an unqualified name is unchanged, and no existing argument name contains `.` or `@`. A recipe that used such a name was already an error. `get_query` now fills keys; identical for recipes without `@key` |
| Correctness | the cut floor (AC-4) is load-bearing: without it a boundary caches the un-overridden prefix |
| Security | `@key` keeps `fill_key`'s refusals; a caller can extend only a key the recipe already names |
| Recovery | core steps are additive; consumer steps revert independently |
| Certainty | high for core and mcp; medium for the template's ordinal agreement (pinned by a test) |
