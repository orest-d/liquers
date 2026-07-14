# dependency-scheduling Design Tracking

**Created:** 2026-07-14

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-07-14)
- [ ] Phase 2: Solution & Architecture (drafted, reviewed, awaiting approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

- 2026-07-14: Phase 1 drafted. Design decisions recorded from planning session:
  local-queue-only parking (no global Submitted parking for scheduled dependencies),
  wait resumes parent as `Processing` (new `leave_dependencies_and_resume`),
  supersedes plan20260707.md WP-1 Phase 2A (`EvaluationOutcome::Delegated`).
- 2026-07-14 (Phase 1 feedback round): local dependency queues moved from `AssetData`
  into `JobQueue` (implementation detail of the queue mechanism; lazily-created
  per-dependent-id entries, removed when drained; zero per-asset memory once the
  asset is produced). Drain/wait API becomes manager-mediated (`AssetManager` trait
  methods implemented by `DefaultAssetManager` against its JobQueue); the drain still
  executes inside the dependent asset's own future, so the progress argument is
  unchanged.
- 2026-07-14 cycle-detection verification (user model: only keyed assets can be
  dependencies; non-keyed assets are expressions potentially dependent on keyed
  assets; existing DependencyManager detection should prevent cycles). Verified
  against code — the model is only PARTIALLY enforced today:
  1. Queries are used as ad-hoc dependency keys (`DependencyKey::from(&Query)`,
     metadata.rs:254; used at context.rs:211,229 and assets.rs:800-801), so the
     keyed-only rule is not a strict code invariant.
  2. Cycle checks run only when the DEPENDENT asset is keyed (context.rs:209,
     assets.rs:839); a non-keyed expression evaluating a dependency performs no
     check. Undetected hangs today: (a) cached query asset whose command evaluates
     its own query (self-wait); (b) K1 → Q → K1 cycles threading through a non-keyed
     expression.
  3. `Context::evaluate` never inserts DependencyManager edges during evaluation
     (only pending records + weak `add_dependent_asset`); edges appear only via the
     delegation path, `register_plan_dependencies` (which skips first-seen deps with
     no version, assets.rs:2654) and post-Ready `track_asset`. Two keyed assets that
     purely dynamically evaluate each other can both pass the check → deadlock.
  Resolution (uses the existing mechanism; no second detector): the scheduling API
  attributes expression evaluations to the nearest KEYED ancestor (evaluation context
  carries the ancestor's `DependencyKey`); `schedule_dependency` checks
  `would_create_cycle(ancestor, dep)` and registers the edge at schedule time
  (mirroring assets.rs:854-860), plus the direct dependent==dependency comparison for
  unkeyed self-cycles. Red tests planned: unkeyed self-cycle, K1→Q→K1, purely-dynamic
  keyed mutual cycle — all must fail with `Error::dependency_cycle`, no hang.
- 2026-07-14 leftover local-queue entries (open question 1 RESOLVED by user).
  Causes: dependency error or the parent's own/earlier-step error before the wait
  (the interpreter pre-pass schedules all steps' dependencies up front), parent
  cancellation, command-level conditional scheduling that never awaits the handle,
  external override/set or expiration mid-evaluation, manager/queue shutdown. All
  funnel into one cleanup point: removal of the parent's local-queue entry at the
  parent's terminal status. Policy — distinguish by shareability:
  - SHARED (managed) assets, i.e. present in the manager maps (`assets` keyed map,
    `query_assets`): keep as `Submitted` — insert the leftover into the global
    JobQueue jobs list at cleanup (under local-only parking it was never there), so
    the worker eventually runs it and plain `asset.get().await` waiters are not
    stranded.
  - NON-SHARED (unmanaged) assets — volatile (fresh AssetRef per request, never in
    the maps) and ad-hoc `apply` assets outside the maps: discard with a debug log;
    the parent's handle/local-queue entry were the only references.
  Authoritative shareability test: manager-map membership (encodes "someone else can
  obtain this AssetRef"; covers ad-hoc assets automatically; the cleanup runs in
  JobQueue/DefaultAssetManager, which owns the maps).
- 2026-07-14 Phase 2 gate decisions (user):
  - Relevant commands: CONFIRMED none — mechanism is command-transparent; no
    liquers-lib namespace is modified.
  - Cycle model REFINED (supersedes the "nearest keyed ancestor" wording above):
    "only keyed assets can be nodes of a dependency tree; a non-keyed asset should
    be treated as the set of keyed assets it depends on." Phase 2 implements this
    as expression ATTRIBUTION SETS in the DependencyManager
    (ScheduleNode::{Keyed,Expression}; transient expression_dependents /
    expression_keyed_deps / expression_expr_deps maps;
    register_scheduled_dependency expands expression edges onto their keyed
    dependents with cycle checks; remove_expression cleanup at expression terminal
    status). Late-joining keyed dependents of a shared expression inherit edges to
    the expression's already-known keyed dependencies, so K2→Q→K2 through a shared
    expression is caught; pure-expression self-schedules and attribution-traversal
    re-entry fail fast with Error::dependency_cycle.
  - Phase 2 multi-agent review (2 reviewers: Phase 1 conformity, codebase
    alignment): NO ISSUES FOUND; fixer skipped per workflow.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
