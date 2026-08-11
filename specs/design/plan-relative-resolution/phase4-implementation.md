# Phase 4: Implementation Plan - plan-relative-resolution

## Overview

**Feature:** Recipe CWD Propagation and Relative Query Resolution.

**Architecture:** `PlanBuilder` and serialized plans remain source-relative. `Recipe::to_plan`
records recipe provenance with one executable `SetCwd` prefix and one init `Info`; a
crate-private `CwdCursor` mirrors ordered resolution for dependency analysis and pre-scheduling,
while `Context` is the sole live runtime CWD.

**Estimated complexity:** High. **Estimated time:** 16-22 experienced Rust-developer hours.

**Prerequisites:** Phases 1-3 are approved; no Cargo dependency, feature, public query API, or
serialized schema change is allowed. No command namespace is in scope.

### Non-negotiable execution contracts

1. `Recipe::to_plan`, `Recipe::to_plan_for_key`, and `PlanBuilder::build` produce a **fresh
   preliminary plan**. Dependency analysis is run once for that plan and one entry CWD. Reusing a
   finalized/deserialized plan under another CWD is not supported by this change; there is no
   refresh/re-finalization API and no promise that derived volatility, expiration, dependencies,
   or init diagnostics can be recomputed in place.
2. Runtime operands are resolved immediately before use. Runtime missing-base installation is
   atomic in the existing `Context::cwd_key` mutex; no guard crosses warning delivery or an
   `.await`.
3. Dependency discovery and pre-scheduling simulate only behavior needed to identify or order
   dependencies. `UseKeyValue`, `UseQueryValue`, and direct-store steps are resolved at runtime;
   they do not manufacture `PlanDependency` records or trigger an earlier warning merely because
   an analysis pass ran.
4. The exact fallback warning is
   `Relative key/query has no CWD; using logical root '/'.` It is emitted at most once per shared
   live `Context`. Standalone analysis uses the same text as one init warning.
5. No optimizer, static absolute rewrite, `PlanBuilder` CWD state, new `Plan` provenance field, or
   runtime-CWD serialization is introduced.

## Implementation Steps

### Step 1: Add the pure ordered CWD cursor

**File:** `liquers-core/src/query.rs`

**Action:**

- Add the exact warning string as `pub(crate) const RELATIVE_WITHOUT_CWD_WARNING: &str` and add the
  non-Serde, crate-private `CwdCursor` beside the query AST.
- `resolve_key` calls `Key::to_absolute` only when the first key element is `.` or `..`. An ordinary
  key is cloned unchanged. A missing base first installs `Key::new()` in the cursor and raises its
  one-shot fallback flag.
- `set_cwd_from` resolves the argument first and then replaces the cursor, so `a/b`, `..`, `./c`
  becomes `a/c` in order.
- `resolve_query_scoped` transforms the parsed AST directly: resolve resource segments in source
  order and recursively visit every `ActionParameter::Link`, preserving `QuerySource`, headers,
  `Position`, and the absolute flag. Never encode and reparse.
- An absolute query uses a temporary root only for its own resource path. Its links retain their
  own absolute/relative status. A child query gets a cursor clone and its explicit CWD does not
  leak; however, if the parent had no CWD and the child requires the root fallback, copy both
  `cwd = Some(Key::new())` and the fallback flag back to the parent. A later sibling therefore
  starts from root and cannot request a second fallback warning.

**Signatures:**

```rust
pub(crate) const RELATIVE_WITHOUT_CWD_WARNING: &str =
    "Relative key/query has no CWD; using logical root '/'.";

#[derive(Clone, Default)]
pub(crate) struct CwdCursor {
    cwd: Option<Key>,
    defaulted_to_root: bool,
}

impl CwdCursor {
    pub(crate) fn new(cwd: Option<Key>) -> Self;
    pub(crate) fn resolve_key(&mut self, key: &Key) -> Key;
    pub(crate) fn resolve_query_scoped(&mut self, query: &Query) -> Query;
    pub(crate) fn set_cwd_from(&mut self, key: &Key) -> Key;
    pub(crate) fn current(&self) -> Option<Key>;
    pub(crate) fn take_root_fallback(&mut self) -> bool;
}
```

**Ordered unit tests in `query.rs`:**

1. `cwd_cursor_resolves_only_leading_dot_and_parent`
2. `cwd_cursor_resolves_ordered_cwd_changes`
3. `cwd_cursor_missing_relative_base_uses_root_once`
4. `cwd_cursor_child_root_fallback_updates_parent_and_sibling`
5. `cwd_cursor_absolute_query_uses_private_root_without_fallback`
6. `cwd_cursor_scopes_child_cwd`
7. `cwd_cursor_preserves_query_source_and_positions`
8. `cwd_cursor_resolves_deep_links_and_long_key_without_reparse`

**Validation:**

```powershell
cargo test -p liquers-core --lib cwd_cursor_
cargo check -p liquers-core
```

Expected: all eight cursor tests pass; `Query::to_absolute` and its callers are unchanged.

**Rollback:** Apply an inverse patch removing only the constant, cursor, and `cwd_cursor_*` tests;
rerun both commands. Do not use checkout or reset.

**Agent Specification:** Model: sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
Phase 1-3, `query.rs`, `parse.rs`, `CLAUDE.md`. Rationale: recursive AST scoping and provenance
require focused semantic judgment.

---

### Step 2: Record recipe CWD without resolving plan operands

**File:** `liquers-core/src/recipes.rs`

**Action:**

- Keep the existing build and value/link override order. After overrides succeed, call
  `self.get_cwd()` once; if it returns `Some(cwd)`, insert exactly one `Step::SetCwd(cwd.clone())`
  at executable step zero and add exactly one `Step::Info` to `init_steps` with
  `Recipe set CWD to '<encoded>'`.
