---
title: Environment, Context and Evaluation Reference
kind: reference
audience: internal
area: [core/context, core/plan]
reviewed: 2026-08-31
---
# DOC-04: Environment, Context, and End-to-End Evaluation

## Outcome

DOC-04 establishes an API-reference-level description of the runtime boundary
formed by `Environment`, `EnvRef`, and `Context`.

The primary reference is the module Rustdoc in
[`liquers-core/src/context.rs`](../../../liquers-core/src/context.rs). It now defines:

- The ownership and initialization relationship between `Environment` and `EnvRef`
- The services and associated types bound by `Environment`
- The end-to-end path from query evaluation to command execution
- The return-time difference between queued and inline environments
- The lifetime and sharing behavior of `Context`
- Dependency evaluation versus ad-hoc application
- Payload presence, cloning, injection, nested propagation, and keyed boundaries
- The current limited role of `Session` and `User`
- Native, inline, payload-bearing, and library environment choices
- Application-facing APIs versus framework lifecycle hooks

## Authority and sources

Claims were verified in this order:

1. [`liquers-core/src/context.rs`](../../../liquers-core/src/context.rs)
2. [`liquers-core/src/assets.rs`](../../../liquers-core/src/assets.rs)
3. [`liquers-core/src/interpreter.rs`](../../../liquers-core/src/interpreter.rs)
4. [`liquers-core/src/commands.rs`](../../../liquers-core/src/commands.rs)
5. [`liquers-lib/src/environment.rs`](../../../liquers-lib/src/environment.rs)
6. Core manager, dependency-scheduling, injection, expiration, and volatility tests
7. [`specs/reference/PAYLOAD_GUIDE.md`](../PAYLOAD_GUIDE.md),
   [`specs/reference/PROJECT_OVERVIEW.md`](../PROJECT_OVERVIEW.md), and
   [`specs/reference/ASSET_LIFECYCLE.md`](../ASSET_LIFECYCLE.md) as supplementary design and
   historical material

Source and executable tests take precedence where the supplementary documents
conflict with implementation.

## Concept inventory

| Concept | Primary API | Reference responsibility |
|---|---|---|
| Runtime type binding | `Environment` | Associated types and service/lifecycle hooks |
| Shared runtime handle | `EnvRef<E>` | Initialized environment access and top-level evaluation |
| Command context | `Context<E>` | Current asset, metadata, logging, progress, dependencies, payload |
| Identity | `User` | Minimal system, anonymous, or named identity |
| Session | `Session`, `SimpleSession` | Minimal user holder, not connected to evaluation |
| Value binding | `Environment::Value` | Concrete `ValueInterface` used by states and commands |
| Command binding | `Environment::CommandExecutor` | Concrete executor used by the interpreter |
| Payload binding | `Environment::Payload` | One payload type per environment; optional per context |
| Asset binding | `Environment::AssetManager` | Queued or inline execution model |
| Recipe binding | `get_recipe_provider` | Key-to-recipe resolution |
| Persistence binding | `get_async_store` | Asset persistence when `async_store` is enabled |
| Interpreter hook | `Environment::apply_recipe` | Plan finalization and application contract |
| Initialization hook | `Environment::init_with_envref` | Constructs, installs and starts the manager |
| Construction surface | `EnvironmentBuilder` | Recommended path; configures services and builds |

## Ownership and initialization

An environment is configured before it is shared, and is **ready to evaluate when
it becomes observable**. There is one readiness sequence, in
`Environment::try_to_ref`, and both construction paths run it:

```text
EnvironmentBuilder::build(self)          owned Environment
    -> resolve services                      |
    -> GenericEnvironment                    |
    \_________________________________ Environment::try_to_ref(mut self)
                                             -> CommandMetadataRegistry::refresh_metadata_versions()
                                             -> EnvRef<E>(Arc<E>)
                                             -> Environment::init_with_envref(envref)
                                                    -> construct AssetManager with the EnvRef
                                                    -> install it in the environment
                                                    -> AssetManager::start()
                                             -> EnvRef, ready to evaluate
```

