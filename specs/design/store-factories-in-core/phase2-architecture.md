---
title: "Phase 2: Architecture — Store configuration and factories in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, store/backends, web, docs]
---
# Phase 2: Solution & Architecture — Store Configuration and Factories in `liquers-core`

## Overview

Two new modules in `liquers-core` — `store_config.rs` (the serde data types, moved verbatim) and
`store_factory.rs` (a redesigned `StoreFactory` trait, a first-wins chain, a map-based factory, the
core factory, and `StoreRouterBuilder`). `liquers-store` keeps OpenDAL, gains an OpenDAL factory and
a default core-then-OpenDAL chain, and re-exports everything moved. `liquers-web` drops
`liquers-store` from its manifest. The one substantive redesign is that a factory now *describes*
the store types it claims — name, documentation, configuration arguments and availability — which is
what makes an unclaimed type an error that lists what the build actually supports.

## Known-Issue Preflight

Searched: the five issues linked from `design/environment-builder/DESIGN.md`; every row of
`specs/index.csv` whose `area` includes `core/store`, `store/config`, `store/backends`, `web` or
`build`; and the integration points named in Phase 1 (`liquers-web/src/store/builder.rs`,
`liquers-web/src/environment.rs`, `scripts/check-build-matrix.sh`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`). Terminal and `closed` records excluded.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `STORE-CONFIG-IN-CORE` | draft | P0 | This design resolves it. Its stated boundary and verification list are superseded by Phase 1. | — | no | Close at Phase 5 with the corrected boundary and `complexity: L` | Keep P0 |
| `RECIPE-PROVIDER-BY-NAME` | draft | P0 | Now designed as `design/recipe-provider-selection/` (in review). It **cites this design in both directions**: it rejects "put the choice in `liquers-store` next to `StoreConfig`" precisely because `STORE-CONFIG-IN-CORE` moves configuration *down* into core, and it records that a `StoreFactory`-shaped registry does **not** transfer to recipe providers, because `AsyncRecipeProvider` is generic in `E` and `dyn RecipeProviderFactory` is therefore not object-safe — "`StoreFactory` has no such problem because `AsyncStore` is not generic". That is independent confirmation of this design's object-safety assumption. It states explicitly: "consumer, not prerequisite. It can land in either order." | no | no | None; cite the object-safety finding in the new guide | Keep P0 |
| `COMMAND-DECLARATION-FORMAT` | draft | P0 | Now designed as `design/command-declaration/` (in review). Names this design as "document #1; independent of this one". Independent surface (commands, not stores). | no | no | Monitor | Keep P0 |
| `WEB-NATIVE-IO-TIER2` | accepted | P3 | Adds an IndexedDB store type to `WebStoreFactory`. It must be expressible under the redesigned trait, i.e. carry a `StoreTypeInfo` with its arguments. Confirms the trait must stay `!Send`-friendly (IndexedDB is Promise-based and non-`Send`). | no | no | Design constraint honoured: no `Send`/`Sync` bound on the trait or on the map factory's closures | Keep P3 |
| `STORE-OPENDAL-SLASH-HANDLING` | accepted | P1 | Now designed as `design/opendal-path-mapping/` (in review). **Its interaction assessment is stale** — see below. Still not blocking: no source file overlaps. | no | no | Correct that design's "Not touched" list once this lands; see below | Keep P1 |
| `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` | draft | P1 | Now designed as `design/payload-env-recipe-provider-fallback/` (in review). Recipe-provider defaulting in the payload environment; no store surface. | no | no | None | Keep P1 |
| `CORE-STORE-OPENBIN-MISSING` | accepted | P3 | `openbin` is on `AsyncStore`, already in core. Unaffected by where factories live. | no | no | None | Keep P3 |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | `StoreConfig::key_prefix` returns `Key`, not an absolute-key newtype. Moving it neither helps nor worsens; if that issue lands later it changes one signature in the moved code. | no | no | Monitor | Keep P3 |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | Checked the code being moved: `config.rs` and `store_builder.rs` contain no library `unwrap()`/`expect()` — the only hit is inside a doc-comment example, which is permitted. Nothing is imported into core. | no | no | None; record the check | Keep P2 |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | Core already owns `AsyncFileStore` and its `tokio::fs` use. This design adds no tokio surface to core; the `filesystem` constructor moves next to the store it constructs. | no | no | None | Keep P3 |
| `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE` | draft | P3 | A `liquers-store` test-quality issue, unrelated to the split. | no | no | None | Keep P3 |
| `CORE-SESSION-AND-KEY-ACL`, `ASSETS-IMPROVEMENTS`, `RESOURCE-NAME-ASCII-ONLY`, `CORE-ASSET-GC`, `STORE-COMMAND-NAMESPACE-MISSING` | accepted/draft | P2–P3 | Share `core/store` by area only; each concerns store *behaviour* or asset lifecycle, not configuration or construction. | no | no | Discarded from `affects_docs` candidates | Unchanged |

### Blocking and Priority Decision

**No blocker.** Nothing in the open set must be resolved before this design proceeds, and no
priority change is recommended. The three P0 records are the `environment-builder` prerequisites and
carry maintainer-assigned scheduling weight, not §4.4 severity — a tension already recorded in
`environment-builder/DESIGN.md` and untouched here.

`WEB-NATIVE-IO-TIER2` is the one non-blocker with a real design constraint, and it is honoured
rather than deferred: the trait and the map factory's closures carry no `Send`/`Sync` bound, so a
Promise-based IndexedDB store remains expressible.

#### `opendal-path-mapping`: its assessment of this design is out of date

That design's §"Related open issues" says of `STORE-CONFIG-IN-CORE`: *"moves `StoreConfig` into
`liquers-core`; it does not move `opendal_store.rs`, so there is no ordering constraint, but a merge
conflict in `store_builder.rs` is possible if both land close together. This change does not edit
that file."* That was written against the issue's **original data-only boundary**. Under the widened boundary
this design now carries, `store_builder.rs` is not merely at risk of conflict — it is gutted:
`create_store` is removed, `StoreRouterBuilder` moves to core, and `create_opendal_store` becomes
`OpendalStoreFactory::create`.

Their §"Not touched" list (`liquers-store/src/config.rs`, `liquers-core`) and their "Read,
unchanged" entry for `create_opendal_store` are **not** stale: both describe what *their* change
edits, and both remain true. Only the line reference under "Read, unchanged" will need
re-resolving, because this design relocates that function.

**The good news is that the conclusion survives the correction.** Checked file by file:

| | `opendal-path-mapping` edits | This design edits |
|---|---|---|
| `liquers-store/src/opendal_store.rs` | yes (only source file) | no |
| `liquers-store/src/config.rs`, `store_builder.rs`, `lib.rs`, `Cargo.toml` | no | yes |
| `liquers-core/*`, `liquers-web/*` | no | yes |

**No source file is touched by both**, so there is no textual merge conflict in either landing
order — an improvement on their "possible", which was the right call on the information they had.

**Their document has been updated with this** (2026-08-29): the `STORE-CONFIG-IN-CORE` bullet in
their Phase 2 §"Related open issues" now states the widened scope and the ruled-out conflict, their
`create_opendal_store` reference is annotated as relocated by this design, and their `DESIGN.md`
notes carry a dated cross-reference. See §Cross-design coordination in this folder's `DESIGN.md` for
what to re-check at each remaining phase. Two shared expectations are recorded so neither design
breaks the other:

- Both plan to run `bash scripts/check-build-matrix.sh` and both care about the `opendal`-off
  configuration. This design changes what that configuration *contains* — `OpendalStoreFactory` is
  compiled either way and reports `Unavailable` when the feature is off — so their "the `opendal`-off
  configuration must still compile" check remains meaningful but tests a different shape.
- Their validation item (d) asserts a prefixed store reports `key_prefix() == data`.
  `StoreConfig::key_prefix` moves to `liquers-core` with its behaviour unchanged, so that assertion
  holds; only its import path moves.

## Data Structures

### `StoreArgumentInfo` — one configuration key of one store type

```rust
// liquers-core/src/store_factory.rs
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct StoreArgumentInfo {
    /// Configuration key as it appears under `config:` in the document, e.g. `root`, `bucket`.
    pub name: String,
    /// Human-readable label, for a form or a generated table.
    pub label: String,
    /// What the argument means and what a valid value looks like.
    pub doc: String,
    /// JSON type this key accepts — see the vocabulary note below.
    #[serde(default)]
    pub argument_type: StoreArgumentType,
    /// A store cannot be constructed without it; `require_config_string` will fail.
    #[serde(default)]
    pub required: bool,
    /// Value used when the key is absent. `None` for a required argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}
```

**Why not `command_metadata::ArgumentType`.** Phase 2's first draft reused it. The gate decision that
store arguments carry **JSON types** rules it out: `ArgumentType` is a *command-parameter*
vocabulary. It splits number into `Integer`/`Float`/`IntegerOption`/`FloatOption`, carries
`Enum`/`GlobalEnum` variants whose resolution needs a `CommandMetadataRegistry`, and has no container
variant at all — so it cannot express the browser `http` type's `keys: [...]`. Store configuration is
a `HashMap<String, serde_json::Value>` parsed from a JSON/YAML document, and its type vocabulary
should be JSON's. Reusing `ArgumentType` here would import command-registry concepts into a place
that has no registry, and still fail on the one container case that already exists.

This is the one place the "reuse core structures, don't shadow them" instruction is *not* followed,
and the reason is that these are different concepts rather than the same concept twice. Core has no
JSON-type enum today (checked: `type_system.rs`, `media_type.rs`, `command_metadata.rs`), so nothing
is being shadowed. Flagged at the gate rather than decided silently.

**Builder methods**, in the `StoreConfig::with_prefix` style already in the moved code:
`StoreArgumentInfo::new(name, argument_type)`, `.with_label(…)`, `.with_doc(…)`, `.required()`,
`.with_default(serde_json::Value)`, plus `StoreArgumentInfo::derived(name, default)` which infers
`argument_type` from the default's JSON type and falls back to `Any` when the default is `null`
(an `Option<T>` field hides its type that way). `required()` takes no argument — it reads better at the call
site than `.required(true)`, and there is no `.optional()` because that is the default.

**Ownership:** all fields owned. A `StoreTypeInfo` is built once per factory and cloned into error
messages; there is nothing large enough to justify `Arc`.

### `StoreArgumentType` — the JSON type of one configuration key

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StoreArgumentType {
    String,
    /// JSON has one numeric type; a store type wanting an integer says so in `doc`.
    Number,
    Boolean,
    /// Allowed but discouraged — see below.
    Array,
    /// Allowed but discouraged — see below.
    Object,
    /// Unconstrained; any JSON value.
    #[default]
    Any,
}
```

**Variant semantics.** `String`, `Number`, `Boolean` are the scalars a store argument should
normally be. `Array` and `Object` exist because at least one real type needs a container — the
browser `http` store's `keys: [input.csv, sub/report.json]` — and because adding a variant to a
public serde enum later is a breaking change while including it now is one line. `Any` is the
default for an argument a factory has not described precisely.

**Scalars are preferred, and the guide says so.** A configuration document should stay ergonomic and
directly representable as JSON; nesting objects inside `config:` makes both the document and
`config_as_string_map` harder to reason about. `Object` is present for the case that genuinely needs
it, not as an invitation.

**No default match arm** on this enum anywhere.

### `StoreTypeAvailability` — why a known type cannot be built here

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub enum StoreTypeAvailability {
    /// The type can be constructed in this build.
    #[default]
    Available,
    /// The type is real and documented but this build cannot construct it.
    /// The string names the feature or target responsible.
    Unavailable(String),
}
```

**Variant semantics.** `Available` — `create` may still fail on a bad configuration, but the type is
buildable. `Unavailable(reason)` — `create` returns that reason as an error, and the type is listed
separately from the supported set. **No default match arm** on this enum anywhere.

**Why an enum rather than `available: bool` plus `reason: Option<String>`:** the two-field form
admits `available: true, reason: Some(...)`, which is meaningless. The enum makes the invariant
unrepresentable.

**Why this exists at all.** It preserves behaviour the current `create_store` `match` provides and
which `LANGUAGE-INTEGRATION_GUIDE.md` makes a *conformance requirement*: `STORE13` — "a store type
that exists but is unavailable in this build is refused with a message naming the feature or target
responsible". Two live cases: `fs`/`s3`/… when `liquers-store`'s `opendal` feature is off, and
`filesystem` on `wasm32`. Without this field, splitting dispatch across factories would either lose
those messages or degrade them to "unknown store type", which `STORE13` exists to forbid.

### `ArgumentCoverage` — whether the argument list is a specification or guidance

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub enum ArgumentCoverage {
    /// Liquers owns this store type. `arguments` **is** the specification: an argument not listed
    /// is not accepted, and the list is expected to stay accurate.
    #[default]
    Complete,
    /// The store type's arguments are defined by an external project. `arguments` is *guidance*,
    /// deliberately not exhaustive, and any further key is passed through to the backend.
    /// The string is where the authoritative documentation lives.
    Partial { authority: String },
}
```

**This distinction is required by the design, not an OpenDAL workaround.** Liquers is meant to
accept store backends it does not own, and an externally-owned backend can only ever be described
incompletely — its arguments change on someone else's release schedule. Without a way to *say* the
description is partial, every such backend forces a bad choice: claim completeness and be silently
wrong on the next upstream release, or describe nothing and give the user no guidance at all.
OpenDAL is simply the first and largest instance; the browser types are the counter-example, owned
here and describable in full.

`Partial` removes the problem structurally rather than by discipline: **an incomplete list is only a
lie if completeness was claimed.** A `Partial` type states in the type system that its arguments are
guidance and that the truth lives elsewhere, so an upstream release adding a field makes our
description *less complete*, never *wrong*. Nothing has to be noticed for the documentation to stay
honest, which is the only property that survives contact with a dependency's release cadence.

Consequences, all deliberate:

- **`Complete` types may reject an unknown key; `Partial` types must not.** For `memory`,
  `filesystem` and the browser types, an unlisted key is a typo worth reporting. For an OpenDAL
  type, an unlisted key is probably a field we have not described, and refusing it would break a
  configuration that OpenDAL itself accepts.
- **The unclaimed-*type* error is unaffected.** It enumerates store *types*, which are ours to know
  completely, not their arguments.
- **No default match arm** on this enum anywhere.

### `StoreTypeInfo` — one store type a factory claims

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct StoreTypeInfo {
    /// The `type:` value in a store configuration entry, e.g. `memory`, `s3`, `localstorage`.
    pub store_type: String,
    pub label: String,
    pub doc: String,
    /// Configuration keys this type accepts, in a stable order.
    #[serde(default)]
    pub arguments: Vec<StoreArgumentInfo>,
    #[serde(default)]
    pub availability: StoreTypeAvailability,
    /// Whether `arguments` is exhaustive. See `ArgumentCoverage`.
    #[serde(default)]
    pub coverage: ArgumentCoverage,
}
```

Builder methods (`new`, `with_label`, `with_doc`, `with_argument`, `unavailable`) follow the
`StoreConfig::with_prefix` style already in the moved code.

### `StoreConstructor` — the parametrisable half

```rust
pub type StoreConstructor = Box<dyn Fn(&StoreConfig) -> Result<Box<dyn AsyncStore>, Error>>;
```

**No `Send`/`Sync` bound, deliberately** — the same reasoning the existing `StoreFactory` trait
documents. A factory is transient (consumed while the router is built) and only the `AsyncStore` it
produces has thread requirements, which `AsyncStore` already states. A bound here would exclude the
browser factory, which holds JavaScript handles, and would foreclose `WEB-NATIVE-IO-TIER2`.

### `StoreTypeMap` — a factory assembled from named constructors

```rust
pub struct StoreTypeMap {
    entries: BTreeMap<String, (StoreTypeInfo, StoreConstructor)>,
}
```

`StoreTypeMap` overrides `resolve` with a map lookup rather than inheriting the scanning default,
so chain dispatch does not rebuild every `StoreTypeInfo` per store entry.

**`BTreeMap`, not `HashMap`:** `store_types()` feeds error messages that list supported types.
`HashMap` iteration order varies between runs, which would make those messages — and any test
asserting on them — nondeterministic. `BTreeMap` sorts by type name, which is also the order a
reader wants.

**Not `Serialize`:** it holds `Box<dyn Fn…>`. The *descriptions* serialize; the constructors do not.

### `ChainedStoreFactory` — first-wins composition

```rust
pub struct ChainedStoreFactory {
    factories: Vec<Box<dyn StoreFactory>>,
}
```

Order is significant and is the whole contract: the **first** factory whose `resolve` returns
`Some` creates the store, and the chain writes that resolved name onto the config before calling
`create`. Intended assembly is bottom-up — `liquers-core`, then `liquers-store`, then
`liquers-lib`, then the integration — so a core store type means the same thing everywhere by
default. A caller needing an override composes their own chain with their factory first; the API
permits it and the default ordering simply does not.

### The per-crate factory convention

Every crate contributing store types provides **exactly two things**, and this is the convention the
guide will teach:

| | Names | Contains |
|---|---|---|
| Its own factory | `CoreStoreFactory`, `OpendalStoreFactory`, `WebStoreFactory` | **only** that crate's own store types |
| Its default factory | `default_store_factory()` | its own chained after everything below it that should be available |

| Crate | Own factory claims | `default_store_factory()` returns |
|---|---|---|
| `liquers-core` | `memory`, `filesystem` | core only — nothing is below it |
| `liquers-store` | the OpenDAL types | core, then OpenDAL |
| `liquers-web` | `localstorage`, `js`, `http`, `https` | core, then web (**not** OpenDAL — it is not in the browser's graph) |

**One deviation, for a real reason.** `liquers-core`'s and `liquers-store`'s
`default_store_factory()` take no arguments; `liquers-web`'s takes a `WebStoreFactory`, because that
factory is *stateful* — it holds the page objects a `js` store entry can name, which are registered
at runtime and cannot come from a configuration document. The convention is about what each crate
provides, not about a signature every crate must match.

**The default factory is what a consumer should reach for.** Composing a chain by hand is for
someone who wants a different order or a subset. This keeps "which stores do I get" answerable by
naming one function per crate, and keeps each own-factory small enough to read.

### `StoreRouterBuilder` — no built-in knowledge

```rust
pub struct StoreRouterBuilder {
    config: StoreRouterConfig,
    factory: Box<dyn StoreFactory>,
}
```

The factory is a **required** field, not an accumulating `Vec` with a hidden fallback. This is the
structural expression of "there should not be a builtin": the builder cannot construct a store type
nobody gave it. `with_factory` **replaces** it; `chain_factory` is the convenience that chains onto
what is there and replaces. All ordering logic lives in `ChainedStoreFactory`, so the builder holds
exactly one factory and never has to reason about precedence.

### Moved verbatim

`StoreRouterConfig` and `StoreConfig` move to `liquers-core/src/store_config.rs` with one change:
`store_type` gains `#[serde(default)]`, so an entry whose type will be *inferred* can omit it. Today
nothing omits it, and an empty type resolves nowhere — the default `resolve` rejects it explicitly —
so the observable behaviour is unchanged. It is here because making the field required now would
force a format change later, which is the whole point of having run the URI audit first. `StoreConfig::metadata` moves as-is — it is documented "reserved for future use" and never
read, and removing it would be a breaking format change for no gain.

## Trait Implementations

### Trait: `StoreFactory` (moved and widened)

```rust
// liquers-core/src/store_factory.rs
pub trait StoreFactory {
    /// Store types this factory claims, with their configuration arguments and availability.
    fn store_types(&self) -> Vec<StoreTypeInfo>;

    /// Which of this factory's store types, if any, this entry describes.
    ///
    /// The default is an exact match on `config.store_type` against `store_types()`, which is
    /// today's behaviour. A factory **may override this to infer** the type from the entry —
    /// from a `uri`, or from anything else it recognizes.
    ///
    /// Taking the whole `StoreConfig` rather than a `&str`, and returning the resolved name
    /// rather than a bool, is what makes inference possible at all. See §Resolution below.
    fn resolve(&self, config: &StoreConfig) -> Option<String> {
        let requested = config.store_type.as_str();
        (!requested.is_empty() && self.store_types().iter().any(|t| t.store_type == requested))
            .then(|| requested.to_string())
    }

    /// Create a store from an entry this factory resolved.
    ///
    /// **Invariant: `config.store_type` is always the name `resolve` returned.** The chain fills
    /// it in before calling, so `create` never has to re-derive it or handle an empty type.
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}
```

### Resolution: the store type is an output, not only an input

`resolve` replaces the `claims(&str) -> bool` of this design's first draft, after the maintainer
observed the asymmetry that motivates it:

> A URI is allowed to be deliberately ambiguous; a store type is not. The store type may be
> **inferred** by the factory — whether or not there is a URI.

That is a better model than the one this document started with. A store type is the *resolved
identity* of an entry; a URI, a path shape, or anything else a factory recognizes is *input* to that
resolution. Making the type an output rather than only a lookup key has three consequences:

1. **All backend knowledge stays in the factory.** This design already concluded that a URI must be
   interpreted by the factory, because the mapping from a URI's authority to a configuration key is
   per-backend. Resolution is the same argument one step earlier: only the factory knows what its
   own entries look like.
2. **There is one resolution mechanism, not two.** An earlier sketch had the *chain* map URI schemes
   to types from a `uri_schemes` list while `claims` matched type names — two paths that could
   disagree. With `resolve`, `uri_schemes` on `StoreTypeInfo` becomes documentation and discovery,
   never dispatch.
3. **`create` gets a stronger contract.** The chain writes the resolved name onto the config before
   calling, so a factory's `create` can always trust `config.store_type`.

**Why this lands now rather than later.** Changing `claims(&str) -> bool` into
`resolve(&StoreConfig) -> Option<String>` after the fact is a **breaking trait change** for every
implementor that overrode it. Adopting it now costs one method's shape and defaults to exactly the
current behaviour; deferring it costs a break. This is the one non-additive finding of the URI
compatibility audit in [`design/store-config-uri/`](../store-config-uri/), and the reason that audit
was run before this design's gate rather than after.

**The risk, stated so it is not discovered later.** Inference from arbitrary configuration is magic,
and magic in a routing decision is how a document silently changes meaning — a factory that inferred
`filesystem` from the presence of a `path` key would reroute any other type that later gained a
`path`. Two rules keep it bounded, and the guide must state both:

- **A factory may only resolve to a store type it declares in `store_types()`.** The declared set
  stays the vocabulary; inference chooses within it, never beyond it.
- **Inference should key on something whose purpose is identification** — a `uri` scheme, an
  explicit type — not on the incidental presence of an argument.

No in-tree factory needs inference today; every one of them is served by the default. The capability
is forward-looking, and `resolve`'s default implementation means adopting it changes no behaviour.

**Bounds:** none, preserved from today and load-bearing (see `StoreConstructor` above).

**Object safety:** kept — no generic methods, no `Self` by value. `Box<dyn StoreFactory>` is used
throughout.

**This is a breaking change to `store_types()`**, whose return type goes `Vec<String>` →
`Vec<StoreTypeInfo>`. Justified rather than avoided: the trait has exactly two implementors outside
the crate that defines it (`WebStoreFactory`, and `CountingFactory` in `liquers-store`'s tests), both
in-tree and both edited by this change anyway; `liquers-py` does not use it. Adding a parallel
`store_type_info()` with a default implementation would leave two sources of truth for what a factory
claims, and the supported-types error would silently degrade for any factory that implemented only
the old one.

**Implementors:**

| Implementor | Crate | Claims |
|---|---|---|
| `StoreTypeMap` | `liquers-core` | whatever it was built with |
| `ChainedStoreFactory` | `liquers-core` | the union of its members, first-wins on duplicates |
| `OpendalStoreFactory` | `liquers-store` | `OPENDAL_STORE_TYPES` plus `opendal_*` prefixes |
| `WebStoreFactory` | `liquers-web` | `localstorage`, `js`, `http`, `https` (unchanged set) |

`ChainedStoreFactory::store_types()` returns the union with earlier members winning, so the list a
user sees matches the dispatch they will get.

`OpendalStoreFactory` is compiled **whether or not** the `opendal` feature is on. With the feature
off its `store_types()` still lists the OpenDAL types, each marked
`Unavailable("requires the 'opendal' feature")`, and `create` returns that message. Likewise the core
factory lists `filesystem` on every target, marked
`Unavailable("not available on wasm32: needs tokio::fs")` on wasm. This is what keeps `STORE13`
satisfied after the dispatch is split.

## Generic Parameters & Bounds

**None introduced.** Everything is concrete or `dyn`. `StoreRouterBuilder` deliberately takes
`Box<dyn StoreFactory>` rather than being generic over `F: StoreFactory`: the builder is constructed
once per environment, dynamic dispatch is irrelevant at that frequency, and a generic parameter would
propagate into every signature that stores or passes a builder.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `StoreFactory::create` | No | Constructs a store handle; performs no I/O. `AsyncMemoryStore::new` allocates, `AsyncFileStore::new` stores a path, `Operator::via_iter` is sync. |
| `StoreFactory::store_types` / `resolve` | No | Pure data; resolution parses at most a URI. |
| `StoreRouterBuilder::build` | No | Only calls `create` and `expand_env_vars`. |
| `expand_env_vars` | No | Reads the process environment; no I/O wait. |
| Everything the stores then do | Yes | Already async via `AsyncStore`; unchanged. |

This matches today exactly — no sync/async boundary moves. The async default applies to store
*operations*, which are untouched; configuration and construction are and remain synchronous.

## Function Signatures

### `liquers-core/src/store_config.rs`

Moved verbatim; signatures unchanged from `liquers-store/src/config.rs`:

```rust
impl StoreRouterConfig {
    pub fn new() -> Self;
    pub fn add_store(&mut self, store: StoreConfig);
    pub fn from_yaml(yaml: &str) -> Result<Self, Error>;
    pub fn from_json(json: &str) -> Result<Self, Error>;
    #[cfg(feature = "toml")]
    pub fn from_toml(toml: &str) -> Result<Self, Error>;
    pub fn to_yaml(&self) -> Result<String, Error>;
    pub fn to_json(&self) -> Result<String, Error>;
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;
}

impl StoreConfig {
    pub fn new(store_type: &str) -> Self;
    pub fn with_prefix(mut self, prefix: &str) -> Self;
    pub fn with_config(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self;
    pub fn key_prefix(&self) -> Result<Key, Error>;
    pub fn get_config_string(&self, key: &str) -> Option<String>;
    pub fn get_config_string_expanded(&self, key: &str) -> Option<Result<String, Error>>;
    pub fn require_config_string(&self, key: &str) -> Result<String, Error>;
    pub fn require_config_string_expanded(&self, key: &str) -> Result<String, Error>;
    pub fn config_as_string_map(&self) -> Result<HashMap<String, String>, Error>;
    pub fn expand_env_vars(&mut self) -> Result<(), Error>;
}

pub fn expand_env_vars(input: &str) -> Result<String, Error>;
```

`expand_env_vars` keeps its bare `std::env::var` and moves unchanged — **no `#[cfg]` gate and no
closure parameter.** On `wasm32-unknown-unknown` `std::env::var` compiles and returns `Err`, which is
the same behaviour the crate has today; `liquers-web` already avoids the path entirely via
`build_without_env_expansion`. Gating it would make the function absent rather than failing, breaking
`liquers-web`'s ability to *warn* about unexpanded `${…}`; a closure parameter would change every
call site to solve a problem nobody has. Documented rather than engineered around.

### `liquers-core/src/store_factory.rs`

```rust
/// Only the store types `liquers-core` implements: `memory`, and `filesystem` off wasm.
pub fn core_store_factory() -> StoreTypeMap;

/// Core's convenience chain. Nothing is below core, so this is `core_store_factory()`
/// in a chain — named for the convention, so a consumer writes the same call in every crate.
pub fn default_store_factory() -> ChainedStoreFactory;

impl StoreTypeMap {
    pub fn new() -> Self;
    pub fn with_store_type(self, info: StoreTypeInfo, create: StoreConstructor) -> Self;
}

impl ChainedStoreFactory {
    pub fn new() -> Self;
    /// Append: this factory is consulted after everything already in the chain.
    pub fn chain(self, factory: Box<dyn StoreFactory>) -> Self;
}

impl StoreRouterBuilder {
    pub fn new(config: StoreRouterConfig, factory: Box<dyn StoreFactory>) -> Self;
    pub fn from_yaml(yaml: &str, factory: Box<dyn StoreFactory>) -> Result<Self, Error>;
    pub fn from_json(json: &str, factory: Box<dyn StoreFactory>) -> Result<Self, Error>;

    /// Replace the builder's factory outright.
    pub fn with_factory(self, factory: Box<dyn StoreFactory>) -> Self;
    /// Convenience: chain `factory` after the current one and replace.
    /// Equivalent to `with_factory(ChainedStoreFactory::new().chain(current).chain(factory))`.
    pub fn chain_factory(self, factory: Box<dyn StoreFactory>) -> Self;

    pub fn build(mut self) -> Result<AsyncStoreRouter, Error>;
    pub fn build_without_env_expansion(self) -> Result<AsyncStoreRouter, Error>;
}

/// The error a chain returns for an entry no member resolves.
pub fn unknown_store_type_error(store_type: &str, known: &[StoreTypeInfo]) -> Error;
```

`with_factory` survives with **replace** semantics rather than the append it has today. The
distinction matters: append hid where in the order a factory landed, which is exactly the ambiguity
first-wins is meant to remove. Replace is unambiguous, and `chain_factory` covers the common case
without reintroducing a hidden position.

**Construction must be fast.** `StoreFactory::create` builds a handle; it must not fetch bulk data.
A store type that genuinely benefits from pre-fetching — a remote metadata database, say — is making
a trade-off it must document, because startup is where every store in a configuration is built and a
slow `create` is a slow start. This is the constraint that follows from validating on construction
rather than separately, and the guide states it.

### `liquers-store/src/store_factory.rs` (new)

```rust
pub struct OpendalStoreFactory;
impl StoreFactory for OpendalStoreFactory { /* … */ }