- Do not call `Query::to_absolute`, modify `PlanBuilder`, or rewrite `Plan::query`, query-derived
  keys, or links. `to_plan_for_key` continues to delegate to `to_plan`, so it inherits this prefix
  before enforcing the keyed-payload boundary.
- Preserve the existing parse error for malformed programmatic CWD and
  `DefaultRecipeProvider`'s rejection of YAML-authored `cwd`.
- Round-trip the resulting `Recipe` and `Plan` through JSON and YAML. Assert raw operands,
  `QuerySource`/positions, prefix, and init diagnostic survive; assert no cursor/context runtime
  field appears in either serialized form.

**Implementation shape:**

```rust
if let Some(cwd) = self.get_cwd()? {
    plan.steps.insert(0, Step::SetCwd(cwd.clone()));
    plan.init_info(format!("Recipe set CWD to '{}'", cwd.encode()));
}
```

**Ordered unit tests in `recipes.rs`:**

1. `recipe_to_plan_preserves_programmatic_cwd`
2. `recipe_prefix_info_is_exactly_once_and_precedes_query_steps`
3. `recipe_to_plan_rejects_invalid_programmatic_cwd`
4. `recipe_plan_round_trip_keeps_raw_operands_and_prefix` (including provenance assertions)
5. extend `test_default_recipe_provider` for provider-derived CWD and authored-CWD rejection

**Validation:**

```powershell
cargo test -p liquers-core --lib recipe_to_plan_
cargo test -p liquers-core --lib recipe_prefix_info_
cargo test -p liquers-core --lib recipe_plan_round_trip_
cargo test -p liquers-core --lib test_default_recipe_provider
cargo check -p liquers-core
```

Expected: exactly one raw prefix/info; `./input.txt`, `../c`, and relative links remain raw.

**Rollback:** Inverse-patch the prefix/info block and only these tests. Preserve all existing
provider and recipe tests.

**Agent Specification:** Model: sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
`recipes.rs`, `plan.rs`, approved planning flow, provider tests. Rationale: this is a public recipe
contract with strict ordering and serialization evidence.

---

