---
id: STORE-FACTORIES-IN-CORE
kind: design
title: Store configuration and factories in liquers-core
workflow: liquers-project
status: draft
phase: architecture
area: [core/store, store/config, store/backends, web, docs]
gh_pr: []
issues: [STORE-CONFIG-IN-CORE]
affects_docs: [reference/STORE_CONFIG_FSD.md, reference/api/DOC_01_ARCHITECTURE_REFERENCE.md, guides/LANGUAGE-INTEGRATION_GUIDE.md, guides/STORE_FACTORY_GUIDE.md]
created: 2026-08-27
superseded_by:
---
# Store Configuration and Factories in `liquers-core` — Design Tracking

**Created:** 2026-08-27

## Phase Status

- [x] Phase 1: High-Level Design (approved)
- [x] Phase 2: Solution & Architecture (awaiting approval)
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves feature `STORE-CONFIG-IN-CORE` (P0 by maintainer decision), one of three recorded
prerequisites for the document-driven setup path in `design/environment-builder`.

**Scope widened at the user's direction after the first Phase 1 draft; complexity M -> L.** The
issue as filed proposed moving *pure data only* and explicitly left `StoreFactory` and
`StoreRouterBuilder` in `liquers-store`. That boundary is rejected: `liquers-web` needs the builder
and the factory trait as much as the config types, so under the data-only boundary its
`liquers-store` dependency survives and the stated goal is not met. The committed target is that
**`liquers-web` depends on `liquers-store` not at all**, which requires the config types, the
`StoreFactory` trait, factory chaining and `StoreRouterBuilder` all to land in `liquers-core`.
`liquers-store` is reduced to the OpenDAL backend crate plus compatibility re-exports.

**Three pieces that do not exist today.** Factory *chaining* into a composite factory; a *core
factory* for the stores core already implements (`memory`, and `filesystem` off wasm); and a
*parametrisable* factory assembled from a map of store-type names to creation functions rather than
a trait impl. `liquers-store` supplies an OpenDAL factory and a ready-made core-then-OpenDAL chain.

**Phase 1 gate decisions (user, second round).** Chaining is **first-wins**, with core registered
first, then `liquers-store`, then `liquers-lib`, then the integration — so the core definition of a
store type is stable and no downstream crate can redefine it. The overlap warning and its
`eprintln!` are **not implemented**; `store_types()` stays on the trait, so a factory still reports
what it claims and a caller can detect overlap if it wants to. The map-based parametrisable factory
is **confirmed**. `liquers-store`'s `opendal` feature is **kept** — non-OpenDAL backends in that
crate are expected, so an OpenDAL-free configuration keeps its purpose even though the wasm reason
in its manifest comment no longer applies.

**The browser's `http` override changes mechanism.** Today `WebStoreFactory` beats the built-in
OpenDAL `http` *because* factories are consulted before built-ins;
`design/liquers-web-store/phase2-architecture.md` argues explicitly that consulting factories second
would make that impossible. Under first-wins with core first, a later factory can never override an
earlier one — the override survives only because `liquers-web` drops `liquers-store` and the OpenDAL
factory claiming `http` is never in its chain. Same outcome, different mechanism, so that rationale
is superseded rather than relocated and the new rule must be documented where a reader finds it.

