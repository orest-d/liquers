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

### Diagnostic surface: `AssetInfo`, `MetadataRecord`, `Metadata`

Phase 1 D8 deferred this "pending a consumer". Diagnostics **is** that consumer, so it is now in
scope. Each addition mirrors its `is_volatile` counterpart exactly.

```rust
// AssetInfo (metadata.rs:654-656) — note the existing field already carries a
// legacy-support serde(default) comment; follow the same pattern.
pub struct AssetInfo {
    pub is_volatile: bool,                        // unchanged
    #[serde(default)]
    pub payload_required: PayloadRequirement,     // new
}

// MetadataRecord (metadata.rs:816-820)
pub struct MetadataRecord {
    pub is_volatile: bool,                        // unchanged
    #[serde(default)]
    #[serde(skip_serializing_if = "PayloadRequirement::is_none")]
    pub payload_required: PayloadRequirement,     // new
}

impl MetadataRecord {
    /// Mirrors `is_volatile()` (metadata.rs:1246-1248).
    pub fn payload_required(&self) -> PayloadRequirement;
    /// Mirrors `set_volatile()` (metadata.rs:1261-1264).
    pub fn set_payload_required(&mut self) -> &mut Self;
}

impl Metadata {
    /// Mirrors `is_volatile()` (metadata.rs:2085-2101), including legacy-JSON extraction.
    pub fn payload_required(&self) -> PayloadRequirement;
}
```

**Simpler than `is_volatile` in one respect:** `MetadataRecord::is_volatile()` returns
`self.is_volatile || self.status == Status::Volatile`. There is no `Status::PayloadRequired`
(Phase 1 D7/D8), so `payload_required()` is a plain field read with no status disjunction.

**Wiring — the five sites that copy `is_volatile` must copy the new field too:**

| Site | Current line |
|---|---|
| `MetadataRecord::to_asset_info` | `is_volatile: self.is_volatile` (`metadata.rs:992`) |
| `MetadataRecord::from(asset_info)` | `metadata.is_volatile = asset_info.is_volatile` (`metadata.rs:746`) |
| `Plan::to_metadata_record` | `mr.is_volatile = self.is_volatile` (`plan.rs:1456`) |
| `Plan::update_metadata_record` | `mr.is_volatile = self.is_volatile` (`plan.rs:1496`) |
| `MetadataRecord` legacy-JSON load | `m.is_volatile = ...` (`metadata.rs:1348-1350`) |

Legacy JSON without the key must default to `None`, matching the existing `unwrap_or(false)`
treatment of `is_volatile`.

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
- `Step::GetAsset` / `GetAssetBinary` / `GetAssetMetadata` / `GetAssetRecipe(key)` → **`None`
  unconditionally.** Keys are a payload boundary (see below); a keyed asset is evaluated
  independently through the manager and never inherits a payload. This is where the traversal most
  sharply differs from `IsVolatile`, which *does* consult the manager for keyed steps
  (`interpreter.rs:477-482`)
- `Step::Evaluate(query)` / `Step::Plan(plan)` → delegate
- `Step::GetAssetDirectory` / `GetResourceDirectory` → `None` (they return `Ok(true)` for volatility,
  but a directory listing needs no payload)
- `GetResource` / `GetResourceMetadata` → `None`; these already carry an
  "ADD SUPPORT FOR RESOURCE VOLATILITY CHECK!" TODO (`interpreter.rs:487,492`) — do not extend that
  gap, just return `None`
- `UseQueryValue`, `Filename`, `Info`, `Warning`, `Error`, `SetCwd`, `UseKeyValue` → `None`

Because no arm consults the asset manager, this traversal is **cheaper than `IsVolatile`** — its only
async work is `make_plan` in the `Query` impl.

**Efficiency note:** this is a second async traversal alongside `IsVolatile`. Both short-circuit at
`Plan` (cached field), so the cost is the recipe-resolution path. If profiling later shows it
matters, the two can be consolidated into one traversal returning the derived `EvaluationClass`;
`IsVolatile` is `pub(crate)`, so that refactor has no external blast radius. Not done now —
mirroring keeps the change reviewable.

### Extended trait: `AssetManager<E>` (`assets.rs:2630-2714`)

