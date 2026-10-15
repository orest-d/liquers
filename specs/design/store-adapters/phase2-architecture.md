# Phase 2: Solution & Architecture — store-adapters

## Overview

Option (C) of Phase 1 question 1. The **access policy** is a decorator, `ReadOnlyStore`, over any
`AsyncStore`. **Metadata and writes** go through one generic store, `MetadataPipelineStore`, built
from four parts: a backend reached through new **data-only methods** of `AsyncStore` (default:
refuse), a **storage model**, **handler chains** for the four stages, and a **write strategy**.
Every part is named in the configuration by one mechanism: actions of a foreign realm,
`store_config`, declared in the shared registry, written as queries and compiled into closures at
store build (§4, §Configuration). A mount with no `metadata` section is not wrapped at all, so today's behaviour is
untouched (AC-6). A router with central storage wraps itself in the same pipeline, its members
reached through their data-only methods (AC-12).

Rejected: decorators for everything (no layer sees data and metadata as one write, a listing pays
one call per entry per layer); a separate data-only backend trait every store implements (the
user's objection: it touches every store); a per-backend "do not write on read" flag (each backend
reimplements it, and the storage model stays inside the backend).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | P2 | Locks are per process; two processes writing one group or key still race | No | No | Documented limit (Phase 1 Q10) |
| `CORE-STORE-OPENBIN-MISSING` | accepted | P3 | `created` under a streaming writer (Phase 1 Q8); no method to wrap today | No | No | Note in `STORE_SEMANTICS.md` |
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | Alternative (a) of §Configuration would put metadata functions in a command namespace | No | No | Decided with question 2 |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | A handler defined in JavaScript or Python needs the same registry as a store type | No | No | Registry is `dyn`, so the issue can extend it later |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` | draft | P3 | Handler chains pass `MetadataRecord` by value; one copy per stage | No | No | Accepted cost; records are small |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | — | P2 | Front-matter fields beyond title/description have nowhere to go | No | No | Out of scope (Phase 1 non-goals) |

No blocker. `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO`, required by AC-8, closed 2026-10-13.

## Interfaces

All in `liquers-core/src/store/` (a new module tree beside `store.rs`, re-exported), unless noted.
Async traits use the existing `#[cfg_attr(not(target_arch = "wasm32"), async_trait)]` /
`async_trait(?Send)` pair and `MaybeSend + MaybeSync` bounds, so the whole design builds for
`liquers-web`.

### 1. Data-only access — new `AsyncStore` methods with refusing defaults

```rust
// AsyncStore additions (store.rs). Defaults return Error::not_supported(..).
async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error>;          // bytes only; never touches metadata
async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error>;
async fn remove_data(&self, key: &Key) -> Result<(), Error>;
async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error>; // atomic where DataCapabilities::rename
async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error>;  // raw names: no sidecar filtering
async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error>; // Ok(None): no data object
async fn get_stored_metadata(&self, key: &Key) -> Result<Option<Metadata>, Error>; // the backend's own record, never repaired
fn data_capabilities(&self) -> DataCapabilities { DataCapabilities::default() } // all false
fn is_read_only(&self) -> bool { false }                                // the AC-1 runtime accessor
async fn sync(&self) -> Result<SyncReport, Error> { Ok(SyncReport::default()) } // Phase 1 Q12
fn version_verification(&self, _key: &Key) -> Option<VersionVerification> { None } // Phase 1 Q4

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataCapabilities { pub data_access: bool, pub atomic_write: bool, pub rename: bool,
                              pub modified_time: bool, pub etag: bool }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFacts { pub size: u64, pub modified: Option<BackendTime>, pub etag: Option<String> }

/// A backend timestamp in one form: RFC 3339, UTC, truncated to the backend's own precision, so
/// a value written to metadata and read back compares exactly with a fresh `data_facts` (Q8).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BackendTime(String);
impl BackendTime { pub fn new(t: chrono::DateTime<chrono::Utc>, precision: TimePrecision) -> Self; } // chrono: SystemTime::now is unavailable on wasm32
pub enum TimePrecision { Seconds, Millis, Micros, Nanos }
```

