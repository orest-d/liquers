# Phase 1: High-Level Design — plan-policy

## Purpose

Large, cheap intermediates (a parsed or filtered frame) waste memory and disk when they are kept,
and today only a keyed recipe can say "do not keep this". This design lets the **command** that
produces a value, the **query** (with directives), and the **recipe** say whether a result is
`stored` and `cached`. The plan computes the effective flags and the reason for them, every asset
honours them, and the predecessor boundary is never cut at a result that is not cached. It also
makes `v` positional and retires the three `TODO: support …` markers in `plan.rs`.

## Problem Example

```yaml
sales_summary.txt:
  query: "-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU/describe"
```

Today the plan is cut at `-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU`. The filtered frame
becomes a cached asset and stays in memory, although `eq` is quick and the frame is used once.
Nothing can prevent this. `pl/eq` cannot declare it, the query cannot say it, and the recipe's
`cached: false` applies only to the final keyed asset.

Expected: the author writes `…/eq-region-EU/cached-false/describe`, or `pl/eq` is registered with
`cached: false`. The boundary walk skips the uncached candidate and cuts one level earlier:
`Evaluate(-R/data/sales.csv/-/ns-pl/from_csv) Action(eq) Action(describe)`. The parsed frame is
cached, the filtered one is not, `describe`'s small result is cached and stored as before, and the
plan log says why: `Predecessor boundary expanded at '…/eq-region-EU/cached-false': its result is
not cached (directive 'cached-false')`.

Both queries validate with `liquers-validate` (with `--command cached` standing in for the new
directive until it exists).

## Analysis: the three markers

| Marker | Decision |
|---|---|
| `cache` | The `stored`/`cached` model below. **Intermediate** caching is the predecessor boundary, so a result that is not cached is never a boundary. |
| `inline flag` | **Not added.** Inlining a command `c` in `a/b/c/d` gives `Evaluate(a/b) c d`, the same plan that `cached: false` on `c` produces. The claimed speed-up and queue avoidance belong to the boundary `Evaluate(a/b)`, which inlining `c` keeps. "Inline" already names `EvalMode::Inline` and `run_inline`. |
| `volatile flags` | All volatility instruments already exist. `Plan::is_volatile` stays: it is the analysis result, read by `get_query_asset`, `cut_predecessor` and dependency recording. The one gap, positional `v`, is merged in. |

`qinline` was also considered and rejected. `qinline c` would make `a/b/c/d` mean `a/b/q/c/d`, which
`q` already expresses and the builder already plans (`UseQueryValue(a/b) c d`). Hiding the quoting
inside a command makes a pipeline that never evaluates its prefix look like one that does. That
hidden quoting would also need special cases in dependency, volatility, expiry, cut and validation.

## The retention model

- **The flags belong to a value and are decided by whoever produced it. They are not contagious.**
  Each prefix result has a `(stored, cached)` pair, true/true by default.
  - An action takes the pair from its command's metadata. A command that declares nothing gives
    the default, so `a/b` is not stored when `b` declares `stored: false`, and `b/a` is stored.
  - A `stored` or `cached` **directive** overrides the pair for the value at its own position.
  - A recipe's explicit field overrides the plan's final pair. A conflict is logged.
- **Volatility is contagious and positional.** `a/b/v/c` is volatile from `v` onward and stable in
  `a/b`. `v` at the head still means the whole query. A recipe's `volatile: true` stays whole-plan,
  because it has no position.
- **One rule for the boundary.** A candidate whose result is volatile, needs a payload, or is not
  cached is not cut; the walk steps back past it. `stored: false` alone does not block a boundary.
  The boundary is still a valid in-memory cache entry.

## Scope and Acceptance Criteria

- **AC-1** Directive skips a boundary
  WHEN the problem example is evaluated with `cached-false` after `eq-region-EU`
  THEN the plan is `Evaluate(…/from_csv) eq describe`, the result equals today's, and the log
  names the directive
- **AC-2** Command declaration
  WHEN a command registered with `cached: false` (or `stored: false`) ends a query
  THEN the query's asset is not registered for reuse (not written to the store), and the log names
  the command
- **AC-3** Not contagious
  WHEN `b` declares `stored: false` and `a` declares nothing
  THEN `a/b` is not stored and `b/a` is stored
- **AC-4** Directive vocabulary
  WHEN `stored`, `stored-true`, `stored-false`, `cached`, `cached-true` or `cached-false` appears in
  a query
  THEN it emits no step, sets the flag for the value at its position, rejects any other argument
  with a positioned error, and is reserved in every namespace like `q`, `v` and `ns`
- **AC-5** Query assets honour `cached`
  WHEN a non-keyed query whose result is not cached is evaluated twice, on either manager
  THEN it is evaluated twice and never inserted into `query_assets`, and it is not volatile
