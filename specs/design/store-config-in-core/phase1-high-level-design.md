---
title: "Phase 1: High-Level Design — Store configuration and factories in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, store/backends, web, docs]
---
# Phase 1: High-Level Design — Store Configuration and Factories in `liquers-core`

Resolves feature [`STORE-CONFIG-IN-CORE`](../../issues/STORE-CONFIG-IN-CORE.md)
(P0; complexity reclassified **M → L**, see §Scope), a recorded prerequisite of
[`environment-builder`](../environment-builder/DESIGN.md).

## Feature Name

Store configuration and store factories in `liquers-core`

## Purpose

`liquers-core` already defines every store abstraction (`AsyncStore`, `AsyncStoreRouter`) and two
concrete stores (`AsyncMemoryStore`, `AsyncFileStore`), but the vocabulary for *describing* a store
and the machinery for *building* one live a crate above it, in `liquers-store`. Moving the
configuration types, the `StoreFactory` seam and `StoreRouterBuilder` down into `liquers-core`
lets core own an `EnvironmentConfig` that describes a store, and reduces `liquers-store` to what its
optional `opendal` feature already implies it is: the OpenDAL backend crate. `liquers-web` then
drops its `liquers-store` dependency entirely.

## Scope

Widened at the user's direction from the issue as filed. The issue proposed moving *pure data only*
and explicitly left `StoreFactory` and `StoreRouterBuilder` behind. That boundary is now rejected:
`liquers-web` needs the builder and the factory trait as much as the config types, so leaving them
means the `liquers-store` dependency survives and the stated goal is not met. Complexity moves
**M → L** accordingly; the issue file records the new boundary at Phase 5.

## Core Interactions

### Query System

None added. `StoreConfig::key_prefix` already calls `liquers_core::parse::parse_key`, so the moved
code reaches *down* into the crate it lands in — one import instead of one dependency edge.

### Store System

The whole configuration-to-router path moves to `liquers-core`:

| Moves to `liquers-core` | Stays in `liquers-store` |
|---|---|
| `StoreRouterConfig`, `StoreConfig`, their `from_*`/`to_*` methods | `AsyncOpenDALStore` and the `opendal` dependency |
| `expand_env_vars` and the per-config expansion | `OPENDAL_STORE_TYPES`, `is_opendal_store_type`, `get_opendal_scheme` |
| `StoreFactory` trait | An **OpenDAL factory** implementing the moved trait |
| A **chaining / composite factory** | A **default chained factory**: core's, then OpenDAL's |
| A **parametrisable factory** built from named creation functions | Compatibility re-exports of everything moved |
| A **core factory** for `memory` and `filesystem` | |
| `StoreRouterBuilder`, `create_router_from_yaml` / `_json` | |

Three new pieces, not present today:

- **Chaining.** Factories compose into a composite factory. A factory chained later **overrides**
  one chained earlier for any store type both claim. Where the overridden and overriding factories
  claim overlapping `store_type` strings, chaining emits an `eprintln!` warning naming the type and
  both claimants — an override is legitimate (it is how a browser replaces `http`), but a silent
  one is how a configuration quietly points somewhere unintended.
- **A core factory.** `liquers-core` supplies the factory for the stores it already implements:
  `memory` and, off wasm, `filesystem`.
- **A parametrisable factory.** A `StoreFactory` assembled from named creation functions rather
  than by implementing the trait, so an integration can contribute a store type with a closure.
  *(Interpretation of "some parametrisable store creation function/method" — confirm at this gate;
  the alternative reading is a parameter-driven variant of `create_store` itself.)*

`liquers-store` then supplies its own OpenDAL factory **and** a ready-made chain of core's followed
by its own, so a native consumer gets today's behavior from one call.

### Command System

None. No command is added, removed or re-signed; `specs/command_registry.yaml` is untouched.

### Asset System

None directly. The point of the move is that a later `EnvironmentConfig` can carry store, recipe
provider and asset-manager options in one core-side document; that type is not in scope here.

### Value Types

None. No `ExtValue` variant, no `TypeInfo`, no serializer change.

### Web/API

**`liquers-web` drops `liquers-store` from `Cargo.toml`.** It imports exactly four items from it —
`StoreConfig`, `StoreRouterConfig`, `StoreFactory`, `StoreRouterBuilder` — and all four move.
`WebStoreFactory` keeps implementing the trait at its new path, and `build_router` chains it after
core's factory instead of relying on "factories are consulted before built-ins". Two test files
(`tests/store_js_STORE.rs`, `tests/eval_EVAL.rs`) and `src/environment.rs` change import paths only.