The window in which the environment's manager slot is empty is entirely inside
`try_to_ref`, and no `EnvRef` escapes during it. That is the readiness guarantee,
and it is what `QUEUED-MANAGER-STARTUP-READINESS` was about: startup used to be a
detached task, so a caller could evaluate against a manager whose command versions
were not registered yet. The visible symptom was silent —
`AssetManager::register_plan_dependencies` skips a dependency whose version the
manager does not know, so a plan evaluated in that window registered no dependency
edges and nothing ever invalidated the assets built from it.

| Path | For | Failure |
|---|---|---|
| `EnvironmentBuilder::build` | applications and integrations; the recommended, documented path | `Result` |
| `Environment::try_to_ref` | an ad-hoc or hand-written `Environment` | `Result` |
| `Environment::to_ref` | the same, where the error cannot occur | panics |

`to_ref` and `try_to_ref` consume the environment, so configuration requiring
`&mut self` — command registration, store or provider selection — must happen
first. Before sharing, the sequence refreshes every command `metadata_version`,
finalizing metadata changed by registration helpers such as `register_command!`.
`build()` inherits that refresh by delegating rather than reimplementing the
sequence, so it cannot drift out of step.

`EnvRef::new` is **deprecated**. It performs only the `Arc` wrapping: no metadata
refresh, no `init_with_envref`, so the manager is never constructed or started. An
`EnvRef` from it is not safe to evaluate through, which is why the sequence above
is the only supported way to obtain one.

`init_with_envref` carries the whole obligation, and is the seam that lets one
generic `try_to_ref` body serve the built-in environments and a hand-written one
alike. It is fallible, and on return the manager must be constructed with the
supplied reference, installed, and started. A custom environment implements it with
the same deferred-slot shape the built-ins use:

```rust,ignore
fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
    let manager = Arc::new(ImmediateAssetManager::new(envref));
    let _ = self.asset_store.set(manager.clone());
    manager.start()
}
```

The manager is constructed *inside* the hook because it needs the `EnvRef` — that
is the construction cycle, and the environment's `OnceLock` manager slot is where
it is broken. The manager itself therefore has no unset state at all: its
environment reference is a constructor parameter, and the `set_envref` method and
its "environment not set" panic are gone.

`AssetManager::start` is synchronous and fallible. Its work is uncontended
in-memory map writes — at startup the version map is empty, so every key inserts
vacant and no expiration cascade can fire — and it touches no store. Synchronous
startup is what lets `to_ref` keep its existing signature while becoming correct.
`start` is idempotent; `refresh_command_versions` is the separate, re-runnable
operation for metadata changed after construction, and returns the dependency keys
whose version changed so a caller can cascade them.

**Synchronous does not mean runtime-free.** The queued manager spawns its job queue
and expiration monitor from its constructor, so `Queued` still requires an active
Tokio runtime. `Inline` spawns nothing and constructs with no reactor present,
which is what wasm needs.

## Environment types and the manager kind

`GenericEnvironment<V, P, K>` is the one built-in environment, parameterized by
value type, payload type and an **asset-manager kind**. The four previous structs
are type aliases of it, so every existing signature still names a real type:

| Alias | Kind | Payload |
|---|---|---|
| `SimpleEnvironment<V>` | `Queued` | `()` |
| `SimpleEnvironmentWithPayload<V, P>` | `Queued` | `P` |
| `ImmediateEnvironment<V>` | `Inline` | `()` |
| `ImmediateEnvironmentWithPayload<V, P>` | `Inline` | `P` |
| `liquers_lib::DefaultEnvironment<V, P>` | `DefaultKind` | `P` |

`AssetManagerKind` selects the execution model at compile time and carries the
manager as a generic associated type. It has to be a marker rather than the manager
type itself: the manager is parameterized by the environment, so naming it directly
produces an infinitely recursive type. `DefaultKind` is `Queued` natively and
`Inline` on wasm.

The kind is a type parameter and not a configuration value on purpose — two
branches of a match on `"queued"` / `"inline"` produce two different concrete
environment types, and `Environment` is not object-safe, so they cannot be erased
behind a `dyn`.

## Environment contract

`Environment` is a static generic integration boundary. It is `Sized`, has
associated types, and is not intended as `dyn Environment`.

| Associated type | Required role |
|---|---|
| `Value` | Concrete value representation for `State` and commands |
| `CommandExecutor` | Executes action steps for this same environment type |
| `SessionType` | Result of `create_session`; currently outside evaluation |
| `Payload` | Cloneable target-appropriate payload type |
| `AssetManager` | Asset manager specialized for this environment |