/// Core's store types, then OpenDAL's. The chain a native consumer wants.
pub fn default_store_factory() -> ChainedStoreFactory;

/// Field names and defaults for one OpenDAL service config, taken from the linked OpenDAL.
///
/// Sound because `Configurator: Serialize` is a trait bound, every service config derives
/// `Default`, and none carries `skip_serializing_if` — so serializing a default yields every
/// field. Returns an empty list rather than an error when the config is not a JSON object:
/// `ArgumentCoverage::Partial` already says the list may be incomplete, so "no arguments
/// described" is a correct answer rather than a failure.
#[cfg(feature = "opendal")]
fn derived_arguments<C: opendal::Configurator + Default>() -> Vec<StoreArgumentInfo>;
```

**With `opendal` off, derivation yields nothing** — the config types are not compiled — and the
OpenDAL types are already `StoreTypeAvailability::Unavailable` in that build, so an empty argument
list is consistent rather than a second degradation.

Retained in `liquers-store/src/config.rs` (unmoved): `OPENDAL_STORE_TYPES`, `is_opendal_store_type`,
`get_opendal_scheme`. They name backends core cannot build, so they stay with the factory that uses
them.

## Integration Points

### `liquers-core`

**New files:** `src/store_config.rs`, `src/store_factory.rs`. Declared in `src/lib.rs` after
`pub mod store;`.

**`Cargo.toml`:** one addition —

```toml
toml = { version = "0.8", optional = true }

