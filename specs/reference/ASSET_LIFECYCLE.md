---
title: Asset Evaluation — Flows and Public Surface
kind: reference
audience: internal
area: [core/assets]
reviewed: 2026-09-04
---
# Asset Evaluation — Flows and Public Surface

## Overview

How a query becomes a value: what the public surface is, which methods survive and how they relate,
what happens step by step, and **why** evaluations differ from one another when there is only one
evaluation path.

This document replaces the former "Asset Lifecycle — Comprehensive Map", whose stated purpose was
to catalogue duplication between `Context` and `AssetRef` "as a basis for potential refactoring".
That refactoring happened (`specs/design/evaluate-path-consolidation/`), so the catalogue's subject
no longer exists. The analysis is preserved as
[`archive/2026-09-04-asset-lifecycle-duplication-audit.md`](../archive/2026-09-04-asset-lifecycle-duplication-audit.md).

The authoritative API detail is the module rustdoc of `liquers-core/src/assets.rs`; this document
gives the model that rustdoc assumes.

## 1. The public surface

An asset is **constructed and managed by an `AssetManager`**. There is no supported way to evaluate
an `AssetRef` directly: the evaluation body is private to the module, and the run entry points are
crate-internal. A caller obtains a handle from a manager, then reads from it.

| You want to… | Call |
|---|---|
| Evaluate a query | `EnvRef::evaluate` → `AssetManager::get_asset` |
| Evaluate a query with a payload | `EnvRef::evaluate_immediately` → `AssetManager::apply` |
| Fetch a keyed resource | `AssetManager::get` |
| Apply a recipe to a supplied state | `AssetManager::apply` |
| Evaluate a nested query from inside a command | `Context::evaluate`, `Context::get_dependency_state` |
| Apply a query to a state from inside a command | `Context::apply` |
| Wait for a value | `AssetRef::get` |
| Look without waiting | `AssetRef::poll_state`, `AssetRef::status`, `AssetRef::try_poll_state` |
| Describe an asset to a client | `AssetRef::get_asset_info` |
| Install or remove a keyed value | `AssetManager::set_state`, `set_binary`, `remove`, `to_override` |

**Framework infrastructure**, not application API: `AssetData`, `AssetServiceMessage`, `JobQueue`,
`MetadataSaver`, `RunClaim` / `InlineRunClaim`, the run entry points, and direct access through
`AssetRef::data`.

## 2. The surviving methods, and how they relate

Four layers. Each has one job.

```
AssetManager::get_asset / get / apply        ← entry points: decide WHAT ASSET to build
        │
AssetRef::run / run_inline                   ← harnesses: decide HOW IT IS DRIVEN (spawn or not)
        │
AssetRef::evaluate(payload)                  ← the one body: decides NOTHING, does everything
        │
Environment::apply_recipe → interpreter::apply_plan → commands
```

| Method | Visibility | Purpose | Relationship |
|---|---|---|---|
| `AssetManager::get_asset(query)` | public | resolve a query through the maps, fast-track or schedule | delegates to `get` for a pure key |
| `AssetManager::get(key)` | public | resolve a keyed asset | builds a **keyed** asset |
| `AssetManager::apply(recipe, state, payload)` | public | ad-hoc evaluation against a supplied state | builds a **non-keyed** asset; always evaluates before returning |
| `AssetManager::apply_immediately(…)` | **deprecated** | compatibility shim | forwards to `apply` |
| `AssetRef::run(payload)` | crate | spawning harness | `#[cfg(not(wasm32))]`; wraps `evaluate` |
| `AssetRef::run_inline(payload)` | crate | spawn-free harness | wraps `evaluate`; used by inline managers and on wasm |
| `AssetRef::evaluate(payload)` | **private** | the single evaluation body | reached by both harnesses, and by nothing else |
| `Environment::apply_recipe` | public hook | plan building and interpretation | the extension point for a custom environment |