Service accessors must return the mutually compatible registry, executor, manager,
recipe provider, and store belonging to the same runtime.

`apply_recipe` is the environment's interpreter hook. The built-in implementations:

1. Convert the recipe to a plan using the environment's command metadata.
2. Finalize static dependencies, volatility, and dependency expiration.
3. Combine plan and recipe expiration.
4. Apply the combined expiration through the context.
5. Execute the plan with `apply_plan`.

The trait signature does not enforce those steps. A custom implementation that
omits them changes dependency, volatility, expiration, metadata, or command
semantics.

`init_with_envref` is the other lifecycle hook, and unlike `apply_recipe` its
contract *is* enforced in one respect: it is fallible, so an implementation that
cannot produce a started manager has somewhere to say so rather than leaving the
caller with an unusable reference. What it must do — construct the manager with the
supplied reference, install it, start it — is still a protocol the trait describes
rather than checks. See §Ownership and initialization.

## Top-level evaluation

| Entry point | Input state | Payload | Return-time contract |
|---|---|---|---|
| `EnvRef::evaluate` | Manager-created empty state | None | Queued manager may return after submission; inline manager returns after evaluation |
| `EnvRef::evaluate_immediately` | Empty state | Required | Evaluation finishes before return |
| `EnvRef::apply_recipe` | Caller supplied | Already in `Context` | Framework hook; delegates directly to `Environment::apply_recipe` |

Both public evaluation methods parse any `TryToQuery` input before calling the
asset manager. They return `AssetRef`, not `State`. Under queued execution,
`AssetRef::get` is the normal waiting operation.

`evaluate_immediately` delegates to `AssetManager::apply_immediately`, creates an
ad-hoc asset, and does not persist the produced value. It is not merely a queued
evaluation followed by a wait.

## Context lifetime and sharing

A normal context is created by `AssetRef::create_context` for one asset
evaluation. `apply_plan` clones it for each plan step and commands receive those
clones.

| Context component | Clone behavior |
|---|---|
| Current `AssetRef` | Same shared asset |
| Environment | Same `Arc<E>` |
| Current working key | Shared `Arc<Mutex<Option<Key>>>` |
| Asset service sender | Sender clone to the same channel |
| Pending dependencies | Shared async mutex and vector |
| Payload | `E::Payload::clone`; not a shared cell unless the payload provides interior sharing |
| Volatility flag | Copied boolean |

Therefore `set_cwd_key`, logs, progress, and dependency additions affect the
shared evaluation context. Replacing `payload` or calling `set_payload` on one
context clone does not replace the payload stored by other clones. A payload can
deliberately contain `Arc`, mutexes, or other interior-shared application state.

`clone_context` is behaviorally equivalent to `Clone::clone`, despite being an
async method.

## Working-key and relative-resolution contract

The working key is a Liquers `Key`, not a filesystem directory. `Context` owns it,
and `Context::get_cwd_key` / `set_cwd_key` are **crate-private**: the working key
is framework state, not a value a command may read or move.

### Why the working key is not part of the command-facing API

The reason is *identity*, not resolution. Since freezing (DOC-08), every operand a
command receives through its plan is already absolute, so nothing needs the live
key to resolve. What remains is what a command could do *with* it, and both routes
break the asset model:

- **Reading it.** A command that varies its result by directory produces a value
  that its query does not describe. Two directories then share one query text, one
  `DependencyKey` and one cache entry for results that legitimately differ.
- **Moving it.** A command that installed a new working key mid-plan would
  invalidate any ahead-of-time dependency pre-pass, because an opaque action could
  change the cursor after analysis had already walked past it.

Nothing marks which commands read the directory, so the alternative to closing
these routes is to carry a CWD in *every* query. That is sound but wasteful: it
multiplies cache entries per folder for the majority of queries that never consult
one, and it defeats the sharing that makes a large input feeding many analyses a
single asset.

The supported replacement is a `-R-key/.` **link argument**, which delivers the
directory as data: explicit in the query, overridable per call, visible to the
planner, and part of the identity of the result it affects. See the Command
Registration Guide, "Passing the working directory (or any relative query) into a
command".

### Relative queries are refused at the command boundary