**Phase 1 gate decisions (user, third round).** The builder gets **no built-in fallback** — every
store it creates comes from a factory it was given — and each crate instead offers a **default
factory** as a convenience (core's is the core factory; `liquers-store`'s is core's chained with
OpenDAL's). An **unclaimed `store_type` is an error that lists the store types the chain supports**,
enumerated from the factories themselves, so the message is accurate for the build in hand.
**Overriding is a chain the caller composes**, putting their factory first: first-wins fixes the
*default* ordering, not the only possible one. And a factory **describes, per store type, the
configuration arguments it accepts** — which is what makes the supported-types error possible and
lets the configuration format be documented from the code that implements it.

The argument-description requirement is the piece with the most design freedom left.
`command_metadata.rs`'s `ArgumentInfo` is the nearest precedent but is shaped for positional command
parameters (`multiple`, `injected`, `gui_info`, `CommandParameterValue`) while store configuration is
a `HashMap<String, serde_json::Value>` of named keys. Phase 2 chooses reuse, subset or a
store-specific type, and decides how many optional-vs-required/default/enum fields are worth the
cost to every factory implementation.

Phase 1 correction to the issue: its verification item 3 was unachievable under the data-only
boundary (`liquers-web` also uses `StoreRouterBuilder` and implements `StoreFactory`). Under the
widened boundary it is achievable and strengthens to "no `liquers-store` dependency at all". The
issue's "what moves and what does not" table is superseded and is corrected at Phase 5, along with
`complexity: L`.

Documentation intent changed with the scope: Phase 1's earlier `neither` on a guide no longer holds.
"How do I add a store type" and "how do I override a built-in one" become repeatable tasks with a
real answer, so a new `specs/guides/STORE_FACTORY_GUIDE.md` is provisionally committed, with
`WebStoreFactory` as the worked example.

Open for Phase 2: how rich the per-store-type argument description is, and whether the resulting
store-type registry should be exportable the way `specs/command_registry.yaml` is; whether a factory
can explain a type it knows of but cannot build (the `opendal`-off and wasm-`filesystem` messages
worth not losing); whether `with_factory` survives alongside chaining, given that with no built-in
fallback `StoreRouterBuilder::new(config)` alone can build nothing; whether `expand_env_vars`'s bare
`std::env::var` moves verbatim, is `#[cfg]`-gated or takes a closure; the re-export shape; `toml`
feature forwarding; and whether the §3 `area` vocabulary needs `core/store` widened now that
`store/config` names files that will not exist.

Noted for filing rather than absorbing: with per-type argument descriptions in hand, a chain could
validate a `StoreRouterConfig` — unknown type, unknown key, missing required key — without
constructing a single store. Attractive, and beyond this design's scope.

## Phase 2 notes

**No blocker found** in the known-issue preflight. `WEB-NATIVE-IO-TIER2` (P3) is the one non-blocker
with a real design constraint, honoured rather than deferred: no `Send`/`Sync` bound on the trait or
on the map factory's closures, so a Promise-based IndexedDB store stays expressible.

**Reuse found rather than invented.** `command_metadata::ArgumentType` covers store argument types
and is reused instead of a parallel enum; `Error::not_supported` replaces `general_error` for an
unclaimed store type; `AsyncMemoryStore` / `AsyncFileStore` are already core types, so the core
factory constructs nothing new.

**`StoreFactory::store_types()` changes return type** (`Vec<String>` -> `Vec<StoreTypeInfo>`).
Breaking, and taken deliberately: two in-tree implementors, both edited anyway, and a parallel
`store_type_info()` with a default impl would leave two sources of truth for what a factory claims.

**`StoreTypeAvailability`** preserves what `create_store`'s single `match` provides today and what
`LANGUAGE-INTEGRATION_GUIDE.md` makes conformance requirement `STORE13`: a type that is real but
ungated-off in this build must be refused with the feature or target responsible, never as "unknown".

**Two findings outside the Phase 1 boundary, surfaced at the gate rather than absorbed:**

1. `from_yaml` / `from_json` / `from_toml` / `expand_env_vars` use `Error::new(ErrorType::ParseError,
   ...)`, which `CLAUDE.md` forbids, and no typed constructor fits (`key_parse_error` and
   `query_parse_error` both require a `Position`). Proposed: add `Error::parse_error(String)` to
   `liquers-core/src/error.rs` rather than move a known violation into the crate that enforces the
   rule most strictly.
2. `scripts/check-build-matrix.sh` has **no `liquers-core` rows at all**, and core is about to gain
   its first optional feature (`toml`) and target-conditional store availability. Four rows proposed.

**The documentation finding that matters most.** `LANGUAGE-INTEGRATION_GUIDE.md` §"Taking only part
of the store support crate" enumerates three resolutions to exactly this problem, recommends option 3
(optional backend feature) and explicitly rejects option 2 (move the types into `liquers-core`) as
"widens core for one consumer's benefit". This design does option 2. The rejection was written when
`liquers-web` was the only consumer and no longer holds once core itself must embed a store
description; the section is rewritten to record the reversal and its reason, while keeping option 3's
three cost lessons, which remain true of the surviving `opendal` feature. Conformance item `STORE12`
("a factory that overrides a shared type name resolves to the integration's implementation") also
needs restating: after this change `liquers-web` has nothing to override.

## Merge with `main` (2026-08-29)

Pulled `origin/main` at `2bb336f`. **No code changed** — four new design folders, `specs/README.md`,
`specs/index.csv` and four issue front-matters. Merged clean; three consequences for this design.

**1. This folder was misnamed and broke the index.** `scripts/docs_index.py --check` reported
`duplicate id STORE-CONFIG-IN-CORE (also specs/issues/STORE-CONFIG-IN-CORE.md)` — `init_feature.py`
derived the design id from the folder name, and the folder was named after the issue. The three
sibling designs main added all name the folder for the *solution* while the issue names the
*problem* (`RECIPE-PROVIDER-BY-NAME` -> `recipe-provider-selection`,
`STORE-OPENDAL-SLASH-HANDLING` -> `opendal-path-mapping`). Renamed
`store-config-in-core` -> `store-factories-in-core`, id `STORE-FACTORIES-IN-CORE`, which also
describes the widened scope better. `--check` now reports 0 errors.

**2. Registration that Phase 1 owed and had not paid.** `CLAUDE.md` requires a PR adding a design
folder to update `specs/README.md`; it had not been. Added a §Stores capability line, repointed the
issue's `design:` from `environment-builder` to this folder (matching what main did for the three
siblings), applied the `complexity: M -> L` reclassification to the issue file, noted this design in
`environment-builder/DESIGN.md` alongside its three, and regenerated `specs/index.csv` and the
README's generated blocks.