`liquers-axum`, `liquers-lib` and `liquers-py` never name the configuration, factory or builder
types; one `liquers-lib` example constructs `AsyncOpenDALStore` directly and is unaffected.

### UI

None.

## Crate Placement

**`liquers-core`** — two new modules declared in `lib.rs`: `store_config.rs` (the data types and
`expand_env_vars`) and `store_factory.rs` (the trait, the chaining, the parametrisable factory, the
core factory and `StoreRouterBuilder`). Splitting them keeps a consumer that only wants to *parse* a
configuration from pulling in construction. New dependency: `toml` only, carried across as the same
optional feature `liquers-store` already gates `from_toml` behind; `serde`, `serde_derive`,
`serde_json` and `serde_yaml` are already non-optional in core.

**`liquers-store`** — keeps `opendal_store.rs`, the OpenDAL type tables and the `opendal`
dependency, gains an OpenDAL `StoreFactory` and the default chain, and turns `config.rs` /
`store_builder.rs` into re-export shims so no existing call site is edited.

**`liquers-web`** — one `Cargo.toml` line removed, four import paths rewritten, one factory
registration expressed as a chain.

Dependency flow is respected throughout: code moves *down* the chain
(`liquers-core` ← `liquers-store` ← `liquers-web`), never up.

## Consequences to decide in Phase 2

1. **Precedence inverts.** `StoreRouterBuilder::with_factory` is documented today as first-wins —
   "a later factory cannot shadow an earlier one" — and chaining is last-wins. There is one in-tree
   caller and it registers a single factory, so nothing breaks; but the contract reverses and
   `liquers-web/src/store/builder.rs`'s module doc asserts the old rule in prose.
2. **`create_store`'s error quality is currently centralized.** One `match` distinguishes
   *unknown type*, *type needs the `opendal` feature* and *type unavailable on wasm*. Split across
   factories, `liquers-core` alone cannot say "that is an OpenDAL type"; a core-only build degrades
   to "unknown store type". `liquers-store`'s default chain restores the full message.
3. **`eprintln!` is silent on wasm.** `wasm32-unknown-unknown` has no stderr, and the browser is
   where an override is most likely — `liquers-web` already uses `console.warn` for its
   unexpanded-`${VAR}` warning for exactly this reason. The requested `eprintln!` is the right
   native behavior; whether wasm gets a routed variant is Phase 2's call.
4. **No in-tree chain actually overlaps.** Core claims `memory` / `filesystem`; OpenDAL claims `fs`,
   `s3`, `http`, … ; `WebStoreFactory` claims `localstorage`, `js`, `http`, `https`. Core+OpenDAL
   and core+Web are both clean, because `liquers-web` no longer chains the OpenDAL factory. The
   warning therefore has no in-tree trigger and needs a deliberate test.
5. **`liquers-store`'s `opendal` feature may lose its reason to exist.** Its manifest comment says
   it is optional "so that a wasm32 consumer can depend on this crate for its configuration and
   builder alone" — precisely what this change removes. With `opendal` off, the crate becomes
   re-exports and nothing else. Phase 2 should decide whether the feature stays, and whether
   `liquers-store`'s name still describes it.
6. **`expand_env_vars` puts a bare `std::env::var` in core.** Not a regression — `liquers-store` is
   already in every wasm build — but core is in more places. Move verbatim, `#[cfg]`-gate it, or
   take the lookup as a closure.
7. **`StoreConfig::metadata`** is documented "reserved for future use" and never read. Assume it
   moves verbatim; dropping it would be a breaking format change.

## Documentation Intent

**Reference:** *Extend* `specs/reference/STORE_CONFIG_FSD.md`. Its title names `liquers-store` and it
is the settled description of this format; after the move the format, the factory seam and the
builder are core's and only the OpenDAL backend is `liquers-store`'s. It must also gain the chaining
and override rules, which are new behavior no document states. Requires a `## History` row and a
`reviewed:` bump in the same commit (§9.2). No new reference — a second document on the same format
would compete with this one.

**Guide:** *New*, provisionally `specs/guides/STORE_FACTORY_GUIDE.md`. Phase 1's earlier `neither`
no longer holds: "how do I add a store type" and "how do I override a built-in one" are now
repeatable tasks with a real answer (implement or parametrise a factory, chain it last), and
`liquers-web`'s `WebStoreFactory` is a complete worked example to link. Confirm at Phase 2 whether
this earns its own file or a section of the reference.

