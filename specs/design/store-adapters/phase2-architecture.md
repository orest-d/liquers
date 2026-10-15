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
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | A language store is mounted as an instance under a Rust type; `read_only` and `metadata` still apply to it (§Language Integration) | No | No | Unchanged; its handler counterpart filed as `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` |
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

**Realm name.** *Confirmed 2026-10-15:* `store_config` — the realm exists only inside store configuration, its
queries configure rather than compute, and the name reads as what it is in a header
(`-store_config/…`). Considered: `storeconf` (shorter, less obvious), `store_handlers` (too
narrow: the realm also names storage models and write strategies), `store` (collides in
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage { Default, Validate, Write, Read }

/// One step's builder: hard-coded where the handler is defined, a plain `fn` pointer. It is given
/// the stage so one handler can mean the right thing in each (`media_type` fills in Write and
/// checks in Validate) and refuse a stage it has no meaning in.
pub type StepBuilder = fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>;
/// A component's builder (storage model, write strategy): same registration, different product.
pub type ComponentBuilder = fn(&ResolvedParameterValues) -> Result<StoreComponent, Error>;
pub enum StoreComponent { Storage(Arc<dyn MetadataStorage>), Writes(Arc<dyn WriteStrategy>) }
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
one key); how several guarded chains combine (first match or all, in order); and whether a guard
needs a pattern rather than a key (`front_matter` only under `*.md`). Recorded as an issue at Phase 5 if not implemented.

**Namespace.** Every built-in entry is in namespace `""` of realm `store_config`. The realm already
keeps them apart from ordinary commands, and `PlanBuilder` searches only the registry's
`default_namespaces` (`""`, `"root"`) unless the query says `ns-…` (`plan.rs`
`namespaces_for_query`), so a spec needs no `ns` instruction. An entry's *kind* — handler, storage
model, write strategy — is checked against the configuration field it is written in
(`storage: transactional` is a `configuration_error`), so one namespace is not ambiguous. Entries
from another crate may use their own namespace and be reached with `ns-<name>/…`. (This revises the
`meta` / `store` split proposed before the realm was named.)

**Messages.** `Info`, `Warning` and `Error` are plan steps the planner emits (a deprecated
parameter, a fallback it took); compiled, each appends its message to the record's `log` every time
the chain runs, so what the planner noticed at build stays visible on every record it shaped.

#### Command catalogue — realm `store_config`

Parameters are positional in a spec (`media_type-fill`), with the defaults shown; `multiple`
consumes the rest. *Stages* are where an entry may appear; any other stage is a
`configuration_error` at build. `mode` is `overwrite` (replace the field) or `fill` (set it only
where it is empty), AC-15. Every handler compares against `previous` where it says "changed", so
step order does not matter.

**Handlers** (`StepBuilder`; written in `on_default`, `on_validate`, `on_write`, `on_read`):