### Step 3: Make Context atomic and derive owner identity from the bound AssetRef

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/context.rs`

**Action in `assets.rs`:**

- Reuse the existing immutable construction-time `AssetData::query: Arc<Option<Query>>`; do not
  derive identity from mutable `AssetData::recipe`. Add one named accessor so callers cannot
  accidentally repeat the old recipe-key inference:

```rust
impl<E: Environment> AssetRef<E> {
    pub(crate) async fn bound_key_candidate(&self) -> Option<Key> {
        self.query().await.and_then(|query| query.key())
    }
}
```

- Keep `AssetManager::owned_key_asset` non-evaluating. Its existing volatile behavior is part of
  the contract: a volatile entry is removed and produces `None`.

**Action in `context.rs`:**

- Add the five crate-private runtime helpers below. Runtime `resolve_key_from_cwd` and
  `resolve_query_from_cwd` lock `cwd_key` once, build a cursor from the guarded value, resolve, and
  install `Key::new()` directly through that same guard only if fallback occurred while it was
  still unset. Drop the guard, then call `warning`; if warning delivery fails, return that existing
  typed `Error` while retaining root in the Context.
- `set_cwd_from_key` first calls the key resolver. Therefore a failed first warning leaves root
  installed and does not apply the requested later CWD; after successful resolution it replaces
  the CWD with the resolved key.
- Change the existing `get_cwd_key` and `set_cwd_key` lock sites to the same poison-recovery
  pattern while touching this cell, so the new path does not inherit or add a library panic.
- `install_logical_root_if_unset` is only for interpreter pre-pass fallback. It acquires the mutex
  itself and must never be called from inside either runtime resolver's existing guard (no
  recursive lock). All mutex poison handling follows the repository's recovery pattern
  `unwrap_or_else(|e| e.into_inner())`; introduce no `unwrap`/`expect` in library code.
- Normalize the query at the top of `schedule_dependency_asset`, before payload requirement,
  `DependencyKey`, graph registration, version lookup, and manager access. `evaluate` and
  `get_dependency_state` inherit that behavior. Normalize at the top of `apply` before payload
  inspection or either manager call.

**Owner-key algorithm (exact order):**

1. Obtain `candidate` from `self.assetref.bound_key_candidate().await`; `None` means temporary or
   non-key-bound and returns `Ok(None)`.
2. Snapshot the current recipe under its read lock, then drop the lock. Its identity is
   `recipe.store_to_key()?` when present, otherwise `recipe.key()?`. If that identity does not
   equal `candidate`, return `Ok(None)`; this rejects a provider recipe whose declared output does
   not match the immutable asset binding.
3. Call `envref.get_asset_manager().owned_key_asset(&candidate).await`. Do not call `get`,
   `get_asset`, `recipe`, or any other evaluating lookup.
4. Return `Some(candidate)` only when the returned owner's `id()` equals
   `self.assetref.id()`. Return `None` for no owner, a different owner, volatile eviction,
   temporary/ad-hoc assets, or provider mismatch.
5. Use `owner_key()` in `schedule_dependency_asset` for keyed-dependent classification and
   `add_dependent_asset`. If it is `None`, retain the existing construction-time-query expression
   classification where applicable, but never register that asset as a keyed owner.

**Signatures:**

```rust
impl<E: Environment> Context<E> {
    pub(crate) fn resolve_key_from_cwd(&self, key: &Key) -> Result<Key, Error>;
    pub(crate) fn resolve_query_from_cwd(&self, query: &Query) -> Result<Query, Error>;
    pub(crate) fn set_cwd_from_key(&self, key: &Key) -> Result<(), Error>;
    pub(crate) fn install_logical_root_if_unset(&self) -> bool;
    pub(crate) async fn owner_key(&self) -> Result<Option<Key>, Error>;
}
```

**Ordered unit tests:**

1. `assets::tests::bound_key_candidate_uses_immutable_original_query`
2. `context::tests::owner_key_matches_non_evaluating_registered_owner`
3. `context::tests::owner_key_rejects_temporary_ad_hoc_volatile_and_provider_mismatch`
4. `context::tests::resolver_installs_root_once_across_context_clones`
5. `context::tests::root_fallback_warning_delivery_error_propagates` (also asserts root retention)
6. `context::tests::absolute_operands_ignore_missing_cwd`
7. `context::tests::absolute_query_does_not_absolutize_relative_link`
8. `context::tests::context_entry_points_resolve_relative_queries`

The owner tests call `owned_key_asset` and compare `AssetRef::id()` without evaluating the asset.

**Validation:**

```powershell
cargo test -p liquers-core --lib bound_key_candidate_
cargo test -p liquers-core --lib owner_key_
cargo test -p liquers-core --lib resolver_installs_root_once
cargo test -p liquers-core --lib root_fallback_warning_delivery_error_
cargo test -p liquers-core --lib absolute_operands_
cargo test -p liquers-core --lib absolute_query_
cargo check -p liquers-core
```

Expected: no evaluating owner lookup, one atomic root installation, warning after unlock, and no
new panic path.

**Rollback:** Inverse-patch the accessor, Context helpers, normalized local variables, and these
tests. Restore the prior expressions locally; do not replace either whole source file.

**Agent Specification:** Model: opus. Skills: rust-best-practices, liquers-unittest. Knowledge:
`assets.rs`, `context.rs`, `dependencies.rs`, keyed-recipe-ownership design, Phases 1-3. Rationale:
identity ownership, lock ordering, and payload/dependency boundaries are the highest-risk seam.

---

### Step 4: Analyze dependencies once on each fresh preliminary plan

**Files:** `liquers-core/src/plan.rs`, `liquers-core/src/interpreter.rs`,
`liquers-core/src/recipes.rs`

**Action in `plan.rs`:**

- Change `find_dependencies` to accept `&mut CwdCursor` and use
  `crate::maybe_send::BoxFuture` for recursive async traversal.
- `has_volatile_dependencies(envref, plan, initial_cwd)` creates one cursor, runs discovery once,
  assigns `plan.dependencies`, emits the sorted dependency init diagnostics and at most one exact
  standalone fallback warning, and then derives volatility. It performs discovery even if the
  preliminary builder already marked the plan volatile so expiration can reuse the same set.
- `has_expirable_dependencies(envref, plan)` never calls `find_dependencies` for the same plan and
  reuses `plan.dependencies`. It collects each key-backed asset/recipe dependency key at most once,
  so paired `StateArgument` and `Recipe` records cannot rebuild the same child plan twice. Do not add a cross-module private
  “refresh” helper, CWD provenance field, or init-diagnostic deduplication scheme.
- When expiration follows a dependency recipe, build a fresh child with
  `recipe.to_plan_for_key(cmr, &key)`, call the volatility pass once with `None` (the recipe prefix
  carries provider CWD), then recurse into expiration using that populated child. Never use
  `recipe.get_query()` plus bare `PlanBuilder` on this path.
- Keep dependency ordering deterministic using the existing key/relation sort.

**Dependency policy (exhaustive):**

| Step | Discovery behavior |
|---|---|
| `GetAsset`, `GetAssetBinary`, `GetAssetMetadata` | Resolve key, add `StateArgument`, perform cycle check, add `Recipe` and recurse through `to_plan_for_key` when a recipe exists |
| `GetAssetDirectory` | Resolve key, add directory `StateArgument`, no recipe recursion |
| `GetAssetRecipe` | Resolve key and add only the `Recipe` dependency |
| `Evaluate` | Resolve a scoped query, fork the cursor, build its fresh child plan, recurse, and promote key-convertible child dependencies to `StateArgument` |
| `Action` | Add command metadata/implementation dependencies; recursively visit `DefaultLink`, `ParameterLink`, `OverrideLink`, `EnumLink`, and `MultipleParameters`, resolve each link with a fork, and retain its exact link relation/name |
| `Plan` | Recurse with the same mutable cursor so its final CWD affects following outer steps |
| `SetCwd` | Advance the cursor only; create no dependency |
| `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`, `UseKeyValue`, `UseQueryValue` | Create no dependency and do not resolve merely for discovery; runtime is authoritative, and a later dependency that itself needs root establishes the same base |
| `Filename`, `Info`, `Warning`, `Error` | No dependency and no cursor effect |

Every `Step` and `ParameterValue` match is exhaustive; no wildcard arm is added.

Phase 2 listed `UseQueryValue` among operands inspected by dependency traversal. The final review
narrows that timing without changing ordered meaning: `UseQueryValue` and `UseKeyValue` create no
dependency and cannot set an explicit later CWD, so resolving them early can only install/warn
about root before execution. If a later dependency needs that root it installs the identical base
itself; if none does, runtime emits the warning when the value step actually executes. Keeping
these two variants runtime-only therefore preserves subsequent dependency identities and better
honors the approved interpreter-authority rule.

**Signatures and fresh-plan call graph:**

```rust
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cursor: &'a mut CwdCursor,
) -> crate::maybe_send::BoxFuture<'a, Result<Vec<PlanDependency>, Error>>;

pub(crate) async fn has_volatile_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    initial_cwd: Option<Key>,
) -> Result<bool, Error>;

