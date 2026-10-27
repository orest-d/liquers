# Phase 1: High-Level Design — command-execution-class

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-2 written, not approved, nothing
implemented. Phases 3 and 4 are intentionally absent (readiness `phase2-blocked`).

## Purpose

An interactive client — the egui desktop application or a web dashboard, where "every view is a
query" (`specs/reference/VISION.md`, delivery forms and use cases) — needs a cheap view to answer at
once even while the server is busy. Today the default asset manager schedules every non-keyed query
that is not already finished on one first-in, first-out job queue of fixed capacity (4), so a cheap
query waits behind any long-running work. `EXTENDED-FAST-TRACK` asks for a command-level
**execution class** (`fast`, `default`, `slow`) and scheduling that lets work declared fast avoid
that wait.

## Problem Example

A server built with `DefaultAssetManager` (capacity 4) is running four 10-minute evaluations of a
model-training command. The UI asks for the first ten rows of a small table
(validated with `liquers-validate`: `GetAsset[data, sales.csv]`, `Action{pl/slice n=10}`):

```text
-R/data/sales.csv/-/ns-pl/head-10
```

Today: `DefaultAssetManager::get_asset` (`liquers-core/src/assets.rs` ~8508) finds no finished
asset, `fast_track` refuses (the query is not a bare key, `AssetData::try_fast_track` ~1391,
`// TODO: support for quick plans`), and `job_queue.submit` parks it `Submitted` until a slot
frees: the view stays empty for up to ten minutes. Should be: when `ns-pl/head` (an alias of
`pl/slice`) and the resource load are declared fast, the query starts at once and answers in
milliseconds, while the training jobs keep their slots.

What the source asked for that is already settled at HEAD:

- `apply()` / `apply_immediately()` fast-track (source items 23, 24): `apply` absorbed
  `apply_immediately` and runs inline in the caller's future without a job-queue slot on both
  managers (`AssetManager::apply` doc, assets.rs ~6007). Nothing remains.
- A cheap step inside a query is no longer scheduled as its own asset: a command's `cached: false`
  makes the plan run it inline after the nearest kept prefix (`Plan::uncached_by`, design
  `plan-policy`, complete). That is about memory, not latency: the enclosing query still queues.

## Scope and Acceptance Criteria

Stated under the recommended answers of the blocking questions (Design Readiness); a different
answer changes AC-2 and AC-3.

- **AC-1** Declaring the class
  WHEN a command is registered with an execution class (`register_command!` statement, or the equivalent in a command declaration document and the Python decorator)
  THEN its `CommandMetadata` carries the class, `specs/command_registry.yaml` exports it, and a command that declares none is `default`
- **AC-2** A fast query does not wait behind slow work
  WHEN every slot of the queued manager is held and a non-keyed query whose plan is fast (every action `fast`, every resource fast-trackable or a fast plan itself) is requested
  THEN it starts without waiting for a slot and its value is available while the long jobs still run
- **AC-3** Fast work is bounded
  WHEN more fast queries arrive than the fast lane admits
  THEN the excess waits for the fast lane (never for the slow slots), and slow and default work keep their capacity
- **AC-4** Unclassified work is unchanged
  WHEN no command of a query declares a class
  THEN scheduling, status sequence and results are as at HEAD, on the queued and the inline manager
- **AC-5** The decision is visible
  WHEN a query is scheduled
  THEN its asset's metadata log records which lane ran it and why (the first non-fast step, when not fast)

Non-goals: pre-emption of running jobs; per-realm class overrides and argument-dependent classes
(source open questions 2-3; follow-ups); changing what `cached: false` means; the inline manager's
behaviour (it already evaluates in the caller); keyed assets (a stored key is already
fast-tracked, and a recomputed key keeps its slot so its persistence path is unchanged).

## Core Interactions

Commands (`CommandMetadata`, `register_command!`, declaration documents, registry export), Plan
(a plan-level class beside `uncached_by`), Assets (`DefaultAssetManager::get_asset`, `JobQueue`),
bindings (Python decorator and `liquers-web` registration, metadata only).