| Entry | Parameters | Stages | Effect | Needs bytes |
|---|---|---|---|---|
| `defaults` | — | Default, Write | Default: the record today's stores synthesize for an entry with no stored metadata (`default_metadata`): `key`, `filename`, `is_dir`, `status: Source` for data. Write: what `finalize_metadata` does today — `key` set, `filename` seeded, `status: Source` where it is `None` | no |
| `set` | `field: String`, `value: String`, `mode = overwrite` | Default, Write, Read | Sets one text field: `title`, `description`, `media_type`, `data_format`, `type_identifier`, `unicode_icon`; any other name fails at build | no |
| `require` | `fields: Vec<String>` (multiple) | Validate | Refuses a record whose named fields are empty, naming the first (AC-14); the fields `set` accepts | no |
| `media_type` | `mode = overwrite` | Default, Validate, Write, Read | Default/Write/Read: sets `media_type` from the extension (`media_type.rs` `file_extension_to_media_type`), leaving it alone for an unknown extension. Validate: refuses a supplied `media_type` that disagrees with a known extension (Phase 1's "corrected media type") | no |
| `file_facts` | `mode = overwrite` | Default, Write, Read | Default/Read: `file_size`, `created`, `etag` from `DataFacts`. Write: `file_size` from the bytes; `created` and `etag` are set after the commit from the backend's facts, by the pipeline, whatever the chain says (§6) | no |
| `content_hash` | `when = on_change` (`off` · `always` · `on_change`) | Default, Write, Read | Sets `version` to `Version::from_bytes`. Write: always hashes (the bytes are at hand) unless `off`. Read: `always` hashes every read; `on_change` only when `size`, `created` or `etag` differ from `previous` or `previous` has no `version`. A detected change is handled as `verify_stored_versions` handles one (AC-4); the entry's presence is what the mount's `version_verification` reports | `always`; `on_change` when changed |
| `updated` | — | Write | Sets `updated` to the current time in the record's existing format; the one time Liquers rather than the backend owns (Phase 1 Q8) | no |
| `front_matter` *(liquers-lib)* | `mode = overwrite`, `fields: Vec<String> = [title, description]` (multiple) | Default, Write, Read | For a `text/markdown` entry, parses the YAML front matter (`liquers_records::search::front_matter_end`) and projects the named fields; a field outside the `set` list is refused at build until `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` gives it a home. Other media types: no effect | Markdown only, and on Read only when changed |
| `front_matter_sync` *(liquers-lib)* | `fields: Vec<String> = [title, description]` (multiple) | Write | Rewrites the front matter of the bytes from the record's fields, setting `StepOut::data`; turns `set_metadata` into a data write (AC-5) | yes |

**Storage models** (`ComponentBuilder`; written in `storage`):

| Entry | Parameters | Model | MVP |
|---|---|---|---|
| `native` | — | The backend's own sidecar through `get_stored_metadata` / `set_metadata` (today's behaviour) | built |
| `per_entry` | `layout = sidecar` (`sidecar` · `root` · `prefix` · `folder`) | `{key}.__metadata__` · `__metadata__/{key}` · `{folder}/__metadata__{name}.json` · `{folder}/__metadata__/{name}.json` (the Phase 1 table, named because a template's `/` cannot be a query literal) | built |
| `grouped` | `file = __metadata__.json` | One JSON map name → record per folder | built |
| `none` | — | Nothing stored; handlers synthesize every record, cached by facts | built |
| `central` | `mount: String` | Records kept in another mount of the router, by key (AC-12) | interface; `memory_central` (no parameters) is the test implementation, registered only under `cfg(test)` and the conformance feature |

A database model (sqlite, sled, fjall) registers here when it exists; none is registered now.

**Write strategies** (`ComponentBuilder`; written in `writes`):

| Entry | Parameters | Strategy | Requires |
|---|---|---|---|
| `direct` | — | `DirectWrite`, as today | `data_access` |
| `atomic` | — | `AtomicWrite` | `atomic_write` or `rename` (AC-17) |
| `transactional` | — | `JournalWrite` | `data_access` |

Reserved names (`.__staged__`, `.__txn__`) are fixed by the strategy, not parameters, so the
reserved-name check (AC-3) never depends on a configuration value.

**Not an entry.** `read_only` is the mount's boolean: it needs no parameters, and a query for "do
nothing" would only be a second spelling.

A `LegacyMetadata` record bypasses the chains and is stored and returned untouched, with a warning:
`Metadata::metadata_record` has no lossless conversion, and the legacy branch exists to keep unknown
fields.

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
    pub storage: Option<String>, pub writes: Option<String>,
    pub persist_defaults: Option<bool>, pub on_default: Option<String>, pub on_validate: Option<String>,
    pub on_write: Option<String>, pub on_read: Option<String>,
}
```

**Defaults (AC-22, Phase 1 Q13 revised: no presets).** A field left out of a section takes the
value below; a field that is set replaces its default whole, and `""` means no steps. Nothing is
merged, so a section reads as what it does. The defaults are today's behaviour made explicit, plus
the facts that cost no extra read; anything that reads bytes, writes on read or refuses a write is
opt-in.

| Field | Default | Why |
|---|---|---|
| `read_only` | `false` | today |
| `storage` | `native` | byte-identical to today's sidecars |
| `writes` | `direct` | today; works on every backend, reserves no names |
| `persist_defaults` | `false` | a read never writes (the git-checkout complaint of Phase 1); with no section the store is unwrapped and keeps today's write-on-read (AC-6) |
| `on_default` | `defaults/media_type-fill/file_facts` | today's synthesized record, plus media type and facts |
| `on_validate` | `""` | nothing refused unless asked |
| `on_write` | `defaults/media_type-fill/file_facts/updated` | today's `finalize_metadata`, plus media type |
| `on_read` | `media_type-fill/file_facts` | facts come with the read; no bytes, no hash |

A router-level section has the same defaults. The values live in one `const` beside
`MetadataConfig` and are documented in `STORE_CONFIG_FSD.md`, so the table and the code cannot
drift apart unnoticed (a test compares them).

Every `Option<String>` component is a **spec** in the one mechanism below. `metadata: {}` counts
as no section (AC-6). The old reserved map was never
written by anything; a non-empty one now fails with the field named.

## Configuration (Phase 1 question 2 — decided: alternative b, the `store_config` realm)

```yaml
- type: filesystem
  prefix: specs
  config: { path: ./specs }
  read_only: true
  metadata:
    storage: none
    on_read: media_type/file_facts/content_hash-on_change/front_matter
