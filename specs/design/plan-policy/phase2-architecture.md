# Phase 2: Solution & Architecture — plan-policy

## Overview

The design has five parts:

1. A **command flag** (`CommandMetadata::cached`). The plan builder records it as
   `Plan::uncached_by`. The boundary walk steps back past a candidate whose result is not cached,
   so the uncached command runs inline.
2. A **caching strategy** (`CacheStrategy`). It is held by the asset in its recipe's `cached`
   field and resolved against two manager defaults. It is read only when an asset is constructed
   or registered, never by the plan.
3. **Two registration decisions**, one for a top-level query and one for a dependency, so that a
   boundary follows its creator.
4. A **`cut_predecessors` switch**, read by `finalize_plan`.
5. **Positional `v`.** The `v` instruction becomes `VolatilitySource::Positional`, and the
   empty-tail guard is relaxed for a volatile plan.

Rejected alternatives:

- **Strategy on `Step::Evaluate`.** It changes a serialized step, and the plan would then depend on
  who executes it.
- **Strategy on `Context`.** A context is the command's view of services, not a record of how the
  asset was created (user, 2026-10-10).
- **Checking which intermediates exist at finalisation.** It makes the plan depend on the manager's
  state (user, 2026-10-10).
- **A new `AssetData` field.** `AssetData::recipe` already describes how the asset is created, and
  its `cached` already reaches metadata.
- **Plain `bool` fields** for `CommandMetadata::cached` and `AssetManagerOptions::cut_predecessors`.
  Both types derive `Default`, so a plain bool would default to "not cached" and "never cut".

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK` | draft | P2 | Recursive boundaries nest one asset per prefix. The command flag reduces the depth, and the strategies leave it unchanged | No | No | Note in Risks |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | `result` / `none` on a keyed recipe goes through the existing uncached keyed path with the same race | No | No | Unchanged; documented as a limitation |
| `CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY` | draft | P3 | More plans have boundaries in different places, with the same loss | No | No | None |
| `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` | draft | P3 | `get_query_asset` already builds a plan for volatility; `uncached_by` is read from that same plan | No | No | None |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Unregistered boundaries have exactly one waiter, so they do not widen it | No | No | None |
| `CORE-ASSET-GC`, `QUERY-CANNOT-MARK-CACHED-INTERMEDIATES` | accepted / draft | P3 | Out of scope (size limit; in-query directive) | No | No | Linked |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | closed | — | It removed an old `cache` field. Old metadata with `"cache": false` must still be ignored, not read as `cached` | — | No | Test pins it |

## Interfaces

New module `liquers-core/src/cache_strategy.rs`, pure and synchronous:

```rust
/// How much of an evaluation is kept for reuse. Closed set: a new value is a compile error at
/// every match.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStrategy { None, Result, #[default] All }

impl CacheStrategy {
    pub fn keeps_result(self) -> bool;          // Result | All
    pub fn keeps_intermediates(self) -> bool;   // All
    pub fn is_all(&self) -> bool;               // for skip_serializing_if
}
// Never glob-imported: `CacheStrategy::None` must not shadow `Option::None`.
impl From<bool> for CacheStrategy;              // true -> All, false -> None
impl<'de> Deserialize<'de> for CacheStrategy;   // "none" | "result" | "all" | true | false
impl fmt::Display for CacheStrategy;            // the serde name, for log lines

/// serde `deserialize_with` for `Option<CacheStrategy>` fields: also accepts "default" -> None.
pub fn deserialize_optional_strategy<'de, D: Deserializer<'de>>(d: D)
    -> Result<Option<CacheStrategy>, D::Error>;
```

Changed types (all fields have serde defaults, so old documents load unchanged):

```rust
// command_metadata.rs, CommandMetadata
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cached: Option<bool>,                  // None = true
pub fn cached(&self) -> bool;              // self.cached.unwrap_or(true)

// plan.rs, Plan: the command whose `cached: false` makes this plan's result not worth keeping.
// Each action sets it (Some if its command declares cached: false, else None); a resource step
// or `q` sets it to None; `Filename`, `ns` and `v` leave it. Not contagious.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub uncached_by: Option<CommandKey>,

// recipes.rs, Recipe: widened from Option<bool>; None and "default" = the manager default.
#[serde(default, skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_strategy")]
pub cached: Option<CacheStrategy>,
pub fn effective_cache_strategy(&self, default: CacheStrategy) -> CacheStrategy;
// removed: `Recipe::cached(&self) -> bool` (it cannot know the default); callers listed below