[features]
toml = ["dep:toml"]
```

Same version and same optionality as `liquers-store` has today. Not in `default`, so no consumer's
dependency graph changes. Nothing else is added: `serde`, `serde_derive`, `serde_json` and
`serde_yaml` are already non-optional.

### `liquers-store`

**No compatibility shims — gate decision, "reuse core structures, don't shadow them; there is no
need to keep any backwards compatibility at this moment".** Phase 2's first draft kept
`liquers-store::config` as a re-export module so no call site would need editing. That is now
rejected: a re-export of a core type from `liquers-store` is exactly the shadowing the instruction
forbids, and it would leave two documented import paths for one type with nothing to choose between
them.

Consequently `liquers-store` shrinks to three files:

| File | Fate |
|---|---|
| `src/opendal_store.rs` | unchanged |
| `src/store_factory.rs` | **new** — `OpendalStoreFactory`, `default_store_factory()`, the OpenDAL type helpers, and the `create_router_from_*` convenience functions |
| `src/lib.rs` | declares the two modules; its `pub use config::{…}` line goes |
| `src/config.rs` | **deleted** — types to core; `OPENDAL_STORE_TYPES`, `is_opendal_store_type`, `get_opendal_scheme` fold into `store_factory.rs`, which is their only caller |
| `src/store_builder.rs` | **deleted** — `StoreFactory`/`StoreRouterBuilder` to core; `create_store` dissolved into the two factories; convenience functions to `store_factory.rs` |

`create_store` is removed rather than relocated: its memory and filesystem arms become
`core_store_factory()`, its OpenDAL arm becomes `OpendalStoreFactory`, and its unknown-type arm
becomes the chain's error. Nothing calls it outside its own tests.

**This supersedes the issue's verification item 1** — "`liquers_store::config::StoreRouterConfig`
still resolves (re-export); no call site edited" — which assumed the data-only boundary and a
compatibility shim. Both are gone. Call sites move to `liquers_core::store_config` and
`liquers_core::store_factory`; all of them are in this repository and are enumerated under
`liquers-web` below. Phase 5 rewrites that verification list in the issue file.

**`Cargo.toml`:** `toml = ["liquers-core/toml"]` — the feature forwards and the crate's own `toml`
dependency is dropped, since nothing in `liquers-store` parses TOML once `from_toml` lives in core.
The `opendal` feature is **kept** — non-OpenDAL backends are expected in this crate — but its manifest comment must
change: the wasm-consumer justification it gives is exactly what this design removes.

### `liquers-web`

**`Cargo.toml`:** delete the `liquers-store` line.

**`src/store/builder.rs`:** imports move to `liquers_core::store_config` / `store_factory`;
`WebStoreFactory::store_types` returns `Vec<StoreTypeInfo>` describing `localstorage` (`namespace`,
`quota_bytes`), `js` (`object`) and `http`/`https` (`url_prefix`, `keys`) — arguments the module
currently documents only in a doc-comment YAML block. `build_router` becomes:

```rust
ChainedStoreFactory::new()
    .chain(Box::new(core_store_factory()))
    .chain(Box::new(factory))
