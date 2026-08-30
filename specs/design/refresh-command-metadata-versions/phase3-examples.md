# Phase 3: Examples & Use-cases - Refresh Command Metadata Versions

## High-Level Introduction

These runnable examples turn the Phase 1 invariant into tests: command metadata can be assembled in
multiple steps, but the registry must publish current `metadata_version` values before an
environment is shared. The progression starts with the registry method in isolation, then checks the
`Environment::to_ref` lifecycle boundary, then updates the existing macro/declaration regression so
the macro path no longer needs manual recomputation.

## Example Type

**User choice:** Runnable tests/prototypes.

No standalone user-facing example binary is planned. This is an internal lifecycle invariant, so the
canonical runnable artifacts should be unit and integration tests.

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | Macro declaration parity without recomputation | Demonstrates the issue fix at the macro-facing regression point | Agent 1 |
| 2 | Example | Registry refresh after post-add mutation | Demonstrates the core registry operation without environment setup | Agent 2 |
| 3 | Example | Boundaries that do not refresh | Documents direct `EnvRef::new`, post-init registration, and queued readiness non-goals | Agent 3 |
| 4 | Unit Tests | Core registry and trait behavior | Exact unit test suite for registry refresh and `Environment::to_ref` | Agent 4 |
| 5 | Integration Tests | Macro and dependency-manager consumers | End-to-end checks that public registration paths observe refreshed versions | Agent 5 |

## Example 1: Macro Declaration Parity Without Recomputing

### Connection to the High-Level Design

This is the direct regression for `MACRO-LEAVES-STALE-METADATA-VERSION`. It proves that
macro-registered metadata is already finalized after `to_ref`, without a manual
`update_command_metadata_version` call in the test.

### Scenario

A command is registered with `register_command!`, the environment is converted with `to_ref`, and
the command metadata is read from the shared environment. The stored version should agree with the
declaration path for equivalent metadata.

### Sequence of Steps

1. Create `SimpleEnvironment<Value>` and register `repeat(state, count: i64)` with the macro.
2. Convert the environment with `env.to_ref()`, which calls `refresh_metadata_versions`.
3. Read command metadata through `envref.get_command_metadata_registry()`.
4. Build equivalent `CommandDeclaration` metadata.
5. Normalize `impl_version`, then compare metadata and `metadata_version`.

### Core Example Code

```rust
use liquers_core::context::{Environment, SimpleEnvironment};

#[test]
fn int02_declaration_and_macro_agree_after_to_ref_refreshes_metadata_version() {
    type CommandEnvironment = SimpleEnvironment<Value>;

    fn repeat(state: &State<Value>, count: i64) -> Result<Value, Error> {
        let text = state.try_into_string()?;
        Ok(Value::from(text.repeat(count.max(0) as usize)))
    }

    let mut env = SimpleEnvironment::<Value>::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn repeat(state, count: i64) -> result)
        .expect("the macro registers the command");

    let envref = env.to_ref();
    let macro_metadata = envref
        .get_command_metadata_registry()
        .get(CommandKey::new("", "root", "repeat"))
        .cloned()
        .expect("the finalized macro-registered command is in the registry");

    // Build `declaration_metadata` using the existing INT02 declaration setup.
    let mut macro_comparable = macro_metadata.clone();
    macro_comparable.impl_version = declaration_metadata.impl_version.clone();

    assert_eq!(macro_metadata.metadata_version, declaration_metadata.metadata_version);
    assert_eq!(macro_comparable, declaration_metadata);
}
```

### Guide and Executable Example

This belongs in `liquers-core/tests/command_declaration.rs`, not in a guide. It is the canonical
executable proof that the macro path and declaration path agree after environment finalization.

**Expected output:**

```text
test int02_declaration_and_macro_agree_after_to_ref_refreshes_metadata_version ... ok
```

## Example 2: Registry Refresh After Post-Add Mutation

This unit-level example protects the new public registry method without involving macro expansion
or environment construction.

```rust
#[test]
fn refresh_metadata_versions_recomputes_mutated_commands() {
    let mut registry = CommandMetadataRegistry::new();
    let mut command = CommandMetadata::new("refresh_me");
    command.impl_version = Version::new(7);
    registry.add_command(&command);

    let key = CommandKey::new("", "root", "refresh_me");
    let stale = registry.get(key.clone()).unwrap().metadata_version;
    let stored = registry.get_mut(key.clone()).unwrap();
    stored.with_doc("filled in after registration");
    stored.with_argument(ArgumentInfo::integer_argument("count", false));

    registry.refresh_metadata_versions();
    let refreshed = registry.get(key).unwrap();

    assert_ne!(refreshed.metadata_version, Version::new(0));
    assert_ne!(refreshed.metadata_version, stale);
    assert_eq!(refreshed.impl_version, Version::new(7));
}
```

## Example 3: `to_ref` Refreshes Before Sharing

This example checks the Phase 2 architecture directly: `Environment::to_ref(mut self)` owns the
environment, refreshes the mutable command metadata registry, then wraps it in `EnvRef`.

