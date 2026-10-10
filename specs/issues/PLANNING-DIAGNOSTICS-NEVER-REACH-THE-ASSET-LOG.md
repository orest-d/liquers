---
id: PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG
kind: issue
title: Planning diagnostics never reach the evaluated asset's log
status: closed
priority: P2
complexity: M
area: [core/plan, core/assets]
design: 
created: 2026-10-10
github:
---
## Problem

**Example.** Evaluating `seed/t1/t3` with `assets.cut_predecessors: false`, through
`AssetManager::get_asset`, gives an asset whose metadata log is empty. `finalize_plan` did add
`Step::Info("Predecessor boundary not cut: cut_predecessors is false")` to `plan.init_steps`.
Expected: the line appears in the asset log, because `Plan::init_steps`
(`liquers-core/src/plan.rs`) is documented as "Diagnostics produced during planning and analysis …
metadata projection copies it into the asset log".

Nothing performs that projection on the evaluation path. `Metadata::update_from_plan` and
`Plan::update_metadata_record`, which do copy `init_steps` into a `MetadataRecord` log, have no
caller in `liquers-core/src`. `GenericEnvironment::apply_recipe` (`context.rs`) finalizes the plan
and hands it to `apply_plan` without projecting it. So every planning diagnostic is lost from the
evaluated asset:

- why a predecessor boundary was cut or expanded ("Predecessor boundary expanded at …: it is
  volatile");
- an alias resolution ("Command 'x' is an alias of 'y'");
- a recipe's CWD;
- a command's `cached: false` declaration;
- the input-state and `cut_predecessors` declines.

## Impact

Diagnosability. A user asking why an intermediate was not cached, or why a boundary moved, finds
nothing in the asset's log. The plan itself still holds the lines, and `liquers-validate` shows
them. The asset-level "Not cached for reuse: …" line is unaffected, because the asset manager writes
it directly (`assets.rs` `mark_unregistered`). Workaround: build and finalize the plan by hand and
read `init_steps`.

## Expected behaviour

The finalized plan's `init_steps` are appended to the evaluating asset's log once, before its
steps run, on every evaluation path (queued, inline, `apply`). Existing tests that assert exact log
contents need reviewing, since every evaluated asset gains these lines.

## Discovery

Found while implementing `plan-policy` Step 9, 2026-10-10. The integration test meant to assert
the `cut_predecessors` diagnostic in the asset log saw an empty log. It asserts the switch's effect
(recomputation) instead. Not fixed there: every evaluated asset's log would change, which deserves
its own review.

## Resolution

Closed 2026-10-10. `interpreter::apply_plan_state`, the one entry point every evaluation path goes
through, now appends the plan's `init_steps` (`Info` / `Warning` / `Error`) to the evaluating
asset's log once, before the payload gate and the first step. That covers the queued and inline
managers, keyed and ad-hoc queries, and `apply`.

The lines are appended directly to the asset's metadata (`AssetRef::append_log_entries`), not sent
as `Context` log messages. A `LogMessage` saves the metadata at once, so for a keyed asset it would
have left a metadata-only store entry before the value existed. A first version that did so broke
`test_to_override_skips_store_write_when_nonserializable`. Appending directly keeps persistence
exactly as before; the lines are written with the asset's ordinary save.

Evidence: `liquers-core/tests/cache_strategy.rs`, where `uncached_command_runs_inline_*` asserts the
boundary reason and `cut_predecessors_false_expands_*` asserts the switch's line in the asset log,
on both managers. All 42 `liquers-core` suites pass, including the existing exact-log and
persistence tests. Reference: `DOC_08_RECIPES_PLANS.md` §Predecessor boundaries.