## Crate Placement

`liquers-core`, `liquers-macro`, `liquers-lib` (annotations, registry), `liquers-py`,
`liquers-web`; the dependency flow is unchanged.

## Documentation Intent

- Reference: extend `REGISTER_COMMAND_FSD.md`, `ASSET_LIFECYCLE.md`, `api/DOC_03_*`, `RECIPES_AND_PLANS.md`.
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` (when to declare `fast`).
- Other documents: `specs/command_registry.yaml` regenerated; `liquers-py/README.md` matrix row.
- Documents to update: the above, and `ENVIRONMENT_CONFIG.md` for a configurable lane capacity.

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — new metadata field, macro statement and scheduler lane across
  five crates (rules 1, 3, 4).
- **Leading issue:** what "fast" guarantees (question 1).
- **Explanation:** the gap persists at HEAD and a mechanism exists (Phase 2), but the three
  candidate meanings of `fast` give incompatible behaviour for the Problem Example, and no use case
  measured at HEAD says which one is wanted. Examples and tests cannot be fixed before that choice,
  so Phases 3-4 are not written.
- **Open questions:**
  1. **Blocking - meaning of `fast`:** with four 10-minute jobs holding every slot and 100 UI
     requests for `-R/data/sales.csv/-/ns-pl/head-10` arriving at once:
     (a) *bypass* — fast plans run inline in the caller like `apply`: all 100 answer in
     milliseconds, but nothing bounds how many run at once;
     (b) *reserved lane* — a second pool (`fast_capacity`, e.g. 4) admits fast plans only: the
     first four start at once, the rest wait milliseconds for each other, never for the jobs;
     (c) *priority* — one pool, fast plans jump the waiting line: all 100 still wait up to ten
     minutes, because nothing pre-empts running jobs.
     Recommended: (b), the only one that meets both AC-2 and AC-3; (c) does not solve the Problem
     Example at all.
  2. **Blocking - is it wanted now:** the source predates inline `apply`, `cached: false` and
     evaluation sessions, and no issue reports a measured stall. Recommended: confirm with the
     egui or web dashboard use case before building; if confirmed, implement (b) as the first slice.
  3. **Open design - resource loads:** is a `-R/...` load fast? A stored key is fast-tracked
     without a slot today, but a missing key runs its recipe, and a large file on a remote store is
     not cheap. Recommended: a resource step counts as fast only when its key fast-tracks (the
     check `get_asset` already makes); otherwise the plan is `default`.
  4. **Open design - what `default` maps to** (source question 1): recommended `slow` lane (today's
     behaviour), so only an explicit `fast` changes anything.
  5. **Proposed resolution - name and syntax:** `execution: fast` in `register_command!`, an
     `Option<ExecutionClass>` field read through an accessor (as `cached`), so an omitted value
     keeps `CommandMetadata::default()` meaning `default`.

## Design Dependencies

- `plan-policy` (complete) — **overlaps**: `cached: false` already runs cheap steps inline; the
  plan-level class is derived beside `uncached_by` in the same builder pass. Frozen (E4), not
  extended.
- `dependency-management`, `dependency-resolved-once-per-evaluation` (complete) — **overlaps**: a fast plan's
  dependencies must use the same claim-based wait (`wait_for_dependency`), which already borrows the
  waiter's slot, so a fast lane needs no new deadlock rule.
- `EXTERNAL-MANAGER-EVALUATION-SESSION-SHARING` (design in review) — **overlaps**: touches how a
  manager schedules dependencies; a lane must not change its contract. No order.
- `DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES` (issue, filed with this design) — **requires**
  (documentation): the entry-point table to extend is stale for `apply`; fix first or together.

## References

Source `specs/issues/EXTENDED-FAST-TRACK.md` (re-scoped 2026-10-20); code and documents as named
in Phase 2.
