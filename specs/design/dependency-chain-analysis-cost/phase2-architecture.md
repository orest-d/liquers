# Phase 2: Solution & Architecture - Direct dependency records and linear dependency analysis

## Overview

There are three changes. All the behaviour is in `liquers-core`; outside it, only call sites change
(see "`DefaultRecipeProvider` construction", a scope amendment to Phase 1's "liquers-core only"):

- **Planner.** Plan-time dependency analysis becomes one memoized depth-first walk that produces two
  separate things: the **direct** dependency list, which is recorded, and the **transitive
  summary** (declared volatility, combined expiry), which is applied to the plan.
- **Asset manager.** It gains the dependency-manager walk over *stored* dependency records. The
  `OnLoad` fast-track check and the explicit audits use this walk, so transitive freshness after a
  restart is decided in one place.
- **Recipe provider.** `DefaultRecipeProvider` caches each directory's parsed recipes, checked
  against the stored bytes, so a recipe lookup no longer costs a full YAML parse.

## Corrections to Phase 1 found while specifying

**1. Audits need the walk.** Phase 1 Decision 1 says an explicit audit (`trigger_dependency_audit_all_registered`) catches the
restart case under `Explicit`. With direct records, that is only true if the audit uses the walk
below. `audit_gaps` (`assets.rs:5681`) resolves a gap's **stored version** (`dependency_version`),
and `l1`'s stored version is unchanged: `l1` is stale, not rewritten. So the walk serves the audit
too. That is what Decision 1 intends ("the dependency manager uses all information available"),
and Phase 1 is amended to say so.

**2. The status case under `Explicit`.** `try_fast_track` also refuses a load when a recorded
dependency's stored *status* does not permit reuse (`dependency_blocks_fast_track`,
`assets.rs:1375`), under every policy. Today `l2` records `l0` directly, so a stored `Expired` `l0`
blocks `l2` even when `l1` is still stored `Ready`. That happens when `l0` expired in a process
that never loaded `l1`. With direct records `l2` sees only `l1`, which is `Ready`, so under
`Explicit` it is served. This is the same consequence as Decision 1's version case: no extra
metadata scan under `Explicit`. Under `OnLoad` the walk checks status recursively, and an audit
catches it. Recorded as accepted under Decision 1, for the maintainer to confirm.

**3. The recipe cache checks bytes, not `directory_changed`.** Phase 1 Core Interactions said the
cache is invalidated by `directory_changed`. A hook misses `recipes.yaml` edits made behind Liquers'
back, which today's re-read catches. Comparing the stored bytes keeps today's freshness exactly and
needs no hook.

## Known-Issue Preflight

The issues searched were the `index.csv` rows in `draft`, `accepted` or `in_progress` with area
`core/assets` or `core/plan`, plus every title mentioning dependency, recipe, fast track, audit,
volatility, expiry or cycle.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft | P3 | `create_plan_with_init_metadata` (`recipes.rs:632`) runs the two analysis passes with `let _ =` and CWD `None`. Our merged pass replaces both calls there, so the function shrinks, but its discarded result and its CWD are that issue's subject. | no | no | Keep its behaviour (`let _ =`, CWD `None`) and leave the issue open. Do not fix it here. |
| `COMBINED-EXPIRES` | accepted | P2 | The summary combines expiries with the existing `Expires` `|` / `combine`. We rely on it being associative and idempotent (the memo merges per key, not per path). | no | no | Phase 3 adds one test that a diamond (two paths to one key) gives the same expiry as today. |
| `EXTENDED-FAST-TRACK` | accepted | P2 | Touches `try_fast_track`, which we edit only in its `OnLoad` branch. | no | no | Monitor. |
| `ASSETS-FIX1` | accepted | P2 | General TODO cleanup in the asset lifecycle. No overlap with the edited code. | no | no | None. |
| `DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK` | draft | P2 | A `recipe_opt` override in `DefaultAssetManager`; the walk calls the provider's `recipe_opt` as today. | no | no | None. |

