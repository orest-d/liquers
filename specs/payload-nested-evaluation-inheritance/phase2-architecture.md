# Phase 2: Solution & Architecture - Payload Inheritance in Nested Evaluation

## Overview

Payload requirement becomes a declared, propagated property mirroring `volatile`: a
`PayloadRequirement` enum on `CommandMetadata`, a `Plan::payload_required` field computed during
plan building, and an async `RequiresPayload` trait paralleling the existing `IsVolatile` trait.
`Context::schedule_dependency_asset` then switches on it — payload-free dependencies keep today's
queued, cached path unchanged, while payload-requiring ones forward the parent's payload to the
already-existing `AssetRef::run_immediately` path.

**Key discovery shaping this architecture:** the payload-bearing evaluation machinery already exists
at the asset level. `AssetRef::run_immediately(Option<E::Payload>)`,
`run_immediately_inline(Option<E::Payload>)`, and `evaluate_immediately(Option<E::Payload>)`
(`assets.rs:1670`, `:1717`, `:1883`) all accept a payload; only `apply_immediately` reaches them
today. The work is **routing and declaration**, not new evaluation machinery.

## Data Structures

### New Enum: `PayloadRequirement`

**Location:** `liquers-core/src/command_metadata.rs` (beside `CommandMetadata`; `plan.rs` already
imports from this module).

```rust
/// Whether a command or plan needs an evaluation payload to run.
///
/// `Optional` — runs without a payload but receives one when available — is
/// deliberately not implemented; see `specs/ISSUES.md`. Adding it re-opens the
/// non-volatile-with-payload state and is a breaking change for exhaustive matches,
/// which is intended: every match site must decide how to treat it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PayloadRequirement {
    /// Command does not use a payload. Cacheable and shareable as today.
    #[default]
    None,
    /// Command refuses to run without a payload. Implies `volatile`.
    Required,
}
```

**Methods** (no default match arms anywhere):

```rust
impl PayloadRequirement {
    /// Combine two requirements (plan aggregation). `Required` dominates.
    pub fn join(self, other: Self) -> Self;
    /// True when a payload must be present for evaluation to proceed.
    pub fn is_required(self) -> bool;
    /// True when no payload is needed — used by `skip_serializing_if`.
    pub fn is_none(self) -> bool;
}
```

**Ownership:** `Copy` — a two-variant fieldless enum, cheaper to copy than to borrow, matching how
`bool volatile` is used today.

**Serialization:** `#[serde(default)]` is **mandatory** — existing serialized `CommandMetadata` and
`Plan` documents have no such field and must continue to load. Paired with
`skip_serializing_if = "PayloadRequirement::is_none"` so existing serialized output stays
byte-identical when nothing requires a payload.

### Modified: `CommandMetadata` (`command_metadata.rs:776-781`)

```rust
pub struct CommandMetadata {
    // ... existing fields
    pub volatile: bool,                       // unchanged

    /// Whether this command needs an evaluation payload.
    /// `Required` also sets `volatile` at registration time, so all existing
    /// volatility propagation applies without change.
    #[serde(skip_serializing_if = "PayloadRequirement::is_none")]
    #[serde(default)]
    pub payload_required: PayloadRequirement,
}
```

Two orthogonal declaration keys per Phase 1 D7 — `volatile` is untouched.

### Modified: `Plan` (`plan.rs:1353-1380`)

```rust
pub struct Plan {
    // ... existing fields
    pub is_volatile: bool,                    // unchanged, still no serde(default)

    /// Computed during plan building, mirroring `is_volatile`.
    #[serde(default)]
    pub payload_required: PayloadRequirement,
}
```

**Deliberate deviation:** `is_volatile` carries a comment that it has *no* `serde(default)` and is
always required in the serialized format. `payload_required` **does** get `#[serde(default)]`,
because plans serialized before this change contain no such field and must still deserialize.
Matching `is_volatile`'s strictness would break every stored plan.

### Modified: `PlanBuilder` (`plan.rs:880-1010`)