```

A chain is a query: actions separated by `/`, parameters by `-`, parsed by `parse_query` and planned
**once, at store build**, by the existing `PlanBuilder` against the shared `CommandMetadataRegistry`
(`PlanBuilder::new` needs only the registry — no environment, recipe provider or store), then
compiled (§4). `PlanBuilder` takes the realm from the transform segment's header (`plan.rs`,
`last_transform_query_name`), so a spec may be written `-store_config/media_type/…`. **In a
configuration the realm prefix may be omitted**, and `PlanBuilder` is not changed for it: the
compiler fills the realm into the parsed query before planning. If the query's transform segment has
no header, it gets `SegmentHeader { name: "store_config", level: 0, parameters: [] }`; a header with
an empty name has the name set; a header naming any other realm is a `configuration_error` (a
handler spec cannot run in realm `""`). The filled query is what the compiler plans, what an error
message quotes, and what `liquers-validate` should be given, so validation and configuration see the
same text. Filling happens on the AST, never by prefixing the string, so a spec that begins with a
resource segment (`-R-key/…`, below) keeps its shape.

**Where the registry comes from.** `EnvironmentBuilder::build` holds its `CommandRegistry` when it
builds the store (`environment_builder.rs`), so it passes the `command_metadata_registry` and the
`StoreConfigFunctions` to `StoreRouterBuilder` — no construction cycle, since a compiled chain never
needs the environment. A bare `StoreRouterBuilder` (tests, `liquers-store`'s `create_router*`,
`liquers-web`) uses `StoreConfigFunctions::builtin()` with its metadata, or the pair it is given.

**The other components.** Storage models and write strategies are `store_config` entries too
(§4 catalogue), with a `ComponentBuilder` instead of a `StepBuilder` — e.g. `per_entry-folder`,
`grouped-__metadata__.json`, `transactional`. A `storage` or `writes` spec is one action: a second
step is a `configuration_error`. A new component of any kind is one hard-coded registration and no
configuration design (AC-10, AC-21).

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
- `liquers-core/src/store_config.rs`: `read_only`, `MetadataConfig` and its defaults;
  `store_factory.rs`: `build_without_env_expansion` keeps returning `AsyncStoreRouter` (its callers
  in `liquers-store`'s `create_router*`, `liquers-web`'s `build_router` and `EnvironmentBuilder`
  depend on the type) and wraps each mount (`Arc::from` the factory's `Box`, then pipeline, then
  `ReadOnlyStore`); a new `build_store(self) -> Result<Arc<dyn AsyncStore>, Error>` also applies
  the router-level `read_only` and `metadata`, and `EnvironmentBuilder::build` switches to it;
  `StoreRouterBuilder::with_metadata_functions(&CommandMetadataRegistry, MetadataFunctionRegistry)`,
  called by `EnvironmentBuilder::build` with its command registry.
- `liquers-core/src/commands.rs`: `CommandRegistry` carries a `StoreConfigFunctions` beside its
  metadata registry (data only — never executed); the executor refuses the `store_config` realm with
  `not_supported`. `plan.rs`: unchanged — the compiler fills the realm into the query (§Configuration).
- `liquers-macro`: unchanged.
- `liquers-core/src/metadata.rs`: `MetadataRecord` and `AssetInfo` gain `created:
  Option<BackendTime>` and `etag: Option<String>` (serde default, skipped when `None`).
- `liquers-core/src/environment_builder.rs`: `VersionVerification` stays the environment default;
  a mount's `content_hash` handler overrides it.
- `liquers-store/src/opendal_store.rs`: data-only methods, capabilities per OpenDAL service.
- `liquers-lib`: `front_matter` handler, registered in `liquers-lib`'s store factory.
- `liquers-axum/src/store/handlers.rs`: nothing new — `NotSupported` already maps to 501.
- `liquers-web`: `LiquersStore` exposes `isReadOnly`; `JsStore` maps the optional data-only methods of a store object (§Language Integration); the built-in browser stores keep refusing data access (later).
- Conformance: `liquers-core/src/store_conformance/` gains fixtures for adapted stores (AC-7) and a
  failure-injecting backend for AC-18 to AC-20.

## Language Integration

What this design gives a language integration (`liquers-web` today, `liquers-py` later), as four
facts. The guide does not copy them as answers. It turns them into questions every integration
design must answer for its own language (§Documentation Architecture, "Language integration guide
update"); the facts below are what such a design can rely on.

**1. Can a store implemented in another language use the configuration? Yes, both new fields.**
A language store reaches a document as an instance behind a Rust type (`type: js, config: {object:
…}`), and `read_only` and `metadata` sit on `StoreConfig`, beside `type`, not inside `config`. The
builder applies them to whatever the factory returned, so they work for a store whatever language
it is written in. `read_only` needs nothing from the store: `ReadOnlyStore` refuses writes before
they reach it. `metadata` needs the store's **data-only methods** (§1). The adapter (`JsStore`, a
future Python adapter) implements them by calling optional methods of the language object
(`getData`, `setData`, `removeData`, `renameData`, `listdirData`, `dataFacts`), and reports in
`data_capabilities` which ones it found. A missing method is not guessed from `get`, whose metadata
side effects the adapter cannot see. An object with none of them reports no data access, so a
`metadata` section on its mount is a `configuration_error` naming the mount and the missing
capability, raised at build (AC-9). This is the same rule a Rust backend follows.

**2. To what extent can it use the new configuration methods? All of them, within its
capabilities.** Everything configured runs in Rust, around the store. The language store only
supplies bytes and facts, so nothing is reimplemented per language.

| Part | What the language store must provide |
|---|---|
| `read_only` | nothing |
| storage `per_entry`, `grouped`, `none` | the data-only methods (the models keep records as data objects) |
| storage `native` | also `getStoredMetadata` / `setMetadata`, if the store keeps its own metadata |
| writes `direct`, `transactional` | `setData` (and `removeData` for a delete) |
| writes `atomic` | also `renameData`, or a declared atomic write |
| every built-in handler | nothing: it runs on the record in Rust; `dataFacts` makes `file_facts` and `content_hash-on_change` cheap |

`dataFacts` returns size, modification time and ETag. Its time crosses the bridge as an RFC 3339
string with a declared precision, so `BackendTime` round-trips the same way from every language
(Phase 1 Q8).

**3. Can it execute the compiled closures? Not in this design, and it does not need to.** A
compiled chain is owned by the `MetadataPipelineStore` that wraps the language store, so the chains
already run on every access to that store, in Rust. Handing a chain *to* the language (say,
`compileStoreConfig(spec, stage)` returning a function from record to record, for a store that
wants to apply configured handlers itself) would be possible. A `CompiledChain` needs no
environment and is `Send + Sync`, and the STORE bridge already converts `MetadataRecord`. But
nothing needs it, and it would invite a second place where handlers run. Not built.

**4. Can it define its own `store_config` command? Not in this design. The obstacles are known,
and none of them is in the realm itself.**
- **Declaration:** solved already. A language handler is a `CommandMetadata` in realm
  `store_config`, declared in the [Command Declaration Format](../../reference/COMMAND_DECLARATION.md)
  like any language command.
- **Builder type:** `StepBuilder` is a plain `fn` pointer, which cannot capture a language callable.
  A language entry needs a boxed builder in `StoreConfigFunctions`' table. The closure types
  (`CompiledFn`, `NeedsBytesFn`) already capture whatever they need, so only the table changes.
- **Synchronous:** a chain is synchronous (§4). An async JavaScript function cannot be awaited
  inside it, so a language handler must be synchronous.
- **Cost per call:** a Python handler takes the GIL on every call, and a listing runs the read chain
  once per entry. That is a measurable cost to document, not a blocker.
- **Timing:** chains compile at store build, so a language handler must be registered before the
  store is built. This is the same constraint as `POST-INIT-COMMAND-REGISTRATION`.
- **Errors:** a language exception crosses through the ERROR bridge and follows the stage rules of
  §4 (a warning on read, abort on validate).

Recorded as `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` (draft, filed with this revision). It is
the handler counterpart of `LANGUAGE-STORE-TYPE-NOT-DEFINABLE`, and the two will likely be
designed together.

## Error Handling

| Failure | Constructor |
|---|---|
| Write on a read-only store; data-only method on a backend without it | `Error::not_supported` |
| Unknown component, bad parameter, capability missing, reserved-name clash | `Error::configuration_error` naming the mount and the field (AC-9, AC-17) |
| A spec that does not parse | the `parse_query` error, with its position |
| Validator rejects supplied metadata | `Error::conversion_error_with_message` naming the field (AC-14) |
| Key reserved by the model or the strategy | `Error::key_not_supported` (existing §8 behaviour) |
| Read-stage handler fails | not an error: a warning in the record's log (Phase 1 Q9) |
| Backend failure during commit | the backend's error; the journal leaves the write recoverable |

## Relevant Commands

A new realm, `store_config`, all built-in entries in namespace `""` (§4 catalogue, which fixes the
parameter lists). No `register_command!` lines: each entry is a `CommandMetadata` plus a builder,
registered in code by `register_builtin_store_config` (core: `defaults`, `set`, `require`,
`media_type`, `file_facts`, `content_hash`, `updated`; `native`, `per_entry`, `grouped`, `none`,
`central`; `direct`, `atomic`, `transactional`) and by `liquers-lib`'s counterpart (`front_matter`,
`front_matter_sync`). No existing namespace (`pl`, `lui`, `rec`, …) is touched.
**`specs/command_registry.yaml` must be regenerated** — the realm's entries appear in it — with a
CHANGELOG line; `registry_export` keeps it honest.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/STORE_SEMANTICS.md` | reference | New sections: data-only access, read-only, storage models and reserved names, handler stages, write strategies and recovery, grouped-write guarantees |
| `specs/reference/STORE_CONFIG_FSD.md` | reference | `read_only`, `metadata`, the defaults table, the spec mechanism |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | guide | Implementing data-only access; writing a handler, a storage model or a write strategy |
| `specs/reference/REALMS.md` | reference (**new**) | What a realm is today (a `CommandKey` component chosen by the segment header), the default realm `""`, and the issues a foreign realm raises: addressable vs executable, a subject that is not a `Value`, filling an omitted realm before planning, which plan steps a realm accepts, namespaces inside a realm, registry export; `store_config` as the first worked case; the open ones linked to `CORE-MULTI-REALM-INTERPRETER` and `TYPE-REGISTRY-NOT-REALM-AWARE` |
| `specs/guides/STORE_CONFIG_GUIDE.md` | guide (**new**) | Writing `metadata` sections: stages, the command catalogue, the defaults, validating specs; then defining and registering a new handler, storage model or write strategy (`CommandMetadata` + builder, hard-coded registration, the closure type, `needs_bytes`, stage refusal, tests) |
| `specs/guides/STORE_FACTORY_GUIDE.md` | guide | Passing `StoreConfigFunctions` to the builder; pointer to `STORE_CONFIG_GUIDE.md` |
| `specs/reference/ASSETS.md` | reference | `created`; `Storing` under the write strategies (added here; Phase 1 listed no other documents) |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | `AsyncStore` data-only methods; the pipeline in the store layer |
| `CLAUDE.md` | instructions | "Adding a Store Backend": implement data-only access to take a `metadata` section |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | §STORE, §COMMAND, §ENVIRON, tests and Appendix A — planned below |
| `liquers-web/README.md` | crate doc | `isReadOnly`; the optional data-only methods of a store object |
| `.claude/skills/liquers-validate/SKILL.md` | skill | validating store specs (realm `store_config`; give the tool the filled form `-store_config/…`) |
| `specs/reference/PROJECT_OVERVIEW.md` §Realm | reference | the first foreign realm: addressable, plannable, not executable; pointer to `REALMS.md` |

