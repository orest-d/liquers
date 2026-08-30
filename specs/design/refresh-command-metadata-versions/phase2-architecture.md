# Phase 2: Solution & Architecture - Refresh Command Metadata Versions

## Overview

The command metadata registry gets a clearly named finalization operation,
`refresh_metadata_versions`, which recomputes every stored command's `metadata_version` from the
completed metadata. `Environment::to_ref(mut self)` becomes the common pre-sharing boundary: it
refreshes the owned environment's registry before wrapping the environment in `EnvRef` and before
manager startup can load command versions into the dependency manager.

## Known-Issue Preflight

Searched `DESIGN.md`, Phase 1, `specs/index.csv`, and `specs/issues/` for open records in
`core/commands`, `macro`, `core/context`, `core/assets`, `web`, `lib/commands`, `build`, and
`docs`, plus records mentioning `metadata_version`, `impl_version`, `to_ref`, command registration,
and `environment-builder`.

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `MACRO-LEAVES-STALE-METADATA-VERSION` | in_progress | P1 | The target issue. Requires registry recomputation after macro mutation and before evaluation. | yes | yes | Resolve here. | Keep P1 |
| `QUEUED-MANAGER-STARTUP-READINESS` | accepted | P1 | Manager startup loads command versions asynchronously after `to_ref`; this design only ensures the registry snapshot is correct before that load begins. | no | no | Preserve ordering; leave readiness to `environment-builder`. | Keep P1 |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | Future post-share edits will need a re-runnable refresh plus dependency-version reload. This design fixes pre-share finalization only. | no | no | Name the operation so the later design can reuse it. | Keep P3 |
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | draft | P3 | Neighboring version field; this design must not alter `impl_version` preservation. | no | no | Monitor; tests should assert `impl_version` is unchanged by metadata refresh. | Keep P3 |
| `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` | draft | P3 | Future metadata fields should be included automatically in `metadata_version` if serialized. | no | no | No architecture change; refresh covers future fields. | Keep P3 |

### Blocking and Priority Decision

Only `MACRO-LEAVES-STALE-METADATA-VERSION` blocks this project, and it is the issue being resolved.
No external prerequisite remains. The queued-manager readiness issue can still expose evaluation
before dependency-manager startup completes, but the registry data that startup reads will no longer
be stale.

## Data Structures

No new structs, enums, `ExtValue` variants, or serialized fields are introduced.
`CommandMetadataRegistry` keeps the same owned fields:

```rust
pub struct CommandMetadataRegistry {
    pub commands: Vec<CommandMetadata>,
    pub default_namespaces: Vec<String>,
    pub global_enums: HashMap<String, EnumArgument>,
}
```

Ownership remains unchanged. Refresh mutates the owned `Vec<CommandMetadata>` in place before the
environment is moved into `Arc`, so no `Mutex`, `RwLock`, `RefCell`, or clone of the registry is
needed.

`liquers-lib::CommandRegistryAccess` already returns `&mut CommandRegistry<Self>` for
`DefaultEnvironment`, and remains useful for library-specific registration helpers. It cannot solve
the generic `Environment::to_ref` problem because the trait default only knows `Self:
Environment`, not the `liquers-lib` extension trait.

Serialization remains unchanged. `metadata_version` is already `#[serde(skip)]`; refresh changes the
runtime value used by dependency tracking, not exported registry shape.

## Trait Implementations

### `Environment`

Every `Environment` implementor adds a mutable registry accessor:

```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;

    fn to_ref(mut self) -> EnvRef<Self> {
        self.get_mut_command_metadata_registry().refresh_metadata_versions();
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone());
        envref
    }
}
```

Implementors return the same registry field mutably that they already expose immutably:

```rust
fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
    &mut self.command_registry.command_metadata_registry
}
```

This is sound because `to_ref` owns `self`; the mutable borrow happens before `Arc::new`.
No post-share mutable path is added.

## Generic Parameters & Bounds

No new generic parameters or trait bounds are introduced. The new method uses the existing
`Environment: Sized` requirement and the existing associated command executor storage.

## Sync vs Async Decisions

`refresh_metadata_versions` and the `to_ref` call are synchronous. They perform deterministic
in-memory recomputation over registered metadata and do not touch stores, asset managers, or tasks.
Keeping this sync preserves `to_ref(self) -> EnvRef<Self>` and works for native queued
environments, inline core environments, and wasm paths that use `ImmediateEnvironment` or
`liquers-lib::DefaultEnvironment`'s wasm-selected immediate manager. Native-only core
`SimpleEnvironment` variants stay behind their existing `#[cfg(not(target_arch = "wasm32"))]`.

