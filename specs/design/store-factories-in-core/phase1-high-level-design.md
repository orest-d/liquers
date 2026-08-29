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

- **Chaining, first-wins.** Factories compose into a composite factory; the **first** factory in the
  chain claiming a `store_type` handles it, and a later one cannot shadow it. The intended order is
  bottom-up — `liquers-core` first, then `liquers-store`, then `liquers-lib`, then the integration —
  so the core definition of a store type is stable by default. Overriding remains available: a
  caller who needs it composes their own chain and puts their factory first. *(User decision: no
  overlap warning is implemented.)*
- **A core factory.** `liquers-core` supplies the factory for the stores it already implements:
  `memory` and, off wasm, `filesystem`.
- **A parametrisable factory.** *(User decision: confirmed.)* A `StoreFactory` assembled from a map
  of store-type names to creation functions rather than by implementing the trait, so an integration
  can contribute a store type with a closure.

**No built-in fallback; a *default* factory instead.** `StoreRouterBuilder` gains no hidden
knowledge of store types: every store it creates comes from a factory it was given. What replaces
today's `create_store` fallback is a **default factory** each crate offers as a convenience —
`liquers-core`'s is the core factory; `liquers-store`'s is core's chained with its OpenDAL factory,
so a native consumer gets today's behavior from one call. Nothing is implicit, and a caller who
wants a different composition simply builds one.

**A factory describes what it claims.** Beyond the store-type names it already reports, a factory
carries, per store type, a description of the configuration arguments that type accepts. Two things
depend on it: an unclaimed `store_type` is an **error that lists the types the chain does support**,
which is only possible because the composite can enumerate its members; and the same data documents
the configuration format from the code that implements it rather than from a hand-written table.
Precedent and shape are discussed under §Consequences.

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

1. **The browser's `http` override stops working by precedence and starts working by omission.**
   This is the one behavioral subtlety in the change. Today `WebStoreFactory` claims `http`/`https`
   and beats the built-in OpenDAL `http` *because* factories are consulted before built-ins —
   `liquers-web-store/phase2-architecture.md` argues explicitly that "consulting factories second
   would make that impossible". Under first-wins with core registered first, that argument no longer
   applies: a later factory can never override an earlier one. It still works, for a different
   reason — `liquers-web` drops `liquers-store` entirely, so the OpenDAL factory that claims `http`
   is simply never in the browser's chain. The outcome is the same and the mechanism is not, so the
   rationale in `liquers-web-store` is superseded rather than merely relocated, and the new rule
   must be stated where a reader will find it.
2. **Overriding a core store type is a chain the caller composes, not a capability the API denies.**
   First-wins plus the default ordering makes `memory` and `filesystem` stable *by default*; a
   caller who genuinely needs a different `memory` builds a chain with their factory first. Worth
   documenting explicitly, because "first-wins" read alone suggests a prohibition that does not
   exist.
3. **The error for an unclaimed type gets better, not worse.** Today one `match` in `create_store`
   distinguishes *unknown type*, *needs the `opendal` feature* and *unavailable on wasm*, and that
   `match` has no home once the dispatch is split across factories. The replacement is stronger: an
   unclaimed type is an error that **lists the store types the chain does support**, assembled from
   the factories themselves, so the message is accurate for the build in hand instead of describing
   a type set that may not be compiled in. Phase 2 decides whether a factory can additionally
   explain a type it *knows of* but cannot build — the `opendal`-off and wasm-`filesystem` cases,
   which are the two messages worth not losing.
4. **Per-store-type argument descriptions are new surface with real design freedom.** The nearest
   precedent is `command_metadata.rs`'s `ArgumentInfo`, but it is shaped for positional command
   parameters (`multiple`, `injected`, `gui_info`, `CommandParameterValue` defaults) while store
   configuration is a `HashMap<String, serde_json::Value>` of named keys. Phase 2 decides whether to
   reuse it, subset it, or define a smaller store-specific type — and how far to go: name, type and
   documentation per key are clearly wanted; required-vs-optional, defaults and enumerated values
   are all plausible and each one adds a field every factory implementation must fill. It also
   raises whether the store-type registry should be exportable the way
   `specs/command_registry.yaml` is.
5. **`expand_env_vars` puts a bare `std::env::var` in core.** Not a regression — `liquers-store` is
   already in every wasm build — but core is in more places. Move verbatim, `#[cfg]`-gate it, or
   take the lookup as a closure.
6. **`StoreConfig::metadata`** is documented "reserved for future use" and never read. Assume it
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

## Decisions settled at this gate

| Question | Decision |
|---|---|
| Chaining precedence | **First-wins.** Core is registered first, then `liquers-store`, then `liquers-lib`, then the integration, so the core definition of a store type is stable. |
| Overlap warning | **Not implemented.** The trait keeps `store_types()`, so a factory still reports what it claims and overlap remains detectable by a caller that cares. |
| `eprintln!` on overlap | **Not implemented**, so the wasm-has-no-stderr problem does not arise. |
| "Parametrisable store creation function/method" | **A `StoreFactory` built from a map** of store-type names to creation functions. |
| `liquers-store`'s `opendal` feature | **Kept.** Non-OpenDAL backends in `liquers-store` are expected, so an OpenDAL-free configuration of the crate keeps its purpose. |

## Open Questions

1. **How rich is the per-store-type argument description?** Name, type and documentation per
   configuration key are clearly wanted. Required-vs-optional, defaults, and enumerated values are
   each plausible and each adds a field every factory implementation must fill. Reuse
   `ArgumentInfo`, subset it, or define a store-specific type? Should the resulting store-type
   registry be exportable the way `specs/command_registry.yaml` is?
2. **Can a factory explain a type it knows of but cannot build?** The two messages worth preserving
   are "that type needs the `opendal` feature" and "`filesystem` is unavailable on wasm". Both are
   `#[cfg]`-conditional knowledge a factory *has*; whether the trait gives it a way to say so is a
   design choice, and the alternative is that those types simply do not appear in the supported list
   for that build.
3. **Does `with_factory` survive alongside chaining,** re-expressed as "chain this after", or is it
   deprecated in favour of building a chain and handing it over whole? With no built-in fallback,
   `StoreRouterBuilder::new(config)` alone can now build nothing, so the builder's constructor may
   want the factory as a required argument.
4. **Does configuration validation without construction become possible?** With per-type argument
   descriptions in hand, a chain could check a document — unknown type, unknown key, missing
   required key — without constructing a single store. Attractive, and clearly beyond this design;
   worth filing rather than absorbing.
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