**One** new method with a default implementation — no existing signature changes, so no implementor
breaks (rust-best-practices: extend, don't mutate). A second proposed method,
`payload_required(&Key)`, was dropped: keys are a payload boundary, so keyed steps never need to be
asked.

```rust
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

Both managers already route volatile queries to the fresh-unshared constructors, so no change is
needed there: because `payload ⟹ volatile`, a payload-requiring query is already resolved to a fresh
asset. Note that only `get_volatile_query_asset` is in play — `get_volatile_resource_asset` is the
keyed path, which payload never reaches.

### Preserving immediacy when a payload-evaluated asset depends on a payload-free one

The common shape is a payload-evaluated parent P depending on a payload-free child C. C is requested
from the asset manager normally — correct for caching and sharing, but it would seem to cost P's
immediacy by routing C through the job queue.

**Existing machinery already avoids this**, and no new work is needed:

- `Context::get_dependency_state` → `wait_for_dependency`, which "direct-claims the child if still
  runnable (**no queue slot consumed**), or subscribes" (`assets.rs:1768`).
- `Context::evaluate` → `evaluate_local_queue` → `AssetManager::drain_dependencies`, which claims and
  "inline-run[s] each still-runnable entry sequentially inside the caller's future"
  (`assets.rs:2692-2698`).

Both paths run a runnable dependency inline in the caller's future rather than waiting on a queue
slot. P therefore keeps its immediacy while C remains a normal cached, shared, registered asset.

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

### Decided: keyed recipes cannot require a payload — keys are a payload boundary

**Keys are global; a payload is per-evaluation.** A keyed recipe that required a payload would need a
*global* payload, which is not a thing this design provides. This is an accepted limitation, and it
supersedes the suggestion in Phase 1 D6 that keyed assets could carry payloads.

Consequences — all simplifying:

1. **`Step::GetAsset` / `GetAssetBinary` / `GetAssetMetadata` / `GetAssetRecipe` are a boundary.**
   They return `PayloadRequirement::None` unconditionally: a keyed asset is evaluated independently
   through the manager, payload-free. Payload requirement never propagates *through* a key.
2. **`AssetManager::payload_required(&Key)` is no longer needed** and is dropped from this design —
   one of the two proposed `AssetManager` additions disappears.
3. **A keyed recipe whose plan comes out `Required` is an error.** This answers the Phase 1 open
   question about keyed payload recipes (warning / error / silently accepted): it is an **error**,
   raised where the plan for a key is built.
4. **The `evaluate_recipe` delegation hop is correct as-is** (`assets.rs:1738-1776`). Its dropping of
   the payload for pure-key recipes is not a gap but the intended boundary. No change needed there.
5. **Payload-evaluated assets are therefore never keyed** — always ad-hoc or expression assets. This
   is what makes the dependent-side question below resolve itself.

### `liquers-core/src/plan.rs` — local detection

**Diagnostic reasoning in `init_steps`** — this costs nothing, because `mark_volatile` already has
exactly the required shape (`plan.rs:923-929`):

```rust
fn mark_volatile(&mut self, reason: &str) {
    if !self.is_volatile {          // fires once, on transition only
        self.is_volatile = true;
        self.plan.init_info(reason.to_string());
    }
}
```

The transition guard is what makes it satisfy "only if there *is* a command requiring payload": a
plan with no such command never calls it, so no message appears; a plan with several emits one
message naming the first cause, not one per step. `mark_payload_required` mirrors this exactly.

Message content should name the trigger so the diagnostic is actionable, e.g.
`"Payload required due to command 'get_user_id'"` and, for the link-parameter path,
`"Payload required due to link parameter to payload-requiring query: <query>"` — parallel to the
existing volatility messages.

```rust
impl<'c> PlanBuilder<'c> {
    /// Mark plan as payload-requiring and add explanatory Step::Info to init_steps.
    /// Mirrors `mark_volatile` (plan.rs:923-929), including the once-only transition guard.
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

Raised at **both** plan level and entry points: plan building records and can reject the requirement,
and each entry point additionally checks, since only it knows whether a payload is actually in hand.

| Scenario | Constructor | When |
|---|---|---|
| Keyed recipe whose plan requires payload | `Error::general_error(...)` | plan build for the key — invalid by construction, independent of any caller |
| Plan requires payload, context has none | `Error::general_error(...)` | `schedule_dependency_asset`, before scheduling |
| Same, at top level via `EnvRef::evaluate` | `Error::general_error(...)` | manager entry point |
| Payload→payload cycle | `Error::dependency_cycle(...)` | `Context` active-query set, on re-entry |
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
| `MetadataRecord::payload_required` | `#[serde(default)]` + `skip_serializing_if = "PayloadRequirement::is_none"` | **In scope** — diagnostics is the consumer Phase 1 D8 was waiting for. Legacy documents load as `None` |
| `AssetInfo::payload_required` | `#[serde(default)]` | Matches the existing legacy-support treatment of `AssetInfo::is_volatile` (`metadata.rs:654-656`) |

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
| liquers-core | `plan.rs` | `Plan` field, `PlanBuilder` field + 3 methods, `build()`, **plan splitting**, `to_metadata_record` + `update_metadata_record` |
| liquers-core | `metadata.rs` | `AssetInfo` + `MetadataRecord` fields, `payload_required()` / `set_payload_required()`, `Metadata::payload_required()`, legacy-JSON load, `to_asset_info` / `From<AssetInfo>` |
| liquers-core | `interpreter.rs` | `RequiresPayload` trait + 6 impls; pre-pass must leave payload-requiring queries to `do_step` (`interpreter.rs:85-86`) |
| liquers-core | `assets.rs` | 1 `AssetManager` default method; `DefaultAssetManager` override (queued) + `ImmediateAssetManager` override (new — it inherits the default today) |
| liquers-core | `context.rs` | routing switch in `schedule_dependency_asset`; active-query cycle guard; `apply`; `ImmediateEnvironmentWithPayload`; module rustdoc at `:76-80` |
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

## Addition discovered in Phase 3: `ImmediateEnvironmentWithPayload<V, P>`

There is currently **no environment pairing a payload with the inline asset manager**:
`SimpleEnvironmentWithPayload` (`context.rs:956`) uses `DefaultAssetManager`, and
`ImmediateEnvironment` (`context.rs:846`) has `Payload = ()`. `liquers-lib::DefaultEnvironment<V, P>`
does not help — its `SelectedAssetManager` is cfg-selected at compile time
(`environment.rs:20-22`), so a native build cannot choose the inline manager.

Since Phase 1 requires verification on **both** managers, `liquers-core` gains
`ImmediateEnvironmentWithPayload<V, P>`, mirroring `SimpleEnvironmentWithPayload` with
`type AssetManager = ImmediateAssetManager<Self>` and no spawning in `init_with_envref`. Mechanical,
and useful beyond this feature — it is the only way to exercise the wasm-compatible payload path
natively.

## Decided

- **`Context::apply` inherits payload** — via `apply_immediately` when required.
- **Dependency registration** — a payload asset may have dependencies and is cycle-checked, but is
  never a registered dependency (see the D1 clarification above).
- **Cycle detection** — an active-query path guard on `Context`.
- **Keyed recipes cannot require a payload.** Keys are global, payloads are per-evaluation; a global
  payload is not designed. Accepted limitation, enforced as an error at plan build for the key.
- **`Plan::payload_required` gets `#[serde(default)]`** although `is_volatile` deliberately does not.
  Backward compatibility with stored plans outweighs matching `is_volatile`'s strictness.
- **Missing-payload errors are raised at both plan level and entry points.**

No open questions remain for Phase 2.

### Cycle detection: a path guard on `Context`

Skipping the graph edge removes the site where `would_create_cycle` runs
(`dependencies.rs:415-445`), and since neither end of a payload→payload chain is a graph node, the
graph could never see such a cycle regardless.

`Context` carries an **active-query set**, mirroring the visited `stack` in `find_dependencies`
(`plan.rs:1688`): entering a payload-evaluated nested query pushes its query onto the set and fails
if already present. This covers exactly the case the graph structurally cannot, while the existing
plan-level detection continues to handle static recipe chains.

The set is shared across context clones (like `pending_dependencies`, `context.rs:328`) so it tracks
the evaluation path rather than a single action.

### Dependent-side registration: resolved by the keyed-recipe boundary

An earlier draft asked whether a payload-evaluated *parent* should be registered as a dependent node.
The keyed-recipe decision settles it, with no special case needed:

1. A payload-evaluated asset is **never keyed** (keyed recipes cannot require payload), so
   `current_key_opt` is always `None` and the classification at `context.rs:391-397` always yields
   `ScheduleNode::Expression` or `None` — never `Keyed`.
2. `ScheduleNode::Expression` is **already not a graph node**: "Only keyed assets are graph nodes; an
   expression is expanded onto its attribution set (the keyed assets that depend on it)"
   (`dependencies.rs:412-414`).
3. That attribution set is **provably empty** for a payload-requiring expression: a keyed asset
   depending on it would inherit the payload requirement and would therefore be a keyed recipe
   requiring payload — which is now an error.

So the existing code already does the right thing on the dependent side. Only the dependency-side
actions (#1 edge, #3 back-ref) need to be skipped.