## Function Signatures

### `liquers-core/src/command_metadata.rs`

```rust
impl CommandMetadataRegistry {
    /// Recomputes every command's metadata version from its currently stored metadata.
    pub fn refresh_metadata_versions(&mut self) -> &mut Self;

    #[deprecated(note = "use refresh_metadata_versions")]
    pub fn update_all_metadata_versions(&mut self) -> &mut Self;
}
```

`refresh_metadata_versions` uses the current `calculate_metadata_version` helper. It must preserve
each command's `impl_version`, matching today's calculation behavior where the implementation
version is zeroed for the metadata hash but not overwritten in the stored command.

`update_command_metadata_version` may stay as the single-command low-level helper. It can keep its
name because the issue only requires a registry-wide finalization operation.

### `liquers-core/src/context.rs`

```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;
    fn to_ref(mut self) -> EnvRef<Self>;
}
```

Affected implementors in this tree:

- `SimpleEnvironment<V>`
- `SimpleEnvironmentWithPayload<V, P>`
- `ImmediateEnvironment<V>`
- `ImmediateEnvironmentWithPayload<V, P>`
- `liquers-lib::DefaultEnvironment<V, P>`
- `liquers-py::context::Environment`

`context.rs` test helpers use aliases such as `type TestEnvironment = ImmediateEnvironment<Value>`,
not independent `Environment` impls.

`liquers-py` must receive the new accessor so the crate stays source-compatible with the trait.
Its runtime-incomplete methods, including `todo!()` paths and a cfg-gated `get_async_store`, stay out
of scope; compile compatibility remains a validation target because `liquers-py` is a default
workspace member.

## Integration Points

| File | Change |
|---|---|
| `liquers-core/src/command_metadata.rs` | Add `refresh_metadata_versions`; deprecate or delegate `update_all_metadata_versions`; update unit test names/calls. |
| `liquers-core/src/context.rs` | Add `get_mut_command_metadata_registry`; call refresh at the start of default `to_ref`; implement the accessor for all core environments and tests. |
| `liquers-lib/src/environment.rs` | Implement `get_mut_command_metadata_registry` for `DefaultEnvironment`. |
| `liquers-py/src/context.rs` | Implement `get_mut_command_metadata_registry` by returning `&mut self.command_registry.command_metadata_registry`; do not attempt unrelated `liquers-py` completion. |
| `liquers-core/tests/command_declaration.rs` | Flip the INT02 expectation: macro metadata should already equal declaration metadata after `to_ref`, not require manual recomputation. |
| `specs/design/environment-builder/DESIGN.md` | Note that a future builder must preserve this refresh-before-manager-startup invariant. |

## Relevant Commands

No new commands and no query namespaces change. Existing command namespaces are only consumers:
macro-registered commands in `root`, library commands in `root`, `dep`, `lui`, `egui`, `pl`, and
other registered namespaces must receive current metadata versions before evaluation.

## Error Handling

No new error path. Version calculation already returns `Version::new(0)` if serialization fails;
this design does not change that behavior. Phase 3 should include a regression test for nonzero
metadata versions on normal macro-registered commands rather than introduce a fallible refresh API.

## Documentation Architecture

Reference updates:

- `specs/reference/COMMAND_DECLARATION.md`: record that `metadata_version` is computed by registry
  refresh/finalization and not declarable.
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: include registry metadata refresh
  in the `Environment::to_ref` initialization sequence.

Guide updates:

- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: update only if Phase 4 changes author-facing
  command-registration guidance. Expected outcome: no new command-author step is needed.

Issue/design updates:

- Close `specs/issues/MACRO-LEAVES-STALE-METADATA-VERSION.md` in Phase 5 after tests pass.
- Keep the environment-builder note as design coordination, not as a dependency.

Authoritative `affects_docs` for Phase 5:

- `specs/reference/COMMAND_DECLARATION.md`
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` if author-facing behavior changes

## Rust Best-Practices Review

- Mutation before sharing avoids runtime interior mutability and preserves Rust's ownership model.
- The new accessor is an explicit trait requirement, so external implementors get a compile error
  rather than silently skipping refresh.
- No new clone-heavy path is introduced; refresh iterates mutably over stored commands.
- The public registry method needs rustdoc because it is a lifecycle boundary.