**Other documents to create:** *None* beyond the above.

**Specific documents to update:**

| Path | Change |
|---|---|
| `specs/reference/STORE_CONFIG_FSD.md` | Crate ownership split; chaining and override rules; `History`; `reviewed:` |
| `specs/reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` | Line 128: config **and builder** types are `liquers_core` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | Line 729: an integration needs `liquers-core` alone for stores |
| `README.md` (repo root) | Line 93: `liquers_core` for config/builder; `liquers_store` for OpenDAL |
| `CLAUDE.md` | "Adding a Store Backend" — the four steps change crate and now start from a factory |
| `specs/DOCS_STRUCTURE_GUIDE.md` §3 | `core/store` gains `store_config.rs`/`store_factory.rs`; `store/config` shrinks or retires |
| `specs/design/liquers-web-store/` | Its Phase 2 architecture states the first-wins factory rule; note the supersession |
| `specs/issues/STORE-CONFIG-IN-CORE.md` | Widened boundary, `complexity: L`, corrected verification; `status: closed` at Phase 5 |
| `specs/design/environment-builder/DESIGN.md` | Prerequisite table: layering constraint lifted |
| `specs/README.md`, `specs/index.csv` | New design folder; issue status |

**Audience and outcome.** Internal. A developer arriving afterwards should learn from
`STORE_CONFIG_FSD.md` and the factory guide alone that the schema, the factory seam and the builder
are core's, that chaining is last-wins, and that `liquers-store` supplies OpenDAL — without opening
this design folder.

## Correction to the issue's stated verification

`STORE-CONFIG-IN-CORE.md` verification item 3 reads "`liquers-web` builds without depending on
`liquers-store` for configuration". Under the issue's own data-only boundary that was unachievable,
because `liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`. Under the
widened boundary it becomes achievable and is strengthened to the stronger claim the user asked for:
**`liquers-web` builds with no `liquers-store` dependency at all.** Its table of "what moves and
what does not" is likewise superseded — `StoreRouterBuilder` and `StoreFactory` move. Phase 2
restates the verification list in full.

## Open Questions

1. **Is "parametrisable store creation function/method" the closure-registry reading?** Assumed: a
   `StoreFactory` built from a map of store-type names to creation functions, so a type can be
   contributed without a trait impl. The alternative is a parameterised `create_store` free
   function. This changes the shape of the API and should be settled at this gate.
2. **Does `StoreRouterBuilder` keep an implicit built-in fallback,** or must every builder be given
   a factory chain explicitly? Today `create_one` falls through to `create_store`. An explicit chain
   is cleaner and makes core-only versus OpenDAL builds obvious; an implicit core fallback keeps
   `StoreRouterBuilder::from_yaml(…)?.build()` working with no ceremony.
3. **Does `with_factory` survive alongside chaining,** re-expressed as "chain this last", or is it
   deprecated in favour of building a chain and handing it over whole?
4. **Where does the overlap warning fire** — at chain construction (once, cheap, complete) or at
   store creation (only for types actually used)? Construction-time is proposed.
5. **Re-export shape in `liquers-store`:** explicit `pub use` lists or globs, and deprecation
   attributes or not?
6. **Feature forwarding:** does `liquers-store/toml` become `["liquers-core/toml"]`?
7. **`area` vocabulary (§3):** does `core/store` absorb the new modules, or does the closed
   vocabulary gain a value? `store/config` names files that will no longer exist.

## References

- Issue: [`specs/issues/STORE-CONFIG-IN-CORE.md`](../../issues/STORE-CONFIG-IN-CORE.md)
- Parent design: [`specs/design/environment-builder/`](../environment-builder/DESIGN.md) —
  `DESIGN.md` §"Preparatory work for document-driven setup", `phase3-examples.md` §Scenario 4
- Factory seam as designed: [`specs/design/liquers-web-store/phase2-architecture.md`](../liquers-web-store/phase2-architecture.md)
- Reference: [`specs/reference/STORE_CONFIG_FSD.md`](../../reference/STORE_CONFIG_FSD.md)
- Sibling prerequisites: [`COMMAND-DECLARATION-FORMAT`](../../issues/COMMAND-DECLARATION-FORMAT.md),
  [`RECIPE-PROVIDER-BY-NAME`](../../issues/RECIPE-PROVIDER-BY-NAME.md)
- Source: `liquers-store/src/config.rs`, `liquers-store/src/store_builder.rs`,
  `liquers-web/src/store/builder.rs`, `liquers-core/src/store.rs`