This answers Phase 1 question 1's condition: seven data methods and four answers about the store,
all defaulted, so no existing implementation changes and no second store trait exists. `is_read_only`
and `sync` sit on `AsyncStore` rather than on the decorators because the router, the HTTP API and
`liquers-web` hold only `Arc<dyn AsyncStore>`. Implemented by `AsyncFileStore`, `AsyncMemoryStore` (modified time from `chrono::Utc::now()` at
write) and `AsyncOpenDALStore` (`liquers-store`). Their data-only path builders reserve only what
the backend itself needs in that mode (`.__lock__` for the file store); the pipeline enforces the
storage model's reserved names instead. The router forwards all of them to the owning member,
answers `is_read_only` as "it has members and every member is", and `data_capabilities` as the
intersection of its members'. `version_verification` lets the asset manager, which does the
verifying (`decide_external_change` in `assets.rs`), ask the store for a mount's policy: `Some` from
a mount with a `content_hash` handler overrides `AssetManagerOptions`, `None` keeps the environment
default. `get_stored_metadata` is what makes M1 possible
before the pipeline exists: `ReadOnlyStore` reads data and stored metadata through it and
synthesizes the rest without writing.

### 2. Access policy

```rust
pub struct ReadOnlyStore { inner: Arc<dyn AsyncStore> }
impl ReadOnlyStore { pub fn new(inner: Arc<dyn AsyncStore>) -> Self; }
impl AsyncStore for ReadOnlyStore { /* set, set_metadata, remove, removedir, makedir, set_data,
    remove_data, rename_data → Error::not_supported; is_read_only → true; reads: if the inner store
    has data access, get_data + get_stored_metadata + the default chain in memory, else forward */ }
```

A store **without** data-only access is forwarded as it is; if it writes on read, `ReadOnlyStore`
cannot stop it. That limit of AC-1 is documented, and the factory warns when it wraps such a store
(today: none of the built-in ones, after this design; any `liquers-web` or third-party store).

### 3. Storage model

```rust
#[maybe_async_trait] // the cfg_attr pair of AsyncStore (store.rs): async_trait / async_trait(?Send) on wasm32
pub trait MetadataStorage: MaybeSend + MaybeSync {
    fn name(&self) -> String;
    fn reserved_names(&self) -> ReservedNameSet;         // owned: names come from configuration
    async fn get(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<MetadataRecord>, Error>;
    async fn get_folder(&self, backend: &dyn AsyncStore, folder: &Key)
        -> Result<Option<Vec<(String, MetadataRecord)>>, Error> { Ok(None) } // Some: one read per folder (AC-11)
    /// The objects to write so that `key` has `record` (None: remove it). Grouped models read and
    /// patch their group file here, under the group lock the pipeline holds.
    async fn plan(&self, backend: &dyn AsyncStore, key: &Key, record: Option<&MetadataRecord>)
        -> Result<Vec<ObjectWrite>, Error>;
    fn group_of(&self, key: &Key) -> Option<Key> { None } // the lock and cache unit
}
```

`ReservedNames` stays the `const`, `'static` type the backends use; `ReservedNameSet { suffixes,
exact, prefixes: Vec<String> }` is its owned counterpart for models and strategies, and both
implement one predicate (`fn reserves(&self, segment: &str) -> bool`), so §8's "any segment" rule is
written once. Built: `NativeStorage` (the backend's own sidecar through `get_stored_metadata` / `set_metadata`;
the default, byte-identical to today), `PerEntryStorage { template: KeyTemplate }` (the four
templates of Phase 1), `GroupedStorage { file: String }` (`__metadata__.json`, a JSON map name →
record), `NoStorage`, and a test-only `MemoryCentralStorage`. A database model implements the same
trait and returns `plan` empty, committing in its own transaction (interface only).

### 4. Handlers — the `store_config` realm and its plan compiler

