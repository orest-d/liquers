---
title: Environment, Context and Evaluation Reference
kind: reference
audience: internal
area: [core/context, core/plan]
reviewed: 2026-03-02
---
# DOC-04: Environment, Context, and End-to-End Evaluation

Status: Complete
Last reviewed: 2026-07-26

## Outcome

DOC-04 establishes an API-reference-level description of the runtime boundary
formed by `Environment`, `EnvRef`, and `Context`.

The primary reference is the module Rustdoc in
[`liquers-core/src/context.rs`](../../liquers-core/src/context.rs). It now defines:

- The ownership and initialization relationship between `Environment` and `EnvRef`
- The services and associated types bound by `Environment`
- The end-to-end path from query evaluation to command execution
- The return-time difference between queued and inline environments
- The lifetime and sharing behavior of `Context`
- Dependency evaluation versus ad-hoc application
- Payload presence, cloning, injection, and non-inheritance by nested assets
- The current limited role of `Session` and `User`
- Native, inline, payload-bearing, and library environment choices
- Application-facing APIs versus framework lifecycle hooks

## Authority and sources

Claims were verified in this order:

1. [`liquers-core/src/context.rs`](../../liquers-core/src/context.rs)
2. [`liquers-core/src/assets.rs`](../../liquers-core/src/assets.rs)
3. [`liquers-core/src/interpreter.rs`](../../liquers-core/src/interpreter.rs)
4. [`liquers-core/src/commands.rs`](../../liquers-core/src/commands.rs)
5. [`liquers-lib/src/environment.rs`](../../liquers-lib/src/environment.rs)
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
| Initialization hook | `Environment::init_with_envref` | Manager back-reference and startup |

## Ownership and initialization

An environment is configured before it is shared:

```text
owned Environment
    -> Environment::to_ref(self)
    -> EnvRef<E>(Arc<E>)
    -> Environment::init_with_envref(envref)
    -> initialized AssetManager back-reference/startup
```

`Environment::to_ref` consumes the environment. Further configuration that
requires `&mut self`, such as command registration or store/provider selection,
must therefore happen first.

`EnvRef::new` only performs the `Arc` wrapping and does not call
`init_with_envref`. It is the low-level operation used by `to_ref`, not a
behaviorally equivalent constructor. Evaluation through an uninitialized manager
can panic when the manager requests its missing environment back-reference.

The built-in native queued environments construct `DefaultAssetManager`, which
spawns its job queue and expiration monitor during manager construction. Their
`new` methods consequently require an active Tokio runtime. Their initialization
hook installs the environment reference and spawns `AssetManager::start`.

`ImmediateEnvironment` constructs `ImmediateAssetManager` without spawning. Its
initialization hook installs the back-reference, and manager startup occurs lazily
on first evaluation.

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

`init_with_envref` is the other lifecycle hook. It must at least install the
manager's environment back-reference and arrange startup. The trait also does not
enforce or provide a default for this protocol.

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

## Dependency and apply methods

| Context method | Schedules | Waits | Records dependency | Inherits payload |
|---|---:|---:|---:|---:|
| `evaluate` | Yes | No direct state wait; may inline-run locally queued work | Yes | No |
| `get_dependency_state` | Yes | Yes | Yes | No |
| `apply` | According to manager mode | According to manager mode | No | No |
| `evaluate_local_queue` | Previously scheduled local dependencies | Runs locally queued work; does not wait for work already running elsewhere | Records were created during scheduling | N/A |

`Context::evaluate` does more than `EnvRef::evaluate`: it cycle-checks and
registers the parent-child scheduling edge, captures the dependency asset, records
the observed version, and drains the local dependency queue. It still returns an
`AssetRef`.

`get_dependency_state` is the direct schedule-and-wait operation used by the
interpreter for linked and resource dependencies.

`Context::apply` is an ad-hoc transformation of a supplied state. It neither
records a dependency nor forwards the current payload.

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

Ordinary `EnvRef::evaluate`, manager `get`/`get_asset`, and `Context::evaluate`
create nested assets without a payload. The existing
`test_payload_not_inherited_in_nested_evaluation` verifies that a child command
requiring injected payload fails when reached through `Context::evaluate`.