Phase 1's Design Dependencies:
- `dependency-edge-superseded-version` is now `complete`.
- `dependency-audit-and-expiry-provenance` (complete) decided "direct dependencies only;
  transitivity comes from the cascade" (`phase2-architecture.md:144`). This design brings the
  records in line with it and reuses its audit machinery (`audit_gaps`, `audit_version`,
  `ExpiryCause::StaleDependency`, `metadata.rs:423`).

Nothing blocks this design.

## Data Structures

All new types are `pub(crate)` in `liquers-core`. None is serialized.

### `plan.rs`: the analysis result

```rust
/// What a keyed recipe contributes to the plans that read it, transitively.
#[derive(Debug, Clone, Default)]
pub(crate) struct DependencySummary {
    /// The volatile-declaring key at or upstream of this key that sorts first by its
    /// `DependencyKey` string; merging keeps the minimum. Today's message names the first such key
    /// in the dependency list, which is sorted by that string (`plan.rs:2668`), so the named key
    /// is unchanged.
    declared_volatile: Option<Key>,
    /// Combined expiry of this key's recipe and everything upstream of it: today's
    /// `dependency_expires` after the recursive `has_expirable_dependencies_impl`.
    expires: Expires,
}

/// One analysis of one plan. Lives for one call of `analyze_plan_dependencies`, so the memo
/// cannot go stale: recipes do not change in the middle of an analysis.
struct DependencyWalk<E: Environment> {
    envref: EnvRef<E>,
    /// Keys on the current DFS path ("grey"). O(1) cycle check; replaces `Vec::contains`.
    on_path: HashSet<Key>,
    /// Finished keys ("black"), memoized per (resolved key, CWD the recipe plan was resolved
    /// against). The CWD is part of the memo key because a recipe without its own `cwd` resolves
    /// its relative operands against the caller's cursor (`find_dependencies_respects_nested_recipe_cwd`).
    /// A recipe from `DefaultRecipeProvider` always carries its `cwd`, so in practice every key hits.
    done: HashMap<(Key, Option<Key>), Option<DependencySummary>>,
}
```

`done` stores `None` for a key that has no recipe (a plain stored resource), so that is memoized
too.

**Ownership.** The walk owns its maps. The summaries are small (`Option<Key>`, `Expires`) and are
cloned out of the memo. The walk borrows each plan only while it is walking that plan.

### `assets.rs`: the stored-records walk

```rust
/// What the dependency manager concludes about a dependency, from everything it knows plus the
/// stored dependency records reachable from it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoredDependencyState {
    /// The dependency manager already holds a version: authoritative, nothing read.
    Known(Version),
    /// Stored with a durable version, and every recorded dependency, recursively, still holds.
    /// The walk has registered this version and the dependency's edges in the manager.
    Confirmed(Version),
    /// Stored, but `dependency` (this key or one upstream of it) no longer holds what was
    /// recorded. Nothing is registered for it.
    Stale { version: Version, dependency: DependencyKey },
    /// No durable version: absent from the store, not store-resolvable, a stored record cycle, or a
    /// store error that today's `OnLoad` branch already treats as "cannot confirm".
    Unresolvable,
}
```

Every `match` on it is explicit, with no `_ =>`.

### `recipes.rs`: the recipe cache

```rust
pub struct DefaultRecipeProvider {
    /// Per directory: the stored `recipes.yaml` bytes the entry was parsed from, and its recipes by
    /// filename. A hit costs the `get_bytes` it costs today plus one byte comparison, instead of a
    /// YAML parse, a `RecipeList` clone and `set_cwd`.
    cache: scc::HashMap<Key, Arc<CachedRecipes>>,
}

struct CachedRecipes {
    bytes: Vec<u8>,
    list: RecipeList,                 // cwd already set
    by_name: HashMap<String, usize>,  // filename -> index into list.recipes; first occurrence
                                      // wins (`entry().or_insert`), as `RecipeList::get` does
}
```

