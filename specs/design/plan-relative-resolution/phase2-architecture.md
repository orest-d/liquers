# Phase 2: Solution & Architecture - plan-relative-resolution

## Overview

Relative resolution is an ordered interpreter concern. `PlanBuilder` preserves source-relative
keys, queries, and links; `Recipe::to_plan` records recipe CWD with one executable `SetCwd` prefix
and one init diagnostic. Dependency analysis and pre-scheduling simulate the same ordered CWD
state without mutating it, while execution resolves each operand against the live CWD held by
`Context` immediately before use.

This gives a plan one meaning rather than a builder-resolved meaning plus a runtime meaning. It
also keeps serialized and programmatically transformed plans honest: `SetCwd` remains an ordered,
observable context effect until a future semantics-preserving normalization pass proves otherwise.

## Known-Issue Preflight

The preflight searched `specs/index.csv` and the linked issue bodies for open work in
`core/query`, `core/plan`, `core/assets`, `core/context`, and storage traversal.

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| [`CORE-PLAN-RELATIVE-RESOLUTION-MISSING`](../../issues/CORE-PLAN-RELATIVE-RESOLUTION-MISSING.md) | draft/local | P1 | This project directly resolves the missing planning and runtime behavior. | Yes, by this project | No external blocker | Implement this design | Keep P1 |
| [`CORE-RECIPES-EXPAND-PREDECESSORS-CRASH`](../../issues/CORE-RECIPES-EXPAND-PREDECESSORS-CRASH.md) | draft/local | P1 | Both predecessor policies must preserve interpreter-owned CWD semantics; this design must not depend on enabling the currently failing policy. | No | No | Test CWD behavior with the default policy and at an `Evaluate` boundary where feasible | Keep P1 |
| [`CORE-EVALUATE-PATH-CONSOLIDATION`](../../issues/CORE-EVALUATE-PATH-CONSOLIDATION.md) | draft/local | P1 | Recipe application is duplicated across environment implementations. Keeping the recipe prefix in `Recipe::to_plan` and resolution in shared `Context`/interpreter paths avoids multiplying this fix across those implementations. | No | No | Monitor; do not require consolidation | Keep P1 |
| [`CORE-PLAN-POLICY-AND-DEFAULTS`](../../issues/CORE-PLAN-POLICY-AND-DEFAULTS.md) | draft/local | P2 | The revised design adds no PlanBuilder CWD policy or configuration. | No | No | Keep CWD outside builder policy | Keep P2 |
| [`STORE-FILESTORE-PATH-TRAVERSAL`](../../issues/STORE-FILESTORE-PATH-TRAVERSAL.md) | draft/local | P1 | CWD expansion consumes logical `..` segments before a store call, but it is not a storage-security boundary and must not be described as one. | No | No | Preserve separate store validation responsibility | Keep P1 |
| [`ASSETS-FIX1`](../../issues/ASSETS-FIX1.md) | draft/local | P2 | It records historical identity mismatches involving CWD normalization. Resolving identity at interpreter and dependency boundaries addresses this project without taking over the broader feature. | No | No | Monitor affected identity tests | Keep P2 |
| [`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`](../../issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md) | draft/local | P2 | `Context::apply` must normalize its input before its existing apply semantics; this does not redefine bare-key application or dependency tracking. | No | No | Keep concerns separate | Keep P2 |
| [`IMMEDIATE-MANAGER-NO-FAST-TRACK`](../../issues/IMMEDIATE-MANAGER-NO-FAST-TRACK.md) | draft/local; implementation merged in PR #27 | P1 | The Phase 3/4 `ImmediateEnvironment` fixtures rely on plain `-R` being able to load eligible stored source assets. Current HEAD now fast-tracks them; tests must seed complete eligible metadata so they exercise CWD resolution instead of accidentally falling through to recipe lookup. | No | No | Reverify manager-parametric behavior and make asset-backed fixture metadata explicit | Keep P1 |

`PARAMETER-ESCAPING-INCOMPLETE` was inspected and excluded: resolution operates on parsed ASTs and
does not add a parse/encode round trip. No GitHub-owned `tracked` or `in_progress` record touching
these integration points is present in the local index.