The two harnesses differ **only** in how the service-message loop is driven: `tokio::spawn` plus
`tokio::select!` natively, `futures::join!` plus `futures::select!` inline. That is a platform
split, not duplication — collapsing it would mean giving up the spawned loop natively or faking a
spawn on wasm. `CORE-TOKIO-REMOVAL` owns changing it.

## 3. The execution flow, step by step

Every entry point reaches the same body, and the order inside it is invariant. Steps 5–7 are
load-bearing and must not be reordered.

| # | Step | Why it is here |
|---|---|---|
| 1 | Resolve volatility | later steps branch on it; it must be settled before any of them |
| 2 | Resolve the recipe | a key recipe either **hands off** to the key's registered owner, or is resolved through the recipe provider and adopted, so the asset evaluates its real recipe rather than a placeholder |
| 3 | Apply the recipe, with any payload installed on the context | `Environment::apply_recipe` → `interpreter::apply_plan`, which holds the authoritative payload gate |
| 4 | Record observed dependencies into metadata | **unconditional, for every entry point** — this is the asymmetry `CORE-EVALUATE-PATH-CONSOLIDATION` named |
| 5 | Install the value, its type identifier and type name | merged into the live metadata, never installed as a snapshot: the service loop is writing progress and log entries to the same record concurrently |
| 6 | Finalize status | the single status authority, and it must run **before** the notification and **before** persistence, so nothing observes or stores a non-final status |
| 7 | Send `ValueProduced` | after step 6, so a client that polls on the notification sees a terminal status |
| 8 | Persist | only if this is a **keyed** asset and this evaluation did not hand off |
| 9 | Register with the dependency manager | self-limiting on status and ownership, so ad-hoc assets register nothing |

On failure the body propagates with `?` and the harness's failure routine is the single authority:
it clears the value, records the error in metadata, sets `Status::Error` and notifies.

## 4. Why the flows differ

There is one path. Evaluations still differ, along axes that are **properties of the asset**,
each decided when the asset is constructed — not branches in the evaluating code.

| Axis | Why it exists | What it changes |
|---|---|---|
| **Keyed or not** | whether the asset is associated with a key, and so whether anything can ask for it again | store target, map membership, reuse |
| **Initial state supplied** | the caller injects input that the identity does not describe, so the result is not reproducible from that identity | not keyed, so never stored and never reused |
| **Payload** | per-call caller context, deliberately *not* part of identity, and it cannot cross a key boundary | never mapped, never reused, never loadable |
| **Volatility** | the result is valid but single-use | stored, but with a status `try_fast_track` refuses |
| **Delegation** | another asset owns the key | hand-off: the owner writes, and no second dependency edge is recorded |
| **Fast-track** | a stored value is already valid | evaluation is skipped entirely |
| **Queued or inline** | manager policy (`AssetManager::eval_mode`) | scheduling and the status sequence only |

The first five are properties of the asset, the sixth is a relationship between two assets, and
**only the last is policy**.

This is why "one evaluation path" does not mean "every entry point is interchangeable". They are
thin *in evaluation logic*; construction still decides what an asset is, and construction is what
decides whether two concurrent callers converge on one asset. Two `apply` calls build two separate
ad-hoc assets and legitimately run the body twice; two `get_asset` calls for one query converge on
one mapped asset and run it once.

## 5. Keyed assets, storing, and persistence

**A keyed asset is an asset associated with a key.** Whether it is keyed, and which key, is known
when the manager creates it — the key is the argument the constructor was called with — and is
recorded on the asset. It is never re-derived from the recipe afterwards, because provider
resolution replaces that recipe mid-evaluation.

Three properties follow, in one direction only:

```text
stored     ⟹ keyed        only a keyed asset is written to the store
persistent ⟹ stored       only a stored asset can be loaded back
```

Read the contrapositive: **not keyed means never stored and never loadable.**

- A **volatile keyed** asset *is* keyed, so it is stored. It is not persistent: it is written with
  a status `try_fast_track` refuses, so the bytes exist for a person to inspect but the system will
  not reuse them.
- An **ad-hoc `apply`** asset is not keyed, so whether it may write never arises — even when its
  recipe is shaped like a key.