`Context::evaluate`, `Context::get_dependency_state` and `Context::apply` reject a
query carrying a CWD-relative resource operand, recursively including link
parameters, with `ErrorType::NotSupported`. The error names the offending segment's
position and points at `-R-key/.`.

The test is **operand form**, not `Query::absolute`. A query with no key operand at
all — `greet-Hello` — means the same thing in every directory and stays valid. A
command that needs a sibling takes the directory as a link argument and builds an
absolute query from it.

### Ordered resolution during execution

The interpreter still installs `SetCwd` in order, and a nested `Step::Plan` shares
the same context so its final key remains active after control returns. After
freezing, those steps are provenance rather than a dependency of any operand: the
operands they once governed are already absolute.

Linked queries remain independently scoped. A link starts from the enclosing
scope's key, but a `cwd` instruction inside it does not modify its parent or a
sibling link. Diagnostics are deliberately *not* scoped the same way: a link that
falls back to logical root still owes the caller its one warning, so freezing
merges that flag out of the link scope without merging the key.

When a relative operand is resolved with no working key installed, the context
atomically installs the empty logical root and emits this warning exactly once
across all its clones:

```text
Relative key/query has no CWD; using logical root '/'.
```

Freezing reports whether that fallback was *used* rather than installing it
eagerly, so a plan with no relative operand stays silent.

Resolved identities are used consistently for dependency records, cycle checks,
manager lookup, and cache reuse. Thus `./input.txt` under `a/c` is tracked and
cached as `a/c/input.txt`, not under its raw spelling or a sibling CWD. Conversely
an operand that was already absolute is returned unchanged, so `-R/data/big.csv`
referenced from many directories remains **one** asset.

A Context is registered as a keyed dependency owner only when its asset's immutable
construction-time query yields the same key as the current recipe and a
non-evaluating manager lookup returns that exact `AssetRef` id. Temporary, ad-hoc,
volatile/evicted, provider-mismatched, and differently owned assets are not treated
as keyed owners.

## Dependency and apply methods

| Context method | Schedules | Waits | Records dependency | Payload behavior |
|---|---:|---:|---:|---|
| `evaluate` | Yes | No direct state wait; may inline-run locally queued work | Yes | Inherits only when the nested plan requires it |
| `get_dependency_state` | Yes | Yes | Yes | Inherits only when the nested plan requires it |
| `apply` | According to manager mode | According to manager mode | No | Inherits for payload-required plans and evaluates them immediately |
| `evaluate_local_queue` | Previously scheduled local dependencies | Runs locally queued work; does not wait for work already running elsewhere | Records were created during scheduling | N/A |

`Context::evaluate` does more than `EnvRef::evaluate`: it cycle-checks and
registers the parent-child scheduling edge, captures the dependency asset, records
the observed version, and drains the local dependency queue. It still returns an
`AssetRef`.

`get_dependency_state` is the direct schedule-and-wait operation used by the
interpreter for linked and resource dependencies.

`Context::apply` is an ad-hoc transformation of a supplied state. It does not
record a dependency. For a payload-required plan it forwards the current payload
and uses immediate application; other plans follow the manager's ordinary mode.

Pending dependency methods are public, but are chiefly finalization primitives.
`take_pending_dependencies` is destructive: it clears the shared collection.

## Payload contract

Each environment chooses exactly one `Payload` type satisfying `PayloadType`.
Each context stores `Option<E::Payload>`, so the associated type does not imply
that a payload is present.

`EnvRef::evaluate_immediately` always supplies a payload. The asset installs it on
the context before recipe evaluation. Context clones used by actions in that
evaluation receive payload clones, which supports direct context access and
`InjectedFromContext` parameters.

Ordinary top-level `EnvRef::evaluate` and keyed manager reads do not supply a
payload. During an evaluation that has one, `Context::evaluate`,
`get_dependency_state`, and `apply` inspect the nested plan's
`payload_required` field. A required payload is forwarded and the nested asset is
evaluated inline; a missing payload is an error. Payload-required evaluation is
volatile, unshared, and not persisted.

Keys are a payload boundary because keyed assets have global identity while a
payload belongs to one evaluation. A keyed recipe that requires a payload is
rejected. Payload-evaluated dependency chains use path-based cycle detection
instead of registering payload-specific assets in the shared dependency graph.