pub(crate) async fn has_expirable_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<(), Error>;
```

- In `interpreter.rs`, keep public `finalize_plan`'s signature. Document its precondition as a
  fresh unfinalized plan; snapshot `context.get_cwd_key()` exactly once, pass that entry value to
  volatility, let expiration reuse the populated dependencies, and call `context.owner_key().await?` for edge
  registration. Never use `plan.query.key()` as owner identity.
- Add private `make_plan_with_cwd`; public `make_plan` delegates with `None`. It builds once, calls
  volatility with the entry snapshot and then expiration over the populated dependencies, and returns that finalized fresh
  plan. Legacy `evaluate(..., cwd_key)` calls this helper before creating/executing its Context.
- In `recipes.rs::create_plan_with_init_metadata`, migrate volatility to its three-argument
  signature with `None` and expiration to its two-argument signature; it already constructs a fresh plan. This closes every current callsite
  in `interpreter.rs` and `recipes.rs`, so Step 4 is compile-green before runtime Step 5.

```rust
async fn make_plan_with_cwd<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
    initial_cwd: Option<Key>,
) -> Result<Plan, Error>;
```

Do not call finalization twice on one `Plan`; callers needing another entry CWD rebuild from the
source `Query` or `Recipe`. No test promises cross-CWD re-finalization of the same plan.

**Ordered unit tests in `plan.rs` and `interpreter.rs`:**

1. `find_dependencies_resolves_all_link_variants_with_ordered_cwd`
2. `find_dependencies_child_query_cwd_does_not_leak`
3. `find_dependencies_nested_plan_propagates_cwd`
4. `find_dependencies_respects_nested_recipe_cwd`
5. `find_dependencies_non_dependency_value_steps_do_not_warn_early`
6. `volatility_populates_dependencies_once_and_expiration_reuses_them`
7. `expiration_nested_recipe_uses_keyed_recipe_plan`
8. `make_plan_with_cwd_uses_one_entry_snapshot`
9. `finalize_relative_plan_uses_context_owner_key`

**Validation:**

```powershell
cargo test -p liquers-core --lib find_dependencies_
cargo test -p liquers-core --lib volatility_populates_dependencies_once_
cargo test -p liquers-core --lib expiration_nested_recipe_
cargo test -p liquers-core --lib make_plan_with_cwd_
cargo test -p liquers-core --lib finalize_relative_plan_
cargo check -p liquers-core
```

Expected: all three source files compile together; each preliminary plan is traversed once; keyed
expiration preserves recipe CWD/overrides/payload rejection; owner registration uses the bound id.

**Rollback:** Inverse-patch the three signatures and all enumerated callsites as one unit, plus
only the new tests. Do not leave a mixed two-/three-argument call graph.

**Agent Specification:** Model: opus. Skills: rust-best-practices, liquers-unittest. Knowledge:
`plan.rs`, `interpreter.rs`, `recipes.rs`, `dependencies.rs`, approved finalization architecture.
Rationale: boxed recursion, derived-plan state, and current callsite migration require holistic
reasoning.

---

### Step 5: Resolve operands and nested parameters in the interpreter

**Files:** `liquers-core/src/interpreter.rs`, `liquers-core/src/context.rs`

**Pre-scheduling action:**

- Replace `schedule_plan_dependencies` with boxed `schedule_plan_dependencies_from`, seeded by a
  wrapper from `context.get_cwd_key()`. It simulates `SetCwd` and recursively shares the cursor
  through `Step::Plan` without copying the simulated final CWD to Context.
- Pre-schedule only dependency-producing runtime work: resolved keyed
  `GetAsset`/`GetAssetBinary`/`GetAssetMetadata`, keyed `Evaluate`, and keyed action links including
  links inside `MultipleParameters`. Deduplicate by resolved `Key`. `GetAssetRecipe`, directories,
  direct-store steps, `UseQueryValue`, and `UseKeyValue` remain on demand.
- Fork cursors for evaluated queries and links. If dependency simulation first needs root, call
  `context.install_logical_root_if_unset()` once and, only if it installed root, deliver the exact
  warning after the helper returns. A warning error aborts pre-scheduling with root retained.

```rust
fn schedule_plan_dependencies_from<'a, E: Environment>(
    plan: &'a Plan,
    context: &'a Context<E>,
    cursor: &'a mut CwdCursor,
    seen: &'a mut HashSet<Key>,
) -> crate::maybe_send::BoxFuture<'a, Result<(), Error>>;
```

**Execution action:**

- In `do_step`, resolve all `GetResource*`, `GetAsset*`, `GetAssetRecipe`,
  `GetAssetDirectory`, `Evaluate`, `UseQueryValue`, and `UseKeyValue` operands immediately before
  use. `SetCwd` calls `context.set_cwd_from_key(&key)`. The stored `Plan` remains unchanged.
- `Step::Plan` continues to call `apply_plan` with the same `Context`; its CWD changes persist.
- Action-link execution resolves against live CWD. Keep the existing top-level link fast path:
  evaluate to `Arc<E::Value>` and call `CommandArguments::set_value`, which is lossless for custom
  values and must not pass through JSON.
- Add a boxed recursive helper only for elements contained by `MultipleParameters`:

```rust
fn materialize_nested_parameter<'a, E: Environment>(
    context: &'a Context<E>,
    parameter: &'a ParameterValue,
) -> crate::maybe_send::BoxFuture<'a, Result<ParameterValue, Error>>;
```

The helper explicitly maps all variants:

| Input | Output |
|---|---|
| `DefaultLink(name, query)` | `DefaultValue(name, json)` |
| `ParameterLink(name, query, position)` | `ParameterValue(name, json, position)` |
| `OverrideLink(name, query)` | `OverrideValue(name, json)` |
| `EnumLink(name, query, position)` | `ParameterValue(name, json, position)` |
| `MultipleParameters(values)` | recursively materialized `MultipleParameters`, in original order |
| `DefaultValue`, `ParameterValue`, `OverrideValue`, `Placeholder`, `Injected`, `None` | unchanged clone |

Each link is resolved through `Context`, evaluated once, then converted with
`E::Value::try_into_json_value`; attach the stored `Position` for parameter/enum links and the
linked query context for default/override links to any scheduling or conversion error. Do not use
a wildcard arm.

This JSON conversion is an existing representation limitation for values inside variadic
`ParameterValue`: a nested link yielding `Value::Key`, `Value::Query`, bytes, or another
non-JSON-capable custom value returns the existing `ConversionError`. It is not silently stringified.
Top-level links remain lossless. Existing `FromParameterValue<Vec<V>>` also rejects a nested
`MultipleParameters` aggregate; this project tests recursive materialization but does not redesign
that extractor or flatten the user's structure.

Before `CommandArguments::new`, clone the action's `ResolvedParameterValues` and replace each
top-level `MultipleParameters` entry with the helper's returned tree. Construct `CommandArguments`
from that rebuilt parameter list, then run the existing top-level-link loop and populate its
lossless `values` side channel with `set_value`. This wiring is required because variadic extraction
reads `CommandArguments.parameters`; merely calling `set_value` cannot expose links nested inside a
`MultipleParameters` value.

**Ordered interpreter unit tests:**

1. `resolves_ordered_cwd_changes`
2. `chained_cwd_updates_resolve_key_value` using entry `a/b` and exactly
   `-R-cwd/../-R-cwd/./c/-R-key/.`, returning the `Key` value `a/c` without actions
3. `dependency_preschedule_tracks_cwd_without_mutating_context`
4. `evaluate_cwd_applies_before_dependency_analysis`
5. `manual_plan_resolves_relative_steps_from_context`
6. `nested_plan_inherits_and_updates_cwd`
7. `action_observes_current_cwd`
8. `multiple_parameter_links_materialize_in_order`
9. `multiple_parameter_non_json_link_reports_positioned_conversion_error`
10. `missing_resolved_resource_preserves_key_not_found_for_absolute_key`
11. `resolver_scopes_nested_links`, using exactly
    `pass-~X~-R-cwd/./child/-/cwd~E/append_cwd` from entry CWD `a/b` and asserting
    `a/b/child|a/b`

The last test starts from `a/c`, requests `./missing.txt`, and asserts `ErrorType::KeyNotFound` plus
the resolved key `a/c/missing.txt`; resolution must not replace it with a generic error.

**Validation:**

```powershell
cargo test -p liquers-core --lib chained_cwd_updates_resolve_key_value
cargo test -p liquers-core --lib dependency_preschedule_tracks_cwd_
cargo test -p liquers-core --lib nested_plan_inherits_
cargo test -p liquers-core --lib multiple_parameter_
cargo test -p liquers-core --lib missing_resolved_resource_
cargo test -p liquers-core --lib resolver_scopes_nested_links
cargo check -p liquers-core
```

Expected: the action-free chain returns `a/c`; nested plans share CWD; pre-scheduling does not
advance live Context to simulated final CWD; variadic conversion failures retain type/position.

**Rollback:** Inverse-patch scheduler, `do_step`, materialization helper, and these tests together.
Do not revert Context or plan semantics independently after this step lands.

**Agent Specification:** Model: opus. Skills: rust-best-practices, liquers-unittest. Knowledge:
`interpreter.rs`, `context.rs`, `commands.rs`, `value.rs`, Phases 2-3. Rationale: live ordering,
pre-pass isolation, wasm-compatible boxed recursion, and value preservation must agree.

---

### Step 6: Complete inline regression, warning, concurrency, and serialization coverage

**Files:** `liquers-core/src/query.rs`, `liquers-core/src/recipes.rs`,
`liquers-core/src/assets.rs`, `liquers-core/src/plan.rs`, `liquers-core/src/context.rs`,
`liquers-core/src/interpreter.rs`

**Action:** Complete the preceding ordered tests and add these cross-cutting cases where they are
closest to private state:

1. `relative_operand_without_cwd_warns_and_uses_root`
2. `deterministic_context_clones_share_root_and_one_warning`
3. native-only `concurrent_root_fallback_warns_once`
4. `absolute_outer_query_keeps_relative_link_independent`
5. `serialization_keeps_raw_cwd_links_positions_and_no_runtime_schema`
6. `deep_multiple_parameters_and_long_key_preserve_order_and_provenance`

For both warning-count tests, execute through `ImmediateEnvironment`, await the owning
`AssetRef::get()` to terminal completion, then read the asset's finalized metadata after the
evaluation path has drained pending dependencies/service messages. Count log entries whose level
is warning **and whose message exactly equals** `RELATIVE_WITHOUT_CWD_WARNING`; assert exactly one,
not merely “contains a warning”. Do not drain `Context::take_pending_dependencies` or an analogous
metadata queue mid-evaluation.

The cross-target deterministic test resolves on two sequential Context clones and asserts their
shared root and final metadata. The native-only race uses
`#[cfg(not(target_arch = "wasm32"))]`, `tokio::sync::Barrier`, and
`tokio::time::timeout(Duration::from_secs(2), ...)` around two first-resolution tasks; it asserts
completion, one warning, and no deadlock. The deterministic test remains the wasm-relevant proof.