The test should use `ImmediateEnvironment<Value>` because it is spawn-free, does not require a Tokio
runtime for construction, and is one of the wasm-covered environment implementors. The same trait
default should also be exercised through `SimpleEnvironment<Value>` in a Tokio test because that is
the native queued environment path.

```rust
#[test]
fn immediate_environment_to_ref_refreshes_metadata_versions() {
    let mut env = ImmediateEnvironment::<Value>::new();
    let key = CommandKey::new("", "root", "late_fill");

    env.command_registry
        .command_metadata_registry
        .add_command(&CommandMetadata::new("late_fill"));
    let stale = env
        .command_registry
        .command_metadata_registry
        .get(key.clone())
        .unwrap()
        .metadata_version;
    env.command_registry
        .command_metadata_registry
        .get_mut(key.clone())
        .unwrap()
        .with_doc("mutated before sharing");

    let envref = env.to_ref();
    let refreshed = envref.get_command_metadata_registry().get(key).unwrap();

    assert_ne!(refreshed.metadata_version, Version::new(0));
    assert_ne!(refreshed.metadata_version, stale);
}
```

**Expected output:**

```text
test immediate_environment_to_ref_refreshes_metadata_versions ... ok
```

## Example 4: Boundaries That Do Not Refresh

`EnvRef::new` remains a low-level wrapper and should not be tested as though it were the public
finalization path. Tests that need initialized environments should use `Environment::to_ref`.

Post-init registration is also a non-goal. Calling `refresh_metadata_versions` alone would not be
enough for future dynamic registration, because dependency-manager command-version state would also
need to be reloaded and dependent assets expired.

Queued manager readiness remains a non-goal. Refreshing before `init_with_envref` fixes the data
read by startup, but does not make detached startup completion observable.

## Corner Cases

### 1. Memory

Refresh is an in-place pass over `Vec<CommandMetadata>` and clones one command at a time inside
`calculate_metadata_version`. Tests do not need large registries for this issue. The important
memory assertion is indirect: no new shared lock, cloned registry, or persistent duplicate registry
is introduced.

### 2. Concurrency

The refresh must happen before `Arc` sharing. Tests should not add concurrent mutation, because the
Phase 2 design deliberately avoids post-share interior mutability. A compile-level trait break for
external implementors is acceptable and preferred over silently skipping refresh.

### 3. Errors

`refresh_metadata_versions` remains infallible and uses the existing fallback where serialization
failure maps to `Version::new(0)`. There is no new error path to assert. Existing tests should keep
using `unwrap()` or `expect()` only inside tests.

### 4. Serialization

`metadata_version` and `impl_version` stay skipped in JSON/YAML serialization. The existing
`test_command_metadata_versions_default_zero_and_skipped_in_json` remains valid and should not be
changed except for nearby naming consistency if desired.

### 5. Integration

The critical integration point is manager startup. `to_ref` must refresh before
`init_with_envref`, so queued managers read current command versions when they call
`load_command_versions`. The future `environment-builder` design must verify that builder-created
environments delegate through this same refreshed `to_ref` path or call the same registry lifecycle
operation before manager startup.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

No command-author workflow is introduced. Keep `COMMAND_REGISTRATION_GUIDE.md` conditional: update
it only if implementation changes what command authors should do. Reference documentation should
point to the integration test in `liquers-core/tests/command_declaration.rs` as the executable
proof.

### Usage and Meaning

`metadata_version` is runtime dependency-tracking state derived from command metadata. The meaning
to preserve is: equal completed metadata produces equal `metadata_version`, and changing completed
metadata changes the version that dependency tracking sees.

### Repeatable Development Guidance

Future registration paths that mutate stored command metadata before sharing should rely on
`Environment::to_ref`. Future post-share registration work must pair `refresh_metadata_versions`
with a dependency-manager command-version reload; this design does not add that reload.

### Corrections and Unexpected Learning

`TestEnvironment` is only an alias to `ImmediateEnvironment<Value>`, so there is no separate test
environment implementation. `liquers-lib::CommandRegistryAccess` helps `DefaultEnvironment`
registration helpers but does not replace the generic core trait accessor needed by `to_ref`.

## Test Plan

### Unit Tests

File: `liquers-core/src/command_metadata.rs`

- `refresh_metadata_versions_recomputes_mutated_commands`
  - Arrange: add one command, store the computed version, mutate doc and at least one argument via
    `get_mut`.
  - Assert: refreshed version is nonzero and differs from the stale version.
  - Catches: registry-wide refresh not implemented or not recomputing mutated stored metadata.

- `refresh_metadata_versions_refreshes_every_command`
  - Arrange: add commands `a` and `b`, manually set both stored `metadata_version` fields to zero.
  - Assert: after refresh, both are nonzero.
  - Catches: implementation only refreshes one command or accidentally keeps the old single-command
    path.

- `refresh_metadata_versions_preserves_impl_version`
  - Arrange: add a command with `impl_version = Version::new(9)`, mutate metadata, refresh.
  - Assert: `impl_version` is still `Version::new(9)`.
  - Catches: refresh overwriting implementation-version tracking while calculating the metadata
    hash.