The check compares the stored **bytes** themselves, with no version and no hash, so it is exactly
as fresh as today: any change, including an edit made behind Liquers' back, is a miss. It needs no
invalidation hook. The stored bytes are the source of truth. This deliberately differs from
`ManifestRecipeProvider`, which checks a store version; `recipes.yaml` written with
`Metadata::new()` carries no version, and that is the common case in the tests and the benchmark.
Memory is one copy of each directory's `recipes.yaml`, held while the provider lives.

## Function Signatures

### `liquers-core/src/plan.rs`

```rust
/// Analyse `plan`'s dependencies once: set `plan.dependencies` to the DIRECT dependencies, apply
/// the transitive summary (volatility, expiry, info messages), warn on a root fallback.
/// Replaces the pair `has_volatile_dependencies` + `has_expirable_dependencies`, which every
/// caller ran back to back (`interpreter.rs:65-66`, `:176-179`, `recipes.rs:632-634`).
pub(crate) async fn analyze_plan_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    initial_cwd: Option<Key>,
) -> Result<(), Error>;

impl<E: Environment> DependencyWalk<E> {
    fn new(envref: EnvRef<E>) -> Self;

    /// Direct dependencies of `plan`, and the merged summary of the keyed recipes it reads.
    /// Keyed `GetAsset*` operands stop the direct list; `Evaluate` and nested `Step::Plan`
    /// pass their direct dependencies through to the enclosing plan (today's relabelling of
    /// `Evaluate` children to `StateArgument` is kept).
    fn walk_plan<'a>(
        &'a mut self,
        plan: &'a Plan,
        cursor: &'a mut CwdCursor,
    ) -> BoxFuture<'a, Result<(Vec<PlanDependency>, DependencySummary), Error>>;

    /// Summary of the keyed recipe at `key`, memoized. `Err` "Circular dependency detected"
    /// when `key` is on the current path. `Ok(None)` when no recipe serves `key`.
    fn summarize_key<'a>(
        &'a mut self,
        key: &'a Key,
        cursor: &'a CwdCursor,
    ) -> BoxFuture<'a, Result<Option<DependencySummary>, Error>>;
}
```

`find_dependencies` stays as a thin `pub(crate)` wrapper,
`find_dependencies(envref, plan, cursor)`, which runs `DependencyWalk::new(envref).walk_plan(plan,
cursor)` and returns the direct list. It loses the `stack` parameter, since the walk owns its path
set. Its six unit-test call sites change (`plan.rs:4156, 4235, 4306, 4342, 4377, 4504`). Two of them
assert a nested recipe's link in the outer list and change expectation:
`find_dependencies_respects_nested_recipe_cwd` (`plan.rs:4356`) and
`expiration_nested_recipe_uses_keyed_recipe_plan` (`plan.rs:4455`). They are rewritten to assert the
link on the *recipe's* own analysis, and the expiry result is unchanged. `has_volatile_dependencies` and `has_expirable_dependencies` are
removed. Their tests call `analyze_plan_dependencies` instead (Phase 3 lists them).

**Errors.** If the walk fails (a cycle, a recipe whose plan cannot be built), the error is written
to the plan with `dependency_check_error` and returned, and **nothing** is applied: no dependency
list, no volatility, no expiry. That matches today, where `has_volatile_dependencies` returns before
`has_expirable_dependencies` runs, and `create_plan_with_init_metadata` skips the expiry pass when
`plan.error` is set. The callers keep their handling: `?` in `interpreter.rs`, `let _ =` in
`recipes.rs`.

**Cycle detection and complexity.** `summarize_key` checks `on_path` before `done`, inserts on
entry and removes on exit. The cycle error message and the `with_key` are today's. Every keyed
recipe reachable from the plan is visited once per analysis, and every edge once, so the walk is
O(V+E). Each visit does one `recipe_opt`, which is O(1) on a cache hit, plus one
`to_plan_for_key`. The three analyses per evaluation stay at their call sites (Decision 4), so an
evaluation of link *i* costs about 3·i lookups instead of 3·(i+1)².