Serialization tests inspect JSON/YAML values as well as round-tripping: raw `SetCwd(../c)`, raw
links, source/positions, and init `Info` remain; keys such as `cwd_cursor`,
`defaulted_to_root`, or `context_cwd` are absent. The long-key/deep-multiple case uses no timing
threshold; it proves complete traversal and avoids a brittle performance benchmark.

**Validation:**

```powershell
cargo test -p liquers-core --lib relative_operand_without_cwd_
cargo test -p liquers-core --lib deterministic_context_clones_
cargo test -p liquers-core --lib concurrent_root_fallback_
cargo test -p liquers-core --lib serialization_keeps_raw_
cargo test -p liquers-core --lib deep_multiple_parameters_
cargo test -p liquers-core --lib
```

Expected: exact warning count is one only after terminal metadata observation; all library tests
pass on native; wasm compilation does not include the native barrier test.

**Rollback:** Remove only newly added tests by inverse patch. If a test exposes an implementation
defect, keep the test and return to its owning step; do not weaken the assertion.

**Agent Specification:** Model: sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
all six changed modules, metadata/asset completion flow, Phase 3 matrix. Rationale: broad fixture
coverage follows established patterns once core semantics are fixed.

---

### Step 7: Add the end-to-end recipe CWD suite

**File:** `liquers-core/tests/recipe_cwd_resolution.rs` (new)

