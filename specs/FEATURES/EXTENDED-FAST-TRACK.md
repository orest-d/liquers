# EXTENDED-FAST-TRACK

Status: Draft

## Summary
`EXTENDED-FAST-TRACK` expands fast-track execution beyond simple loading and query-only shortcuts.
The feature introduces command-level execution class metadata and runtime scheduling policies that prioritize interactive workloads while still supporting explicitly slow work.

This is a broader execution model update, not only a "fast command" label.

## Moved from ASSETS-FIX1
| Original issue # | Previous location | Marker | Migration note |
|---|---|---|---|
| 4 | `assets.rs:401` | `TODO: support for quick plans` | Move from narrow quick-plan handling to command-qualified fast-track pipeline. |
| 23 | `assets.rs:2380` | `TODO` fast-track for `apply()` | Treat `apply()` as schedulable work item with execution class and queue policy. |
| 24 | `assets.rs:2399` | `TODO` fast-track for `apply_immediately()` | Align immediate execution with class-aware policy; preserve low-latency path where allowed. |

## Requirements
1. Fast-track must support selected command execution, not just resource loading and trivial plan shapes.
2. Commands must be classifiable via command metadata for scheduling decisions.
3. Initial execution classes should include at least:
   - `fast`: ultra-low-latency, interactive-safe commands.
   - `slow`: commands that are expected to be heavier and should not starve interactive work.
   - `default`: backward-compatible fallback when no class is set.
4. Runtime should support differentiated scheduling (for example, separate queues or queue priorities) driven by execution class.
5. Fast-track eligibility must remain conservative: only commands explicitly qualified as `fast` can use interactive fast path.
6. Existing behavior must remain valid for unclassified commands (`default`) until migration is complete.

## Design Notes
- Command metadata is the source of truth for execution class.
- Queue strategy can evolve:
  - phase 1: single queue with class-aware prioritization;
  - phase 2: separate interactive (`fast`) and background (`slow/default`) queues.
- `apply()` and `apply_immediately()` should use the same classification rules to avoid policy drift.
- The model should make starvation and fairness explicit (e.g., bounded fast-lane bursts).

## Open Questions
1. Should `default` map to `slow`, or to medium priority with configurable behavior?
2. Do we need per-realm overrides for execution class policy?
3. Should metadata allow dynamic class selection based on argument values?

## Suggested Implementation Phases
1. Metadata extension:
   - Add execution-class field to command metadata.
   - Add parser/registration support and defaults.
2. Planning and eligibility:
   - Compute plan-level fast-track eligibility from command classes.
   - Reject fast path when any step is non-`fast`.
3. Scheduler integration:
   - Add class-aware queue selection/prioritization.
   - Route `get_asset`, `apply`, and `apply_immediately` through shared classification policy.
4. Observability:
   - Record class and queue decisions in logs/metadata for debugging.
5. Validation:
   - Add tests for class propagation, queue routing, fairness, and fallback behavior.
