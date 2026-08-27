---
title: "Phase 1: High-Level Design — Store configuration types in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, web, docs]
---
# Phase 1: High-Level Design — Store Configuration Types in `liquers-core`

Resolves feature [`STORE-CONFIG-IN-CORE`](../../issues/STORE-CONFIG-IN-CORE.md) (P0, complexity M),
a recorded prerequisite of [`environment-builder`](../environment-builder/DESIGN.md).

## Feature Name

Store configuration types in `liquers-core`

## Purpose

`StoreRouterConfig`, `StoreConfig` and `expand_env_vars` are pure serde data, but they live in
`liquers-store`, which depends on `liquers-core`. Nothing in `liquers-core` can therefore embed
them. Moving them down one crate — leaving every backend, factory and builder where they are — lets
`liquers-core` own an `EnvironmentConfig` that describes a store, which is what the planned
document-driven JavaScript and Python setup needs.

## Core Interactions

### Query System

None added. `StoreConfig::key_prefix` already calls `liquers_core::parse::parse_key`, so the moved
code reaches *down* into the crate it lands in — one import (`crate::parse::parse_key`) instead of
one dependency edge.

### Store System

The configuration *vocabulary* (`stores:` list, `type` / `prefix` / `config` / `metadata`,
`${VAR}` expansion, YAML/JSON/TOML parsing) moves to `liquers-core`. The configuration *machinery*
that turns it into stores — `StoreRouterBuilder`, `StoreFactory`, `create_store`, the memory and
filesystem constructors, `OPENDAL_STORE_TYPES`, `is_opendal_store_type`, `get_opendal_scheme` and
every backend — stays in `liquers-store`. Core learns the shape of a store description without
learning that S3 exists.

Worth noting for Phase 2: the two built-in types `create_store` handles without OpenDAL —
`memory` and `filesystem` — construct `liquers_core::store::AsyncMemoryStore` and
`AsyncFileStore`, which already live in core. Only the OpenDAL branch reaches for something core
does not have.

### Command System

None. No command is added, removed or re-signed; `specs/command_registry.yaml` is untouched.

### Asset System

None directly. The point of the move is that a later `EnvironmentConfig` can carry store, recipe
provider and asset-manager options in one core-side document, but that type is *not* in scope here.

### Value Types

None. No `ExtValue` variant, no `TypeInfo`, no serializer change.

### Web/API

`liquers-web` imports `liquers_store::config::{StoreConfig, StoreRouterConfig}` in
`src/store/builder.rs`, `src/environment.rs` and two test files. Those imports keep compiling
through a re-export; migrating them to `liquers_core::store_config` is optional follow-up, not a
requirement of this change. `liquers-axum` and `liquers-lib` do not name the config types at all.

### UI

None.

## Crate Placement

**`liquers-core`** — new module `liquers-core/src/store_config.rs`, declared in `lib.rs`. Receives
`StoreRouterConfig`, `StoreConfig`, their constructors, builders, accessors, the six
`from_*`/`to_*` serialization methods, `expand_env_vars`, and the unit tests covering them.
Dependency cost: `serde`, `serde_derive`, `serde_json` and `serde_yaml` are already non-optional in
core; only `toml` is new, and it carries across as the same optional feature `liquers-store`
already gates `from_toml` behind.

**`liquers-store`** — keeps `store_builder.rs`, the OpenDAL type tables and all backends.
`liquers-store/src/config.rs` becomes a thin re-export module so `liquers_store::config::…` and
`liquers_store::{StoreConfig, StoreRouterConfig}` resolve unchanged and no call site is edited.

Dependency flow is respected: this moves code *down* the chain
(`liquers-core` ← `liquers-store`), never up.

## Documentation Intent

**Reference:** *Extend* `specs/reference/STORE_CONFIG_FSD.md`. It is the settled description of this
configuration format and its title names `liquers-store`; after the move the format is owned by
`liquers-core` and only its instantiation is `liquers-store`'s. That split is exactly what a
reference must state. No new reference — a second document describing the same format would compete
with this one. Requires a `## History` row and a `reviewed:` bump in the same commit (§9.2).

**Guide:** *Neither.* Nothing about how a developer writes or uses a store configuration changes;
the format, the field names and the `${VAR}` syntax are byte-identical. Reconsider only if Phase 2
concludes the re-export should be deprecated, which would make "which import path do I use" a
question a guide has to answer.

**Other documents to create:** *None.* The change is a relocation with a compatibility shim; the
Phase 5 summary carries the learning.

**Specific documents to update:**