// environment_builder.rs, AssetManagerOptions: three flat keys under `assets:`
#[serde(default, skip_serializing_if = "CacheStrategy::is_all")]
pub recipe_cache_strategy: CacheStrategy,
#[serde(default, skip_serializing_if = "CacheStrategy::is_all")]
pub query_cache_strategy: CacheStrategy,
// Option, not bool: `AssetManagerOptions` derives `Default`, so a bool would default to "never cut".
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cut_predecessors: Option<bool>,
pub fn cut_predecessors(&self) -> bool;                              // unwrap_or(true)
pub fn with_recipe_cache_strategy(self, s: CacheStrategy) -> Self;   // and the other two
```

`AssetManager` trait. These are provided accessors in the style of `dependency_audit_policy()`, so
a custom manager compiles unchanged and gets today's behaviour:

```rust
fn recipe_cache_strategy(&self) -> CacheStrategy { CacheStrategy::All }
fn query_cache_strategy(&self) -> CacheStrategy { CacheStrategy::All }
fn cut_predecessors(&self) -> bool { true }
```

The asset and the context (async, because they read `AssetData` under its lock):

```rust
impl<E: Environment> AssetRef<E> {
    /// How this asset was created: its recipe's `cached`, else the manager default for its kind
    /// (keyed -> recipe_cache_strategy, non-keyed -> query_cache_strategy).
    pub async fn cache_strategy(&self) -> CacheStrategy;
}
impl<E: Environment> Context<E> {
    pub async fn cache_strategy(&self) -> CacheStrategy;   // delegates to its asset
}
```

Macro: a `cached: <bool>` statement, parsed like `volatile:` (`syn::LitBool`), which emits
`cm.cached = Some(<bool>);`. **Commands:** no new commands. Register with
`register_command!(cr, fn select_columns(state, columns: Vec<String>) -> result cached: false)`.

## Integration Points

All in `liquers-core` unless noted.

- **`plan.rs`**
  - `PlanBuilder`: after each action, set `plan.uncached_by` from the command's metadata (and its
    alias's, if either declares `false`), with a `Step::Info`. Resource steps and `q` clear it;
    `Filename` and `ns` leave it.
  - `v`: `mark_volatile(…, VolatilitySource::Positional)`. `VolatilitySource::Declared` keeps only
    the recipe-level sources.
  - `cut_predecessor`: a third reason in the walk, `candidate.uncached_by` ("its result is not
    cached: command 'x' declares cached: false"). The early guard allows
    `predecessor_steps == steps.len()` when `self.is_volatile`, which gives `a/b/v` →
    `[Evaluate(a/b)]`.
  - The three `TODO` markers are replaced by documentation of the fixed builder behaviour.
- **`interpreter.rs` `finalize_plan`:** if `!manager.cut_predecessors()`, skip the cut with a
  `Step::Info`. The input-state rule is unchanged.
- **`assets.rs`, both managers**
  - Top-level `get_query_asset(query)`: build the plan once (it already does, for volatility). The
    asset is registered iff it is non-volatile, `uncached_by.is_none()` and
    `query_cache_strategy().keeps_result()`. Otherwise use a fresh, unregistered, non-volatile
    asset (modelled on `get_uncached_resource_asset`).
  - `get_dependency_asset(parent, query)`: `DefaultAssetManager` overrides it today;
    `ImmediateAssetManager` gains an override. For a non-keyed query, first
    `lookup_query_asset(query)`: a usable existing asset is returned, whatever the strategy.
    Otherwise construct one whose recipe has `cached = Some(parent.cache_strategy().await)`, and
    register it iff non-volatile, `uncached_by.is_none()` and
    `creator.keeps_intermediates()`. A keyed query is unchanged (it follows its own recipe).
  - `get_resource_asset` (keyed): `!cached.unwrap_or(true)` becomes
    `!recipe.effective_cache_strategy(self.recipe_cache_strategy()).keeps_result()`.
    `AssetRef` key-owner check (`!recipe.cached()`, `assets.rs` ≈2585) likewise.
  - Provider-recipe adoption (`metadata.cached = recipe.cached`, ≈3625) and every constructor set
    `metadata.cached = Some(false)` exactly when the asset is not registered, with a log entry
    giving the reason (strategy and its origin, or the command).
  - `with_policies` takes `&AssetManagerOptions` instead of three arguments, and is called from
    `environment_builder.rs` `AssetManagerKind::build` for both kinds.
- **`recipes.rs`:** the field type; `effective_cache_strategy`; `get_asset_info` sets
  `cached: Some(false)` only for an explicit `none`; the tests of `cached()` move to the new
  method.
- **`environment_config.rs`:** nothing new. The keys arrive through `assets: AssetManagerOptions`.
- **`liquers-macro/src/registration.rs`:** `CommandSignatureStatement::Cached(bool)`, parsing, and
  codegen.
- **`liquers-records/src/provider.rs`:** two sites, `Some(manifest.cached.into())`.
- **`specs/command_registry.yaml`:** regenerate only if a registered command declares `cached`
  (none does in this change). The exporter serialises the field when it is `Some`.

## Error Handling

- **An unknown strategy word in a recipe or configuration** (`cached: some`) is a serde error from
  `deserialize_optional_strategy`. It names the accepted values and reaches the caller through the
  existing `EnvironmentConfig::from_yaml` / recipe-parse paths
  (`Error::from_error(ErrorType::General, …)`). A recipe that fails to parse is reported per
  recipe, as today.
- **A non-boolean `cached:` in `register_command!`** is a compile error (`syn::Error` at the
  literal).
- **No new runtime errors.** Registration decisions cannot fail. A lookup that finds an expired or
  failed asset falls through to construction, as `get_dependency_asset` already does for
  stale-terminal states.

## Relevant Commands

No new commands, and no existing command changes its flags. Namespaces involved: none. The keyword
is available to every namespace (`pl`, `lui`, `img`, …) for later adoption.

## Documentation Architecture

Every reference or guide below gets a `## History` row and a `reviewed:` bump in the same commit.

