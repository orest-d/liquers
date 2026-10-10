# Phase 1: High-Level Design — plan-policy

## Purpose

Today every prefix of a chain becomes a cached asset (measured, `DESIGN.md` notes), so a long chain
over a large value keeps one copy per step. Nobody can limit this: not the command author, not the
recipe author, and not the operator of a public service. This design adds three instruments for
that:

- a **command flag** for execution: an uncached command runs inline;
- a **caching strategy** per recipe, with environment defaults for recipes and for ad-hoc queries;
- a **debugging switch** that disables cutting.

It also makes `v` positional and retires the three `TODO: support …` markers in `plan.rs`.

## Problem Example

An internet-facing service lets guests run ad-hoc queries. Its recipes are the approved
computations, such as:

```yaml
sales_summary.txt:
  query: "-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU/describe"
```

The query validates with `liquers-validate`.

**Today**, evaluating it caches the final result and every prefix, including the filtered frame
`…/from_csv/eq-region-EU`. `eq` is quick and its output is large. Each guest query caches every
prefix too, and nothing can stop either.

**Expected:**

- `pl/eq`, registered with `cached: false`, runs inline. The plan is
  `Evaluate(-R/data/sales.csv/-/ns-pl/from_csv) Action(eq) Action(describe)`, and the log says
  `not cached: command 'pl/eq' declares cached: false`.
- The operator sets `assets.query_cache_strategy: none`. Guest queries then create no cache entries,
  but still reuse the parsed frame `…/from_csv` that the approved recipe cached.

## Analysis: the three markers

| Marker | Decision |
|---|---|
| `cache` | The caching strategies (`none` / `result` / `all`). Intermediate caching *is* the predecessor boundary, because a boundary is a cache entry. |
| `inline flag` | The command's `cached: false`: its output is never a boundary, so the command runs inline after the nearest kept prefix. A separate `inline` keyword was rejected, because the word already names the inline asset manager. `qinline` (a command receiving its predecessor's query) was rejected too: `q` already expresses it explicitly, and hiding it would mislead dependency, volatility and cut analysis. |
| `volatile flags` | All volatility instruments already exist. `Plan::is_volatile` stays, as the analysis result. The one gap is positional `v` (merged in). |

## The model

**Caching strategy.** `none`, `result` or `all`:

| Strategy | Result kept | Intermediates |
|---|---|---|
| `all` (default) | yes | created and cached, as today |
| `result` | yes | not created; an existing one is reused |
| `none` | no | not created; an existing one is reused |

- **The strategy follows the origin.**
  - A keyed recipe uses its `cached:` (`default`, `none`, `result`, `all`; `true` = `all`,
    `false` = `none`). Absent or `default` means `assets.recipe_cache_strategy`.
  - An ad-hoc query uses `assets.query_cache_strategy`.
  - A boundary uses the strategy of the evaluation that created it. The strategy changes no value,
    so it is not part of the boundary's identity.
  - An intermediate that already exists is reused by everyone.
- **Keyed result:** decided by the effective recipe strategy alone.
- **Non-keyed result:** first the last command (`cached: false` means not kept), then the query
  strategy.
- **Intermediate:** created only under `all`, and only when its producing command does not
  declare `cached: false`. The command flag can only restrict. A recipe cannot force an intermediate
  that its command declared not worth caching.
- **Volatile values are never kept**, as today. Positional `v` makes `a/b/v/c` volatile from `v`
  onward, so `a/b` can be a boundary. `v` at the head still means the whole query. A recipe's
  `volatile: true` stays whole-plan.
- **`assets.cut_predecessors: false`** (debugging) never cuts and never reuses, giving a fully
  expanded plan to compare against.
- **`stored` is unchanged** and stays recipe-only.

## Scope and Acceptance Criteria

- **AC-1** Command runs inline
  WHEN `a/b/c/d` is evaluated and `c` is registered with `cached: false`
  THEN the plan is `Evaluate(a/b) c d`, the result equals today's, and the log names `c`
- **AC-2** Uncached command ends an ad-hoc query
  WHEN a non-keyed query ending with a `cached: false` command is evaluated twice, on either manager
  THEN it runs twice, is never inserted into `query_assets`, and is not volatile
- **AC-3** Query strategy
  WHEN `query_cache_strategy` is `result` (or `none`) and an ad-hoc query is evaluated
  THEN no intermediate asset is created, and its result is kept (or not kept)
- **AC-4** Existing intermediates are reused
  WHEN the strategy excludes intermediates and a prefix of the query is already cached
  THEN the plan cuts at that prefix and its commands do not run again; when no prefix is cached,
  the plan runs expanded
- **AC-5** Recipe strategy
  WHEN a recipe sets `cached: result`, `none`, `all`, `default`, `true` or `false`, or omits it
  THEN its result and intermediates follow the table above, with `default` and absent meaning
  `recipe_cache_strategy`
- **AC-6** Strategy follows the origin
  WHEN a recipe with strategy `all` runs while `query_cache_strategy` is `none`, and an ad-hoc query
  then shares its prefix
  THEN every boundary the recipe created is cached, and the ad-hoc query reuses them but creates no
  asset of its own
- **AC-7** Keyed result ignores the command flag
  WHEN a recipe with strategy `all` or `result` ends with a `cached: false` command
  THEN its keyed result is kept