**Action:** Use `ImmediateEnvironment<Value>`, `AsyncMemoryStore`, `DefaultRecipeProvider`, and
test-local commands. Declare `type CommandEnvironment = ImmediateEnvironment<Value>;` before all
`register_command!` calls. Plain `-R` is asset access (`GetAsset`); use `-R-stored` only for the
fixture intended to exercise direct `GetResource`.

**Ordered integration tests:**

1. `programmatic_and_provider_cwd_select_their_own_inputs`
2. `explicit_cwd_overrides_recipe_cwd_for_later_relative_link`
3. `resolved_dependency_identity_reuses_cached_asset` (and rejects sibling reuse)
4. `context_boundary_commands_use_active_cwd`
5. `recursive_links_and_multiple_parameters_use_active_cwd`
6. `nested_keyed_recipe_cwd_is_not_bypassed` (including override preservation)
7. `root_fallback_is_single_warning_under_shared_context` (exact text after completion)
8. `absolute_outer_query_keeps_relative_link_independent`

For test 4, register three commands whose returned strings are exactly
`via_evaluate:a/c/hello`, `via_state:a/c/hello`, and `via_apply:a/c/hello`. For `via_evaluate` and
`via_state`, run equivalent resolved keyed requests twice, assert matching resolved dependency
keys and matching reusable `AssetRef::id()` values, then distinguish sibling `a/b/hello.txt`.
For `via_apply`, assert its method tag/value and that it creates no dependency record for the
ad-hoc result; explicitly exclude it from cache-id reuse evidence because `Context::apply` does not
insert the result into the key/query cache.

Seed distinct values at every expected and wrong-sibling key. For every value reached through plain
`-R`, use a `MetadataRecord` with the matching key, `Status::Source`, and type identifier `text` so
current `ImmediateAssetManager::get` exercises its merged fast-track path; keep `Metadata::new()`
only for direct `-R-stored` fixtures such as `recipes.yaml`. The missing-resource assertion uses the
resolved key and `ErrorType::KeyNotFound`. Observe warning metadata only after `asset.get()`
completes. No `FileStore`, production command, manifest entry, or cross-crate fixture is added.

**Validation:**

```powershell
cargo test -p liquers-core --test recipe_cwd_resolution -- --test-threads=1
cargo test -p liquers-core --lib
```

Expected: all eight integration tests pass; method tags prevent boundary wiring from being
interchangeable; only asset-backed boundaries provide cache reuse evidence.

**Rollback:** Delete only the new file with an inverse `apply_patch` and rerun the library suite.
There is no manifest change to undo.

**Agent Specification:** Model: sonnet. Skills: rust-best-practices, liquers-unittest. Knowledge:
Phase 3 prototype, `liquers-core/tests/async_hellow_world.rs`, asset-manager tests, command guide.
Rationale: this fixture spans provider, store, commands, Context boundaries, cache, and metadata.

---

### Step 8: Run green gates, update current-state documentation, and capture Phase 5 evidence

**Files:** `specs/reference/api/DOC_08_RECIPES_PLANS.md`,
`specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`,
`specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, `specs/reference/ASSET_LIFECYCLE.md`,
`specs/reference/PROJECT_OVERVIEW.md`, `specs/README.md`,
`specs/issues/CORE-PLAN-RELATIVE-RESOLUTION-MISSING.md`, and
`specs/design/plan-relative-resolution/phase5-documentation.md` after implementation and review.

**Action:** Run the final gates below and record exact results. Update only claims proven by the
implementation/tests. Every substantive current-state document gets its required `reviewed:` and
`## History` update/link. `CLAUDE.md` needs no change: no new project-wide coding convention or
public extension pattern is introduced.

**Exact documentation changes:**

- `DOC_08_RECIPES_PLANS.md`: provider/programmatic CWD provenance; raw one-step prefix and
  non-executable init `Info`; fresh preliminary-plan finalization contract; source-relative
  serialization.
- `DOC_02_QUERY_LANGUAGE_REFERENCE.md`: ordered `cwd`, independent nested-query scope, absolute
  outer/query distinction, root fallback, and the `a/b -> a/c` examples.