**3. `design/opendal-path-mapping/` assesses this design against its old boundary.** It says a
`store_builder.rs` merge conflict is "possible" and lists `liquers-store/src/config.rs` and
`liquers-core` as "Not touched" — both written before the scope widened. Under the current boundary
`store_builder.rs` is gutted, not merely at risk. Checked file by file anyway: **no source file is
edited by both designs** (theirs is `opendal_store.rs` alone; this one does not touch it), so the
conclusion — no ordering constraint — survives. What goes stale is their documentation, and two
shared expectations are recorded in Phase 2 so neither design silently breaks the other.

**Confirmation from a sibling.** `recipe-provider-selection`'s Phase 2 rejects a `StoreFactory`-shaped
registry for recipe providers with a precise technical reason — `AsyncRecipeProvider` is generic in
`E`, so `dyn RecipeProviderFactory` is not object-safe, whereas "`StoreFactory` has no such problem
because `AsyncStore` is not generic". That independently confirms this design's object-safety
assumption, and is worth citing in the new store-factory guide as the boundary of the pattern.

## Phase 1 gate answers, fourth round (2026-08-29)

All seven Phase 1 open questions answered; four changed Phase 2 materially.

**Minimal, because store selection is fixed at compile time.** Unlike the command metadata registry
and like the type registry, the set of store types is settled when the binary is built, and a user
configures stores at most once. So: no exported registry file, no dynamic registration machinery. Do
provide a list of supported stores with descriptions and per-argument descriptions.