### Blocking and Priority Decision

There is no unresolved prerequisite. The target issue is P1 because it can silently select the
wrong logical asset, but it does not meet the repository's P0 breadth/urgency threshold. The
architecture removes dependencies on the two adjacent P1 refactors by placing behavior in shared
analysis and runtime boundaries. PR #27's immediate-manager fast-track is present in current HEAD
and removes a prior fixture blocker without implementing any CWD semantics. No priority change is
proposed.

## Resolution Contract

Plans retain source-relative operands. Resolution uses a local cursor `Option<Key>` and walks in
execution order only when an operand is analyzed, scheduled, or executed:

1. A resource key is converted with `Key::to_absolute` when a cursor exists.
2. If the resource header selects `cwd`, the resolved key becomes the cursor for later content.
3. Every `ActionParameter::Link` is recursively resolved using the cursor at that action.
4. A linked query receives a copy of the cursor. Its own `cwd` changes do not escape into the
   enclosing query.
5. Links from query text, command defaults, aliases, enum mappings, or recipe overrides are all
   resolved using the cursor at the containing action.
6. A nested `Step::Plan` inherits the live context CWD when execution reaches it. Its changes
   remain visible afterward because nested execution shares the same `Context`.

Resolution needs a CWD only when a key begins with `.` or `..`. If such a key or query is resolved
without a CWD, the cursor first establishes logical root (`Key::new()`, displayed as `/`) and emits
a warning. Ordinary keys and absolute queries are unchanged and do not trigger the fallback or a
warning. Each embedded query operand is assessed independently, so a relative link inside an
otherwise absolute outer query still receives the defined root fallback when it has no base.
For this contract, a key needs CWD only when its first element is `.` or `..`; `Key` has no separate
absolute flag. `Query::absolute` records a leading `/`. The shared resolver must give that query's
own resource path a temporary logical-root base (including for `/.` or `/..`) instead of consulting,
installing, or warning about the incoming Context CWD. This is intentionally stricter than the
unchanged public `Query::to_absolute`, which preserves the flag but does not implement this ordered
runtime policy. Nested link queries retain their independent absolute/relative status: an absolute
outer query does not make a relative link absolute, and a relative outer query does not change an
absolute link. A query is fully unaffected by CWD fallback only when neither it nor any nested query
operand needs a missing base.

Thus a plan may retain `SetCwd(../c)` and `./hello.txt`. When interpreted with initial CWD `a/b`,
`-R-cwd/../c/-/action-~X~-R/./hello.txt~E` sets the live CWD to `a/c` and evaluates the link as
`action-~X~-R/a/c/hello.txt~E`.

## Data Structures

### New Structs

No new public or serialized struct is required. `PlanBuilder<'c>` is unchanged.

A crate-private, non-Serde cursor centralizes pure parsed-query resolution rules used by analysis
and the interpreter; it belongs with the query AST (rather than `PlanBuilder` or `Context`) so
both layers use the same pure operation without a dependency cycle. The final name may change
during implementation:

```rust
#[derive(Clone, Default)]
pub(crate) struct CwdCursor {
    cwd: Option<Key>,
    defaulted_to_root: bool,
}
```

The cursor owns its optional key so it can be cloned to create an isolated child-query scope and
mutably shared when a nested `Step::Plan` must propagate its final CWD to the enclosing plan. It
never owns environment or asset handles. `defaulted_to_root` records that a leading `.` or `..`
needed a missing base; the cursor then stores logical root as `Key::new()`, and its consumer issues
a warning. It is an analysis value, not another live CWD: only `Context::cwd_key` is mutable
execution state.

The existing `Context<E>::cwd_key: Arc<Mutex<Option<Key>>>` remains the runtime state. Context
clones intentionally share it so a `SetCwd` in a nested plan affects subsequent execution in the
same evaluation.

The existing construction-time `AssetData::query: Arc<Option<Query>>` remains the immutable source
for a keyed asset's bound identity even when provider lookup later replaces `AssetData::recipe`.
A crate-private `AssetRef::bound_key_candidate()` accessor returns that query's key candidate so
Context code does not infer ownership from mutable recipe metadata.