- **AC-8** Debugging switch
  WHEN `assets.cut_predecessors` is `false`
  THEN no plan contains a boundary, no existing intermediate is reused, and results are unchanged
- **AC-9** Positional `v`
  WHEN `a/b/v/c` is evaluated
  THEN the plan is volatile, `a/b` is cut as a cached boundary, and `v/a/b/c` cuts nothing
- **AC-10** Visible in metadata
  WHEN a result is not kept
  THEN its `MetadataRecord` and `AssetInfo` carry `cached: Some(false)`, and its log gives the reason
- **AC-11** Defaults and old files unchanged
  WHEN no strategy, recipe value or command flag is set, and `v` is absent or at the head
  THEN behaviour is as today. Existing recipes (boolean `cached`), configurations and serialized
  plans load unchanged, and existing tests pass
- **AC-12** Markers retired, reference corrected
  WHEN `plan.rs` and `DOC_08_RECIPES_PLANS.md` are read
  THEN the three markers are gone, and the reference describes recursive cutting and the strategies
  instead of "one cut retains one intermediate"

**Non-goals.**
- In-query cache directives (`QUERY-CANNOT-MARK-CACHED-INTERMEDIATES`, deferred).
- A cache size limit or eviction (`CORE-ASSET-GC`).
- A `stored` flag for commands.
- Changing any existing library command's flags.

## Core Interactions

- **Commands and macro:** the `cached: false` keyword, a `CommandMetadata` field, and the registry
  export with a `specs/command_registry.yaml` regeneration.
- **Plan and interpreter:** the boundary walk gains the strategy, the command flag and reuse of
  existing assets (via `AssetManager::lookup_query_asset`). Also positional `v`, and log reasons.
- **Assets:** strategy settings on both managers (in `AssetManagerOptions`). Query-asset
  registration honours the result rule. Boundaries carry their creator's strategy.
- **Recipes and metadata:** the recipe's `cached` widens from a boolean to a strategy. Metadata keeps
  a boolean.
- **Configuration:** three `assets:` keys.

## Crate Placement

- `liquers-core`: plan, interpreter, assets, recipes, metadata, configuration.
- `liquers-macro`: the keyword.
- `liquers-lib`: the regenerated registry only.
- `liquers-records`: one-line change where the provider copies the manifest's boolean `cached` into
  each chunk recipe (`provider.rs`), because the recipe field's Rust type widens. Serialized
  manifests and recipes are unaffected. For manifests with `cached: false`, the chunks'
  intermediates stop being cached, which is the intended effect.

## Documentation Intent

- Reference: extend these documents:
  - `DOC_08_RECIPES_PLANS.md`: boundaries, strategies, positional `v`, and correcting the
    one-intermediate claim;
  - `ENVIRONMENT_CONFIG.md`: three keys;
  - `REGISTER_COMMAND_FSD.md`: the keyword;
  - `ASSETS.md`: when assets are kept;
  - `PROJECT_OVERVIEW.md`: the `v` rule.
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` (when to declare `cached: false`).
- Other documents: close both source issues with resolution notes, update `CLAUDE.md`'s DSL
  metadata list, and update `specs/README.md`.

## Scope Changes

- **2026-10-10, before the Phase 1 gate, in five discussion rounds** (recorded in `DESIGN.md`
  Notes). The first draft's single `predecessor_boundary` policy became the model above.
  - Merged `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`: overlap T3 (the same `mark_volatile` /
    `VolatilitySource` site) and T2 (the shared boundary rule), at the user's request.
  - Found and folded in: query assets ignore `cached` (AC-2), and every prefix is cached
    recursively, contrary to `DOC_08` (AC-12).
  - Deferred: in-query directives. Delegated: the size limit, to `CORE-ASSET-GC`.
  - Size grew from `M` to `L`, so the design was converted to the full form.

## Design Dependencies

- **overlaps** `CORE-ASSET-GC`: how much is kept, versus whether it is kept.
- **overlaps** `QUERY-CANNOT-MARK-CACHED-INTERMEDIATES`: builds on these strategies when it is taken
  up.
- **predecessor** `predecessor-cut-equivalence` (complete): the boundary walk this extends.

## Open Questions

1. **(Implementation detail) How a boundary carries its creator's strategy.** Either through the
   creating `Context` when the dependency is submitted, or as a field on `Step::Evaluate`.
   Recommendation: the context, so that the serialized `Step` shape stays as it is. To be fixed in
   Phase 2.
2. **(Implementation detail) Positional `v` reopens two pitfalls** recorded in `DOC_08`:
   - `a/b` and `a/b/v` have the same step count, so a candidate cannot be identified by index alone;
   - in `a/b/v`, the outermost non-volatile prefix is the whole plan.

   Phase 2 must handle both. The `>=` guard in `cut_predecessor` already pins the second.
3. **(Implementation detail) A reused intermediate expires between finalisation and use.** The
   boundary step would then recreate and cache it once. Recommendation: accept this (a rare,
   bounded effect), or have the step fall back to inline steps. Decide in Phase 2.

## References

- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md`, `specs/issues/V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL.md`
- `specs/design/record-streams/phase2-architecture.md` §"C. `stored` and `cached`"
- `specs/reference/api/DOC_08_RECIPES_PLANS.md` "Predecessor boundaries";
  `specs/reference/ENVIRONMENT_CONFIG.md`
