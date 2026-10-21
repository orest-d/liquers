# Phase 1: High-Level Design — post-init-command-registration

## Purpose

A shared environment (`EnvRef`) cannot gain or replace a command. `liquers-web` and `liquers-py`
therefore build a whole new environment for every registration made after the first evaluation,
and they discard the in-memory asset cache each time. Commands should be registered into a running
environment. A change to a command should invalidate exactly the results built with it, and
evaluations already in flight should be left alone. This serves the interactive and
language-integration use cases in `reference/VISION.md`: a notebook or a page defines commands as
it goes.

## Problem Example

Verified at HEAD (18732dd). In Rust:

```rust
let envref = env.to_ref();                    // Environment::to_ref consumes `env`
envref.evaluate("hello").await?;              // cached
// Wanted: register `shout` now. There is no `&mut CommandRegistry` to call it on:
// `EnvRef(Arc<E>)` is shared with the asset manager (init_with_envref), so Arc::get_mut is None,
// and Environment::get_command_executor(&self) / get_command_metadata_registry(&self) return
// plain references, so an implementor cannot put the registry behind a lock.
```

In Python, `@env.command def shout(...)` after a first `env.evaluate(...)` marks the environment for
a rebuild (`reference/PYTHON_INTEGRATION.md` §Rebuild). The next use builds a new environment with
the same store, so every in-memory result is recomputed. `liquers-web` does the same
(`liquers-web/README.md`, the limitation row naming this issue).

Wanted: `envref.update_commands(|cr| { register_command!(cr, fn shout(state) -> result)?; Ok(()) }).await?`
makes `shout` available to the next evaluation. `hello` is still served from the cache. If `shout`
replaced an existing command, the results built with the old `shout` are expired.

## Scope and Acceptance Criteria

- **AC-1** A new command after sharing
  WHEN `update_commands` registers `shout` on a shared environment
  THEN the next `evaluate("shout")` runs it, and nothing had to be rebuilt
- **AC-2** The cache survives
  WHEN `hello` was evaluated before the update and is evaluated again after it
  THEN the second evaluation is served from the asset cache (the command is not re-run)
- **AC-3** A replaced command invalidates its results
  WHEN `update_commands` re-registers `hello` with a different `impl_version` or metadata
  THEN assets depending on `hello` are expired (cascade), and the next `evaluate("hello")` runs the
  new implementation
- **AC-4** A removed command invalidates its results
  WHEN `update_commands` unregisters `hello`
  THEN assets depending on it are expired, and `evaluate("hello")` reports it not registered
- **AC-5** In-flight evaluations are isolated
  WHEN an evaluation has already planned with the old registry when an update is published
  THEN it completes, and every step sees one consistent snapshot of the registry
- **AC-6** A failed update changes nothing
  WHEN the update closure returns `Err` (for example a bad query literal)
  THEN the published registry and every cached asset are unchanged, and the error is returned
- **AC-7** Unshared environments are unchanged
  WHEN an environment is configured and built as today (`EnvironmentBuilder`, `to_ref`)
  THEN registration through `&mut` works as before, and evaluation results are identical
- **AC-8** Both targets
  WHEN the same test runs on native and on wasm32
  THEN it behaves the same. No lock is held across an `.await`

Non-goals: switching `liquers-web` and `liquers-py` from rebuild to `update_commands` (follow-up
`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`); changing stores or recipe providers
after sharing; multi-realm dispatch (`CORE-MULTI-REALM-INTERPRETER`).

## Core Interactions, Crate Placement and Documentation Intent

Commands (`CommandRegistry` becomes cloneable and is published as snapshots), Assets (the existing
`refresh_command_versions_and_expire` hook), Environment (two accessors and one provided method).
All code is in `liquers-core` (`context.rs`, `commands.rs`), plus mechanical call-site fixes in the
crates that pass the registry as an argument. Reference: extend
`reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` and `reference/ASSETS.md`. Guide: extend
`guides/COMMAND_REGISTRATION_GUIDE.md`. `liquers-web/README.md` and
`reference/PYTHON_INTEGRATION.md` change only with the follow-up.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It changes two `Environment` trait method signatures and adds
  `pub` items in `liquers-core` (rules 3 and 4), and it is `L`.
- **Leading issue:** **Open design - trait shape:** change the two accessors to return a snapshot
  handle (recommended), or keep `&` and grow an append-only chain of registry generations.
- **Explanation:** a working mechanism is fully specified (Phase 2): copy-on-write snapshots and
  the existing version-refresh hook. What remains open is whether a breaking change to the
  `Environment` trait is acceptable for a P3 issue whose workaround (rebuild) already works.
- **Open questions:**
  1. **Open design - trait shape.** (a) *Snapshot handle* (recommended):
     `get_command_metadata_registry` and `get_command_executor` return `Snapshot<'_, T>`, which
     derefs to `T`. Method-call sites compile unchanged. About 20 sites that pass the result as an
     argument need `&*`, and the two `Environment` implementors change. (b) *Generations*:
     accessors keep `&T`, and each update appends a never-freed clone of the registry. This is
     non-breaking, but memory grows with every update, and lookup walks the chain. (c) *Not
     planned*: keep the rebuild and close the issue. Example of the incompatibility: under (b) a
     page that registers one command per route change keeps every past registry alive for the
     page's lifetime. Under (a) it keeps one, but an out-of-tree `Environment` implementor must
     change two methods.
  2. **Proposed resolution - consistency inside one evaluation.** Recommended: a step takes the
     snapshot current when it runs. A plan built from an older snapshot may then execute a command
     replaced since. The replacement also expires that asset (AC-3), so its result is not reused.
     The alternative, pinning a snapshot per asset, needs a field on `Context`. AC-5 is stated
     for the recommended reading.
  3. **Proposed resolution - removed commands.** Recommended: expire dependents of removed keys
     (AC-4). `refresh_command_versions` reports only *changed* versions, so the update helper
     computes the removed keys itself.

## Design Dependencies

- `pure-command-context` (in_review) — `overlaps` (weak): it changes `CommandRegistry::execute` /
  `execute_async` (a purity check). This design changes how the registry is reached, not those
  bodies. Either order; the second rebases.
- `global-enum-registration` (in_review) — `overlaps` (weak): `update_commands` refreshes metadata
  versions, which that design makes follow global enums. No ordering.
- `macro-argument-hints-and-query-literals` (in_review) — **overlaps**: its atomic registration is
  what makes AC-6 hold for a failing `register_command!` inside the closure. Without it the
  closure's clone may hold a half-registered command, which is still discarded with the clone.
  So it is not required.

## Consolidated Findings

- The hard constraint is `get_command_metadata_registry(&self) -> &CommandMetadataRegistry` (93
  call sites). The executor maps are already `Arc`'d. A reference into a lock cannot be returned,
  so either the signature changes or old registries are never freed (question 1).
- The dependency side needs no new code: `AssetManager::refresh_command_versions_and_expire` is
  documented as "the hook dynamic command registration needs".
- `GenericEnvironment::command_registry` (pub, ~390 uses, all before sharing) stays the pre-share
  registry. No `EnvRef` reader touches it at HEAD.
- The web/py rebuild also replays `unregister` and `import_metadata`. The follow-up must map each
  to `update_commands`.