### New Enums

None. CWD is either known (`Some(Key)`) or unavailable (`None`); an additional state enum would
not add information.

### ExtValue Extensions (if applicable)

None.

## Trait Implementations

No new trait implementation is required. Existing `Clone`, Serde, equality, hashing, `TryToQuery`,
and environment bounds remain unchanged.

The recursive parameter-link resolver must explicitly match every `ParameterValue` variant.
Link variants recurse; `MultipleParameters` recurses into its elements; non-link variants are
copied unchanged. This follows the Rust practice of avoiding wildcard matches where a new enum
variant should force a compiler-visible decision.

## Generic Parameters & Bounds

No new generic parameter or bound is introduced. Runtime helpers remain under the existing
`E: Environment` bound. Query and plan normalization are environment-independent and synchronous.

## Sync vs Async Decisions

| Function or path | Choice | Rationale |
|---|---|---|
| Query AST resolution | Sync | Pure cloning/transformation; no environment or I/O |
| Recipe-to-plan conversion | Sync | Existing API and command metadata lookup remain synchronous |
| CWD cursor operations | Sync | Pure key/query transformation with no environment or I/O |
| Dependency finalization/discovery | Async | Existing environment and asset-manager calls remain async |
| Interpreter pre-scheduling | Async | It schedules assets, but computes its CWD cursor synchronously between awaits |
| Context evaluate/apply paths | Async | Existing asset scheduling and execution remain async |
| `SetCwd` execution | Async wrapper, sync mutation | It remains a `do_step` future but only resolves a key and updates the existing mutex |

No lock guard is held across an `.await`.

## Function Signatures

The existing public `Query::to_absolute(&self, cwd_key: &Key) -> Self` signature and behavior remain
unchanged. Runtime uses a crate-private ordered resolver rather than silently broadening that API:

```rust
impl CwdCursor {
    pub(crate) fn new(cwd: Option<Key>) -> Self;
    pub(crate) fn resolve_key(&mut self, key: &Key) -> Key;
    pub(crate) fn resolve_query_scoped(&mut self, query: &Query) -> Query;
    pub(crate) fn set_cwd_from(&mut self, key: &Key) -> Key;
    pub(crate) fn current(&self) -> Option<Key>;
    pub(crate) fn take_root_fallback(&mut self) -> bool;
}
```

`resolve_query_scoped` clones the cursor, walks resource segments in their source order, and
recursively resolves `ActionParameter::Link`. A `cwd` resource segment is itself resolved and
advances that private clone; its effect therefore applies to later segments and links in that
query, but does not escape to its containing query or plan. For `Query::absolute`, only that
query's own resource-path resolution uses a temporary logical-root key; the semantic cursor passed
to action links remains unchanged unless the query contains an explicit `cwd` selector. The one
exception is missing-base initialization in a relative child: if that child clone must default to
root, the parent cursor also records root as its current CWD and retains the warning flag.
`set_cwd_from` resolves the argument against the current cursor before replacing it. Dependency traversal carries a mutable cursor so
nested `Step::Plan` effects can propagate exactly as they do in the shared runtime context:

```rust
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cursor: &'a mut CwdCursor,
) -> crate::maybe_send::BoxFuture<'a, Result<Vec<PlanDependency>, Error>>;

fn schedule_plan_dependencies_from<'a, E: Environment>(
    plan: &'a Plan,
    context: &'a Context<E>,
    cursor: &'a mut CwdCursor,
    seen: &'a mut HashSet<Key>,
) -> crate::maybe_send::BoxFuture<'a, Result<(), Error>>;

pub(crate) async fn has_volatile_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    initial_cwd: Option<Key>,
) -> Result<bool, Error>;

pub(crate) async fn has_expirable_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<(), Error>;

impl<E: Environment> AssetRef<E> {
    pub(crate) async fn bound_key_candidate(&self) -> Option<Key>;
}

async fn make_plan_with_cwd<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
    initial_cwd: Option<Key>,
) -> Result<Plan, Error>;

impl<E: Environment> Context<E> {
    pub(crate) fn resolve_key_from_cwd(&self, key: &Key) -> Result<Key, Error>;
    pub(crate) fn resolve_query_from_cwd(&self, query: &Query) -> Result<Query, Error>;
    pub(crate) fn set_cwd_from_key(&self, key: &Key) -> Result<(), Error>;
    pub(crate) fn install_logical_root_if_unset(&self) -> bool;
    pub(crate) async fn owner_key(&self) -> Result<Option<Key>, Error>;
}
```

