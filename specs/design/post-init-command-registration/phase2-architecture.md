# Phase 2: Solution & Architecture — post-init-command-registration

## Overview

Copy-on-write registry snapshots. Before sharing, nothing changes: the environment owns its
`CommandRegistry`, and `&mut` registration works as today. After sharing, `update_commands` clones
the current registry, applies the caller's closure to the clone, refreshes metadata versions and
publishes the clone as the new snapshot. Readers take the snapshot current at the call. The
caller-facing async helper on `EnvRef` then calls the asset manager's version refresh and expires
the dependents of removed commands. Written under question 1 (a).

Rejected: interior mutability of `CommandRegistry` itself (its maps and the metadata registry are
`pub` fields read in ~390 places, and readers would need a guard per access); a registration hook
with no snapshot (the plan and the executor could see different registries); generations (question
1 b); per-asset pinning (question 2).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | snapshots hold executors, not the `EnvRef`, so no new cycle | No | No | none |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` (`pure-command-context`) | draft | P2 | touches `CommandRegistry::execute*` bodies | No | No | rebase |
| `MACRO-QUERY-VALIDATION-AND-HINTS` | accepted | P3 | atomic `register_command!` (AC-6 strength) | No | No | none |

## Interfaces

```rust
// liquers-core/src/context.rs
/// A read handle on a registry an environment may replace after it is shared.
pub enum Snapshot<'a, T> { Borrowed(&'a T), Shared(Arc<T>) }
impl<T> std::ops::Deref for Snapshot<'_, T> { type Target = T; /* both arms */ }

pub trait Environment {
    // changed: was `-> &CommandMetadataRegistry`
    fn get_command_metadata_registry(&self) -> Snapshot<'_, CommandMetadataRegistry>;
    // changed: was `-> &Self::CommandExecutor`
    fn get_command_executor(&self) -> Snapshot<'_, Self::CommandExecutor>;
    /// Applies `update` to a copy of the current registry and publishes it. Sync; never awaits.
    /// Default: `Err(Error::not_supported(..))` for implementors that cannot.
    fn update_command_registry(
        &self,
        update: Box<dyn FnOnce(&mut Self::CommandExecutor) -> Result<(), Error> + '_>,
    ) -> Result<RegistryUpdate, Error> { /* not supported */ }
}

/// What an update changed, for the asset manager.
pub struct RegistryUpdate { pub removed: Vec<CommandKey> }

impl<E: Environment> EnvRef<E> {
    /// `update_command_registry`, then `refresh_command_versions_and_expire`, then cascade-expire
    /// the dependents of `removed` (both `for_command_metadata` and `for_command_implementation`).
    pub async fn update_commands(
        &self,
        update: impl FnOnce(&mut E::CommandExecutor) -> Result<(), Error>,
    ) -> Result<(), Error>;
}
```

`EnvRef::get_command_metadata_registry` / `get_command_executor` (`context.rs` ≈325) return the
same `Snapshot`. `liquers-core/src/commands.rs`: `impl<E: Environment> Clone for
CommandRegistry<E>` by hand (the `Arc`'d executor maps, `command_metadata_registry.clone()`,
`store_config.clone()`; `E` itself need not be `Clone`).

`GenericEnvironment` (`context.rs` ≈1500) gains private fields:

```rust
published: std::sync::RwLock<Option<Published<Self>>>,   // None until the first update
update_lock: std::sync::Mutex<()>,                        // serialises updates
struct Published<E> { executor: Arc<CommandRegistry<E>>, metadata: Arc<CommandMetadataRegistry> }
```

The accessors read `published` under a read lock and clone the `Arc`s, or return
`Snapshot::Borrowed(&self.command_registry…)` when it is `None`. `update_command_registry` takes
`update_lock`, clones the current registry, runs `update`, calls `refresh_metadata_versions` on the
clone, computes `removed` (keys in the old metadata and not the new), and swaps `published` under a
write lock. On `Err` it returns before swapping (AC-6). A poisoned lock maps to
`Error::unexpected_error`.

Sync/async: the registry work is synchronous and holds a std lock only for a pointer swap or clone,
never across `.await` (AC-8). `update_commands` is async because the version refresh and the cascade
are.

## Integration Points

- `liquers-core/src/context.rs`: `Snapshot`, `RegistryUpdate`, trait changes, `EnvRef` methods,
  `GenericEnvironment` fields, `new`, accessors, `update_command_registry`.
- `liquers-core/src/commands.rs`: `Clone for CommandRegistry`.
- `liquers-core/src/{assets.rs, interpreter.rs, plan.rs, recipes.rs, environment_builder.rs}`: call
  sites that pass the registry as an argument get `&*` or a local binding (≈10).
- `liquers-core/tests/environment_builder.rs` `CustomEnvironment`: the two accessors.
- Other crates, argument-passing sites only: `liquers-axum/src/assets/{common,websocket}.rs`,
  `liquers-mcp/src/server.rs`, `liquers-agent/src/lib.rs`, `liquers-web/src/environment.rs`,
  `liquers-py/src/environment.rs`, `liquers-records/src/provider.rs`, `liquers-lib/src/*`, and the
  tests that do the same. Method-call sites compile unchanged through `Deref`.

## Error Handling

- Update on an environment that cannot: `Error::not_supported("…cannot update commands after
  sharing")`.
- Closure error: returned unchanged. Nothing is published.
- Lock poisoning: `Error::unexpected_error`.
- Cascade failure: propagated from `cascade_expire_dependents`. The registry is already published,
  so the next `update_commands` or `refresh_command_versions_and_expire` retries the refresh.

## Relevant Commands

None added. Any command can be registered through the closure, including with `register_command!`.

## Documentation Architecture

- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (reference, internal,
  core/context): §sharing — snapshots, `update_commands`, isolation rule (question 2).
- `specs/reference/ASSETS.md` (reference): version refresh and removal expiry on update.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` (guide): "Registering after the environment is
  shared".
- `affects_docs`: the three above. No `specs/README.md` link changes.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `context.rs`, `commands.rs`, ≈20 argument-passing call sites in 9 crates |
| Crates and workflows affected | every crate implementing or calling `Environment`; only `GenericEnvironment` and one test env implement it |
| Existing tests likely to change | none semantically; a few compile fixes (`&*`) |
| New validation | Phase 3 tests in `liquers-core/tests/post_share_registration.rs`; `scripts/check-build-matrix.sh` (wasm32 rows) |
| Compatibility | breaking for out-of-tree `Environment` implementors (two signatures). Data formats unchanged |
| Concurrency | updates serialised by a mutex. Readers never block on an update longer than an `Arc` clone |
| Performance | a read lock plus `Arc` clone per accessor call after the first update. Before any update, a branch on `None` |
| Recovery | revert. No persisted data depends on it |
| Certainty | high for the mechanism; question 1 decides the shape |
