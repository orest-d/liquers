# Phase 2: Solution & Architecture — command-execution-class

Written under the recommended answers of Phase 1 (reserved fast lane; resource steps fast only when
they fast-track; `default` = today's queue). The design stops here: question 1 (what `fast`
guarantees) and question 2 (whether it is wanted now) are blocking, and they change the queue
design below, the acceptance criteria AC-2 / AC-3 and every scheduling test.

## Overview

A command declares `execution: fast | default | slow`. `PlanBuilder` records the plan's class: the
weakest class of its action steps (`slow` < `default` < `fast`), and `None` for a plan with no
action. `DefaultAssetManager::get_asset` reads the plan it already builds for volatility; a plan
whose class is `fast` and whose resource steps all fast-track is submitted to a **fast lane** of the
`JobQueue`: a second capacity counter (`fast_capacity`, default 4) and its own waiting list, served
first by the worker loop. Everything else uses today's path unchanged. Dependencies resolved inside
a fast run use the existing claim-based `wait_for_dependency`, which runs a claimed child on the
waiter's slot, so a fast run never needs a slow slot.

Rejected alternatives:

- **Bypass (run inline in the caller, like `apply`):** simplest, but unbounded concurrency (fails
  AC-3) and a long "fast" command would block an HTTP handler task.
- **Priority within one pool:** no pre-emption, so it does not help when every slot is held
  (fails the Problem Example).
- **A command hint (`hint execution: "fast"`):** no schema change, but hints are documented as free
  UI hints; scheduling must not depend on a free-form map.
- **Reusing `cached: false` as "fast":** it means "not worth keeping", true of large cheap outputs,
  not a latency promise; conflating them would change caching for every fast command.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES` | draft | P3 | the contract table to extend is stale | yes, or together | no | fix in the documentation step |
| `EXTERNAL-MANAGER-EVALUATION-SESSION-SHARING` | design in review | P3 | dependency scheduling on external managers | no | no | keep lanes internal to `DefaultAssetManager` |
| `CORE-ASSET-GC` | accepted | P3 | none (lanes do not affect retention) | no | no | none |

## Interfaces

```rust
// liquers-core/src/command_metadata.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionClass { Slow, #[default] Default, Fast }   // order = "weakest first"

pub struct CommandMetadata {
    // …
    /// `None` reads as `ExecutionClass::Default`; read through `execution_class()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionClass>,
}
impl CommandMetadata { pub fn execution_class(&self) -> ExecutionClass; }

// liquers-core/src/plan.rs
pub struct Plan {
    // …
    /// Weakest execution class over the action steps; `None` when there is no action step.
    pub execution_class: Option<ExecutionClass>,
}

// liquers-core/src/assets.rs
impl<E: Environment + 'static> JobQueue<E> {
    pub fn with_fast_capacity(capacity: usize, fast_capacity: usize) -> Self;   // sync
    pub async fn submit_fast(self: &Arc<Self>, asset: AssetRef<E>) -> Result<(), Error>;
}
// AssetManagerOptions: `fast_capacity: Option<usize>` (builder `with_fast_capacity`), queued only.
```

`register_command!` DSL: `execution: fast` (also `slow`, `default`), a bare identifier like
`payload: required`; emits `cm.execution = Some(ExecutionClass::Fast)`. An alias's class is the
weaker of the alias's and its target's. All synchronous metadata; the queue methods are async like
`submit`.

## Integration Points

- `liquers-core/src/command_metadata.rs` — enum, field, accessor; `CommandMetadata` serde
  round-trip tests; command declaration documents (`command_declaration.rs`) parse `execution:`.
- `liquers-macro` — parse the statement (`specs/reference/REGISTER_COMMAND_FSD.md` statement table).
- `liquers-core/src/plan.rs` — `PlanBuilder` folds the class per action step beside `uncached_by`;
  recipe plans (`recipes.rs`) inherit it; predecessor cutting does not change it (the class of
  the cut plan is recomputed for the remaining steps).
- `liquers-core/src/assets.rs` — `DefaultAssetManager::get_asset` (~8508): after `fast_track`
  refuses, if `plan.execution_class == Some(Fast)` and every `GetResource`/`GetAsset` step's key
  fast-tracks, `job_queue.submit_fast`; else `submit`. `JobQueue::run` serves the fast waiting list
  first against `fast_capacity`; `try_to_start_immediately` gains the lane; slot accounting in
  `RunClaim` records which counter to release. `get_dependency_asset` keeps today's path (a
  dependency runs on its parent's slot already).
- `liquers-core/src/environment_builder.rs` — `AssetManagerOptions::with_fast_capacity`; refused
  under `Inline` like `job_capacity`.
- `liquers-lib` — declare `execution: fast` on a reviewed set of cheap commands (slicing, metadata,
  text conversion); regenerate `specs/command_registry.yaml`.
- `liquers-py` (`@command` decorator keyword), `liquers-web` (JS registration option) — metadata
  only.

## Error Handling

No new error paths for users. A malformed `execution:` statement is a macro compile error; an
unknown value in a declaration document is `Error::general_error` naming the command, as other
statement values are. `AssetManagerOptions::with_fast_capacity` under `Inline` is refused the
way `job_capacity` is ("a kind returns an error for a field it cannot honour",
`environment_builder.rs`).

## Relevant Commands

No new command. Existing commands gain the statement `execution: fast` after their unchanged
`register_command!` signature line (for example the polars `slice` behind the `pl/head` alias);
their signatures do not change, but `specs/command_registry.yaml` gains the field and is
regenerated with a CHANGELOG line.

## Documentation Architecture

- `specs/reference/REGISTER_COMMAND_FSD.md` — `execution:` row in the statement table.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — when to declare `fast` (bounded, input-sized,
  no I/O beyond its inputs) and that `cached: false` is unrelated.
- `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `specs/reference/ASSET_LIFECYCLE.md`
  — the fast lane in the entry-point contract and scheduling section.
- `specs/reference/RECIPES_AND_PLANS.md` — `Plan::execution_class`.
- `specs/reference/ENVIRONMENT_CONFIG.md` — `fast_capacity`.
- Proposed `affects_docs`: the five above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `command_metadata.rs`, `command_declaration.rs`, `plan.rs`, `assets.rs`, `environment_builder.rs`, `liquers-macro`, `liquers-lib` commands, `liquers-py`, `liquers-web`, the registry export |
| Crates and workflows affected | five crates; every command author (optional statement) |
| Existing tests likely to change | registry freshness (`registry_export`); `JobQueue` capacity tests if the slot accounting is refactored |
| New validation | plan class unit tests; a capacity-saturation test proving AC-2 / AC-3; registry export |
| Compatibility / data | additive serde field, omitted when `None`: existing registries and stored metadata read unchanged |
| Concurrency | two counters must be released by the claim that took them, including the `Drop` repair paths (`RunClaim`, re-park); the highest-risk part |
| Performance | one extra branch in `get_asset`; the plan is already built there |
| Security | a mis-declared `fast` command can occupy the fast lane only; slow capacity is unaffected |
| Recovery | revert per step; a plan with no `fast` command takes today's path |
| Certainty | medium for the mechanism; low for the contract until questions 1-2 are answered |