### Language integration guide update

The guide's §STORE predates configured stores. It knows a store as an instance, a router and a
factory chain, and it has nothing to say about a store that is wrapped by configuration. The update
adds questions to the guide's "The design must answer" lists and adds tests; this design's answers
appear only as the facts a question relies on. Changes, in guide order:

1. **§4 Feature Overview.** No new feature ID. Configured stores belong to `STORE` direction 3
   (composition and configuration), and a new ID would let a design select configuration without
   the store contract it depends on. The `STORE` row is unchanged.
2. **§STORE Contract.** Direction 3 is widened from "a router built from a declarative document" to
   also cover the mount's *access policy and metadata handling* (`read_only`, `metadata`). Both apply
   to every mount, a language-defined one included.
3. **§STORE Objects/API to map or implement.** Added:
   - the optional data-only methods of the language store protocol;
   - `DataCapabilities`, `DataFacts` and `BackendTime`;
   - `is_read_only` and `sync`;
   - `MetadataRecord`'s new `created` and `etag` fields;
   - `MetadataConfig`, its defaults and `StoreConfigFunctions`, *normally reused unchanged*, as the
     router is.
4. **§STORE The design must answer.** New questions, generalized from the four raised at this
   review:
   - Can a store written in the language be named in a configuration document? Do `read_only` and
     `metadata` reach it there, or only stores the integration provides?
   - Which configured parts can each store take: which storage models, which write strategies,
     which handlers? Which data-only methods does the language protocol expose, and which are
     optional? How does the adapter learn the store's data capabilities without inferring them from
     a method that has side effects? When a part is refused, how is it reported? (At build, naming
     the mount and the missing capability, is the target.)
   - How do backend facts (size, modification time, ETag) cross the bridge, and with what time
     precision, so that `created` compares equal after a round trip?
   - Can the language run a compiled `store_config` chain itself, or are chains applied only in Rust
     around the store? If they are exposed, what is the call surface, and how does a
     `MetadataRecord` cross?
   - Can the language define its own `store_config` entry: a handler, a storage model or a write
     strategy? If so:
     - how is it declared (Command Declaration Format, realm `store_config`)?
     - is it synchronous?
     - must it be registered before the store is built?
     - what does it cost per call on a listing?
     - how does an exception map under each stage's rules?

     If not, record `NA` and link `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`.
   - Can the language see that a store is read-only, run `sync`, and read a recovery warning?
   - For direction 2: do the integration's own backends implement data-only access, so a
     `metadata` section can wrap them? If they do not, say so per store.