Each runtime `resolve_*_from_cwd` helper locks `cwd_key` once, constructs a cursor from the guarded
value, resolves a copy, and conditionally installs `Key::new()` through that same guard if and only
if fallback was needed while the value was still absent. It then drops the guard before calling
`Context::warning` with `Relative key/query has no CWD; using logical root '/'.`; a warning-delivery
error is returned even though the root remains installed. `install_logical_root_if_unset` is a
separately locking helper reserved for the interpreter's local pre-pass after its cursor walk; it
must never be called while either runtime resolver holds the non-reentrant mutex. An absolute
`Query` never requests installation. `set_cwd_from_key` resolves through the same runtime path
before replacing `cwd_key`. This makes installation and exact-once warning ownership atomic across
Context clones while leaving the cursor itself environment-independent.

Existing signatures remain stable for `Recipe::to_plan`, `Plan::override_link`,
`Context::evaluate`, `Context::get_dependency_state`, `Context::apply`, `finalize_plan`, and
`apply_plan`. Public `make_plan` also remains stable and delegates to `make_plan_with_cwd(...,
None)`; the legacy `interpreter::evaluate(..., cwd_key)` passes its supplied CWD to the internal
entry point before dependency analysis. Their behavior changes as described below.

## Planning Flow

`Recipe::to_plan` performs these operations in order:

1. Parse `Recipe::query` and build it with the unchanged, source-relative `PlanBuilder`.
2. Apply value and link overrides without changing their relative form.
3. Parse `Recipe::cwd` once through `get_cwd`.
4. When CWD exists, prepend exactly one `Step::SetCwd(cwd)` before the first query-derived step.
5. Add an init diagnostic such as `Recipe set CWD to 'a/b'`. This records that the recipe set the
   initial CWD; it is planning metadata in `Plan::init_steps`, not the executable mutation.

The recipe-derived `SetCwd` establishes the initial state. A later explicit `-R-cwd/<key>` step
overrides it in sequence. Neither `Recipe::to_plan` nor `PlanBuilder` calls `Query::to_absolute` or
rewrites step operands. Direct PlanBuilder callers get the same source-relative behavior; a caller
that wants an entry CWD supplies it in `Context` or constructs a leading `SetCwd` step.

## Interpreter Flow

The interpreter treats `Context` as the live authority:

- `Step::SetCwd(key)` resolves `key` against `Context::get_cwd_key()` before storing it with
  `set_cwd_key`.
- Every key-bearing asset/store step resolves its key immediately before use.
- `Evaluate`, `UseQueryValue`, and action-link queries use `Context::resolve_query_from_cwd`
  (backed by `CwdCursor::resolve_query_scoped`) immediately before identity creation, scheduling,
  or value creation.
- `Context::schedule_dependency_asset` normalizes before constructing `DependencyKey`; this
  centralizes correctness for `evaluate` and `get_dependency_state`.
- `Context::apply` normalizes before payload analysis and before constructing its ad-hoc recipe.
- `Step::Plan` calls `apply_plan` with the same context, so the nested plan inherits the cursor and
  any nested `SetCwd` persists for subsequent outer steps.