| Path | Kind | Change |
|---|---|---|
| `specs/reference/ENVIRONMENT_CONFIG.md` | reference | Three `assets:` keys in the key table, the public-service example, and defaults |
| `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | guide | Choosing strategies; `with_recipe_cache_strategy` and the other two; the debugging switch |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | reference | `Context::cache_strategy`; the trait accessors |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | Boundary rule (three reasons, the command flag); recursive cutting replaces "one cut retains one intermediate"; positional `v` and `a/b/v`; recipe `cached:` values; `Plan::uncached_by`; plan independent of strategy |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`, `specs/reference/PROJECT_OVERVIEW.md` | reference | `v` is positional; the changed meaning; `v/…` recomputes everything |
| `specs/reference/COMMAND_DECLARATION.md`, `specs/reference/REGISTER_COMMAND_FSD.md` | reference | `cached` field and keyword |
| `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `specs/reference/ASSETS.md` | reference | When an asset is registered (keyed, top-level query, dependency); the creator's strategy on the asset; reuse of an existing boundary; `cached: Some(false)` and the log line |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, `specs/guides/COMMAND_DESIGN_GUIDE.md` | guide | When to declare `cached: false` (cheap to recompute, large output) |
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | A custom manager inherits the defaults; what an override of `get_dependency_asset` must do |
| `CLAUDE.md` | other | `cached:` in the DSL metadata list |

Proposed `affects_docs`: every path in the table except `CLAUDE.md`. `specs/README.md`: no new
link; the design is already listed.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | Core: `cache_strategy.rs` (new), `plan.rs`, `interpreter.rs`, `assets.rs`, `recipes.rs`, `command_metadata.rs`, `environment_builder.rs`, `context.rs`, `lib.rs`. Also `liquers-macro/src/registration.rs` and `liquers-records/src/provider.rs` |
| Crates and workflows affected | Core, macro and records. `liquers-lib` only rebuilds. Run `scripts/check-build-matrix.sh` (wasm: the immediate manager override) |
| Existing tests likely to change | `plan.rs`: `the_v_instruction_is_declared`, `a_declared_source_survives_an_earlier_positional_one`, `declared_volatility_declines_before_the_walk` are re-expressed with recipe-level volatility, and new positional-`v` tests are added. `recipes.rs` tests of `cached()`. Records provider tests (`assert_eq!(recipe.cached, Some(true))`). `plan_cwd_freeze.rs` E16 must still show cut and expanded results equal |
| New validation | Unit tests for `CacheStrategy` serde and decisions; walk tests for `uncached_by`; asset tests on both managers for registration, reuse and origin (AC-2 to AC-7); a config round-trip; a macro trybuild for `cached:` |
| Compatibility / data / concurrency / performance / security | Old recipes with boolean `cached`, configurations, plans and command metadata load unchanged. Legacy `"cache"` is still ignored. Behaviour change: `v` after an action. Concurrency: an unregistered dependency has one consumer; reuse goes through `lookup_query_asset` and then the existing stale-terminal checks. Performance: `uncached_by` costs no extra plan builds. Security: `query_cache_strategy: none` bounds what anonymous queries add to memory |
| Recovery | Every knob defaults to today's behaviour. Removing the keys, or setting both strategies to `all` and `cut_predecessors: true`, restores it without a code change |
| Certainty and open questions | High for the plan and recipe parts. Medium for the two registration paths on `ImmediateAssetManager`, whose `get_dependency_asset` is currently the trait default. No open questions |