```

and the module doc's "factories are consulted **before** the built-in types" paragraph is replaced by
the first-wins rule plus the reason the browser's `http` still wins (nothing else in this chain claims
it).

**`src/environment.rs`, `tests/store_js_STORE.rs`, `tests/eval_EVAL.rs`:** import paths only.

### `scripts/check-build-matrix.sh`

Two changes, both load-bearing:

1. Its header justifies the `liquers-store` wasm32 row as proving "the dependency edge liquers-web
   relies on". That edge is deleted; the row's remaining purpose is the `opendal`-off feature split,
   which is still real. Rewrite the comment rather than the row.
2. **Add `liquers-core` rows — the crate has none today.** Core gains an optional feature (`toml`)
   and target-conditional code (`filesystem` availability, `AsyncFileStore`) for the first time, and
   the native default build exercises neither the feature-off nor the wasm path. Proposed:
   `""`, `"--no-default-features"`, `"--features toml"`, and
   `"--target wasm32-unknown-unknown"`.

## Documentation Architecture

### Reference Plan

**Extend `specs/reference/STORE_CONFIG_FSD.md`** (existing; `kind: reference`, `audience: internal`,
`area: [store/config]`). Changes:

- Retitle and rescope: the format, the factory seam and the builder are `liquers-core`'s; OpenDAL
  backends are `liquers-store`'s. The current title says "for `liquers-store`".
- New section on the factory model: the trait, `StoreTypeInfo`/`StoreArgumentInfo`, first-wins
  chaining, the default factories each crate offers, and that overriding is done by composing a
  chain rather than by a precedence rule.
- New section on unclaimed types: the error lists supported types and, separately, known-but-
  unavailable ones with the reason — the `STORE13` contract, now factory-borne.
- Update `area` to `[core/store, store/config]`.
- `## History` row and `reviewed:` bump dated in the same commit (§9.2).