**Semantics preserved exactly**, with the summary rules:
- **Declared volatility.** `declared_volatile(K) = K` if `recipe(K).volatile`, otherwise the first
  `Some` among the summaries of the keys `recipe(K)`'s plan reads. This is today's check of
  `recipe.volatile` over the transitive list, which never looked at a dependency's commands.
  Extending volatility to upstream volatile *commands* would be a behaviour change and is out of
  scope.
- **Combined expiry.** `expires(K) = recipe(K).expires | plan(recipe(K)).expires | (| over the
  summaries read)`. `to_plan` already folds the recipe's own `expires` into the plan, so this
  equals today's recursion.
- **Applying the summary to the plan.** Each merge that changes `plan.expires` emits today's
  "Expiration combined with asset dependency …" info. If at least one merge changed it **and** the
  combined expiry is volatile, the plan is marked volatile with today's message. That is today's
  `changed && plan.expires.is_volatile()` gate (`plan.rs:2886`). A `declared_volatile` key gives "Volatile due to
  dependency on volatile key: …". The "Dependency detected: …" info lines now list direct
  dependencies only.

### `liquers-core/src/assets.rs`: `AssetManager` trait, new default method

```rust
/// The dependency manager's view of `dep_key`, completed from stored dependency records where
/// it has none. Recursive, with an on-path set guarding against a cycle in stored records and a
/// per-call memo. A dependency it confirms is registered (`observe_version` + `load_from_records`),
/// so each key is read from the store at most once per process while it stays confirmed.
/// Never evaluates, never reads a value's bytes: metadata only.
async fn stored_dependency_state(
    &self,
    dep_key: &crate::metadata::DependencyKey,
) -> Result<StoredDependencyState, Error>;
```