- A **query** asset is not keyed and is never stored.

### Persistence outcomes

| Case | Keyed | Written? |
|---|---|---|
| Keyed, non-volatile (recipe-defined) | yes | yes, loadable |
| Keyed, volatile | yes | yes, **not** loadable |
| Keyed, delegating to the owner | yes | no — the owner writes |
| Query asset | no | no |
| `apply`, bare-key recipe | no | no |
| `apply`, recipe with a filename | no | no |
| `apply` with a payload | no | no |
| `set_state(key, state)` | yes | yes — an explicit install, which never evaluates |

### Ownership versus registration

Ownership is approximated by keyedness. Whether the manager **registers** a keyed asset in its key
map is a separate caching-and-sharing decision belonging to the manager: declining to register a
non-volatile keyed asset still produces correct results, and a volatile keyed asset is deliberately
never registered. At most one asset is registered per key.

A non-volatile keyed asset that writes while not the registered owner records a **warning in its
metadata** rather than failing. The gray zone is legal; it is simply no longer silent. The exact
contract — whether registration can be guaranteed, how to observe it without a lock, and what a
protected registration does when the asset expires — is
[`issues/ASSET-REGISTRATION-OWNERSHIP-CONTRACT`](../issues/ASSET-REGISTRATION-OWNERSHIP-CONTRACT.md).

## 6. What reaches metadata

Facts recorded during evaluation, and where a client reads them:

| Fact | `MetadataRecord` | `AssetInfo` |
|---|---|---|
| The key, when the asset is keyed | `key` | `key` |
| Payload requirement of the plan | `payload_required` | `payload_required` |
| Volatility | `is_volatile` | `is_volatile` |
| Observed dependencies | `dependencies` | — |
| Status, type identifier, type name | yes | yes |

The payload requirement is the plan's, not the caller's: a plan needing no payload records `None`
even when one was supplied. A keyed asset and a non-keyed query asset built from the same query are
therefore distinguishable in metadata, which they were not before.

## 7. Execute-once

An asset's body runs once, on both paths.

- Queued: `RunClaim`, an atomic status transition with a `Drop` repair that re-submits to the queue.
- Inline: `InlineRunClaim`, the same transition, with a `Drop` repair that restores a re-runnable
  status instead — there is no queue to re-submit to.

`Status::Dependencies` is an **active** state in both: its runner is merely parked awaiting a
child, so claiming it would let a second caller re-run a live evaluation.

A caller that does not win the claim **waits** for the running evaluation. Refusing it is wrong and
was tried and reverted once: a genuinely async command yields, so a second legitimate request
arrives mid-evaluation and must join the first rather than be turned away.

## 8. Cross-reference

| Topic | Document |
|---|---|
| Asset model, statuses, state machine | [`ASSETS.md`](ASSETS.md) |
| API-level contract | [`api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`](api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md) |
| `Status::Dependencies` semantics | [`DEPENDENCIES_STATUS.md`](DEPENDENCIES_STATUS.md) |
| Payloads | [`PAYLOAD_GUIDE.md`](PAYLOAD_GUIDE.md) |
| Recipes and plans | [`api/DOC_08_RECIPES_PLANS.md`](api/DOC_08_RECIPES_PLANS.md) |
| Why it is built this way | [`../design/evaluate-path-consolidation/`](../design/evaluate-path-consolidation/) |
| The duplication this removed | [`../archive/2026-09-04-asset-lifecycle-duplication-audit.md`](../archive/2026-09-04-asset-lifecycle-duplication-audit.md) |

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-04 | Rewritten. The document's former purpose — cataloguing duplication between the evaluation paths as a basis for refactoring — was completed by `evaluate-path-consolidation`, leaving most of its body false at HEAD. Now describes the public surface, the surviving methods and their relationships, the step-by-step flow, and the axes along which evaluations differ. Paths A–D, the asymmetry table and the issue list are archived. | `design/evaluate-path-consolidation/` phase 5 |
| 2026-08-26 | Previous revision, as the "Comprehensive Map". | — |
