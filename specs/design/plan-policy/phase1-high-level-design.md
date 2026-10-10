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
| `all` (default) | yes | registered and reused, as today |
| `result` | yes | an existing one is reused; a missing one is evaluated unregistered |
| `none` | no | an existing one is reused; a missing one is evaluated unregistered |

**The strategy does not change the plan.** A plan is a function of its query, the command metadata
and the `cut_predecessors` switch, and it is never of the asset manager's state. A boundary
`Evaluate(a/b)` is in the plan whatever the strategy. The strategy acts when that step executes:
an `a/b` the manager already holds is used; a missing one is evaluated, and the strategy decides
whether it is registered for reuse. An unregistered boundary is released after its consumer has
used it. A boundary that expires while in use is the existing problem of any dependency expiring,
not a new one.

**The strategy is a property of the asset:** how the asset was created. It is set when the asset is
constructed, and the asset's `Context` reads it through the asset. A boundary asset is constructed
with the strategy of the asset whose plan contains the boundary step.

- **The strategy follows the origin.**
  - A keyed recipe uses its `cached:` (`default`, `none`, `result`, `all`; `true` = `all`,
    `false` = `none`). Absent or `default` means `assets.recipe_cache_strategy`.
  - An ad-hoc query uses `assets.query_cache_strategy`.
  - A boundary uses the strategy of the asset that created it. The strategy changes no value, so it
    is not part of the boundary's identity.
  - An intermediate that already exists is reused by everyone.
- **Keyed result:** decided by the effective recipe strategy alone.
- **Non-keyed result:** first the last command (`cached: false` means not kept), then the query
  strategy.
- **Intermediate:** registered only under `all`. A command's `cached: false` acts earlier, when the
  plan is built: its output is never a boundary candidate at all, so the command runs inline. The
  command flag can only restrict. A recipe cannot force an intermediate that its command declared
  not worth caching.
- **Volatile values are never kept**, as today. Positional `v` makes `a/b/v/c` volatile from `v`
  onward, so `a/b` can be a boundary. `v` at the head still means the whole query. A recipe's
  `volatile: true` stays whole-plan. A trailing `v` behaves the same way: `a/b/v` is
  `Evaluate(a/b)` with nothing after it. `a/b` is cached, and the `a/b/v` asset is volatile and
  unmanaged.
  - **This changes the meaning of existing queries.** Today a trailing or mid-chain `v` recomputes
    the whole chain. Afterwards only the part after `v` is recomputed, and recomputing everything
    is written `v/a/b`. The two tests that pin the old meaning in `plan.rs` change with it.
- **`assets.cut_predecessors: false`** (debugging) never cuts, so nothing is reused, giving a fully
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
  THEN no intermediate asset is registered, and its result is kept (or not kept)
- **AC-4** Existing intermediates are reused
  WHEN the strategy excludes intermediates and the query is evaluated with and without its
  prefix already cached
  THEN the plan is the same both times. With the prefix cached its commands do not run again;
  without it each command runs once and nothing new is left in `query_assets`
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
  THEN the plan is volatile, `a/b` is cut as a cached boundary, and `c` runs on every request.
  `a/b/v` serves the cached `a/b` as a volatile asset, and `v/a/b/c` cuts nothing
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
- **Plan:** the boundary walk gains the command flag and positional `v`, and the log gives reasons.
  The plan never reads the strategy.
- **Interpreter and assets:** strategy settings on both managers (in `AssetManagerOptions`).
  Executing a boundary step reuses an existing asset, or constructs one with its creator's strategy
  and registers it only under `all`. Query-asset registration follows the result rule.
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

Every document a user or an implementer reads to learn this behaviour is updated in the same PR:

- Reference, configuration and environment:
  - `ENVIRONMENT_CONFIG.md`: the three `assets:` keys, their values and defaults, and the
    public-service example;
  - `api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: the environment and context API, including
    the strategy as a property of the asset and how a context reads it.
- Reference, plan and recipes (`api/DOC_08_RECIPES_PLANS.md`):
  - the boundary rule with the command flag, and recursive cutting (correcting "one cut retains
    one intermediate");
  - positional `v`;
  - the recipe `cached:` values;
  - the plan being independent of the strategy;
  - the `cut_predecessors` switch.
- Reference, query language: `api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` and `PROJECT_OVERVIEW.md`,
  for the `v` rule and its changed meaning.
- Reference, command metadata: `COMMAND_DECLARATION.md` and `REGISTER_COMMAND_FSD.md`, for the
  `cached: false` field and keyword.
- Reference, asset manager: `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` and `ASSETS.md`. When an
  asset is registered or kept, query assets included, the strategy recorded on the asset, and the
  reuse of an existing boundary.
- Guides:
  - `ENVIRONMENT_CONSTRUCTION_GUIDE.md`: choosing strategies, with the public-service setup;
  - `COMMAND_REGISTRATION_GUIDE.md` and `COMMAND_DESIGN_GUIDE.md`: when to declare
    `cached: false`;
  - `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`: what a custom manager must honour.
- Other documents: `CLAUDE.md` (DSL metadata list), `specs/README.md`, and resolution notes on
  both source issues.

Each changed reference or guide gets a `## History` row and a `reviewed:` bump.

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

## Design Readiness

Decision log, kept under the pre-approval given after Phase 2 (2026-10-10). Tiers follow
`autonomous_bulk_design.md` §3.

- **Blocking:** none.
- **Needs decision:** none.
- **Resolved with the user before the Phase 1 gate (2026-10-10):**
  1. A boundary's strategy lives on the asset, set at construction. The context reads it through
     its asset; it is neither a context field nor a `Step` field.
  2. `a/b` and `a/b/v` both cache `a/b`, and `a/b/v` yields a volatile, unmanaged asset holding
     that value. Phase 2 relaxes the `>=` empty-tail guard in `cut_predecessor` for positional
     volatility.
  3. Reuse is decided at execution, not at finalisation. The plan does not depend on the asset
     manager's state. An expiry during use is the existing dependency-expiry behaviour.
- **Proposed resolutions (taken as assumptions in Phases 2–4):**
  4. **Every non-keyed dependency follows its creator's strategy, not only a cut boundary.** This
     covers a link parameter's query and a command's own `context.evaluate`. They are computed on
     behalf of the creating asset, exactly as a boundary is, so the "strategy follows the origin"
     rule applies to them unchanged. A keyed dependency always follows its own recipe.
  5. A recipe's `cached` is serialised as the word (`all`, `result`, `none`). Booleans are still
     accepted when reading.
  6. `Context::cache_strategy()` is public and read-only, so a command can see how its asset is
     being kept.
- **Implementation details (fixed in Phase 4):** `Option` instead of `bool` for the two
  default-`true` flags; `with_policies(&AssetManagerOptions)`; an `ImmediateAssetManager`
  override of `get_dependency_asset`.

## References

- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md`, `specs/issues/V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL.md`
- `specs/design/record-streams/phase2-architecture.md` §"C. `stored` and `cached`"
- `specs/reference/api/DOC_08_RECIPES_PLANS.md` "Predecessor boundaries";
  `specs/reference/ENVIRONMENT_CONFIG.md`
