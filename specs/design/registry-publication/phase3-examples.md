# Phase 3: Examples & Use-cases — publishing registries across realms

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A page plans a server step | route, import, plan | AC-1, AC-4, AC-6, AC-7 |
| 2 | Example | Mounting the router | assembly | AC-9 |
| 3 | Test | `command_metadata::tests::published_registry_is_executable_and_resolved` | no `store_config`, enums inline | AC-1 |
| 4 | Test | `…::tests::import_realm_relabels_and_retargets_aliases` | realm, alias target, `""` untouched | AC-4 |
| 5 | Test | `…::tests::import_realm_refusals_apply_nothing` | `""`, `store_config`, occupied, foreign alias | AC-5 |
| 6 | Test | `…::tests::remove_realm_then_import_replaces` | refresh | AC-4 |
| 7 | Test | `liquers-core/tests/environment_builder.rs` `imported_command_plans_but_has_no_executor` | plan ok, evaluation error | AC-6 |
| 8 | Test | `liquers-axum/src/registry/builder.rs` `tests::commands_route_answers_document_and_etag` | body, `ETag`, `304` | AC-1, AC-3 |
| 9 | Test | `…::tests::types_route_lists_default_realm` | list of `TypeInfo` | AC-2 |
| 10 | Test | `…::tests::nothing_mounted_without_builder` | `404` | AC-9 |
| 11 | Test | `liquers-web` conformance `REALM01_import_realm_plans_and_survives_rebuild` | page API | AC-7 |
| 12 | Test | `liquers-web` conformance `REALM02_types_import_under_realm` | after step 6 | AC-8 |

## Example 1: a page plans a server step

Server (Rust):

```rust
let app = axum::Router::new()
    .merge(QueryApiBuilder::new("/liquer/q").build())
    .merge(RegistryApiBuilder::new("/liquer/api/registry").build())
    .with_state(envref);
```

`GET /liquer/api/registry/commands` answers `{"commands": [...], "default_namespaces": ["",
"root"], "global_enums": {}}` with `ETag: "…"`; a repeat with `If-None-Match` answers `304`.

Page (JavaScript):

```js
const [commands, types] = await Promise.all([
  fetch("/liquer/api/registry/commands").then(r => r.text()),
  fetch("/liquer/api/registry/types").then(r => r.text())]);
liquers.importRealm("server", commands, types);   // module level, like registerCommands
// then evaluate "-R/data/sales.csv/-server/heavy_stats/-/to_text" as any other query
```

The query plans (validated with `--command server//heavy_stats`: `GetResource[data, sales.csv]`,
`Action{server/heavy_stats}`, `Action{to_text}`); without a realm binding, evaluation fails with the
unknown-executor error naming `heavy_stats`. Before the import it fails to plan with *Action
'heavy_stats' not registered; command keys tried: 'server--heavy_stats'* (checked with
`liquers-validate`).

## Example 2: mounting

Nothing is served unless `RegistryApiBuilder` is merged; a deployment that does not want to reveal
its command list simply leaves it out.

## Edge and Error Cases

- Import into `""` or `store_config`: refused.
- Import into a realm with local commands (or a JavaScript command in the page): refused.
- An alias in the remote realm `""` whose target is in realm `gpu`: refused, naming the alias,
  unless `gpu` is imported first under its own local name (out of scope: the import is per realm).
- Remote default namespaces differ: an imported command in namespace `pl` needs `ns-pl/` unless
  `pl` is default locally.
- A command with a computed enum arrives without a provider: unchecked (`computed-enum-alternatives`
  AC-6).
- `importRealm` with `types` before step 6: an error saying type import is not yet available.
- Re-import with a changed registry: replaced; the page's asset cache is discarded by the rebuild.

## Test Plan

Core tests 3-6 in `command_metadata.rs` `tests` with hand-built registries (an alias, a global enum,
a `store_config` entry). Test 7 builds an environment with `EnvironmentBuilder`, imports a realm,
plans with `make_plan`, and evaluates expecting the error. Axum tests 8-10 follow the existing
router tests (`recipes/builder.rs` `tests`) using `tower::ServiceExt::oneshot` on a
`SimpleEnvironment`. Web tests 11-12 join the Node conformance suite
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib import_realm`, `--lib published`,
`--test environment_builder`; `cargo test -p liquers-axum registry`; the liquers-web Node loop and
`./liquers-web/scripts/check-stubs.sh --build`.