**Arguments carry JSON types, not `ArgumentType`.** The first Phase 2 draft reused
`command_metadata::ArgumentType`. That is now rejected: it is a command-parameter vocabulary
(`Integer`/`Float`/`IntegerOption`, `Enum`/`GlobalEnum` needing a `CommandMetadataRegistry`) with no
container variant, so it cannot express the browser `http` type's `keys: [...]`. Replaced by a small
`StoreArgumentType` mirroring JSON — scalars preferred, `Array`/`Object` allowed where genuinely
needed. Store configuration must stay ergonomic and directly representable as JSON. This is the one
place the "reuse core structures" instruction is not followed; core has no JSON-type enum to reuse
(checked `type_system.rs`, `media_type.rs`, `command_metadata.rs`), so nothing is shadowed.

**No compatibility shims at all.** No backwards compatibility is required, and a `liquers-store`
re-export of a core type is precisely the shadowing to avoid. `liquers-store/src/config.rs` and
`src/store_builder.rs` are **deleted**; the crate shrinks to `opendal_store.rs` plus a new
`store_factory.rs`. This supersedes the issue's verification item 1 ("still resolves via re-export;
no call site edited") — every call site moves, and all are in this repository.

**One own factory plus one default chained factory, per crate.** Stated as the convention the guide
teaches: `CoreStoreFactory` / `OpendalStoreFactory` / `WebStoreFactory` each describe only their own
crate's store types, and each crate's `default_store_factory()` chains its own after everything below
it that should be available. The default is what a consumer reaches for. `with_factory` returns with
**replace** semantics (append hid where a factory landed, which is the ambiguity first-wins removes),
plus a `chain_factory` convenience.

**Validate on construction; no separate validation path.** Stores are built at startup, so
construction is the validation. The constraint that follows: `create` must be fast and must not fetch
bulk data. A store type that benefits from pre-fetching is making a trade-off it must document.

**Unbuildable types stay** as a soft quality-of-life requirement, justified by being nearly free —
one enum field set in a `#[cfg]` branch that already exists. The case that matters is a documented
type disabled by a feature.

`liquers-store/toml` forwards to `liquers-core/toml` and drops its own `toml` dependency. The §3
`area` question was withdrawn and decided as bookkeeping: `store/config` is redefined by topic rather
than by a file list, since both files it names are deleted.

## Cross-design coordination

**Standing obligation (maintainer instruction, 2026-08-29): keep
[`design/opendal-path-mapping/`](../opendal-path-mapping/) updated whenever a change here impacts
it.** It is the only in-review design sharing a crate with this one, it is `status: in_review` and
unimplemented, and its author documented this design's interaction from the issue text rather than
from this folder — so it goes stale silently unless someone pushes.

Correction to an earlier reading in this folder: their §"Not touched" list and their "Read,
unchanged" entries describe **their own** edits and remain accurate. Only the interaction assessment
was stale, plus one line reference.

Done so far (2026-08-29): their Phase 2 §"Related open issues" bullet rewritten with the widened
scope and the ruled-out conflict; their `create_opendal_store` reference annotated as relocated by
this design; a dated cross-reference added to their `DESIGN.md` notes.

Re-check and push an update if any of the following changes here:

| Trigger | What to update there |
|---|---|
| The set of files this design edits changes | The no-conflict conclusion in their §"Related open issues" — it rests on `opendal_store.rs` being touched by neither |
| `create_opendal_store` lands somewhere other than `liquers-store/src/store_factory.rs` | Their "Read, unchanged" line reference |
| `StoreConfig::key_prefix` gains any behaviour change (it should not) | Their validation item (d), which asserts `key_prefix() == data` for a prefixed store |
| `scripts/check-build-matrix.sh` rows or the `opendal`-off shape change | Their Phase 2 validation command list, which requires the `opendal`-off configuration to compile |
| This design starts editing `opendal_store.rs` for any reason | Everything above; the two designs would then genuinely conflict |
| Either design reaches implementation | Whichever lands second re-reads the other before starting |

Their design does **not** need to change today; the conclusion improved rather than broke.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