| Path | Change |
|---|---|
| `specs/reference/STORE_CONFIG_FSD.md` | Crate ownership split; `History` row; `reviewed:` bump |
| `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` | Line 128 table row: config types now `liquers_core` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | Line 729: config types no longer require `liquers-store` |
| `README.md` (repo root) | Line 93 table: split config types from `StoreRouterBuilder` |
| `CLAUDE.md` | "Adding a Store Backend" step 2/3 paths; `OPENDAL_STORE_TYPES` stays in `liquers-store/src/config.rs` |
| `specs/DOCS_STRUCTURE_GUIDE.md` §3 | `core/store` row gains `store_config.rs`; `store/config` row loses the moved half |
| `specs/issues/STORE-CONFIG-IN-CORE.md` | `status: closed` at Phase 5 (§4.3) |
| `specs/design/environment-builder/DESIGN.md` | Prerequisite table: record that the layering constraint is lifted |
| `specs/README.md`, `specs/index.csv` | New design folder; issue status |

**Audience and outcome.** Internal. A developer arriving afterwards should learn from
`STORE_CONFIG_FSD.md` alone that the configuration *schema* is core's and the *backends* are
`liquers-store`'s, without opening this design folder.

## Correction to the issue's stated verification

`STORE-CONFIG-IN-CORE.md` verification item 3 reads "`liquers-web` builds without depending on
`liquers-store` for configuration." That is not achievable and is not the benefit: `liquers-web`
also uses `StoreRouterBuilder` and implements `StoreFactory` (`liquers-web/src/store/builder.rs`),
both of which stay. The real gain is the **layering** one — `liquers-core` can define a type
embedding `StoreRouterConfig` — and the accurate restatement is "`liquers-web` can reach the
configuration types without `liquers-store`". Phase 2 will restate the verification list.

## Open Questions

1. **`expand_env_vars` in core, on wasm.** It calls `std::env::var`, which compiles on
   `wasm32-unknown-unknown` but always errs. `liquers-core` is in every wasm build; `liquers-store`
   already is too, so this is not a regression — but is a bare `std::env` call something core
   should contain, or should Phase 2 gate it (`#[cfg(not(target_arch = "wasm32"))]`) or take the
   variable lookup as a closure? `liquers-web` already routes around it via
   `build_without_env_expansion`.
2. **Re-export shape.** Explicit `pub use liquers_core::store_config::{…}` in
   `liquers-store/src/config.rs`, or a glob? Explicit keeps the crate's surface auditable; a glob
   cannot silently drop an item. Deprecation attributes on the re-exports: yes or no?
3. **Feature name collision.** `liquers-store`'s `toml` feature must forward to core's new one.
   Should `liquers-store/toml` become `["liquers-core/toml"]`, or keep `dep:toml` as well?
4. **`§3` area vocabulary.** Does `core/store` absorb `store_config.rs`, or does the closed
   vocabulary gain a `core/store-config` value? Affects the `affects_docs` candidate generation for
   every future design in this space.
5. **Where does the builder boundary actually fall?** `create_memory_store` and
   `create_filesystem_store` build core types from core data; only `create_opendal_store` needs
   `liquers-store`. Should the move stop strictly at data (the issue's position, and the smaller
   change), or should core also gain a minimal builder for the store types it already owns? The
   second is a larger scope than `STORE-CONFIG-IN-CORE` records and Phase 1's recommendation is to
   stay at data — but the question should be answered deliberately, not by default.
6. **Does `StoreConfig::metadata` survive the move as-is?** It is documented "reserved for future
   use" and has never been read. Moving it is free; dropping it is a breaking format change. Assume
   it moves verbatim unless Phase 2 finds a reason.

## References

- Issue: [`specs/issues/STORE-CONFIG-IN-CORE.md`](../../issues/STORE-CONFIG-IN-CORE.md)
- Parent design: [`specs/design/environment-builder/`](../environment-builder/DESIGN.md) —
  `DESIGN.md` §"Preparatory work for document-driven setup", `phase3-examples.md` §Scenario 4
- Reference: [`specs/reference/STORE_CONFIG_FSD.md`](../../reference/STORE_CONFIG_FSD.md)
- Sibling prerequisites: [`COMMAND-DECLARATION-FORMAT`](../../issues/COMMAND-DECLARATION-FORMAT.md),
  [`RECIPE-PROVIDER-BY-NAME`](../../issues/RECIPE-PROVIDER-BY-NAME.md)
- Source: `liquers-store/src/config.rs` (441 lines), `liquers-store/src/store_builder.rs`
