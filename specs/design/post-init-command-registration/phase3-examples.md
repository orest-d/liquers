# Phase 3: Examples & Use-cases — post-init-command-registration

Written under question 1 (a) and the recommended answers to questions 2 and 3.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Register after sharing | `update_commands` in an application | AC-1, AC-2 |
| 2 | Example | Replace a command | results built with the old one expire | AC-3 |
| 3 | Test | `post_share_registration::new_command_after_sharing_runs` | | AC-1 |
| 4 | Test | `post_share_registration::cached_result_survives_an_update` | | AC-2 |
| 5 | Test | `post_share_registration::replacing_a_command_expires_its_results` | | AC-3 |
| 6 | Test | `post_share_registration::removing_a_command_expires_its_results` | | AC-4 |
| 7 | Test | `post_share_registration::snapshot_taken_before_update_is_unchanged` | | AC-5 |
| 8 | Test | `post_share_registration::failed_update_publishes_nothing` | | AC-6 |
| 9 | Test | existing `liquers-core`, `liquers-lib` suites unchanged | | AC-7 |
| 10 | Test | `post_share_registration::*` on an `Inline` environment + wasm32 build | | AC-8 |

## Example 1: Register after sharing

An application builds its environment, serves a query, and then loads a plugin that adds a
command.

```rust
let envref = builder.build()?;                       // shared from here on
envref.evaluate("hello").await?.get().await?;        // computed, cached
envref.update_commands(|cr| {
    register_command!(cr, fn shout(state) -> result)?;
    Ok(())
}).await?;
envref.evaluate("hello/shout").await?.get().await?;  // `hello` from the cache, `shout` runs
```

Validated with `liquers-validate --command hello --command shout -- 'hello/shout'`: it plans as
`Action hello`, `Action shout`.

## Example 2: Replace a command

```rust
envref.update_commands(|cr| {
    register_command!(cr, fn hello(state) -> result version: "2")?;  // new impl_version
    Ok(())
}).await?;
// `hello` and `hello/shout` were Ready; both are now Expired, and the next evaluation runs
// hello v2. `refresh_command_versions` reported for_command_implementation(hello).
```

## Edge and Error Cases

- **Failed closure** (AC-6): the closure registers `a`, then returns `Err`. After the call, `a` is
  absent from `get_command_metadata_registry()`, and no asset changed status.
- **Concurrent updates**: two `update_commands` calls race. Both commands are present afterwards,
  because `update_lock` serialises updates and each one clones the latest snapshot.
- **Snapshot held across an update** (AC-5): a `Snapshot` taken before an update still lists the
  old command set after it.
- **Implementor without support**: `CustomEnvironment` in `tests/environment_builder.rs` keeps the
  default `update_command_registry`. `update_commands` returns `NotSupported`.
- **wasm32** (AC-8): the code uses `std::sync::{RwLock, Mutex}` with no `.await` under a guard. The
  `Inline` manager tests run without a Tokio runtime, the way `inline_builds_without_a_tokio_runtime`
  does.

## Test Plan

- Integration: `liquers-core/tests/post_share_registration.rs`, a `GenericEnvironment<Value, (),
  Inline>` built with `EnvironmentBuilder`, with commands counting their calls in an `AtomicUsize`:
  - `new_command_after_sharing_runs` (AC-1): `evaluate("shout")` fails before the update and
    succeeds after it.
  - `cached_result_survives_an_update` (AC-2): `hello`'s call count stays 1 across the update.
  - `replacing_a_command_expires_its_results` (AC-3): after re-registering `hello` with `version:
    "2"`, the old asset is `Expired`, and the next evaluation calls the new function.
  - `removing_a_command_expires_its_results` (AC-4): `cr.unregister("hello")`. The asset is
    `Expired`, and `evaluate("hello")` errs `ActionNotRegistered`.
  - `snapshot_taken_before_update_is_unchanged` (AC-5): a `Snapshot` kept across the update still
    lacks `shout`, and a fresh one has it.
  - `failed_update_publishes_nothing` (AC-6).
  - `concurrent_updates_both_land` (edge): two `update_commands` joined.
  - `default_environment_reports_not_supported` (edge): `CustomEnvironment`.
- Unit: `liquers-core/src/commands.rs` `clone_shares_executors` (the clone executes the same
  closure); `liquers-core/src/context.rs` `snapshot_derefs_in_both_arms`.
- Regression (AC-7): `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`,
  `cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-mcp`, `cargo test -p
  liquers-agent`.
- wasm (AC-8): `bash scripts/check-build-matrix.sh` (wasm32 rows), and `cargo test -p liquers-web
  --target wasm32-unknown-unknown --features debug-handles` after `cargo clean`.

## Learning Log

- The version-refresh hook already exists and is documented for this purpose, so the design adds
  no dependency-manager code.
- `get_command_executor` has two call sites at HEAD, and `get_command_metadata_registry` has 93.
  Almost all of the latter are method calls, which compile unchanged through `Deref`.