The dependency pre-pass must not mutate `Context`, because doing so would make execution start at
the plan's final CWD. Instead, the public pre-pass wrapper snapshots the context CWD into a local
cursor and calls the boxed `schedule_plan_dependencies_from` helper. That helper walks steps in
order, expands `SetCwd`, key steps, `Evaluate`, and recursively nested action links, then schedules
resolved keyed dependencies. It recursively simulates `Step::Plan` with the same mutable cursor so
the nested plan's final cursor affects later outer steps, matching execution with the shared
context. The helper is boxed specifically because recursive `async fn` is not a finite Rust future.
If this live-context pre-pass is the first supported operation that needs a missing CWD, its wrapper
atomically installs only the live entry CWD as root and logs the warning if and only if it performed
that installation; it never copies the simulated final CWD back to Context. Standalone plan
analysis has no live Context, so the single volatility/discovery pass adds the same text as an init
warning when its cursor uses the root fallback. Expiration reuses the discovered dependency set and
does not run a second cursor walk, so one analysis entry produces at most one diagnostic without a
separate deduplication mechanism.

`find_dependencies` advances the shared cursor for `SetCwd` and nested `Step::Plan`; it resolves
dependency-bearing `GetAsset*`, `GetAssetDirectory`, `GetAssetRecipe`, `Evaluate`, and every action
link variant, including links nested in `MultipleParameters`. `UseKeyValue`, `UseQueryValue`, and
direct-store steps remain runtime-only: they create no `PlanDependency` and resolving them during
analysis could only move the missing-base warning earlier. Dependency keys and cycle checks are
created only from resolved copies; the stored plan remains unchanged. Separately evaluated child
queries, links, and keyed recipes receive forked cursors so their CWD changes cannot leak into the
parent evaluation. A nested `Step::Plan` is different because execution deliberately shares the
parent `Context`.

When dependency traversal resolves a keyed recipe, it must build that recipe through
`Recipe::to_plan_for_key(command_registry, &resolved_key)` (or one helper with the identical
recipe-aware and keyed-payload contract), not the current `recipe.get_query()` plus bare
`PlanBuilder::new` path. This preserves recipe CWD, argument/link overrides, placeholders, the
single initial CWD step, and the existing keyed-recipe payload boundary in nested dependency
analysis.

`finalize_plan` accepts only a fresh preliminary plan and must not register dependency edges under a
raw relative `plan.query.key()`. It snapshots `Context::get_cwd_key()` once and supplies that entry
value to the volatility pass. Volatility performs the single
dependency discovery and stores `plan.dependencies`; expiration reuses that exact resolved set and
does not traverse the same plan again. A finalized or deserialized plan is not re-finalized under a
different CWD; callers rebuild it from its source `Query` or `Recipe`, so no refresh API, provenance
field, or stale-diagnostic cleanup mechanism is introduced.

Edge registration uses `Context::owner_key()`. The helper takes a candidate from the current
`AssetRef`'s immutable construction-time query, rejects a current recipe whose `store_to_key()` (or
fallback `key()`) does not match it, and then calls the non-evaluating
`AssetManager::owned_key_asset`. It returns the candidate only when that registered owner's id
equals the current `AssetRef::id`; temporary, ad-hoc, volatile, provider-mismatched, and differently
owned assets return `None`. This keeps provider replacement from changing the bound owner while
also refusing to trust an unregistered recipe-derived key. `make_plan`, which has no execution
context, supplies `None` as the entry CWD. Resolution changes identity copies only; it does not
rewrite `Plan::query`.

Direct calls to public `Context::set_cwd_key` from a command could make any ahead-of-time dependency
pre-pass incorrect because an opaque action could change the cursor unexpectedly. No current
command does this. This design treats interpreter `SetCwd` and evaluation-entry initialization as
the supported plan-CWD mutation paths; commands may observe CWD and issue relative
`evaluate`/`apply` calls. Changing the visibility of `set_cwd_key` is a compatibility decision
outside this issue and should be tracked separately if command-side mutation needs support.

## Future Plan Normalization and Optimization

A later plan pass may clone a CWD cursor, rewrite every static key/query/link to absolute form, and
remove a `SetCwd` only when it proves the context effect is unobservable. Absolute operands alone
are insufficient: a later action can inspect `Context::get_cwd_key`, call `evaluate` or `apply`
with a relative query, or enter a nested plan. Therefore `SetCwd` is an ordering barrier around
opaque actions and nested plans. Safe removals include a redundant assignment to the same resolved
CWD, an assignment overwritten before any observer, or a trailing assignment after the last
observer. The present project supplies reusable cursor semantics but implements no optimizer and
performs no static plan rewrite.