```rust
pub struct PlanBuilder<'c> {
    // ... existing fields
    is_volatile: bool,                        // unchanged
    payload_required: PayloadRequirement,     // new, mirrors is_volatile
}
```

## Trait Implementations

### New trait: `RequiresPayload<E>` (`liquers-core/src/interpreter.rs`)

Mirrors `IsVolatile<E>` (`interpreter.rs:402-510`) exactly — `pub(crate)`, native `async fn` in
trait (not `#[async_trait]`), never used as `dyn`, so object safety is not a concern.

```rust
pub(crate) trait RequiresPayload<E: Environment> {
    async fn requires_payload(&self, env: EnvRef<E>) -> Result<PayloadRequirement, Error>;
}
```

**Implementors** — one per `IsVolatile` implementor, same recursion structure:

| Implementor | Behavior |
|---|---|
| `ParameterValue` | delegates to the link query when `self.link()` is `Some`, else `None`; needs `Box::pin` for the recursive call exactly as `IsVolatile` does at `interpreter.rs:409` |
| `ResolvedParameterValues` | `join` across parameters, short-circuiting on `Required` |
| `Plan` | returns the cached `self.payload_required` (no re-walk), mirroring `IsVolatile for Plan` |
| `Recipe` | `to_plan(...)` then delegate — note **no** `Recipe`-level override field (Phase 1 D8: payload requirement is derived, not author-declarable) |
| `Query` | `make_plan(env, self.clone())` then delegate |
| `Step` | full explicit match over every variant, **no default arm** |

**`Step` match — the arms that differ from `IsVolatile`:**

- `Step::Action { .. }` → `cmd.payload_required.join(parameters.requires_payload(env).await?)`
- `Step::GetAsset` / `GetAssetBinary` / `GetAssetMetadata` / `GetAssetRecipe(key)` → new
  `AssetManager::payload_required(&Key)` (below)
- `Step::Evaluate(query)` / `Step::Plan(plan)` → delegate
- `Step::GetAssetDirectory` / `GetResourceDirectory` → `None` (they return `Ok(true)` for volatility,
  but a directory listing needs no payload)
- `GetResource` / `GetResourceMetadata` → `None`; these already carry an
  "ADD SUPPORT FOR RESOURCE VOLATILITY CHECK!" TODO (`interpreter.rs:487,492`) — do not extend that
  gap, just return `None`
- `UseQueryValue`, `Filename`, `Info`, `Warning`, `Error`, `SetCwd`, `UseKeyValue` → `None`

**Efficiency note:** this is a second async traversal alongside `IsVolatile`. Both short-circuit at
`Plan` (cached field), so the cost is the recipe-resolution path. If profiling later shows it
matters, the two can be consolidated into one traversal returning the derived `EvaluationClass`;
`IsVolatile` is `pub(crate)`, so that refactor has no external blast radius. Not done now —
mirroring keeps the change reviewable.

### Extended trait: `AssetManager<E>` (`assets.rs:2630-2714`)