- `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: shared live Context CWD, runtime resolution
  boundaries, non-mutating pre-pass simulation, owner identity, and one exact warning.
- `LANGUAGE-INTEGRATION_GUIDE.md`: provider derives CWD from the containing `recipes.yaml`;
  programmatic callers set `Recipe::cwd`; YAML authors do not; link the complete integration test.
- `ASSET_LIFECYCLE.md`: resolved dependency/cache/cycle identity and verified owner registration.
- `PROJECT_OVERVIEW.md`: update **Core Concepts / Query Language** so plain `-R` means managed
  `GetAsset` while `-R-stored` means direct `GetResource`; update **Core Concepts / Recipes** with
  provider/programmatic CWD provenance and raw prefix/init info; update **Core Concepts / Execution
  Flow** and **Context Hierarchy** with ordered resolution and shared live Context CWD.
- `specs/README.md`: retain/update the capability entry to the authoritative references/guide.
- The issue: close/update only after all green evidence exists.

**Six Phase 3 corrections/learnings to record explicitly:**

1. `Recipe::cwd` was already public/Serde while `Recipe::to_plan` ignored it; the default provider,
   not Serde, rejects authored YAML CWD.
2. Builder-side absolute rewriting plus executable `SetCwd` is ambiguous, so the interpreter owns
   semantics and plans remain raw.
3. A nested resource link is `-R/./hello.txt`, not `-R./hello.txt`.
4. Plain `-R` builds managed asset access; `-R-stored` is required for direct store access.
5. An absolute outer query does not make a relative nested link absolute.
6. Existing reference/guide documents are sufficient; no new current-state document is needed.

**Final native and wasm gates (in order):**

```powershell
cargo fmt --all -- --check
cargo check -p liquers-core
cargo test -p liquers-core --lib
cargo test -p liquers-core --test manager_parametric
cargo test -p liquers-core --test recipe_cwd_resolution -- --test-threads=1
$installedTargets = rustup target list --installed
if ($installedTargets -notcontains 'wasm32-unknown-unknown') { throw 'wasm32-unknown-unknown is not installed' }
cargo check --target wasm32-unknown-unknown -p liquers-core --no-default-features --features async_store
```

The installed-target check must succeed before claiming wasm validation. If it fails, record the
missing target and request authorization before `rustup target add wasm32-unknown-unknown`; do not
silently mark the gate skipped.

**Manual parser/plan matrix:**

```powershell
cargo run -q -p liquers-core --features cli --bin liquers-validate -- --no-registry --command action --command pass --command cwd --command append_cwd --command use_link --command identity --detail summary -- '-R-stored/./input.txt/-/identity/result.txt' '-R-cwd/../c/-/action-~X~-R/./hello.txt~E' '-R-cwd/../-R-cwd/./c/-R-key/.' 'pass-~X~-R-cwd/./child/-/cwd~E/append_cwd' '/-R/./hello.txt' '/-R/./data/-/use_link-~X~-R/./hello.txt~E' '/-R/./data/-/use_link-~X~/-R/./hello.txt~E'
```

Expected: seven `Ok`, zero parser/plan warnings, zero errors. This validates syntax only; the test
suites validate execution.

**Documentation validation:**

```powershell
uv run --no-project python .claude/skills/liquers-project/scripts/validate_phase.py plan-relative-resolution 4
uv run --no-project python scripts/docs_index.py
git diff --check
```

Expected: Phase 4 and documentation index validate, with no whitespace errors.

**Rollback:** Inverse-patch each documentation assertion/history row separately. If a code gate
fails, return to the owning numbered step; do not mass-revert code or documentation and never use
checkout/reset on the shared worktree.

**Agent Specification:** Model: sonnet. Skills: rust-best-practices. Knowledge: final code/test
evidence, Phases 1-4, documentation structure guide, all affected documents. Rationale: current-state
documentation must distinguish verified behavior from design intent.

## Step-by-Step Green Gates

| Gate | Must be green before | Evidence |
|---|---|---|
| Step 1 focused tests + core check | Step 2 | Pure cursor API and sibling fallback |
| Steps 1-2 focused tests + core check | Step 3 | Raw recipe plan contract |
| owner/root focused tests + core check | Step 4 | Atomic runtime and immutable owner boundary |
| dependency/finalization focused tests + core check | Step 5 | Fresh-plan call graph compiles across all callsites |
| chained/prepass/materialization focused tests + core check | Step 6 | Interpreter ordering is complete |
| full `--lib` | Step 7 | Inline regressions all green |
| integration target + `--lib` | Step 8 docs | End-to-end behavior is evidence-backed |
| `manager_parametric` + format/native/wasm/manual/docs gates | Phase 5 entry | Existing immediate/default fast-track behavior and complete implementation validation |

Steps 1-5 are serial. Step 6 follows Step 5. Step 7 may be authored alongside Step 6 only after
Step 5 is green, but both must pass together before Step 8.

## Testing Plan

### Unit tests

Tests live inline in `query.rs`, `recipes.rs`, `assets.rs`, `plan.rs`, `context.rs`, and
`interpreter.rs`. Sync pure tests use `#[test]`; async runtime tests use `#[tokio::test]`; tests
using `?` return `Result<(), Box<dyn std::error::Error>>`. The ordered names and focused commands
are specified in Steps 1-6, followed by `cargo test -p liquers-core --lib`.

### Integration tests

`liquers-core/tests/recipe_cwd_resolution.rs` contains the eight ordered end-to-end tests from
Step 7. It uses test-local command registration and memory storage only. Run it serially for
deterministic metadata/cache assertions, then rerun the library suite.

### Objective success criteria

- Entry CWD `a/b` plus `-R-cwd/../-R-cwd/./c/-R-key/.` returns `Value::Key(a/c)`.
- Recipe plans retain raw operands and contain exactly one prefix and one init diagnostic.
- Planning, scheduling, runtime, dependency, cycle, cache, and store identities agree.
- A missing relative base installs logical root and records the exact warning once after terminal
  metadata drain; ordinary keys and absolute resource paths are silent.
- Missing `a/c/missing.txt` remains a typed `KeyNotFound` for that resolved key.
- Top-level links preserve non-JSON values; a non-JSON link inside a variadic value returns a
  positioned `ConversionError`.
- Native library, `manager_parametric`, and new integration tests, installed wasm target
  check/build, seven-query matrix, format, and docs validation are all green.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | sonnet | rust-best-practices, liquers-unittest | Recursive AST semantics and provenance |