- **AC-6** Recipe precedence
  WHEN a recipe sets `cached` or `stored` explicitly and its plan says otherwise
  THEN the recipe wins and the log records the override
- **AC-7** Visible in metadata
  WHEN an asset's result is not stored or not cached
  THEN its `MetadataRecord` and `AssetInfo` show the effective flag as `Some(false)`, and its log
  carries the reason
- **AC-8** Positional `v`
  WHEN `a/b/v/c` is evaluated
  THEN the plan is volatile, `a/b` is cut as a cached boundary, and `v/a/b/c` still cuts nothing
- **AC-9** Defaults unchanged
  WHEN no command, directive or recipe sets a flag and `v` is absent or at the head
  THEN plans, assets and stored files are exactly as today, and existing tests pass unchanged
- **AC-10** Markers retired
  WHEN `plan.rs` is read
  THEN the three `TODO: support …` markers are gone, and the builder documentation says where
  each concern now lives

**Non-goals.** An `inline` or `qinline` command flag. An environment- or recipe-level
"never cut" switch (see Open Questions, 1). Changing any existing library command's flags. A
reason field on metadata. An asset retention or GC policy (`CORE-ASSET-GC`).

## Core Interactions

- **Query/parse:** two new directives, reserved like `q`/`v`/`ns`. No grammar change.
- **Commands and macro:** `stored:` / `cached:` metadata, a registry export field, and a
  `specs/command_registry.yaml` regeneration.
- **Plan:** effective flags with reasons; positional `v`; the boundary walk's third condition.
- **Assets:** both managers' query-asset paths; keyed assets taking the plan's flags when the
  recipe is silent.

## Crate Placement

`liquers-core` (plan, assets, command metadata, recipes) and `liquers-macro` (keywords).
`liquers-lib` only for the regenerated registry. `liquers-py` and `liquers-web` are untouched,
since the new fields have serde defaults.

## Documentation Intent

- Reference: extend `DOC_08_RECIPES_PLANS.md` (retention flags, directives, positional `v`, the
  boundary rule), `REGISTER_COMMAND_FSD.md` (keywords), `ASSETS.md` (query-asset caching), and
  `PROJECT_OVERVIEW.md` (the directive list).
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` with when to declare `cached: false`.
- Other documents: close both source issues with resolution notes mapping each marker.
- Documents to update: `specs/README.md`, and `CLAUDE.md`'s DSL metadata list.

## Scope Changes

- **2026-10-10, discussion before the Phase 1 gate.** The first draft proposed a
  `predecessor_boundary` policy (environment default plus recipe override) as the answer to both
  `cache` and `inline`. The user redirected the scope to the `stored`/`cached` model across command,
  plan, directive, asset and metadata, with reasons in the log, and asked for `inline` and
  `qinline` to be evaluated (both rejected above). Merged `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`
  at the user's request (overlap T3, the same `mark_volatile` / `VolatilitySource` change site, and
  T2, the shared boundary rule). Found while checking: `get_query_asset` ignores `cached` on both
  managers, which is now AC-5. Size: `M` → `L`, so the design was converted to the full form.

## Design Dependencies

- **overlaps** `CORE-ASSET-GC`: retention over time. This design decides only whether a value is
  kept at all.
- **predecessor** `predecessor-cut-equivalence` (complete, frozen): the boundary walk this extends.

## Open Questions

1. **(Proposed resolution) Drop the environment/recipe `predecessor_boundary: expand` switch.**
   Whether to cut does not depend on the manager. Both managers register the boundary in
   `query_assets` and expire it, so sharing, independent expiry and the dependency edge are the same
   on either. Only parallelism differs: the queued manager runs the boundary alongside the plan's
   other dependencies (`schedule_plan_dependencies_from`). The inline manager runs it in sequence,
   at the cost of one extra asset and a map lookup. Tying the policy to the manager kind would make
   the same query leave different assets and dependency edges per manager, which the equivalence
   tests between managers exist to prevent. A global "expand" pays off only when a prefix is never
   reused, which is a property of the command or query, and is now expressed by `cached: false`.
   `finalize_plan_expanded` remains for readers and tools. It can be added later without breaking
   anything if a real workload needs it.
2. **(Open design) The asymmetry between `v` and the retention flags.** After `v` everything is
   volatile, while `cached-false` applies to one value and the next action resets it. This is
   deliberate, because purity flows forward and size does not. It is the most likely point of
   confusion, so the reference will state it next to both directives. Is that enough, or should
   the retention directives be named to signal "this value only"?
3. **(Implementation detail) What counts as the last producer.** `Filename`, `ns` and `cwd` do not
   reset the pair. A resource step (`-R/key`) takes the pair from that key's own recipe. To be fixed
   in Phase 2.

## References

- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md`, `specs/issues/V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL.md`
- `specs/design/record-streams/phase2-architecture.md` §"C. `stored` and `cached`" (the recipe flags)
- `specs/reference/api/DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries"