Two **new methods with default implementations** — no existing signature changes, so no implementor
breaks (rust-best-practices: extend, don't mutate).

```rust
/// Whether the keyed resource's recipe requires a payload.
/// Default mirrors `is_volatile`'s shape (`assets.rs:2667-2674`).
async fn payload_required(&self, key: &Key) -> Result<PayloadRequirement, Error> {
    if let Some(recipe) = self.recipe_opt(key).await? {
        Ok(recipe.requires_payload(self.get_envref()).await?)
    } else {
        Ok(PayloadRequirement::None)
    }
}

/// Resolve `query` as a dependency of `parent`, forwarding `payload` to the
/// evaluation when the dependency requires one.
///
/// Default ignores the payload and delegates, preserving current behavior for
/// third-party managers that do not opt in.
async fn get_dependency_asset_with_payload(
    &self,
    parent: &AssetRef<E>,
    query: &Query,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error> {
    let _ = payload;
    self.get_dependency_asset(parent, query).await
}
```

**Overrides required — the two managers differ, and not symmetrically:**

| Manager | Overrides `get_dependency_asset` today? | What the payload override must do |
|---|---|---|
| `DefaultAssetManager` (`assets.rs:3864`) | **Yes** — resolves via `get_resource_asset` / `get_query_asset`, handles stale-terminal eviction and store fast-track, then queue-schedules | Reuse that resolution verbatim; replace only the final queue submission with `asset.run_immediately(payload)` |
| `ImmediateAssetManager` (`assets.rs:5186-5340`) | **No** — it implements only `get_asset`, `apply`, `apply_immediately`, `get`, and support methods, inheriting the trait-default `get_dependency_asset` that delegates to `get_asset` | Must gain its own override: resolve as `get_asset` does (`:5187-5230`), then call `run_immediately_inline(payload)` instead of `run_inline()` (`:5227`) |

The asymmetry matters because the `ImmediateAssetManager` path is the wasm-compatible one. Its
`apply_immediately` already shows the exact shape needed — `run_immediately_inline(payload)` at
`assets.rs:5248` versus `run_inline()` in `apply` at `:5237`.

Both managers already route volatile queries to the fresh-unshared constructors
(`get_volatile_query_asset` / `get_volatile_resource_asset`), so no change is needed there: because
`payload ⟹ volatile`, a payload-requiring query is already resolved to a fresh asset.

## Function Signatures

### `liquers-core/src/context.rs` — the routing switch

```rust
impl<E: Environment> Context<E> {
    /// Existing method, gains the payload switch.
    pub(crate) async fn schedule_dependency_asset(&self, query: &Query)
        -> Result<AssetRef<E>, Error>;

    /// Unchanged public surface.
    pub async fn get_dependency_state(&self, query: &Query) -> Result<State<E::Value>, Error>;
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error>;
    pub async fn apply(&self, query: &Query, to: State<E::Value>) -> Result<AssetRef<E>, Error>;
}
```

`Context::evaluate` and `get_dependency_state` need **no signature change** — both already route
through `schedule_dependency_asset`, so inheritance arrives via the one switch.

**`schedule_dependency_asset` control flow:**

1. `let req = query.requires_payload(envref.clone()).await?;`
2. `PayloadRequirement::None` → today's path, entirely unchanged.
3. `PayloadRequirement::Required`:
   - `self.payload.is_none()` → return the D5 error immediately (see Error Handling).
   - Perform cycle-check and edge registration **as today** (see refinement below).
   - `manager.get_dependency_asset_with_payload(&self.assetref, query, self.payload.clone()).await`

### D1, as clarified: a payload asset may *have* dependencies but may not *be* one

The governing constraint is **dependency-key identity**: a payload is not part of the dependency key,
so an asset evaluated with one cannot be named by its key or query. That makes the rule directional:

- A payload-evaluated asset **may have dependencies mapped** — it records what it depends on, and
  those dependencies are ordinary registered assets.
- A payload-evaluated asset **may not be a registered dependency** — nothing may hold an edge *to*
  it, because two evaluations with different payloads would share one key.
- **Cycle checking still applies.**

Mapped onto the four registration actions in `schedule_dependency_asset` (`context.rs:369-425`):

| # | Action | When the *scheduled dependency* requires payload |
|---|---|---|
| 1 | `register_scheduled_dependency(dependent, dependency, version)` — graph edge + cycle check | **Skip the edge**; cycle check still required (see open question) |
| 2 | `get_dependency_asset…` — resolve and schedule | Always — this is the payload-forwarding call |
| 3 | `add_dependent_asset(query_dep_key, parent_weak)` — invalidation back-ref keyed by the dependency | **Skip** — it names the payload asset as a key |
| 4 | `self.add_dependency(DependencyRecord)` — the parent's own metadata record | **Keep** — "dependencies mapped" |

Note this supersedes the earlier recommendation in this document to keep edge registration wholesale;
that would have registered an edge *to* a payload asset, which the key-identity constraint forbids.

### `Context::apply` — decided: inherits

`apply` switches on the same requirement, calling `apply_immediately` with the inherited payload when
`Required`, and the existing `apply` otherwise. Consistent with `evaluate`, and matches the
observation that the payload branch is semantically `apply_immediately`. This is the one place an
existing API changes from queued to inline.

### Gap found: keyed payload recipes are unreachable from the top level

`AssetRef::evaluate_recipe` (`assets.rs:1738-1776`) special-cases a pure-key recipe: it calls
`manager.get(&key)` and, when that returns a *different* asset, **delegates** to it through
`record_dependency_on_asset` + `wait_for_dependency`. `AssetManager::get(&Key)` takes no payload.

So `EnvRef::evaluate_immediately("/some/key", payload)` on a **keyed** query silently drops the
payload at the delegation hop. The keyed-recipe benefit that motivated Phase 1 D6 is therefore not
delivered by the routing switch alone — the delegation path needs its own treatment. See open
question 3.

### `liquers-core/src/plan.rs` — local detection

```rust
impl<'c> PlanBuilder<'c> {
    /// Mark plan as payload-requiring and add explanatory Step::Info.
    /// Mirrors `mark_volatile` (plan.rs:923-929).
    fn mark_payload_required(&mut self, reason: &str);

    /// Read `payload_required` from CommandMetadata.
    /// Mirrors `is_action_volatile` (plan.rs:931-938).
    fn action_payload_requirement(&self, command_key: &CommandKey) -> PayloadRequirement;

    /// Recurse into link-parameter sub-plans.
    /// Mirrors `check_parameter_for_volatile_links` (plan.rs:975-995).
    fn check_parameter_for_payload_links(&mut self, param: &ParameterValue) -> Result<(), Error>;
}
```

Both `build()` (`plan.rs:1009-1010`) and **plan splitting** (`plan.rs:1599,1607`) must copy
`payload_required` alongside `is_volatile`. Splitting is the easiest site to miss (Phase 1 D8).

### `liquers-macro/src/registration.rs` — declaration

```rust
enum CommandSignatureStatement {
    // ... existing
    Volatile(bool),                 // unchanged
    PayloadRequired(bool),          // new; macro-local bool, 2 states today
}
```

- **Parse** (`registration.rs:773`): add a `"payload"` arm taking a bare ident (`required` / `none`),
  not a `LitBool` — `volatile` uses `syn::LitBool`, this does not. Unknown idents must be a
  `syn::Error`, not a silent default.
- **Codegen** (mirroring `registration.rs:1225-1229`): when payload is required, emit **both**
  ```rust
  cm.payload_required = liquers_core::command_metadata::PayloadRequirement::Required;
  cm.volatile = true;
  ```
  The second line is Phase 1 D7's shortcut: it makes every existing volatility-propagation path apply
  with no further change.
- The macro crate keeps its own `bool` and emits the fully-qualified enum path, so no new
  compile-time dependency on `liquers-core` is introduced.

**Separate, unrelated fix** (Phase 1 D2): `registration.rs:1531-1536` consumes an unknown trailing
argument ident and silently treats it as not-injected. Tighten to a `syn::Error`. Independent of this
feature; called out so it is not lost.

### `liquers-py/src/command_metadata.rs`

```rust
#[getter]
fn payload_required(&self) -> String;   // mirrors fn volatile() at :362
```

Returned as a string rather than a Python enum, avoiding a PyO3 enum binding for two variants.

## Sync vs Async Decisions

| Function | Async | Rationale |
|---|---|---|
| `PayloadRequirement::join` / `is_required` / `is_none` | No | Pure, `Copy`, no I/O |
| `RequiresPayload::requires_payload` | **Yes** | Resolves recipes through the recipe provider (store I/O), exactly like `IsVolatile` |
| `AssetManager::payload_required` | **Yes** | Calls `recipe_opt` (async) |
| `AssetManager::get_dependency_asset_with_payload` | **Yes** | Schedules/evaluates assets |
| `PlanBuilder::mark_payload_required` etc. | **No** | Local metadata reads only — `PlanBuilder::build()` is sync (`plan.rs:1004`) and must stay so |

This is Phase 1 D5's split: the **local** half is sync inside `PlanBuilder`; the **transitive** half is
async and needs an `EnvRef`, so it lives in the `RequiresPayload` traversal beside
`has_volatile_dependencies` (`interpreter.rs:40-45,74`).

## Error Handling

No new error types. `ErrorType::General` via the existing typed constructor.

| Scenario | Constructor | When |
|---|---|---|
| Plan requires payload, context has none | `Error::general_error(...)` | `schedule_dependency_asset`, before scheduling |
| Same, at top level via `EnvRef::evaluate` | `Error::general_error(...)` | plan finalization |
| Mislabeled command with dynamic dependency | existing `InjectedFromContext` error | at command execution — the accepted D5 limitation |

Message must name the query and state the cause, e.g.
`"Query '<q>' requires a payload, but the evaluation was started without one"`.

**Considered and rejected:** adding an `ErrorType::PayloadRequired` variant — it would touch every
exhaustive match on `ErrorType`. A `payload_required` *constructor* alongside `key_not_found` /
`dependency_cycle` would be rule-compliant, but `general_error` is sufficient.
**Recommend: plain `general_error`, no new `ErrorType` variant.**

## Serialization Strategy

| Field | Annotation | Reason |
|---|---|---|
| `CommandMetadata::payload_required` | `#[serde(default)]` + `skip_serializing_if = "PayloadRequirement::is_none"` | Existing metadata documents lack the field; output stays byte-identical when unused |
| `Plan::payload_required` | `#[serde(default)]` | Stored plans predate the field; deliberately unlike `is_volatile` |
| `MetadataRecord` | **no change** | Phase 1 D8: derived, and `is_volatile` already conveys the operational consequence. Deferred pending a consumer |

**Round-trip requirement:** a `CommandMetadata` serialized before this change must deserialize, and
re-serializing it must not introduce a `payload_required` key.

## Concurrency Considerations

`PayloadRequirement` is `Copy` and fieldless — trivially `Send + Sync`, no locking.

`E::Payload` is already `PayloadType: Clone + MaybeSend + MaybeSync + 'static`
(`commands.rs:343-346`), so `self.payload.clone()` crossing into the scheduling call is sound on both
native and wasm. **No new bounds are introduced anywhere.**

The payload branch calls `run_immediately` / `run_immediately_inline`, which run **inside the
caller's future** rather than consuming a job-queue slot. Phase 1 D6 recorded the trade-off: the
existing claim-based direct-claim mechanism (`assets.rs:1768`) is the precedent, and capacity
borrowing is deliberately *not* introduced.

## Relevant Commands

**No new commands.** This is a core framework change.

### Affected existing namespaces

| Namespace | Relevance |
|---|---|
| `lui` / `egui` (`liquers-lib/src/ui/commands.rs`) | The only production payload consumers (`E::Payload: UIPayload`). Every command reading payload must gain `payload: required` or it silently keeps today's behavior in nested position |
| `liquers-core` test commands (`tests/injection.rs`) | Annotation + the inheritance test rewrite |

**Migration is the main risk** (Phase 1 D2): a payload-reading command without the annotation still
works at top level via `evaluate_immediately` and fails only nested. Phase 3 must include an audit
checklist for all `ui/commands.rs` sites.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `command_metadata.rs` | `PayloadRequirement` enum + `CommandMetadata` field |
| liquers-core | `plan.rs` | `Plan` field, `PlanBuilder` field + 3 methods, `build()`, **plan splitting** |
| liquers-core | `interpreter.rs` | `RequiresPayload` trait + 6 impls; pre-pass must leave payload-requiring queries to `do_step` (`interpreter.rs:85-86`) |
| liquers-core | `assets.rs` | 2 `AssetManager` default methods; `DefaultAssetManager` override (queued) + `ImmediateAssetManager` override (new — it inherits the default today) |
| liquers-core | `context.rs` | routing switch in `schedule_dependency_asset`; `apply`; module rustdoc at `:76-80` |
| liquers-macro | `registration.rs` | `payload:` statement parse + codegen |
| liquers-lib | `ui/commands.rs` | annotate payload-using commands |
| liquers-py | `command_metadata.rs` | getter parity |

**No new dependencies. No crate-flow violations** — all changes are in `liquers-core` and crates that
already depend on it.

## Compilation Validation

- [x] No `unwrap()` / `expect()` — all fallible paths return `Result<_, Error>`
- [x] No new error types; typed constructor only
- [x] No default match arms on `PayloadRequirement` or `Step`
- [x] Trait bounds unchanged — no new bounds on `E`, `E::Payload`, or any implementor
- [x] `AssetManager` extended with defaulted methods only; no implementor breaks
- [x] Async only where I/O occurs; `PlanBuilder::build()` stays sync
- [x] Crate dependency flow respected

**Check:** `cargo test -p liquers-lib --lib --tests` (per CLAUDE.md), plus
`cargo check -p liquers-core --target wasm32-unknown-unknown` for the inline path.

## Decided

- **`Context::apply` inherits payload** — via `apply_immediately` when required.
- **Dependency registration** — a payload asset may have dependencies and is cycle-checked, but is
  never a registered dependency (see the D1 clarification above).
- **`Plan::payload_required` gets `#[serde(default)]`** although `is_volatile` deliberately does not.
  Backward compatibility with stored plans outweighs matching `is_volatile`'s strictness; no
  reasonable alternative.

## Open Questions

### 1. Where does cycle detection for payload chains live?

Skipping the graph edge (action #1 above) removes the site where `would_create_cycle` runs
(`dependencies.rs:415-445`). Since neither end of a payload→payload chain is a graph node, the graph
can never see such a cycle regardless.

| Option | Catches | Cost |
|---|---|---|
| **A. Path guard on `Context`** *(recommended)* | payload→payload cycles, the only kind the graph structurally cannot see | An active-query set carried on `Context`, mirroring `find_dependencies`' visited `stack` (`plan.rs:1688`). Small, self-contained |
| **B. Path guard + check-only graph call** | additionally, cycles routed through registered keyed assets | Requires splitting `register_scheduled_dependency` into check and register halves |
| **C. Plan-level + depth limit** | static chains (already covered by `find_dependencies`) | No new graph work, but a real dynamic cycle only surfaces as a depth-limit error, undiagnosed |

Recommendation **A**: it covers exactly the case the graph cannot, and the existing plan-level
detection already handles static recipe chains.

### 2. When the *parent* is payload-evaluated, is it registered as a dependent node?

The clarified rule names the dependency direction. The dependent direction has the same
key-identity problem: a payload-evaluated parent classified as `ScheduleNode::Keyed`/`Expression`
(`context.rs:391-397`) is named by a key that does not include its payload.

| Option | Rationale |
|---|---|
| **A. Symmetric — not a node in either direction** *(recommended)* | Same key-identity argument. An edge *from* a misidentified node is attributed to every payload instance sharing that key. Metadata dependency records (action #4) still kept |
| **B. Dependent side only** | Retains more graph edges for cycle detection — but under option 1A the path guard covers that, so the retained edges buy little |

### 3. How do keyed payload recipes become reachable? (see gap above)

| Option | Effect |
|---|---|
| **A. Payload-aware delegation** *(recommended)* | Forward payload through the `evaluate_recipe` delegation hop, plus `AssetManager::get_with_payload(key, payload)` and an `EnvRef` entry. Delivers D6's keyed-recipe benefit fully |
| **B. Bypass delegation only** | `evaluate_immediately` detects a pure-key query whose plan requires payload and evaluates in place. Smaller, but `manager.get` stays payload-blind, so nested keyed requests still drop it |
| **C. Document as unreachable** | Keyed payload recipes usable only as nested dependencies. No new API; D6's benefit half-delivered |

This is the largest remaining scope question — **A** grows Phase 2 beyond what is currently written,
**C** shrinks the feature.

### 4. Where is the missing-payload error raised?

| Option | Behavior |
|---|---|
| **A. Both plan and entry points** *(recommended)* | Plan records the requirement; each entry point checks whether a payload is in hand. Uniform coverage; entry points are the only places that know |
| **B. Entry points only** | Simpler, one site per path; top-level payload-less evaluation diagnosed slightly later |
| **C. Plan finalization only** | Earliest, but plan building cannot know whether the caller holds a payload — it can only reject known-unsatisfiable cases |