This directly contradicts the inheritance claim in `PAYLOAD_GUIDE.md` and the
context hierarchy in `PROJECT_OVERVIEW.md`.

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
| `SimpleEnvironmentWithPayload<V, P>` | Native only | `P` | Queued `DefaultAssetManager` | Panics if missing | Async store; legacy sync setter |
| `liquers_lib::DefaultEnvironment<V, P>` | Native or Wasm | `P` | Queued natively, inline on Wasm | Panics if missing | Async store |

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
- Command-facing `Context` payload, metadata, log, progress, working-key,
  dependency, and asset/environment access

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
| P0 | Payload documentation promises nested inheritance that does not exist | `PAYLOAD_GUIDE.md` and `PROJECT_OVERVIEW.md` claim inheritance; `Context::evaluate` has no payload path and the injection test explicitly verifies non-inheritance | Correct the guides now; separately decide whether inheritance should be implemented |
| P0 | `EnvRef::new` creates an evaluation-unsafe uninitialized reference | It is public and looks like a normal constructor but skips `init_with_envref`; manager environment access can panic | Make it private or explicitly unsafe-by-protocol, or introduce a distinct initialized constructor/state |
| P0 | Custom `Environment::apply_recipe` semantics are convention-only | Dependency finalization, volatility, expiration, and plan application are manually duplicated by every environment | Provide a shared default helper or default method and reserve customization for narrower hooks |
| P1 | Manager startup completion is not observable for queued environments | `init_with_envref` spawns `start` and immediately returns; command-version loading can still be in flight | Make initialization async or make first evaluation await idempotent startup |
| P1 | `Session` and `User` imply an evaluation hierarchy that is not implemented | `create_session` has no callers in the runtime, and `Context` contains no session or user | Mark them experimental/minimal until authorization/session propagation is designed |
| P1 | Payload availability differs across environment choices without one comparison reference | Core has no inline payload environment; Wasm payload use relies on `liquers_lib::DefaultEnvironment` | Keep the comparison table public and consider a core `ImmediateEnvironmentWithPayload` or generic core environment |
| P1 | Recipe-provider absence has inconsistent behavior | Core unit-payload environments fall back to trivial recipes; payload and library environments panic | Define one absence/error contract and use a fallible provider lookup if absence is valid |
| P1 | Public context lifecycle methods can break finalization invariants | `take_pending_dependencies` clears records; `set_error` and `set_expires` directly affect the asset | Narrow visibility or split command-facing and engine-facing context traits |
| P1 | Payload mutability semantics are easy to misread | `payload` is public and cloned by value, while guides describe it as mutable/inherited | Document clone semantics and prefer accessors or an explicit shared payload wrapper |
| P2 | Synchronous store and cache configuration APIs are nonfunctional | `with_store` is unused by asset evaluation; `with_cache` always panics | Remove, deprecate, or make them operational |
| P2 | `clone_context` is redundant and unnecessarily async | It performs the same field clones as `Clone::clone` and awaits nothing | Deprecate it in favor of `Clone` |
| P2 | Context convenience logging always writes to stderr | `debug`, `info`, `warning`, and `error` both print and enqueue structured logs | Route console output through configurable logging instead of unconditional side effects |

## Coding-agent and human-developer impact

The improved reference should prevent:

- Constructing `EnvRef::new` and then evaluating without manager initialization
- Registering commands or selecting stores after `to_ref` consumes the environment
- Assuming `EnvRef::evaluate(...).await` always returns a ready value
- Building a native queued environment outside a Tokio runtime
- Expecting immediate-evaluation payloads in nested assets
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
- Explicit nested-payload non-inheritance test
- Context volatility propagation tests
- Expiration and asset finalization tests

Final verification:

- `cargo test -p liquers-core`: 401 executable tests passed; 2 doctests passed
  and 2 doctests were ignored
- `cargo doc -p liquers-core --no-deps`: passed without Rustdoc warnings
- `cargo check --target wasm32-unknown-unknown -p liquers-core`: passed
- All local Markdown link targets in this analysis and the tracker exist
- `git diff --check` passed for the DOC-04 files

The test build still reports existing compiler warnings, including the public
`AssetManager::dependency_manager`/private `DependencyManager` mismatch already
tracked by DOC-03. No new compiler warning was introduced by DOC-04.

## History

| Date | Change | Source |
|---|---|---|
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