## Logging, progress, metadata, and outcome

`progress`, `secondary_progress`, and `add_log_entry` send messages to the current
asset's service channel. `debug`, `info`, `warning`, and `error` also write to
stderr before sending a structured log entry.

`Context::error` only records an error-level log message. It does not fail the
asset. `Context::set_error` changes the asset outcome to `Error`.

`set_filename` mutates current metadata directly. `set_expires` updates metadata
expiration and the asset's resolved deadline. Both are public today, although
`set_expires` exists primarily so environment implementations can complete the
`apply_recipe` protocol.

`get_metadata` returns only `MetadataRecord`; it errors if the asset contains
legacy JSON metadata.

## Built-in environment comparison

| Type | Target | Payload | Manager | Recipe-provider fallback | Store configuration |
|---|---|---|---|---|---|
| `SimpleEnvironment<V>` | Native only | `()` | Queued `DefaultAssetManager` | `TrivialRecipeProvider` with stderr notice | Async store; legacy sync setter |
| `ImmediateEnvironment<V>` | Native or Wasm | `()` | Inline `ImmediateAssetManager` | `TrivialRecipeProvider` | Async store |
| `SimpleEnvironmentWithPayload<V, P>` | Native only | `P` | Queued `DefaultAssetManager` | `TrivialRecipeProvider` with stderr notice | Async store; legacy sync setter |
| `ImmediateEnvironmentWithPayload<V, P>` | Native or Wasm | `P` | Inline `ImmediateAssetManager` | `TrivialRecipeProvider` | Async store |
| `liquers_lib::DefaultEnvironment<V, P>` | Native or Wasm | `P` | Queued natively, inline on Wasm | Configured provider; defaults to `DefaultRecipeProvider` | Async store |

`SimpleEnvironment::with_cache` and
`SimpleEnvironmentWithPayload::with_cache` always panic.

The synchronous `with_store` setters update fields that are not exposed by the
current `Environment` trait or used by the asset manager. `with_async_store` is the
effective persistence configuration.

## Public versus framework APIs

Preferred application-facing APIs:

- Environment constructors and supported provider/store configuration
- Command registration before `to_ref`
- `Environment::to_ref`
- `EnvRef::evaluate` and `evaluate_immediately`
- Read-only service access through `EnvRef`
- Command-facing `Context` payload, metadata, log, progress, dependency, and
  asset/environment access. **Not** the working key: `get_cwd_key` and
  `set_cwd_key` are crate-private, and `evaluate`/`apply`/`get_dependency_state`
  require absolute queries

Framework extension and lifecycle APIs:

- Implementing `Environment`
- `EnvRef::new` and `EnvRef::apply_recipe`
- `Context::new`
- `Environment::apply_recipe` and `init_with_envref`
- `Context::evaluate_local_queue`, `take_pending_dependencies`, `add_dependency`,
  `set_expires`, and `set_error`

Visibility does not consistently enforce this separation.

## Conflicts and unresolved gaps

| Priority | Gap | Evidence and impact | Recommended action |
|---:|---|---|---|
| P0 | Custom `Environment::apply_recipe` semantics are convention-only | Dependency finalization, volatility, expiration, and plan application are manually duplicated by every environment | Provide a shared default helper or default method and reserve customization for narrower hooks |
| P1 | `Session` and `User` imply an evaluation hierarchy that is not implemented | `create_session` has no callers in the runtime, and `Context` contains no session or user | Mark them experimental/minimal until authorization/session propagation is designed |
| P3 | Recipe-provider absence diagnostics are not uniform | Native queued core environments write a stderr notice when falling back to trivial recipes; immediate environments stay silent, and `liquers_lib::DefaultEnvironment` has a default provider | Decide whether provider absence should be quiet, logged, or impossible by construction in the future environment builder |
| P1 | Public context lifecycle methods can break finalization invariants | `take_pending_dependencies` clears records; `set_error` and `set_expires` directly affect the asset | Narrow visibility or split command-facing and engine-facing context traits |
| P1 | Payload mutability semantics are easy to misread | `payload` is public and cloned by value, while guides describe it as mutable/inherited | Document clone semantics and prefer accessors or an explicit shared payload wrapper |
| P2 | Synchronous store and cache configuration APIs are nonfunctional | `with_store` is unused by asset evaluation; `with_cache` always panics | Remove, deprecate, or make them operational |
| P2 | `clone_context` is redundant and unnecessarily async | It performs the same field clones as `Clone::clone` and awaits nothing | Deprecate it in favor of `Clone` |
| P2 | Context convenience logging always writes to stderr | `debug`, `info`, `warning`, and `error` both print and enqueue structured logs | Route console output through configurable logging instead of unconditional side effects |