It is a default method, beside `audit_gaps` and `dependency_version`, so the external asset
manager inherits it. The recursion is a private boxed helper carrying the memo and the on-path set.
For each record it applies today's per-record rules (`assets.rs:1325–1366`), reusing
`dependency_version` for a store-resolvable key's version and `dependency_blocks_fast_track`'s
status predicate (`status_permits_reuse`) for status:
- **Version.** A record is compared with `Version::matches` against a `Known` version (today's
  manager branch, `assets.rs:1326`), and with equality against a `Confirmed` (stored) version
  (today's `OnLoad` branch, `assets.rs:1343`, which deliberately does not let a current 0 match).
  `try_fast_track` uses the same two rules when it compares its own records with the walk's
  answer.
- **Unknown versions.** A recorded `Version::unknown()` is compatible. A non-store-resolvable key
  (command, recipe) with no known version is compatible.
- **Status.** A stored status that does not permit reuse makes the record not hold
  (`dependency_blocks_fast_track`).

`StoredDependencyState` is `pub` and `#[non_exhaustive]`, because it appears in a public trait
method. Keeping it crate-private would only trade a `private_interfaces` warning for nothing.

### Call-site changes

| Site | Today | After |
|---|---|---|
| `interpreter.rs:65-66` `finalize_plan_expanded` | the two passes | `analyze_plan_dependencies(envref, plan, initial_cwd)` |
| `interpreter.rs:176-179` `make_plan_with_cwd` | the two passes | the same |
| `recipes.rs:632-634` `create_plan_with_init_metadata` | the two passes, `let _ =` | `let _ = analyze_plan_dependencies(envref, &mut plan, None)` (behaviour kept, see preflight) |
| `assets.rs:1334` `try_fast_track`, `OnLoad` branch | `dependency_version` + equality + `observe_version` | `stored_dependency_state(&dep_record.key)`: `Known`/`Confirmed(v)` compared with the record; `Stale` and `Unresolvable` refuse |
| `assets.rs:5681` `audit_gaps` | `found = dependency_version(gap)` | first `stored_dependency_state(gap)`. `Stale { dependency, .. }`: expire the gap's dependents with `ExpiryCause::StaleDependency { dependency }` and add a finding. Otherwise `audit_version` on the version, as today |
| `recipes.rs` `get_recipes` / `recipe_opt` / `recipe` | parse per call | through `cache`; `recipe_opt` indexes `by_name` |

The fast-track minimum rule from Decision 1 needs **no change**: `try_fast_track` already calls
`load_from_records` for a consistent load (`assets.rs:1407`), and `add_dependency` keeps an edge
whose dependency has no version yet.

### `DefaultRecipeProvider` construction

A field ends the unit struct. It is used as a value at 115 sites: 108 in `liquers-core`, 6 in
`liquers-axum` tests, and 1 in `liquers-records` (`provider.rs:205`, where it is the receiver of a
method call). `liquers-lib` mentions it only in a comment. Mirror `ManifestRecipeProvider`
(`liquers-records/src/provider.rs:76-100`): `new()`, `#[derive(Default)]`, and an `scc` map. The
call sites change mechanically from `DefaultRecipeProvider` to `DefaultRecipeProvider::new()`; no
behaviour depends on the form. This is a **scope amendment** to Phase 1's "liquers-core only":
public API churn, not new behaviour. **Decision needed (see
Questions):** this churn, or a crate-global content-addressed cache that keeps the unit struct.

## Trait Implementations

- **`AsyncRecipeProvider<E>` for `DefaultRecipeProvider`.** Same methods and signatures.
  `get_recipes`, `recipe` and `recipe_opt` read through the cache. No new trait methods.
- **`AssetManager<E>`.** One new default method, `stored_dependency_state`. It is additive, so
  every implementor (the in-tree managers and the external manager in the tests) inherits it.
  Existing methods keep their signatures. `audit_gaps` and `try_fast_track` change their bodies
  only.
- **No trait bounds added.** `DependencyWalk<E: Environment>` uses only the bounds `Environment`
  already carries (`MaybeSend + MaybeSync`).

## Sync vs Async

Everything stays `async`, because the walks call `recipe_opt`, `get_metadata` and `get_bytes`. The
recursive pieces return `crate::maybe_send::BoxFuture`, as `find_dependencies` does today, so wasm
(`?Send`) keeps compiling. The cache uses `scc::HashMap`, as `ManifestRecipeProvider` does, and no
lock is held across an `.await`.

## Error Handling

No new error types. The cycle error stays
`Error::general_error("Circular dependency detected: …").with_key(&key)`. `recipe_opt` failures are
swallowed exactly where they are today (`if let Ok(Some(recipe))`). The stored-records walk maps a
store error to `Unresolvable`, as the `OnLoad` branch does today (`Err(e) => refuse`). In
`audit_gaps` the error still propagates (`?`), as today.

## Serialization

Nothing new is serialized. Stored `DependencyRecord` lists keep their format and get shorter. Old
stored transitive records are read as they are (Decision 3).

## Relevant Commands

None. This is planner and asset-manager machinery, with no new or changed commands and no command
namespace involved. `specs/command_registry.yaml` is unchanged.

## Integration Points

- **`liquers-core` only** for behaviour. Other crates change only `DefaultRecipeProvider` → `::new()`
  (if chosen).
- **Behaviour visible through `liquers-axum`.** The admin audit endpoints
  (`liquers-axum/src/assets/key_handlers.rs:484-496`) call `trigger_dependency_audit*`. With the
  walk, their reports can now list a stale gap's dependents as expired, with
  `ExpiryCause::StaleDependency`. That is the intended change, and the axum tests are checked in
  Phase 4.
- `liquers-py`: no use of these items (checked: no `find_dependencies`, `has_volatile_*`, or
  `DefaultRecipeProvider` in `liquers-py/src`). `liquers-validate` prints no dependency lists, so
  its output is unchanged.

## Rejected Alternatives

- **Keep transitive records and make them cheap** (the previous draft). Rejected by Phase 1's
  decision.
- **Recursive validation inside `try_fast_track`.** The recursion belongs to the dependency
  manager (Decision 1).
- **Merging the three analyses per evaluation.** Not simpler (Decision 4). Revisit with the
  optional summary cache.
- **Event-driven (`directory_changed`) recipe cache.** It misses edits made behind Liquers' back,
  which today's re-read catches. Comparing bytes is as fresh as today and needs no hook.

## Documentation Architecture

- **Reference, extend `specs/reference/DEPENDENCIES_STATUS.md`** (audience: core developers and
  agents). A section "What a dependency record holds" covering: direct only; what "direct" means
  (nearest keyed operand, pass-through of `Evaluate` / nested plans, the read key's recipe key);
  analysis summary versus record; the fast-track minimum rule; the stored-records walk and how
  `Explicit` / `OnLoad` / audits use it, with the restart example. History row, `reviewed:`.
- **Reference, review `specs/reference/ASSETS.md`** (the fast-track and audit-policy passages near
  line 1153). Update what `OnLoad` guarantees. History row, `reviewed:` if changed.
- **Reference, review `specs/reference/PROJECT_OVERVIEW.md`** if it describes dependency records.
  Expected: no change.
- **Guide:** none, as decided in Phase 1.
- **Other:** a Phase 5 follow-up issue `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION`
  (the optional summary cache, P3).
- **Updates:** the issue file's resolution, `specs/README.md` (design status), and the index via
  `scripts/docs_index.py`.
- **`affects_docs`:** `DEPENDENCIES_STATUS.md`, `ASSETS.md`.
- **Evidence to collect during implementation:** before and after numbers from the benchmark
  (10/20/40/200 links), and the lookup count per link.

## Acceptance Prediction (Phase 1, Decision 5)

The prototype measured 181 501 lookups and 12.2 s for 200 links, about 67 µs per lookup with
everything else included. It ran three walks per analysis: the outer `find_dependencies`, the
expiry walk, and an inner one-level analysis per key. This design runs **one** walk per analysis,
about (i+1) visits, and keeps three analyses per evaluation:

- **Lookups.** 3·Σ(i+1) ≈ 61 000 for 200 links, about a third of the prototype's count.
- **Per visit.** Each visit also costs a `get_bytes` and a byte comparison of a ~16 KB
  `recipes.yaml`, which the prototype's index skipped. Estimated at a few µs, on top of the
  67 µs.
- **Prediction.** 200 links ≈ 61 000 × ~70 µs + ~1 s floor ≈ **4.5–5.5 s**, against a 5 s bound.
  40 links ≈ 2 500 × 70 µs + ~0.2 s ≈ **0.4 s**, against a 1 s bound.

The 40-link bound holds comfortably. The 200-link bound is marginal and is a **question for the
maintainer** (below): measure in Phase 4 and relax it if it is missed, or share one walk across the
three analyses of an evaluation.

## Risk

| Area | Assessment |
|---|---|
| Semantics | Records shrink by design. Volatility and expiry are preserved by construction (rules above) and pinned by tests. The behaviour change under `Explicit` is accepted (Phase 1, Decision 1). |
| Cycles | Static detection is kept and becomes O(1) per check. The dependency manager's `would_create_cycle` is unchanged. |
| Concurrency | The walk memo is local. The recipe cache is `scc` and correct under races, because a stale fill is re-validated against the bytes on the next read. |
| Memory | One `recipes.yaml` copy per directory, per provider. |
| Certainty | High for the planner and the cache (prototype measured). Medium-high for the stored-records walk (new code, but built from existing per-record rules). |

## Carried to Phase 3

- **Restart probe as two tests.** Under `Explicit`: the stale `l2` is served, then expired once
  `l1` is touched. Under `OnLoad`: `l2` is refused immediately. Also the status variant
  (correction 2) under `OnLoad`, and an explicit audit catching the deep change.
- **The two changed unit tests**, listed above.
- **A diamond-expiry test** (`COMBINED-EXPIRES`).
- **The chain benchmark** (10/20/40/200 links), and a 20-link smoke test with a bound.

## Decisions (maintainer, 2026-10-07)

1. **`DefaultRecipeProvider` gets a field**, with `new()` and `Default`. The construction sites
   change mechanically.
2. **200-link bound:** measure in Phase 4, and relax to 8 s if 5 s is missed. Sharing one walk
   across the three analyses is not added now.
3. **The status case under `Explicit`** (correction 2) is accepted as part of Phase 1 Decision 1.
