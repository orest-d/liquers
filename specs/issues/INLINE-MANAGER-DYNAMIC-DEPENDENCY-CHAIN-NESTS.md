---
id: INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS
kind: issue
title: On the inline manager, a chain through a dynamic or volatile dependency still nests one stack level per link
status: draft
priority: P3
complexity: M
area: [core/assets]
design: 
created: 2026-10-12
github:
---
## Problem

`dependency-resolved-once-per-evaluation` makes the stack depth of a cold evaluation independent of
chain length on the inline `ImmediateAssetManager` by priming the *static* upstream bottom-up
(`plan.rs` `upstream_graph`, `ImmediateAssetManager::prime_upstream`). Priming sees only plan-level
`State`/`Link` reads served by a recipe, and skips volatile keys. Two kinds of link are invisible to
it:

- a **dynamic** dependency, which a command requests at run time (`Context::get_dependency_state`,
  `submit`, `evaluate`);
- a **volatile** key.

A third case was left out at implementation: `ImmediateAssetManager::apply_queued` is not primed.
It must return its ad-hoc asset even when the evaluation fails, and that asset records a failure
only by running, so a failed priming would leave it unevaluated.

Everything upstream of such a link is still evaluated inside its dependent's run, one nested
`run_inline` per link (`ImmediateAssetManager::get_dependency_asset`). A long enough chain through
such links can still overflow the stack. The queued manager is not affected, because it spawns every
locally queued run.

## Impact

- **Affected:** inline-manager users (`ImmediateEnvironment`, and `liquers-web` in the browser,
  where the wasm stack is about 1 MiB) whose recipe chains run through commands that fetch their
  inputs dynamically, or through volatile keys, for dozens of links.
- **Severity:** a stack overflow aborts the process (or traps the wasm instance). That is rare: it
  needs a long chain of such links.
- **Workaround:** evaluate upstream links first, or express the dependencies as plan-level links so
  priming sees them.

## Expected behaviour

Stack depth independent of chain length for every dependency kind on the inline manager. A possible
direction is to prime on demand: when a dynamic request finds a cold keyed dependency, walk and
prime its static upstream before running it.

## Discovery

Recorded on 2026-10-12 as the residual limit of `deep-chain-evaluation-stack-depth`, and carried
into `dependency-resolved-once-per-evaluation` (Phase 1 non-goals; Phase 2 §Overview). Filed at that
design's Phase 4 review, rather than after implementation, so the limit is not lost.

Triage: it overlaps `dependency-resolved-once-per-evaluation` (T3, the same change site
`ImmediateAssetManager::get_dependency_asset`), but it is outside that design's agreed scope (a
non-goal there). It is filed without a design of its own, because the design depends on how
priming is finally built; it should get one when that design reaches Phase 5.