## Coding-agent and human-developer impact

The improved reference should prevent:

- Constructing `EnvRef::new` (deprecated) and then evaluating: the manager is never
  constructed or started
- Registering commands or selecting stores after `to_ref` consumes the environment
- Reading final command `metadata_version` before construction refreshes registrations
- Implementing `init_with_envref` without starting the manager, which reintroduces
  `QUEUED-MANAGER-STARTUP-READINESS` for that environment
- Assuming `EnvRef::evaluate(...).await` always returns a ready value
- Building a native queued environment outside a Tokio runtime
- Expecting payload inheritance through keyed assets, which deliberately form a payload boundary
- Treating `Context::error` as an asset failure
- Treating `Context::apply` as dependency-tracked evaluation
- Mutating one context clone's payload and expecting other clones to see replacement
- Selecting `SimpleEnvironmentWithPayload` for Wasm
- Configuring `with_store` or `with_cache` and assuming the asset manager uses it

For coding agents, these distinctions determine correct type selection,
initialization order, waiting behavior, and generated command code. For human
developers, they make the public surface understandable without reading asset and
interpreter internals together.

## Verification

The following executable evidence covers this reference:

- Queued and inline manager-parametric evaluation tests
- Dependency scheduling and cycle tests
- Payload and injected-parameter tests
- Payload inheritance, missing-payload, keyed-boundary, and payload-cycle tests
- Context volatility propagation tests
- Expiration and asset finalization tests
- Ordered CWD, nested scope, absolute outer-query, root-warning, and concurrent
  context-clone tests

Review verification on 2026-08-09:

- `cargo test -p liquers-core --lib`: 446 passed
- `cargo test -p liquers-core --doc`: 5 passed, 2 intentionally ignored
- `cargo doc -p liquers-core --no-deps`: completed with three known private-item
  link warnings
- All relative Markdown links in `specs/reference/api/` resolve
- `git diff --check` passes

The earlier DOC-04 completion also passed
`cargo check --target wasm32-unknown-unknown -p liquers-core`.

The test build still reports existing compiler warnings, including the public
`AssetManager::dependency_manager`/private `DependencyManager` mismatch already
tracked by DOC-03. No new compiler warning was introduced by DOC-04.

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-31 | Replaced the initialization sequence with `try_to_ref`'s and documented `EnvironmentBuilder` as the recommended construction path, `init_with_envref`'s strengthened contract, synchronous fallible manager startup, and `GenericEnvironment` with its four aliases and the asset-manager kind. Retired the P0 `EnvRef::new` and P1 unobservable-startup gap rows. | `design/environment-builder/phase-5` |
| 2026-08-31 | Documented that `Environment::to_ref` refreshes command metadata versions before sharing and that `EnvRef::new` bypasses that lifecycle step. | `design/refresh-command-metadata-versions/phase-5` |
| 2026-08-30 | Updated built-in environment recipe-provider fallback behavior after `SimpleEnvironmentWithPayload` stopped panicking and corrected the already-fixed `liquers_lib::DefaultEnvironment` default-provider row. | PAYLOAD-ENV-RECIPE-PROVIDER-FALLBACK |
| 2026-08-16 | Recorded that the working key is crate-private and why, that `evaluate`/`apply`/`get_dependency_state` refuse relative queries, and that `-R-key/.` is the supported replacement. Restated ordered resolution for frozen plans. | PLAN-CWD-FREEZE |
| 2026-08-11 | Documented the shared live CWD, interpreter and context resolution boundaries, scoped links, nested-plan propagation, root fallback, and resolved dependency and owner identity. | phase-5 |
| 2026-08-09 | Reviewed environment construction, context sharing, dependency evaluation, and payload propagation against HEAD; documented payload-aware nested evaluation and the inline payload environment, and corrected links. | PAYLOAD-INHERITANCE |
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