- `update_all_metadata_versions_delegates_to_refresh_metadata_versions`
  - Arrange: if the deprecated compatibility method remains, force a stale command version.
  - Assert: calling `update_all_metadata_versions` refreshes the version exactly as
    `refresh_metadata_versions` would.
  - Catches: compatibility API drifting from the new lifecycle API.

File: `liquers-core/src/context.rs`

- `immediate_environment_to_ref_refreshes_metadata_versions`
  - Arrange: use `ImmediateEnvironment<Value>`, mutate stored command metadata after insertion.
  - Assert: metadata read through `EnvRef` has a nonzero version different from the stale pre-share
    value.
  - Catches: `Environment::to_ref` not calling refresh or accessor returning the wrong registry.

- `simple_environment_to_ref_refreshes_metadata_versions`
  - Arrange: same as above with `SimpleEnvironment<Value>` under `#[tokio::test]`.
  - Assert: same refreshed-version checks.
  - Catches: native queued environment implementor not wired to the mutable registry accessor.

- `immediate_environment_with_payload_to_ref_refreshes_metadata_versions`
  - Arrange: use `ImmediateEnvironmentWithPayload<Value, ()>` and the same mutation pattern.
  - Assert: same refreshed-version checks.
  - Catches: payload variant accessor omission.

- `simple_environment_with_payload_to_ref_refreshes_metadata_versions`
  - Arrange: use `SimpleEnvironmentWithPayload<Value, ()>` under `#[tokio::test]`.
  - Assert: same refreshed-version checks.
  - Catches: native payload variant accessor omission.

### Integration Tests

File: `liquers-core/tests/command_declaration.rs`

- `int02_declaration_and_macro_agree_including_metadata_version`
  - Change existing test to register the macro command on `SimpleEnvironment<Value>`, call
    `to_ref`, then compare macro metadata from the shared registry with declaration metadata.
  - Remove the current manual `update_command_metadata_version` call and the bug-expecting
    `assert_ne!(stored, recomputed, ...)`.
  - Catches: the original issue, where macro-filled metadata remains stale unless manually
    recomputed.

No dependency-manager integration test is required in this phase. The dependency manager is not
directly inspectable from an external integration test, and a test that only rechecks registry
metadata after `to_ref` would duplicate the `context.rs` lifecycle tests.
`QUEUED-MANAGER-STARTUP-READINESS` remains the owner of deterministic startup-readiness coverage.

File: `liquers-lib/tests/environment_metadata_versions.rs`

- `default_environment_to_ref_refreshes_macro_metadata_versions`
  - Arrange: use `liquers-lib::environment::DefaultEnvironment<Value>`, register a macro command,
    then call `to_ref`.
  - Assert: metadata read through the `EnvRef` is nonzero and differs from the pre-share stale
    version, or matches the same declaration comparison used by INT02.
  - Catches: an existing cross-crate `Environment` implementor missing the mutable accessor or
    drifting from the core lifecycle boundary. This validates the core invariant through
    `DefaultEnvironment`; implementation ownership remains in `liquers-core`.

Cross-crate compile checks:

- `cargo test -p liquers-lib` catches missing `get_mut_command_metadata_registry` on
  `DefaultEnvironment`.
- `cargo check -p liquers-py` is required as a source-compatibility check because `liquers-py` is a
  default workspace member. Its unrelated runtime-incomplete implementation paths remain out of
  scope.

### Manual Validation

Run focused tests first:

```bash
cargo test -p liquers-core refresh_metadata_versions
cargo test -p liquers-core update_all_metadata_versions_delegates_to_refresh_metadata_versions
cargo test -p liquers-core to_ref_refreshes_metadata_versions
cargo test -p liquers-core int02_declaration_and_macro_agree_including_metadata_version
```

Then run crate-level validation:

```bash
cargo test -p liquers-core
cargo test -p liquers-lib
```

Expected result: all focused tests pass, and no command metadata version assertion requires manual
recomputation after `to_ref`.

## Auto-Invoke: liquers-unittest Skill Output

Applied `liquers-unittest` conventions:

- Inline unit tests for private/core methods in `liquers-core/src/command_metadata.rs`.
- Inline unit tests for trait lifecycle behavior in `liquers-core/src/context.rs`.
- Integration regression where the macro, command declaration, and public registry behavior meet in
  `liquers-core/tests/command_declaration.rs`.
- `#[test]` for sync registry and immediate-environment checks; `#[tokio::test]` for queued
  `SimpleEnvironment` checks that initialize `DefaultAssetManager`.
- `type CommandEnvironment = SimpleEnvironment<Value>` before `register_command!` calls.

Optional compile validation:

- `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui`
  - Run only if the wasm target is installed.
  - Keep default features disabled so `polars` does not create an unrelated wasm failure.
  - Enable `webui` so wasm UI dependencies used by `liquers-lib` are present.
  - Catches: cfg-specific implementation gaps for the wasm-selected immediate manager path.