No new reference: a second document describing the same format would compete with this one.

### Guide Plan

**Create `specs/guides/STORE_FACTORY_GUIDE.md`** — `kind: guide`, `audience: internal`,
`area: [core/store, store/config]`. Phase 1's original `neither` no longer holds, because the change
creates genuinely repeatable tasks with non-obvious answers:

| Task | Answer the guide gives |
|---|---|
| Add a store type to an existing crate | `StoreTypeMap::with_store_type` with a `StoreTypeInfo` |
| Contribute store types from an integration | Implement `StoreFactory`; `WebStoreFactory` is the worked example |
| Override a store type someone else defines | Compose a `ChainedStoreFactory` with yours first |
| Choose a chain for my build | `core_store_factory()` vs `liquers_store::default_store_factory()` |
| Say a type exists but is unavailable here | `StoreTypeAvailability::Unavailable(reason)` — and why `STORE13` requires it |

Links to `liquers-web/src/store/builder.rs` as executable evidence, and to `STORE_CONFIG_FSD.md` for
the format itself.

### Other Documents to Create

**None.** The Phase 5 summary carries the rest.

### New Reference or Guide Documents

| Path | Kind | Audience | Area | Purpose |
|---|---|---|---|---|
| `specs/guides/STORE_FACTORY_GUIDE.md` | guide | internal | `core/store`, `store/config` | How to define, contribute, chain and override store types |