## Integration Points

| Crate/file | Change |
|---|---|
| `liquers-core/src/query.rs` | Keep public `Query::to_absolute` stable; host the crate-private `CwdCursor` and recursive AST traversal needed by shared analysis/runtime semantics |
| `liquers-core/src/assets.rs` | Expose the immutable construction-time query key through a crate-private `AssetRef` accessor; retain non-evaluating registered-owner verification |
| `liquers-core/src/plan.rs` | Keep PlanBuilder source-relative; make `find_dependencies` use and propagate the ordered cursor; build nested recipes with `to_plan_for_key` |
| `liquers-core/src/recipes.rs` | Prepend programmatic/provider CWD in `Recipe::to_plan` without rewriting operands; add the recipe init diagnostic |
| `liquers-core/src/interpreter.rs` | Resolve each runtime operand, pass legacy evaluate CWD into analysis, simulate nested plans during pre-scheduling, and avoid raw relative owner identity registration |
| `liquers-core/src/context.rs` | Normalize nested evaluate/apply queries through the shared cursor before payload checks, dependency identity, and manager calls; install/log the root fallback; retain the only live CWD state |

No `liquers-lib`, web, Python, store implementation, or command-library change is required.
No external dependency or Cargo feature is added. Source files reuse their existing crate-local
imports for `Key`, `Query`, `ActionParameter`, `ParameterValue`, `Plan`, and `Step`, adding only
crate-local imports where a type is not already in scope.

## Documentation Architecture

### Reference Plan

Extend these existing references for library users and framework implementers:

- `specs/reference/api/DOC_08_RECIPES_PLANS.md` (`core/plan`, `core/assets`, `core/context`) is the
  authoritative recipe/plan lifecycle contract. Document CWD provenance, the verified pre-fix
  behavior, ordered recursive planning/runtime semantics, initial `SetCwd` versus init `Info`,
  manual-plan guarantees, and verification evidence; remove the resolved CWD gap.
- `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` (`core/query`) defines syntax and AST
  semantics. Keep the existing `Query::to_absolute` contract, and document interpreter-side
  ordered `cwd` state, recursive link resolution, nested-query scoping, and the worked `a/b` to
  `a/c` example.
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (`core/context`, `core/plan`)
  defines runtime behavior. Add the planning/runtime responsibility split, context inheritance for
  nested plans, runtime normalization boundaries, and non-mutating dependency pre-pass cursor.

### Guide Plan

Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (guide; integration authors; `core/commands`,
language bindings). Add a short section showing how provider-loaded recipes receive CWD, how a
custom/programmatic `Recipe` supplies it, why YAML must not, and how integrations can rely on
relative links being resolved at use. No new guide is created.

### Other Documents to Create

None. Phase 5 updates the issue and design tracking records rather than creating a second source of
truth.

### Existing Documents to Review or Update

Authoritative `affects_docs` set:

- `specs/reference/api/DOC_08_RECIPES_PLANS.md` — substantive contract update described above.
- `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` — ordered interpreter resolution and
  syntax example, while preserving the existing public `Query::to_absolute` contract.
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` — live CWD and interpreter
  resolution contract.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — provider and programmatic recipe guidance.
- `specs/reference/ASSET_LIFECYCLE.md` — amend recipe planning/execution flow to show the
  recipe-derived initial CWD and context-backed runtime normalization.
- `specs/reference/PROJECT_OVERVIEW.md` — add one compact sentence to the recipe/execution overview
  explaining provider-derived CWD and ordered relative resolution.

This retains every Phase 1 documentation commitment and uses the existing indexed guide path.

`specs/reference/WEB_API_SPECIFICATION.md` was reviewed and discarded as an update candidate: it
serializes `Recipe::cwd` but does not define planning behavior, and this project changes no HTTP
schema. `specs/reference/REGISTER_COMMAND_FSD.md` was discarded because command registration does
not own query resolution. Store documents were discarded because logical CWD normalization does
not change the storage traversal/security contract.

Phase 5 evidence collection will record: the final provider/programmatic usage guidance; the
planning-versus-runtime development rule; corrections to the pre-fix DOC-08 claim; connections to
dependency identity and store validation; and any unexpected behavior found while testing nested
links, defaults, aliases, or programmatic plans.

### Design and Capability Links

`specs/README.md` already links this design under Core. Keep the link and change its lifecycle
label as phases complete. `specs/index.csv`, this `DESIGN.md`, and the linked issue retain the
project/issue relationship. Each substantively updated current-state document adds a history link
back to `specs/design/plan-relative-resolution/`; the issue records verification and closure when
implementation is complete.

## Relevant Commands

### New Commands

None.

### Relevant Existing Namespaces

None. No `liquers-lib` command namespace defines or owns CWD semantics. This assessment was
confirmed by the user when approving Phase 2.

## Web Endpoints (if applicable)

None. Serialized plans gain a recipe CWD prefix but retain source-relative operands; endpoint
shapes and status mappings do not change.

## Error Handling

- Invalid programmatic `Recipe::cwd` continues to return the existing parse `Error` from
  `Recipe::get_cwd`; no fallback or silent ignore is allowed.
- Invalid recipe query/link text continues to return parser errors with existing query context.
- A missing named recipe override remains the existing `general_error` against the source query.
- Resolution itself is infallible after parsing because `Key::to_absolute` is an AST operation.
- If a leading `.` or `..` needs a missing CWD, resolution succeeds from logical root and records
  the warning; this is a defined fallback, not an error.
- Ordinary keys bypass CWD resolution. An absolute query bypasses the incoming CWD and fallback
  warning; if it contains leading `.` or `..`, those are resolved only against its temporary
  logical-root base.
- Runtime resolver wrappers propagate an existing `Context::warning` logging failure through
  `Result<_, Error>`; they add no new error kind.
- Store traversal validation remains responsible for rejecting a logical key that is unsafe for a
  concrete backend.

No new error variant or public error type is needed.

## Serialization Strategy

No schema field or Serde annotation changes. `Recipe::cwd` remains `Option<String>`, `Plan` keeps
its current internal Serde representation, and `Context`/`CwdCursor` runtime fields are not
serialized. Recipe plans serialize their source-relative `Plan::query`, step keys, and parameter
links plus an initial `SetCwd` step and init `Info`. Existing plans still deserialize and receive
the same interpreter-side resolution; serialization must not be treated as an implicit
normalization pass.

AST objects are transformed directly. The implementation must not encode and reparse queries,
which preserves `QuerySource` and nested `Position` provenance and avoids interaction with
parameter escaping.

## Concurrency Considerations

`Context::cwd_key` is the existing shared mutex. Each access clones or replaces `Option<Key>` and
drops the guard immediately; no guard crosses an await. The scheduling pre-pass uses an owned local
cursor and therefore cannot race ahead by mutating execution state. Query resolution uses owned,
single-threaded cursor state.

Nested dependency evaluation receives its own evaluation `Context`; only nested `Step::Plan`
execution deliberately shares the current context. This preserves existing asset-evaluation
isolation while making intra-plan CWD state sequential.

## Verification Evidence Plan

Phase 3 may refine test module placement, but it must preserve these named behavioral targets:

| Requirement | Planned test | Required assertion |
|---|---|---|
| Provider CWD comes from the containing `recipes.yaml`, not YAML | Extend `recipes::tests::test_default_recipe_provider` | Loaded recipe has the provider directory; authored `cwd` is rejected |
| Programmatic recipe CWD is respected | `recipes::tests::recipe_to_plan_preserves_programmatic_cwd` | Step 0 is the single raw `SetCwd`, init steps contain the recipe CWD `Info`, and query/link operands remain source-relative |
| Explicit relative CWD overrides recipe CWD in order | `interpreter::tests::resolves_ordered_cwd_changes` | Starting `a/b`, runtime `../c` becomes `a/c`, and later keys use `a/c` |
| Nested query links inherit but do not leak CWD | `context::tests::resolver_scopes_nested_links` | Runtime nested `./hello.txt` becomes `a/c/hello.txt`; child-query CWD changes do not alter the outer cursor |
| Defaults, aliases, enum links, and recipe link overrides use action CWD | focused `interpreter::tests::*_link_respects_cwd` cases | Stored links remain relative; scheduling and execution use the expected resolved query |
| Dependency discovery uses recipe-aware planning | `plan::tests::find_dependencies_respects_nested_recipe_cwd` | Dependency identity matches the absolute nested recipe key and includes overrides |
| Pre-scheduling observes CWD without advancing execution state | `interpreter::tests::dependency_preschedule_tracks_cwd_without_mutating_context` | Scheduled key is absolute; context still has its initial CWD before step execution |
| Legacy evaluate CWD reaches analysis | `interpreter::tests::evaluate_cwd_applies_before_dependency_analysis` | `evaluate(..., Some(a/b))` discovers and executes the same resolved dependency identity |
| Programmatic/manual plans are corrected at runtime | `interpreter::tests::manual_plan_resolves_relative_steps_from_context` | Relative key/query/link/`SetCwd` steps reach the same assets as an absolute plan |
| Nested plans inherit live CWD | `interpreter::tests::nested_plan_inherits_and_updates_cwd` | Nested references use the incoming cursor and subsequent steps observe ordered changes |
| Finalization registers the resolved owner | `interpreter::tests::finalize_relative_plan_uses_context_owner_key` | Dependency edges are not registered under raw `plan.query.key()` |
| Owner identity survives provider replacement without trusting mutable recipe metadata | `context::tests::owner_key_matches_non_evaluating_registered_owner` and `context::tests::owner_key_rejects_temporary_ad_hoc_volatile_and_provider_mismatch` | Only the immutable bound candidate whose registered owner id matches is returned |
| Observable `SetCwd` is not treated as redundant | `interpreter::tests::action_observes_current_cwd` | An action sees the resolved CWD and a following relative `Context::evaluate` uses it |
| Missing CWD defaults once to root | `interpreter::tests::relative_operand_without_cwd_warns_and_uses_root` | A pre-pass or runtime first leading `.`/`..` atomically sets Context CWD to `Key::new()`, resolves from `/`, and the combined evaluation logs one warning |
| Absolute operands do not trigger fallback | `context::tests::absolute_operands_ignore_missing_cwd` | Ordinary keys remain unchanged; an absolute query's own path, including `/.` or `/..`, uses a temporary root base without setting Context or warning |
| Relative link inside an absolute query remains independent | `context::tests::absolute_query_does_not_absolutize_relative_link` | The outer resource path ignores Context; its relative link uses an existing Context CWD or installs root with the normal warning when none exists |

Verification output, source-level findings, and any deviations from these planned names are
captured in Phase 5 documentation evidence.

## Compilation Validation

The design adds private `Option<Key>` and fallback-flag cursor fields plus methods using existing `Key`, `Query`, and
`ParameterValue` types. `Query::to_absolute` and `PlanBuilder` retain their public contracts, so
existing call sites continue to compile. `CwdCursor` owns an `Option<Key>` and introduces no
borrowed state across awaits. All `ParameterValue` variants are handled explicitly, including recursive
`MultipleParameters`. No async trait signature, object-safety condition, Send bound, or Serde data
model changes.

Implementation validation should include `cargo check -p liquers-core`, focused unit/integration
tests, `cargo test -p liquers-core --lib`, Rustdoc checks for changed public methods, and native plus
wasm compilation where the normal repository gate requires them.

## References to liquers-patterns.md

- Public APIs use Rustdoc with examples where syntax is subtle.
- PlanBuilder remains source-relative and gains no CWD policy or configuration.
- Errors propagate through `Result` at parse boundaries; no panic or silent fallback is added.
- Parsed domain types (`Key`, `Query`) are used instead of raw strings after validation.
- Async remains confined to I/O and scheduling; pure transformations stay synchronous.
- Shared mutable state uses the existing narrow mutex, with no lock held across `.await`.
- Tests should cover positive flow, malformed CWD input, nested links, explicit CWD overrides,
  defaults/aliases/enum links, manual plans, and regression behavior without a CWD.