| 2 | sonnet | rust-best-practices, liquers-unittest | Public recipe-plan contract |
| 3 | opus | rust-best-practices, liquers-unittest | Atomic Context and immutable owner identity |
| 4 | opus | rust-best-practices, liquers-unittest | Recursive dependency/finalization call graph |
| 5 | opus | rust-best-practices, liquers-unittest | Interpreter ordering and variadic links |
| 6 | sonnet | rust-best-practices, liquers-unittest | Cross-cutting regression fixtures |
| 7 | sonnet | rust-best-practices, liquers-unittest | End-to-end environment/asset fixture |
| 8 | sonnet | rust-best-practices | Validation and evidence-based documentation |

Each step has one owner. Agents must read the named knowledge files before editing and may not
change architecture outside the approved contracts.

## Dependencies and Risk Controls

No Cargo dependency or feature changes are required.

| Risk | Control and acceptance evidence |
|---|---|
| Builder/runtime ambiguity | No builder CWD; raw plan and serialization tests |
| Stale dependency provenance | Fresh preliminary plan only; one entry snapshot; no refresh/re-finalization promise |
| Mutable recipe misidentifies owner | Immutable bound query candidate + non-evaluating owner id verification + provider consistency check |
| Runtime root race/deadlock | One existing mutex critical section; unlock before warning; deterministic and barrier/timeout tests |
| Child fallback warns twice | Propagate root and flag to parent; sibling test |
| Pre-pass changes execution state | Local cursor; only entry root may be atomically installed; final simulated CWD never copied |
| Nondependency causes early side effect | `Use*` and direct-store operands resolve at runtime only |
| Absolute flag leaks into links | Temporary root for outer resource only; independent-link tests |
| Variadic link loses custom value | Top-level lossless fast path; explicit JSON limitation/error for nested variadic storage |
| Owner registration uses raw plan query | Both scheduling and finalization call `Context::owner_key` |
| Typed store error hidden | Resolved `KeyNotFound` assertion |
| Optimizer removes observable CWD | No optimizer; action and nested-plan observer tests remain barriers |

Non-blocking retained compatibility caveat: public `Context::set_cwd_key` still permits opaque
command-side mutation, but no current command uses it; ahead-of-time analysis supports interpreter
`SetCwd` and evaluation-entry initialization only, as approved in Phase 2.

## Rollback Plan

### Per-step and full rollback

Use inverse `apply_patch` edits in reverse order. Remove only the new integration file, then reverse
Step 8 documentation, Step 6 tests, Step 5 interpreter changes, Step 4 call graph, Step 3 Context /
AssetRef helpers, Step 2 recipe prefix, and Step 1 cursor. After each inverse patch, run that
step's focused command. For a full rollback finish with:

```powershell
cargo fmt --all -- --check
cargo check -p liquers-core
cargo test -p liquers-core --lib
git diff --check
```

Files to restore by targeted inverse patches are the six source files, the new integration test,
and the Step 8 documentation set listed above. There is no `Cargo.toml` change. Never use `git checkout`, `git reset`, or a broad
directory replacement; preserve unrelated worktree changes.

### Partial completion

Pause only at a green-gate boundary. Record the last completed step and exact commands/output in
`DESIGN.md`, retain the last passing patch, and resume at the next serial step. A mixed Step 4
signature migration or partially changed interpreter is not a valid pause point.

## Documentation Updates and Phase 5 Evidence

The authoritative `affects_docs` set remains `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md`,
`reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`reference/api/DOC_08_RECIPES_PLANS.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`,
`reference/ASSET_LIFECYCLE.md`, and `reference/PROJECT_OVERVIEW.md`. `specs/README.md` supplies
the capability link and the issue supplies closure evidence. During every step, record requested
versus implemented scope, exact test commands/results, deviations with reasons, reviewer fixes,
the six learning corrections, warning/error observations, and any newly discovered issue. Phase 5
summarizes that evidence and updates current-state documents; it does not rediscover it.

## Phase 4 Review Completion and Certainty

- [x] Reviewer 1: Phase 1 scope/interactions checked; child fallback, documentation intent, and
  observable `SetCwd` coverage closed.
- [x] Reviewer 2: Phase 2 signatures/call graph checked; fresh-plan finalization, owner identity,
  root atomicity, and dependency policy made exact.
- [x] Reviewer 3: Phase 3 examples/tests checked; action-free chain, exact warning observation,
  boundary tags/cache exclusions, serialization, deep links, and typed errors are ordered.
- [x] Reviewer 4: current code compatibility checked; `assets.rs`, `context.rs`, `plan.rs`,
  `interpreter.rs`, and `recipes.rs` callsites/visibility align with existing APIs and
  `crate::maybe_send::BoxFuture`.
- [x] Final holistic review: all findings fixed; rust-best-practices and liquers-unittest applied;
  no blocking question remains.

**Execution certainty: 97%.** The residual 3% is implementation/debugging risk in recursive async
fixtures and existing asset-service timing, not an unresolved design choice. The plan is ready for
the Phase 4 user approval gate after its structural validator and whitespace check pass.

## Phase 5 Entry Criteria

- [ ] Steps 1-7 are implemented, formatted, and pass focused plus aggregate tests.
- [ ] The seven-query matrix returns seven `Ok`, zero warnings, and zero errors.
- [ ] Native and installed-target wasm gates are green; native race evidence is recorded.
- [ ] User and review comments are resolved.
- [ ] Current-state docs and issue match tested behavior, including reviewed/History/capability links.
- [ ] Phase 5 records final scope, deviations, exact evidence, six learnings, and newly filed issues.

## Execution Options

After Phase 4 approval: execute Steps 1-8 now; create ordered tasks; revise this plan; or exit for
manual implementation. Implementation does not complete the design: mandatory Phase 5 starts only
after implementation, validation, and review feedback are complete.