### Existing Documents to Review or Update

Candidates generated from `area: [core/store, store/config, store/backends, web, docs]`, then kept
or discarded:

| Document | Decision | Change |
|---|---|---|
| `reference/STORE_CONFIG_FSD.md` | **keep** | Rescope + factory model + unclaimed-type contract; `History`; `reviewed:` |
| `reference/api/DOC_01_ARCHITECTURE_REFERENCE.md` | **keep** | Line 128: `liquers_core::{StoreConfig, StoreRouterBuilder}`; the table exists to stop agents inventing imports, so a stale row actively misleads |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | **keep** | See the reversal below — the largest single edit |
| `README.md` (repo root) | **keep** | Line 93: config/builder under `liquers_core`, OpenDAL under `liquers_store` |
| `CLAUDE.md` | **keep** | "Adding a Store Backend" — the four steps change crate and now begin from a factory, not a `match` arm |
| `specs/DOCS_STRUCTURE_GUIDE.md` §3 | **keep** | See the decision below |
| `scripts/check-build-matrix.sh` | **keep** | Header rationale + new `liquers-core` rows (see Integration Points) |
| `design/liquers-web-store/` | **keep** | Add a supersession note: its factory-precedence rationale no longer describes the code |
| `reference/PROJECT_OVERVIEW.md` | **discard** | Names `liquers-store (storage backends)` in a crate tree, which remains accurate — the crate keeps its backends |
| `reference/WEB_API_SPECIFICATION.md` | **discard** | No store-configuration content |
| `reference/ASSETS.md`, `DEPENDENCIES_STATUS.md`, `ASSET_LIFECYCLE.md` | **discard** | Share `core/store` by area only; concern asset lifecycle, not store construction |

