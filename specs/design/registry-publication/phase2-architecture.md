# Phase 2: Solution & Architecture — publishing registries across realms

## Overview

Re-checked at HEAD (2026-10-26):

- `CommandMetadataRegistry` (`liquers-core/src/command_metadata.rs` ≈1314) derives `Serialize`;
  `executable_commands()` leaves out `NON_EXECUTABLE_REALMS`; `CommandMetadata::resolve_global_enums`
  inlines global enums (used by `liquers-mcp/src/server.rs`). `add_command`, `remove_command` and
  `get` exist; there is no realm-level import or removal.
- `CommandDefinition::Alias { command: CommandKey, .. }` names its target by full key, realm
  included (`COMMAND_ALIASES.md`).
- `TypeInfo` (`type_system.rs`) derives `Serialize`/`Deserialize`; `TypeRegistry` does not, and has
  `iter()` and `get_in_realm`. `realm-type-registry` adds serde and `import_realm`.
- `liquers-axum` routers follow one shape: `XApiBuilder::new(base).build() -> Router<EnvRef<E>>`,
  handlers `State<EnvRef<E>>` → `Response` (`recipes/builder.rs`, `recipes/handlers.rs`).
- `liquers-web/src/environment.rs` retains what a rebuild replays (`REGISTERED_SPECS`,
  `STORE_CONFIG`) and rebuilds with `rebuild_with` / `rebuild_without`; `new_environment` builds
  the type registry before `WebEnvironment::new_with_type_registry`.

Chosen: publish documents from a new router; import by relabelling in core; retain imports in the
page like the store configuration. Rejected: extending `registerCommands` with a metadata-only mode
(its contract is "every command has an implementation"); importing into `""` (would collide with
local commands and contradict realm naming); a WebSocket push of registry changes (registries do
not change during a run).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `TYPE-REGISTRY-NOT-REALM-AWARE` (`realm-type-registry`) | draft | P2 | type import (AC-8) | for step 6 only | no | `requires` |
| `COMMAND-METADATA-ENHANCEMENTS` (`global-enum-registration`) | accepted | P2 | export cannot meet a dangling enum | no | no | note |
| `CORE-MULTI-REALM-INTERPRETER` (`multi-realm-dispatch`) | accepted | P3 | executes imported commands | no | no | `required-by` |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | who may read the registry | no | no | note (route off by default) |

## Interfaces

`liquers-core/src/command_metadata.rs`:

```rust
impl CommandMetadataRegistry {
    /// Copies `other`'s commands of realm `remote_realm` into realm `local_realm`, with global
    /// enums resolved against `other` and aliases inside `remote_realm` retargeted. All or nothing.
    /// Refuses `local_realm` "" or in NON_EXECUTABLE_REALMS, an occupied `local_realm`, and an
    /// alias whose target realm is not `remote_realm`. Returns the number of commands imported.
    pub fn import_realm(&mut self, local_realm: &str, other: &CommandMetadataRegistry,
        remote_realm: &str) -> Result<usize, Error>;
    /// Removes every command of `realm`; returns how many.
    pub fn remove_realm(&mut self, realm: &str) -> usize;
    /// The document `GET {base}/commands` answers: executable commands, global enums resolved,
    /// `global_enums` empty.
    pub fn published(&self) -> Result<CommandMetadataRegistry, Error>;
}
```

`liquers-axum/src/registry/{mod.rs, builder.rs, handlers.rs}`, re-exported from `lib.rs`:

```rust
pub struct RegistryApiBuilder<E: Environment> { base_path: String, _phantom: PhantomData<E> }
impl<E: Environment> RegistryApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn build(self) -> Router<EnvRef<E>>;   // GET {base}/commands, GET {base}/types
}
```

Handlers serialize to JSON (`application/json`), set `ETag: "<Version of the body>"`, and answer
`304` when `If-None-Match` matches. `types` serializes
`Vec<&TypeInfo>` of `env.get_type_registry().iter()` with an empty realm.

`liquers-web/src/lib.rs` (module level, beside `registerCommands`) and `src/environment.rs`:

```rust
#[wasm_bindgen(js_name = importRealm)]
pub fn import_realm(realm: &str, commands: JsValue, types: JsValue, remote_realm: Option<String>)
    -> Result<JsValue /* IssueReport */, JsValue>;   // delegates to environment::import_realm_on
```

`commands` and `types` are JSON or YAML strings (or `undefined` for `types`). A retained
`IMPORTED_REALMS: RefCell<BTreeMap<String, ImportedRealm>>` (`ImportedRealm { commands:
CommandMetadataRegistry, types: Option<TypeRegistry>, remote_realm: String }`) is applied by
`new_environment`: types before `new_with_type_registry`, commands after the built-ins. A second
import of a realm replaces the retained entry; the environment is rebuilt as `configure_store_on`
does, restoring the previous entry if the rebuild fails. A realm that a registered JavaScript
command already uses is refused.

## Integration Points

- `import_realm` calls `add_command` per command, so metadata versions are computed locally.
- `published` is what both the route and a build-time exporter could share; the existing
  `export-command-registry` binary is unchanged.
- `TypeRegistry::import_realm(realm, &other)` from `realm-type-registry` step 3 (AC-8). Until it
  lands, `importRealm` with `types` returns an error saying types are not yet supported, and step 6
  removes it.
- `multi-realm-dispatch` will look up the bound realm's commands where this design put them.
- `liquers-validate` already merges registry files; a `--import-realm` flag is not needed.

## Error Handling

Route: a `published()` failure (only possible with a dangling global enum before
`global-enum-registration`) answers `500` with the §2.2 envelope and `ErrorDetail`. Import:
`Error::general_error` naming the realm and the cause (occupied realm, reserved realm, alias
target, or the global enum that failed to resolve); nothing is applied. Web: the error is thrown as
a `LiquersError`; a parse error of the documents is reported in the returned `IssueReport` as with
`registerCommands`.

## Relevant Commands

None. The routes serve metadata of whatever commands the server registered.

## Documentation Architecture

`WEB_API_SPECIFICATION.md` (reference, `axum`): a new section "Registry API" between §6 and §7,
§1's builder table, §8's assembly example, and Appendix B no longer listing it. `REALMS.md`
(reference, `core/plan`): "Imported realms" under "How a realm is executed". `liquers-web/README.md`
and its TypeScript stub: `importRealm`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `command_metadata.rs`; three new files in `liquers-axum/src/registry/`, `liquers-axum/src/lib.rs`; `liquers-web/src/environment.rs`, its TypeScript stub |
| Crates and workflows affected | core, axum, web; existing routes and imports untouched |
| Existing tests likely to change | none; `check-stubs.sh` gains `importRealm` |
| New validation | core unit tests, an axum router test with `tower::ServiceExt::oneshot`, a web conformance test under Node |
| Compatibility | additive; the document format is the exporter's |
| Data | none persisted; imported metadata lives in the environment |
| Concurrency / performance | the route serializes the registry per request; `ETag` avoids re-sending |
| Security | the route reveals command names, docs and argument metadata; off unless merged, behind the server's layers |
| Recovery | each part reverts alone; the page API is new |
| Certainty | high for steps 1-5; step 6 waits for `realm-type-registry` |