Decided 2026-10-15 (question 2, alternative b, refined by the user at the Phase 2 gate). A handler is
an action of a **foreign realm**, `store_config`, declared in the **same** `CommandMetadataRegistry`
as every other command — so queries can *address* it (parse, plan, validate, export to
`specs/command_registry.yaml`) — but never *executed* by the interpreter: its subject is a
`MetadataRecord`, which is not a `Value`, so it cannot exist in the `CommandExecutor`, and it does
not need to coexist with realm `""`. (If that is ever needed, `Value` can be extended; for now the
realm is a study of how a quite foreign realm sits beside the rest.) Evaluating a `store_config`
query through the asset manager fails with `Error::not_supported` naming the realm.

**Realm name.** *Proposed:* `store_config` — the realm exists only inside store configuration, its
queries configure rather than compute, and the name reads as what it is in a header
(`-store_config/…`). Considered: `storeconf` (shorter, less obvious), `store_handlers` (too
narrow: the realm also names storage models, write strategies and presets), `store` (collides in
the reader's mind with the planned `store` command namespace, `STORE-COMMAND-NAMESPACE-MISSING`).

**No interpreter: a plan compiler.** A chain is planned once by the existing `PlanBuilder` and then
**compiled into one closure**; nothing walks the plan at run time. All closures are built at
configuration time and owned by the store, so there is no `Arc`, lock or registry lookup on the
read or write path, and the record is passed by `&mut`:

```rust
// liquers-core/src/store/store_config.rs
pub struct FactsIn<'a> {
    pub key: &'a Key,                         // a fact, so key-specific steps are possible later
    pub facts: Option<&'a DataFacts>,         // size, backend time, ETag — no bytes needed
    pub bytes: Option<&'a [u8]>,              // present only when the chain asked for them
    pub previous: Option<&'a MetadataRecord>, // the record as stored, for `on_change` comparisons
}
#[derive(Default)]
pub struct StepOut { pub data: Option<Vec<u8>> } // replacement bytes: front-matter write-back (AC-5)

#[cfg(not(target_arch = "wasm32"))]
pub type CompiledFn = Box<dyn Fn(&mut MetadataRecord, &FactsIn<'_>, &mut StepOut) -> Result<(), Error> + Send + Sync>;
#[cfg(target_arch = "wasm32")]
pub type CompiledFn = Box<dyn Fn(&mut MetadataRecord, &FactsIn<'_>, &mut StepOut) -> Result<(), Error>>;
pub type NeedsBytesFn = Box<dyn Fn(&FactsIn<'_>) -> bool + Send + Sync>; // same cfg pair

pub struct CompiledChain { pub needs_bytes: NeedsBytesFn, pub run: CompiledFn }

/// One step's builder: hard-coded where the handler is defined, a plain `fn` pointer.
pub type StepBuilder = fn(&ResolvedParameterValues) -> Result<CompiledStep, Error>;
pub struct CompiledStep { pub needs_bytes: NeedsBytesFn, pub run: CompiledFn }

#[derive(Default)]
pub struct StoreConfigFunctions { steps: HashMap<CommandKey, StepBuilder>,
                                  components: HashMap<CommandKey, ComponentBuilder> }
impl StoreConfigFunctions {
    pub fn builtin() -> Self;
    pub fn compile(&self, spec: &str, stage: Stage, registry: &CommandMetadataRegistry)
        -> Result<CompiledChain, Error>;
}
/// Hard-coded registration, next to the handlers it registers (no macro):
pub fn register_builtin_store_config(cmr: &mut CommandMetadataRegistry, f: &mut StoreConfigFunctions)
    -> Result<(), Error>;   // liquers-lib has its own, for `front_matter`
```

`Fn`, not `FnMut`: the store calls a chain from many tasks at once through `&self`. `Send + Sync`
because the store is shared as `Arc<dyn AsyncStore>`; the wasm32 alias drops them, as
`MaybeSend` does elsewhere (a trait object cannot name `MaybeSend`, which is not an auto trait).
`&mut MetadataRecord` rather than moving it: composing steps is then a loop, not a chain of moves,
and the record never leaves the pipeline's hands. The steps are moved into the compiled closure,
which loops over them; `needs_bytes` is the `or` of the steps' predicates, evaluated before the
chain so the async pipeline can fetch the bytes first — steps compare against `previous`, not
against the record being built, so the order of `file_facts` and `content_hash` cannot hide a
change. In the **read** stage the compiler wraps each step so that its error becomes a warning in
the record and the chain continues (Phase 1 Q9); a step must therefore check before it mutates. In
the **validate** stage an error aborts the write (AC-14).

**How each plan step compiles** (`plan.rs` `Step`, matched exhaustively):

| Step | Compiles to |
|---|---|
| `Action` in realm `store_config` | the registered `StepBuilder` called with the resolved parameters (literals only; a link, placeholder or `DefaultLink` is a `configuration_error`) |
| `Info(m)`, `Warning(m)`, `Error(m)` | a closure appending `m` to the record's log at that level (`Error` logs, it does not fail) |
| `Plan(p)` | the nested plan, compiled in place |
| `SetCwd` | refused: CWD is not supported |
| `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`, `UseKeyValue` (`-R-key/…`) | refused for now (below) |
| `GetAsset`, `GetAssetBinary`, `GetAssetMetadata`, `GetAssetRecipe`, `GetAssetDirectory`, `Evaluate`, `UseQueryValue`, `Filename` | refused: they need the asset layer or describe an output |

Every refusal is `Error::configuration_error` with the step's position, at store build — a misspelt
handler fails the build, never a read.

**`-R-key/…` — analysed, not supported now.** A resource segment in a `store_config` query would
mean "these steps apply to this key or folder", e.g. `-R-key/docs/notes/-store_config/front_matter-title`
(parses as a resource segment with the `key` instruction followed by a `store_config` transform
segment). It compiles naturally into a guard: `if facts.key.has_prefix(k) { steps }` — which is why
`FactsIn` carries the key now, so the shape need not change later. Open points for when it is
needed: whether a guard matches the key exactly or as a prefix (the `key` instruction today names
one key); how several guarded chains combine (first match or all, in order); and how it interacts
with presets (a preset could carry guarded chains, e.g. `front_matter` only under `*.md`, which
would need a pattern, not a key). Recorded as an issue at Phase 5 if not implemented.

**Built-in handlers** (in `liquers-core`): `defaults`, `require`, `media_type`, `file_facts` (size,
`created`, ETag from the facts), `content_hash` (`off`/`always`/`on_change`; hashing reuses
`Version::from_bytes`; it keeps `version` current in listings, its mount's policy is what
`version_verification` reports, a change it detects is handled as `verify_stored_versions` handles
one — by the asset manager's `ExternalChangePolicy` — and `verify_stored_versions` stays the
whole-store check that catches what `on_change` misses, AC-4), `updated`. `front_matter`, in
`liquers-lib`, reuses `liquers_records::search::front_matter_end`; in the write chain it sets
`StepOut::data` (write-back). Every handler that sets fields takes `mode` — `fill` keeps a value
already present, `overwrite` (default) replaces it (AC-15). A `LegacyMetadata` record bypasses the
chains and is stored and returned untouched, with a warning: `Metadata::metadata_record` has no
lossless conversion, and the legacy branch exists to keep unknown fields.

### 5. Write strategy

```rust
#[maybe_async_trait] // the cfg_attr pair of AsyncStore (store.rs): async_trait / async_trait(?Send) on wasm32
pub trait WriteStrategy: MaybeSend + MaybeSync {
    fn name(&self) -> String;
    fn requires(&self) -> DataCapabilities;               // checked at build: AC-17 error
    fn reserved_names(&self) -> ReservedNames;            // staging and journal names
    async fn commit(&self, backend: &dyn AsyncStore, writes: WriteSet) -> Result<(), Error>;
    async fn recover(&self, backend: &dyn AsyncStore, key: &Key) -> Result<Option<Recovery>, Error>;
    async fn sync(&self, backend: &dyn AsyncStore) -> Result<SyncReport, Error>;
}
pub struct WriteSet { pub primary: Key, pub objects: Vec<ObjectWrite>, pub prepared: Vec<PreparedWrite> }
pub struct ObjectWrite { pub target: WriteTarget, pub key: Key, pub bytes: Option<Vec<u8>> } // None: delete
pub enum WriteTarget { Data, Metadata, Backup(usize) } // the backend; the storage model's store; a member's backup
/// A write a storage model cannot express as objects (a database): two-phase, so the journal can
/// record it between prepare and commit and recovery can finish or abandon it.
pub struct PreparedWrite { pub storage: Arc<dyn MetadataStorage>, pub id: String }
pub struct Recovery { pub key: Key, pub outcome: RecoveryOutcome }   // RolledForward | RolledBack
#[derive(Debug, Clone, Default)] pub struct SyncReport { pub repaired: Vec<Recovery>, pub errors: Vec<(Key, Error)> }
```

`MetadataStorage` gains `async fn prepare(&self, key, record) -> Result<Option<String>, Error>
{ Ok(None) }` and `async fn finish(&self, id: &str, commit: bool) -> Result<(), Error> { Ok(()) }`;
object models never use them. Objects of a central model live in the store that model names, so
`WriteTarget::Metadata` is not the backend.

Built: `DirectWrite` (in order, as today); `AtomicWrite` — where the backend writes an object
atomically (object stores) it writes directly, otherwise it stages to `<key>.__staged__` and calls
`rename_data`, so its `requires()` is "`atomic_write` or `rename`" and only a backend with neither
is refused (AC-17); `JournalWrite` — every object staged, prepared writes prepared, then a journal
`<primary>.__txn__` listing them all, then the moves (`rename_data`, or copy then delete where the
backend has no rename), the prepared writes finished, the journal deleted; `recover` rolls forward
when the journal exists and back (staged objects deleted, prepared writes abandoned) when it does
not. Requires `data_access` only. A versioned-object strategy implements the same trait
later (Phase 1 Q11).

### 6. The pipeline store

```rust
pub struct MetadataPipelineStore {
    backend: Arc<dyn AsyncStore>, storage: Arc<dyn MetadataStorage>,
    chains: StageChains,                         // a CompiledChain per Stage, owned
    writes: Arc<dyn WriteStrategy>, persist_defaults: bool,
    cache: MetadataCache,                        // key → (record, DataFacts); group → (records, facts)
    locks: GroupLocks,                           // scc::HashMap<Key, Arc<tokio::sync::Mutex<()>>>; an entry is evicted when its Arc is no longer shared
}
impl MetadataPipelineStore { pub fn new(parts: PipelineParts) -> Result<Self, Error>; } // validates capabilities and reserved-name clashes
```

**Central storage on a router (AC-12, Q7).** The router's pipeline has the router as its backend
(members reached through their data-only methods) and the central model as `storage`, plus
`backups: Vec<(Key /*member prefix*/, Arc<dyn MetadataStorage>)>` — the storage model each member
configured for itself, if any. A write plans the central objects and, for the owning member's
backup, `WriteTarget::Backup` objects in the same `WriteSet`, so `transactional` commits both or
neither. A read uses the central record only; when it is missing and the member's backup has one,
the backup is returned, copied to central storage, and a warning is logged. A backup that
disagrees with the central record is overwritten at the next write, never merged. Under central
storage the router's section governs handlers and the write strategy for every member; a member's
own `metadata` section may name only its storage model (its backup) — a member section that also
names handlers or a write strategy is a configuration error naming the member, since two chains
for one key would be ambiguous.

`set(k, data, m)`: Validate chain on `m` → Write chain with `Bytes(data)` → `storage.plan` under the
group lock → `writes.commit` with the data object and the planned objects → cache. `get_metadata(k)`:
`writes.recover(k)` → cache hit if `data_facts` unchanged → `storage.get` or, if absent, Default
chain (persisted only if `persist_defaults`) → Read chain with the facts, the bytes fetched first only when its `needs_bytes` says so. Listings use
`get_folder` when the model offers it. `set_metadata(k, m)`: Validate → Write chain with
the facts (a step that sets `StepOut::data` turns it into a data write too) → plan → commit.
`remove(k)`: plan with `None`, commit with the data object deleted. `get`, `get_metadata`,
`listdir` and `listdir_asset_info` each call `writes.recover` first for the key or the folder (and
the folder's group file), and a recovery appends a warning to the record's log (AC-20); `sync`
calls `writes.sync`. `is_supported` and every path refuse a name reserved by the storage model or
the write strategy, with the same `key_not_supported` error as `set` (AC-3). `created` is set from
`data_facts` after each data commit, so a `Partial` checkpoint and the final write each move it
(Phase 1 Q8). All other methods forward to the backend's data-only ones.

### 7. Configuration types

```rust
// StoreConfig (store_config.rs): `metadata` changes from the reserved map to a typed section.
#[serde(default)] pub read_only: bool,
#[serde(default)] pub metadata: Option<MetadataConfig>,
// StoreRouterConfig: #[serde(default)] pub read_only: bool, and
//                    #[serde(default)] pub metadata: Option<MetadataConfig> — central storage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)] // an old, non-empty free-form map is refused, not silently accepted
pub struct MetadataConfig {
    pub preset: Option<String>, pub storage: Option<String>, pub writes: Option<String>,
    pub persist_defaults: Option<bool>, pub on_default: Option<String>, pub on_validate: Option<String>,
    pub on_write: Option<String>, pub on_read: Option<String>,
}
// StoreRouterConfig also gains #[serde(default)] pub presets: Vec<PresetConfig> (AC-22): presets
// are defined once for the whole configuration, not per mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfig { pub name: String, #[serde(default)] pub read_only: Option<bool>,
                          #[serde(flatten)] pub metadata: MetadataConfig } // a preset may name a base preset
```

A preset sets `read_only` too, so `source` makes a mount read-only by itself; an explicit
`read_only` on the mount wins. Built-in presets (Phase 1 Q13): `default` (native storage, today's
handlers, `direct` — what a mount without `metadata` behaves as), `source` (read-only, `none`,
`media_type`, `file_facts`, `content_hash-on_change`), `generated` (native, `atomic`), `memory`
(grouped, `transactional`). `front_matter` is added to `source` by `liquers-lib`'s registration of
its own `source` variant, `docs`.

Every `Option<String>` component is a **spec** in the one mechanism below; a preset fills the
fields left `None`. `metadata: {}` counts as no section (AC-6). The old reserved map was never
written by anything; a non-empty one now fails with the field named.

## Configuration (Phase 1 question 2 — decided: alternative b, the `store_config` realm)

```yaml
- type: filesystem
  prefix: specs
  config: { path: ./specs }
  metadata:
    preset: source
    on_read: media_type-extension/file_facts/content_hash-on_change/front_matter-title-description
```

A chain is a query: actions separated by `/`, parameters by `-`, parsed by `parse_query` and planned
**once, at store build**, by the existing `PlanBuilder` against the shared `CommandMetadataRegistry`
(`PlanBuilder::new` needs only the registry — no environment, recipe provider or store), then
compiled (§4). The realm comes from a transform segment header (`plan.rs`,
`last_transform_query_name`), so a spec may be written `-store_config/media_type-extension/…`; the
compiler also plans a headerless spec in realm `store_config` (`PlanBuilder::with_default_realm`),
the form shown.

**Where the registry comes from.** `EnvironmentBuilder::build` holds its `CommandRegistry` when it
builds the store (`environment_builder.rs`), so it passes the `command_metadata_registry` and the
`StoreConfigFunctions` to `StoreRouterBuilder` — no construction cycle, since a compiled chain never
needs the environment. A bare `StoreRouterBuilder` (tests, `liquers-store`'s `create_router*`,
`liquers-web`) uses `StoreConfigFunctions::builtin()` with its metadata, or the pair it is given.

**The other components.** Storage models, write strategies, the adapter `read_only` and presets
are `store_config` entries too, in namespace `store`, with a `ComponentBuilder`
(`fn(&ResolvedParameterValues) -> Result<StoreComponent, Error>`) instead of a `StepBuilder` — e.g.
`per_entry-folder`, `grouped-__metadata__.json`, `transactional`. A new component of any kind is
one hard-coded registration and no configuration design (AC-10, AC-21).

Specs above were checked with `liquers-validate` (parse and plan; the `-R-key` form as well). Once
the realm is exported to `specs/command_registry.yaml` they validate against the real registry.

Rejected, alternative (a): ordinary commands run through the `CommandExecutor` — the record is not a
`Value`, the store is built before the environment exists, and a misspelt handler would fail on
every read instead of at build time.

## Integration Points

- `liquers-core/src/store.rs`: the `AsyncStore` additions; implementations in `AsyncFileStore`,
  `AsyncMemoryStore`, `AsyncStoreRouter`; `ReservedNames` prefix form.
- `liquers-core/src/store/` (new, declared as `mod …;` inside `store.rs`, as `escape.rs` does with
  `escape/`): `read_only.rs` (patterned on the test decorators `DecoratedStore` / `ArcStore`), `pipeline.rs`, `storage.rs`, `handlers.rs`,
  `writes.rs`, `metadata_functions.rs`.
- `liquers-core/src/store_config.rs`: `read_only`, `MetadataConfig`, `PresetConfig`;
  `store_factory.rs`: `build_without_env_expansion` keeps returning `AsyncStoreRouter` (its callers
  in `liquers-store`'s `create_router*`, `liquers-web`'s `build_router` and `EnvironmentBuilder`
  depend on the type) and wraps each mount (`Arc::from` the factory's `Box`, then pipeline, then
  `ReadOnlyStore`); a new `build_store(self) -> Result<Arc<dyn AsyncStore>, Error>` also applies
  the router-level `read_only` and `metadata`, and `EnvironmentBuilder::build` switches to it;
  `StoreRouterBuilder::with_metadata_functions(&CommandMetadataRegistry, MetadataFunctionRegistry)`,
  called by `EnvironmentBuilder::build` with its command registry.
- `liquers-core/src/commands.rs`: `CommandRegistry` carries a `StoreConfigFunctions` beside its
  metadata registry (data only — never executed); the executor refuses the `store_config` realm with
  `not_supported`. `plan.rs`: `PlanBuilder::with_default_realm`.
- `liquers-macro`: unchanged.
- `liquers-core/src/metadata.rs`: `MetadataRecord` and `AssetInfo` gain `created:
  Option<BackendTime>` and `etag: Option<String>` (serde default, skipped when `None`).
- `liquers-core/src/environment_builder.rs`: `VersionVerification` stays the environment default;
  a mount's `content_hash` handler overrides it.
- `liquers-store/src/opendal_store.rs`: data-only methods, capabilities per OpenDAL service.
- `liquers-lib`: `front_matter` handler, registered in `liquers-lib`'s store factory.
- `liquers-axum/src/store/handlers.rs`: nothing new — `NotSupported` already maps to 501.
- `liquers-web`: `LiquersStore` exposes `isReadOnly`; its stores keep refusing data access (later).
- Conformance: `liquers-core/src/store_conformance/` gains fixtures for adapted stores (AC-7) and a
  failure-injecting backend for AC-18 to AC-20.

## Error Handling

| Failure | Constructor |
|---|---|
| Write on a read-only store; data-only method on a backend without it | `Error::not_supported` |
| Unknown component, bad parameter, preset not found, capability missing, reserved-name clash | `Error::configuration_error` naming the mount and the field (AC-9, AC-17) |
| A spec that does not parse | the `parse_query` error, with its position |
| Validator rejects supplied metadata | `Error::conversion_error_with_message` naming the field (AC-14) |
| Key reserved by the model or the strategy | `Error::key_not_supported` (existing §8 behaviour) |
| Read-stage handler fails | not an error: a warning in the record's log (Phase 1 Q9) |
| Backend failure during commit | the backend's error; the journal leaves the write recoverable |

## Relevant Commands

A new realm, `store_config`, with two namespaces: `meta` (handlers) and `store` (components). No
`register_command!` lines: each entry is a `CommandMetadata` plus a builder, registered in code by
`register_builtin_store_config` (core: `defaults`, `require`, `media_type`, `file_facts`,
`content_hash`, `updated`; `native`, `per_entry`, `grouped`, `none`; `direct`, `atomic`,
`transactional`; `read_only`; the presets) and by `liquers-lib`'s counterpart (`front_matter`).
Phase 3 fixes the parameter lists. No existing namespace (`pl`, `lui`, `rec`, …) is touched.
**`specs/command_registry.yaml` must be regenerated** — the realm's entries appear in it — with a
CHANGELOG line; `registry_export` keeps it honest.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/STORE_SEMANTICS.md` | reference | New sections: data-only access, read-only, storage models and reserved names, handler stages, write strategies and recovery, grouped-write guarantees |
| `specs/reference/STORE_CONFIG_FSD.md` | reference | `read_only`, `metadata`, presets, the spec mechanism |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | guide | Implementing data-only access; writing a handler, a storage model or a write strategy |
| `specs/guides/STORE_FACTORY_GUIDE.md` | guide | Registering metadata functions |
| `specs/reference/ASSETS.md` | reference | `created`; `Storing` under the write strategies (added here; Phase 1 listed no other documents) |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | `AsyncStore` data-only methods; the pipeline in the store layer |
| `CLAUDE.md` | instructions | "Adding a Store Backend": implement data-only access to take a `metadata` section |
| `liquers-web/README.md` | crate doc | `isReadOnly` |
| `.claude/skills/liquers-validate/SKILL.md` | skill | validating store specs (realm `store_config`, the default-realm form) |
| `specs/reference/PROJECT_OVERVIEW.md` §Realm | reference | the first foreign realm: addressable, plannable, not executable |

Proposed `affects_docs`: the first six. `specs/README.md`: capability map entry for store adapters.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `store.rs`, `store_config.rs`, `store_factory.rs`, `metadata.rs`, new `store/` module, `opendal_store.rs`, one `liquers-lib` module |
| Crates and workflows affected | core, store, lib; web and axum only for `is_read_only` |
| Existing tests likely to change | none by AC-6; `store_config` serde tests for the typed `metadata` field |
| New validation | conformance fixtures per adapted store; failure injection for strategies |
| Compatibility | **`MetadataRecord` is `deny_unknown_fields`**: an older build reading a record with `created` fails to parse it as a record and falls back to `Metadata::LegacyMetadata`, which keeps every field (`store.rs`, `opendal_store.rs`); nothing is lost, but that build's typed accessors degrade on new records. Recorded in `STORE_SEMANTICS.md`; a `metadata: {...}` section is refused by an older build, as an unknown field shape |
| Concurrency | group locks are per process (Q10); recovery on read costs one `contains` per read for the journal, cached after a clean `sync` |
| Performance | one compiled closure per stage, no lookup or allocation on the path; read chains run per entry in listings, and the facts, `needs_bytes` and the facts-keyed cache keep a listing free of byte reads on unchanged entries; steps are sync, so a large hash blocks one thread (a blocking pool later, same shape) |
| Bindings | `liquers-py` (`context.rs`) holds stores as `Arc<dyn AsyncStore>` only; defaulted methods do not break it. Its `MetadataRecord` wrapper gains getters for `created` and `etag` |
| Security | none new; read-only enforced in the decorator and reported through `is_read_only` |
| Recovery | journal roll-forward/back (AC-20); `DirectWrite` keeps today's `Storing` marker |
| Certainty and open questions | All Phase 1 questions decided; to confirm: the realm name (`store_config` proposed). Least certain: how much of `PlanBuilder`'s default-namespace and realm handling a headerless spec needs |