#### The `area` vocabulary — decided, not asked

Phase 1 raised this as a question; it is bookkeeping, so it is decided here. §3 is a closed
vocabulary and `store/config` is currently defined by two filenames — "`liquers-store`: `config.rs`,
`store_builder.rs`" — **both of which this design deletes.** Two options:

- *Retire `store/config`.* Clean, but every row carrying it (`STORE_CONFIG_FSD.md` and several
  issues) needs remapping, and retiring a value from a closed vocabulary is churn spread across
  unrelated documents.
- *Redefine it by topic rather than by file list.* One row edited, every existing row still valid.

**Redefine.** `store/config` becomes "store configuration and construction: `liquers-core`'s
`store_config.rs` and `store_factory.rs`, and `liquers-store`'s `store_factory.rs`", and the
`core/store` row stays as-is (`store.rs`, `cache.rs` — the traits). This keeps the `area` value
pointing at the *subject* a reader searches for, which is what §3 values are for, and avoids
rewriting front-matter in documents this design does not otherwise touch. The `store/backends` row
is unchanged.

**Authoritative `affects_docs`:** `reference/STORE_CONFIG_FSD.md`,
`reference/api/DOC_01_ARCHITECTURE_REFERENCE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`,
`guides/STORE_FACTORY_GUIDE.md`.

#### The guide reversal — the finding that most affects documentation

`LANGUAGE-INTEGRATION_GUIDE.md` §"Taking only part of the store support crate" states the problem
this design solves, enumerates three resolutions, and **recommends option 3 while explicitly
rejecting option 2, which is what this design does**:

> 2. **Move the shared types into `liquers-core`.** Works, but widens core for one consumer's
>    benefit and separates the format from the crate whose reference documentation describes it.
> 3. **Make the heavy backends an optional feature of the support crate, enabled by default.**
>    Recommended.

That recommendation is now wrong, and honestly so: it was written when `liquers-web` was the only
consumer, and its objection — "one consumer's benefit" — no longer holds once `liquers-core` itself
must embed a store description for `EnvironmentConfig`. The second objection (separating format from
documentation) is answered by rescoping `STORE_CONFIG_FSD.md` rather than by leaving the code where
it was. The section must be rewritten to record option 2 as taken, with the reason the trade-off
changed, and to keep option 3's three hard-won cost lessons — they remain true of `liquers-store`'s
surviving `opendal` feature.

Two conformance items in the same guide also move:

- **`STORE12`** requires that "one that overrides a shared type name resolves to the *integration*'s
  implementation". After this change `liquers-web` has nothing to override — the OpenDAL factory that
  claimed `http` is not in its chain. The test must be restated in terms of chain order (a factory
  chained *before* another wins) or marked `NA` for an integration that composes its own chain.
- **`STORE13`** is preserved exactly, and gains a mechanism: `StoreTypeAvailability::Unavailable`.
  Worth saying so in the guide, since it is the requirement that justifies the field.

### Design and Capability Links

- `specs/README.md` §Stores: "Store configuration" currently points at `reference/STORE_CONFIG_FSD.md`
  (documented). Add a "Store factories and construction" line pointing at the new guide, and add this
  design folder to the map per `CLAUDE.md`.
- `specs/index.csv`: rows for this design and the new guide; `STORE-CONFIG-IN-CORE` to `closed`.
- `design/environment-builder/DESIGN.md`: prerequisite table — the layering constraint is lifted.
- `design/liquers-web-store/DESIGN.md`: supersession note on the factory-precedence rationale.

### Evidence to Collect During Implementation