5. **§STORE, new subsection "Configured stores: access policy, metadata and write strategies"**,
   after "Configuration":
   - what an integration inherits unchanged (the pipeline, the defaults, refusal at build);
   - the capability table of §Language Integration, as the reference a design checks its stores
     against;
   - the handler counterpart of "What the *language* cannot contribute", with the same three
     consequences restated for handlers: an entry invisible to the registry export, arguments that
     cannot be declared, and a document that leaks how the handler is provided.
6. **§COMMAND The design must answer.** "Realm" already appears in the metadata mapping question. It
   gains one clause: whether a language command may be declared in a foreign realm, which is
   addressable but not executable. Link `REALMS.md`.
7. **§ENVIRON.** One line: an environment built from a configuration with `metadata` sections
   compiles its chains against the environment's command registry, so language registrations that
   such a configuration names must happen before the environment is built.
8. **Tests.** `STORE14`–`STORE18`, a new column in the disposition table, and Appendix A pseudocode
   with real, validated specs:
   - `STORE14`: `read_only` on a configured mount refuses every write to a language-defined store,
     and `is_read_only` reports it.
   - `STORE15`: a `metadata` section on a store without data-only access fails at build, naming the
     mount and the missing capability.
   - `STORE16`: a language store with data-only methods under `per_entry` and `grouped` storage
     round-trips records, applies the default `on_read` chain, and never has its own `get` called
     for metadata.
   - `STORE17`: `created` from the language's backend time compares equal after a write and a
     re-read.
   - `STORE18`: a language-defined `store_config` handler runs in its stage, becomes a warning on
     read and aborts on validate, and is refused if registered after build. `NA` unless the design
     selects language handlers.

   Disposition: `STORE14` is required whenever direction 3 is selected. `STORE15`–`STORE17` are
   required when any store in the integration takes a `metadata` section. `STORE18` follows the
   design's answer to the handler question.
9. **§6 Language-Specific Guidance.**
   - Browser JavaScript: `JsStore`'s optional methods, and that the built-in browser stores refuse
     data access for now.
   - Python: the GIL cost of a per-entry handler.
10. **§8 References.** Add `REALMS.md` and `STORE_CONFIG_GUIDE.md`. Add a `## History` row and bump
    `reviewed:` (§9.2).

Proposed `affects_docs`: the first eight (the two new documents included) and `LANGUAGE-INTEGRATION_GUIDE.md`. `specs/README.md`: capability map entry for store adapters.

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
| Certainty and open questions | All Phase 1 questions decided; realm name `store_config` confirmed 2026-10-15. Least certain: whether `PlanBuilder`'s default-link promotion and parameter resolution behave identically in a non-empty realm (only `plan.rs` unit tests with a test realm `r` have planned outside `""`); Phase 4 tests it first |