- Whether `StoreArgumentType`'s `Array` and `Object` variants earn their place. `Array` has exactly
  one known user (the browser `http` type's `keys`); `Object` has none yet and is included only
  because adding a serde enum variant later is breaking.
- The actual argument descriptions written for OpenDAL types: how many are there, and does writing
  them reveal that `OPENDAL_STORE_TYPES` is a bare list with no documentation anywhere?
- Whether deleting `liquers-store::config` and `store_builder` breaks anything outside the
  repository's visibility. Every in-tree call site is enumerated; external consumers are not, and the
  no-backwards-compatibility decision accepts that.
- Feature-matrix surprises from making `toml` optional in core, of the kind the guide's option-3
  cost list predicts.
- Whether `StoreRouterBuilder::new`'s new arity is ergonomic in practice or wants a convenience
  constructor.

## Relevant Commands

### New Commands

**None.** This design registers, removes and re-signs no command. `specs/command_registry.yaml` is
untouched and `cargo test -p liquers-lib --test registry_export` must stay green unchanged — which is
itself a useful regression check that the change did not leak into the command surface.

### Relevant Existing Namespaces

No command namespace is involved. For completeness, the namespace that *would* be relevant if this
work were extended is the one that does not exist yet: `STORE-COMMAND-NAMESPACE-MISSING` (P3) records
that store contents cannot be read or written from a query at all. Out of scope.

**Ask user:** confirmed nil — no namespace decision is needed for this design.

## Web Endpoints

**None.** No route is added or changed. `liquers-web`'s `configure_store` JS entry point keeps its
signature; only the Rust type behind `JsStoreConfig` changes crate.

## Error Handling

### New Error Types

None. All errors are `liquers_core::error::Error` via typed constructors.

### Error Scenarios

| Scenario | Constructor | Notes |
|---|---|---|
| An entry no factory in the chain resolves | `Error::not_supported` | **Changed from `general_error`.** `ErrorType::NotSupported` is what this actually is, and the only existing assertion on it (`test_unknown_store_type`) checks `is_err()` only. |
| `store_type` claimed but `Unavailable(reason)` | `Error::not_supported` | Message is the reason verbatim — the `STORE13` contract |
| Required config key missing | `Error::general_error` | Unchanged: `require_config_string`'s existing message |
| `${VAR}` unset or unclosed | `Error::general_error` / `ParseError` | Unchanged; moves with `expand_env_vars` |
| YAML/JSON/TOML parse failure | `Error::new(ErrorType::ParseError, …)` | **Pre-existing violation of the no-`Error::new` rule**, in code being moved — see below |
| Backend construction fails (OpenDAL) | `Error::general_error` | Unchanged |

**Pre-existing rule violation carried by the move.** `StoreRouterConfig::from_yaml` / `from_json` /
`from_toml` and `expand_env_vars` construct errors with `Error::new(ErrorType::ParseError, …)`, which
`CLAUDE.md` forbids. There is no typed constructor for a bare parse error that is not a key or query
parse (`key_parse_error` and `query_parse_error` both require a `Position`). Options: add
`Error::parse_error(message: String)` to `liquers-core/src/error.rs` and use it, or move the code
unchanged and file the gap. **Proposed: add the constructor** — the code is landing in `liquers-core`,
where the rule is enforced most strictly, and moving a known violation *into* core and leaving it
there is worse than the one-line addition. Flagged for the approval gate because it touches
`error.rs`, which is outside the boundary Phase 1 drew.

### Unclaimed-Type Message Shape

```
Unknown store type "postgress". Supported store types: filesystem, js, localstorage, memory.
Known but unavailable in this build: fs, gcs, s3 (requires the 'opendal' feature).
```

Assembled from `ChainedStoreFactory::store_types()`, so it is accurate for the build in hand rather
than describing a type set that may not be compiled in. `unknown_store_type_error` is public so a
future configuration validator produces the identical message.

## Serialization Strategy

`StoreTypeInfo`, `StoreArgumentInfo` and `StoreTypeAvailability` derive `Serialize, Deserialize`.
This is not incidental: it makes the store-type set exportable the way `specs/command_registry.yaml`
exports commands, and lets a UI render a configuration form from the same data the error message uses.
`StoreTypeMap` and `ChainedStoreFactory` are **not** serializable — they hold `Box<dyn Fn…>` and
`Box<dyn StoreFactory>`. The split is deliberate: descriptions are data, constructors are code.

An exporter binary is **not** in scope; the derives make one cheap later.

Round-trip expectation for Phase 3: a `StoreRouterConfig` parsed from YAML, serialized to JSON and
re-parsed is unchanged — the existing behaviour, re-asserted in core after the move.

## Concurrency Considerations

**No shared state, no locks, no new thread-safety surface.** A factory is constructed, consumed while
the router is built, and dropped. The `AsyncStoreRouter` it produces has whatever thread properties
`AsyncStore` already states.

The **absence** of `Send`/`Sync` bounds is the concurrency decision here, and it is a constraint to
preserve rather than an omission — `WebStoreFactory` holds `js_sys::Object` handles and is `!Send`,
and `WEB-NATIVE-IO-TIER2` will add another. Adding a bound later would be a breaking change for the
browser.

## Compilation Validation

- [x] All signatures specified; no `unwrap()`/`expect()` in any of them
- [x] Trait bounds minimal — none, justified by the browser factory
- [x] `StoreTypeAvailability` matched exhaustively; no `_ =>` arm planned
- [x] Dependency flow one-way: core ← store ← web, and code moves down it
- [x] Imports named: no cross-module reuse remains — `StoreArgumentType` is local to `store_factory`

Checks to run in Phase 4:

```bash
cargo check -p liquers-core
cargo check -p liquers-core --no-default-features
cargo check -p liquers-core --features toml
cargo check -p liquers-core --target wasm32-unknown-unknown
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

## References to liquers-patterns.md

- [x] Crate dependencies follow the one-way flow
- [x] No `ExtValue` change — no new value type
- [x] No `register_command!` involvement
- [x] `AsyncStore` pattern untouched; async remains the default for store operations
- [x] Error handling uses typed constructors, **with one pre-existing violation to fix** (above)
- [x] Feature gating: `toml` gated in core and forwarded from `liquers-store`; `opendal` gating
      preserved and its rationale comment corrected

## Inline Review Findings

Recorded from the checklist and the `rust-best-practices` lens, since the parallel reviewer agents
were run as sequential passes in this session rather than spawned.

**Pass A — Phase 1 conformity.** Every Phase 1 interaction is addressed; no scope drift found. Two
items Phase 1 left open are resolved *inside* its stated boundary (`with_factory` removed;
`expand_env_vars` moved verbatim). One item crosses it: adding `Error::parse_error` touches
`liquers-core/src/error.rs`, which Phase 1 did not list. Surfaced at the gate rather than absorbed.

**Pass B — codebase alignment.** Signatures checked against `liquers-store/src/store_builder.rs`,
`liquers-web/src/store/builder.rs` and `liquers-core/src/store.rs`. Findings acted on:
`Error::not_supported` already exists and is used
instead of `general_error`; `AsyncMemoryStore`/`AsyncFileStore` are already core types, so the core
factory constructs nothing new; `BTreeMap` chosen over `HashMap` for deterministic error text;
`liquers-core` has **no** build-matrix row today, found by reading `scripts/check-build-matrix.sh`
rather than by area search.

**Revised after the Phase 1 gate answers (second pass).** Four decisions changed this document
materially: `StoreArgumentType` replaces the `ArgumentType` reuse (JSON vocabulary, and
`ArgumentType` cannot express `keys: [...]`); all compatibility shims are dropped, deleting
`liquers-store`'s `config.rs` and `store_builder.rs` and superseding the issue's verification item 1;
`with_factory` returns with replace semantics plus a `chain_factory` convenience; and the per-crate
"own factory + default chained factory" convention is now stated explicitly as the pattern the guide
teaches. The registry-export and validate-without-construction directions are withdrawn, not
deferred.

**Open for the user, not resolvable from context:** the `Error::parse_error` addition; whether
`StoreArgumentType::Object` should ship with no known user; and how far `STORE12` should be restated
versus marked `NA`.
