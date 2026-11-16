# Phase 2: Solution & Architecture — Settable value accessor

## Overview

An accessor is **a root plus a chain of synchronous projection steps**, shared through one **hub**
per root.
- **Root.** The root is asynchronous, and it is **source-agnostic**: anything that can read, and
  optionally write and notify, can be a root (`AccessorRoot`). Four roots are first-class, and they
  follow the asset lifecycle:
  - **store**: the data in the store under a key, in one of three views: the binary data, the
    metadata, or the value deserialized from the data. All three are writable. The key need not
    exist: its status is then `None`, a read fails, and a set creates it (AC-20);
  - **keyed asset**: the asset of a key, evaluated on read. A set makes it `Override` when the key
    has a recipe and `Source` when it has none, and creates it when it does not exist (AC-21);
  - **other assets**: an `AssetRef` the holder already has, a query asset or an unmanaged one. It
    is read-only, because nothing can write it (AC-21);
  - **memory**: a variable in memory. Holders share it by cloning the accessor (AC-22).
  A value that does not exist is a normal state of a root, not an error: its status is `None`.
- **Step.** A step is a lens over a value that is already in memory: `get(parent) -> part` and
  `put(parent, part) -> parent`. Each step also gives an **anchor**, a cheap structural fingerprint
  of where its part sits in the parent.
- **Hub.** The hub holds the projection tree by weak reference, like a signal graph. After a change,
  it classifies each subscribed node:
  - *unchanged*: nothing happens;
  - *changed*: the node gets `Changed`;
  - *invalidated*: the anchor changed or the slot is gone. The node's sink then expires its asset
    (Phase 1 AC-15).

A set is conditional on the part's own fingerprint (Phase 1 question 3, option C). Values are never
mutated in place: a set builds a new root value and writes it through the root.

**The hub is reusable.** `AccessorHub::new(root)` turns any `AccessorRoot` into a full accessor:
projections, fingerprints, conflicts, notification relay and the web routes all come with it. A
custom accessor therefore needs only a root, or a step. `ValueAccessor` stays a `dyn` trait, so an
accessor that needs nothing from the hub can implement it directly and still travel in
`ExtValue::Accessor`. Accessors can add behaviour of their own: validation, support for one data
type (converting the value on `set`), or read-only access. Any of these can sit in the root or in a
step, or in a wrapper around another accessor (`read_only`, `validated`).

**Lifecycle relay.** An accessor passes on what its source reports: status changes (`Submitted`,
`Processing`, `Ready`, `Error`, `Expired`, `Cancelled`), progress (primary and secondary), new log
messages, new versions (`Changed`) and removal (AC-23). The contract for holders is: on `Changed`,
redraw from `get`; on `Expired`, call `get`. For an asset-manager value, that `get` re-evaluates
the asset, and the new value arrives as `Changed`. A holder with nobody to react, such as a
headless service, can ask the hub to do this itself (`ExpiryPolicy::Reload`).

Events are wake-ups, not a record, as for `AssetRef` (a `watch` channel keeps only the latest).
The state they point to is in `info()`: status, message, progress, the number of log messages,
and the current fingerprint. So a holder that may have missed an event calls `info()` and redraws
when the fingerprint differs from the one it drew; it reads new messages with `log(since)`. A
projection shares its root's status, progress and messages, except that a projection whose slot is
gone has status `None`.

**Q3 cost check: option C stands.** Each change costs one root read, plus one derive and hash per
*subscribed* node. Unsubscribed nodes are only marked dirty. When the root version is unchanged,
nothing is derived at all. Letting a step declare a path, to skip whole subtrees without deriving
them, is deferred: it is an optimisation, and no scenario needs it.

**Command names.** The names in Phase 1 (`recipe_query_accessor-1`, `recipe_query-1`) were
illustrative. Phases 3 and 4 use the composable steps named here, for example
`-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n`. This query was
validated with `--command acc/<name>`. Its plan is `KeyValue` → `accessor` → `recipe` → `query` →
`action` → `argument`.

Rejected alternatives:
- **`Arc<Mutex<T>>` inside `Value`** (the old `value-accessor`): it breaks content versioning
  (`VALUE_TYPE_SYSTEM.md` §"What a value variant may hold") and cannot cross a process boundary.
- **Expiring every dependent on every change:** editors would lose their state, an editor would
  expire itself, and disjoint edits would interfere.
- **A writable `DataNode`:** it is borrowed and synchronous, and its leaves are scalars only.
  Phase 1 question 7 settled on one contract, the accessor's set. Document steps therefore read
  YAML/JSON trees directly, not through `DataNode` as Phase 1 sketched.
- **Async steps:** these would mean a boxed future per lookup. Only the root does I/O.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-WRITE-HAS-NO-PRECONDITION` (design `store-conditional-write`, in review) | draft | P3 | `AssetManager::set_state` has no base version. First-writer-wins (AC-5) holds only between accessors sharing a hub. Against any other writer (AC-6) it is last-writer-wins | No | No | Hub-serialised writes now. Adopt `set_if` when that design lands. Error type aligned with it (below) |
| `CORE-SESSION-AND-KEY-ACL` (design `access-policy-at-entry-points`, in review) | accepted | P2 | `PUT /api/acc` can write any key the server can | No | No | Opt-in route builder, trusted server only. The policy check attaches where that design puts it |
| `VALUE-ACCESSOR-CANNOT-LOCK` | draft | P3 | Locking is deferred (question 6) | No | No | `SetOptions` is reserved, `#[non_exhaustive]` |
| `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT` (filed 2026-11-16) | draft | P3 | Python and JavaScript can use accessors, not define roots or steps | No | No | The guide marks direction 2 `NA` with this issue as the reversing condition |
| `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS` | draft | P3 | An element holding an invalidated accessor keeps it | No | No | Left to the UI designs that consume accessors |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`, `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET` | draft | P3 | `ArgumentStep` relies on `ActionEditor` refusals (AC-3) | No | No | Phase 3 edge cases avoid both |
| `ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS` (designs `external-index-sync`, `asset-event-kinds`, not implemented) | draft | P3 | There is no manager-level stream of new versions and removals. A store root, and a keyed-asset root whose key has no live asset (for example a key that does not exist yet), cannot be told of another writer's change (AC-6) | No | No | Such roots detect outside changes on `get` and `refresh`, by comparing versions. They subscribe to the stream when those designs land |
| `CORE-TOKIO-REMOVAL`, `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS`, `STORE-HANDLER-FIELD-ACCESS-HARDCODED` | — | P3 | Same `watch` primitive as `AssetRef`; out-of-process changes are a non-goal; question 7 overlap | No | No | None |

Command namespaces involved: `acc` (new). No existing namespace changes.

## Interfaces

**Core, `liquers-core/src/accessor/`** (`mod.rs`, `hub.rs`, `root.rs`, `steps.rs`, `wrap.rs`). It is generic
over the value `V`, because the value variant cannot name an environment. Every `async_trait` below
uses the `cfg_attr` pair of `data_node.rs` (`?Send` on wasm).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint { pub anchor: u64, pub content: Version }
impl Fingerprint { pub fn token(&self) -> String; pub fn from_token(s: &str) -> Result<Self, Error>; }
pub struct AccessorSnapshot<V: ValueInterface> { pub state: State<V>, pub fingerprint: Fingerprint, pub root_version: Version }
/// Wake-ups relayed from the source (Status, progress, Log, Removed) or produced by the hub
/// (Changed, Invalidated). Log carries the new number of messages; read them with `log`.
pub enum AccessorEvent { Initial, Status(Status), PrimaryProgress(ProgressEntry),
    SecondaryProgress(ProgressEntry), Log(usize), Changed(Fingerprint), Invalidated, Removed }
/// The current state behind the events. `status` is `None` when the value does not exist.
#[derive(Serialize, Deserialize)]
pub struct AccessorInfo { pub status: Status, pub message: String,
    pub primary_progress: Option<ProgressEntry>, pub secondary_progress: Option<ProgressEntry>,
    pub log_len: usize, pub fingerprint: Option<Fingerprint> /* None when the value does not exist */ }
#[derive(Default)] pub enum ExpiryPolicy { #[default] Relay, Reload }
pub struct Capabilities { pub write: bool, pub notify: bool }
#[derive(Default)] #[non_exhaustive] pub struct SetOptions {}
#[derive(Serialize, Deserialize)]
pub struct PartDescription { pub step: String, pub type_identifier: String,
    pub argument: Option<ArgumentInfo>, pub command: Option<CommandKey>,
    pub overridden: bool /* a recipe argument or link override supersedes the query's value */, pub read_only: bool }

#[async_trait] pub trait ValueAccessor<V: ValueInterface>: MaybeSend + MaybeSync + Debug {
    fn address(&self) -> &Query;
    async fn get(&self) -> Result<AccessorSnapshot<V>, Error>;
    /// Checked against this handle's last-seen fingerprint. One query asset is one handle shared by
    /// all its holders, so this catches other handles and outside writers, not another holder of
    /// the same handle: a holder that needs that keeps its own token and uses `set_if`.
    async fn set(&self, value: V, options: SetOptions) -> Result<AccessorSnapshot<V>, Error>;
    /// `expected: None` means the value must not exist yet: a conditional create.
    async fn set_if(&self, value: V, expected: Option<Fingerprint>, options: SetOptions) -> Result<AccessorSnapshot<V>, Error>;
    async fn info(&self) -> Result<AccessorInfo, Error>;                   // never evaluates
    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error>;      // messages from index `since`
    async fn describe(&self) -> Result<PartDescription, Error>;
    fn capabilities(&self) -> Capabilities;
    async fn refresh(&self) -> Result<(), Error>;            // pull root events (wasm has no watcher task)
    fn subscribe(&self) -> watch::Receiver<AccessorEvent>;
    fn project(&self, step: Arc<dyn ProjectionStep<V>>, address: Query) -> Result<Arc<dyn ValueAccessor<V>>, Error>;
    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>);
    /// The watcher future for a host that spawns it itself (wasm); `None` when there is nothing to
    /// watch or the hub already runs it. A projection returns its hub's.
    fn driver(&self) -> Option<BoxFuture<'static, ()>> { None }
}
pub trait ProjectionStep<V>: MaybeSend + MaybeSync + Debug {
    fn name(&self) -> Cow<'_, str>;
    fn get(&self, parent: &V) -> Result<Option<V>, Error>;            // None: the slot is gone
    fn put(&self, parent: &V, part: V) -> Result<V, Error>;
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> { Ok(self.get(parent)?.map(|_| 0)) }
    fn describe(&self, parent: &V) -> Result<PartDescription, Error>;
}
#[async_trait] pub trait InvalidationSink: MaybeSend + MaybeSync { async fn invalidated(&self); }
/// What a custom source implements. The hub supplies everything else.
#[async_trait] pub trait AccessorRoot<V: ValueInterface>: MaybeSend + MaybeSync + Debug {
    async fn read(&self) -> Result<Option<(State<V>, Version)>, Error>;   // None: the value does not exist
    async fn write(&self, state: State<V>) -> Result<Version, Error> {   // default: read-only; creates when absent
        Err(Error::not_supported("this accessor is read-only".into())) }
    async fn info(&self) -> Result<AccessorInfo, Error>;                 // status, progress, log length
    async fn log(&self, since: usize) -> Result<Vec<LogEntry>, Error> { Ok(Vec::new()) }
    fn capabilities(&self) -> Capabilities;
    async fn poll_event(&self) -> Option<RootEvent> { None }            // non-blocking; default: no notifications
    /// A `'static` future (`maybe_send.rs` `BoxFuture`, Send on native only) that completes when
    /// `poll_event` has something; it owns what it waits on, such as a cloned `watch::Receiver`.
    fn watch(&self) -> Option<BoxFuture<'static, ()>> { None }
}
pub enum RootEvent { Status(Status), PrimaryProgress(ProgressEntry), SecondaryProgress(ProgressEntry),
    Log(usize), Changed(Version), Removed }
pub struct AccessorHub<V> { /* root, write lock, projection tree, policy */ }
impl<V: ValueInterface> AccessorHub<V> {
    pub fn new(root: Arc<dyn AccessorRoot<V>>, address: Query, policy: ExpiryPolicy) -> Arc<dyn ValueAccessor<V>>;
}
pub enum StoreView { Binary, Metadata, Value }
pub fn store_accessor<E: Environment>(envref: EnvRef<E>, key: Key, view: StoreView, address: Query) -> Arc<dyn ValueAccessor<E::Value>>;
pub fn key_accessor<E: Environment>(envref: EnvRef<E>, key: Key, address: Query) -> Arc<dyn ValueAccessor<E::Value>>;
pub fn asset_accessor<E: Environment>(asset: AssetRef<E>, address: Query) -> Arc<dyn ValueAccessor<E::Value>>;
/// `None`: an empty variable (status `None`) until the first set. Clone the returned Arc to share it.
pub fn memory_accessor<V: ValueInterface>(initial: Option<State<V>>, address: Query) -> Arc<dyn ValueAccessor<V>>;
pub fn read_only<V>(inner: Arc<dyn ValueAccessor<V>>) -> Arc<dyn ValueAccessor<V>>;
pub fn validated<V>(inner: Arc<dyn ValueAccessor<V>>, check: Arc<dyn Fn(&V) -> Result<(), Error> + Send + Sync>) -> Arc<dyn ValueAccessor<V>>;
/// Type-erased carrier, so a value can return its accessor through generic impls.
pub struct AccessorBox<V>(pub Arc<dyn ValueAccessor<V>>);
pub trait ErasedAccessor: MaybeSend + MaybeSync + Debug { fn as_any(&self) -> &dyn std::any::Any; }
```

`V: ValueInterface` is a bound of the traits and of `AccessorSnapshot`, because `State<V>` requires
it. Roots that need the environment hold it **weakly** and upgrade on each call, as the asset
manager does since `environment-manager-reference-cycle`: an accessor lives in a cached asset, and a
strong `EnvRef` there would make environment → manager → asset → hub → root → environment a cycle.
Every write builds its state with `State::from_value_and_metadata` and resets the status to `None`
first, so a status read back (`Expired`, `Error`) never leaks into `set_binary` / `set_state`
(`written_status` keeps a supplied `Expired` or `Error`). Concrete core types:
- **`StoreRoot<E>`** is the root for data in the store (AC-20). It never evaluates anything: a key
  that only a recipe could make does not exist for it.
  - **Reads** use `AsyncStore::get` (the binary and value views) or `get_metadata` (the metadata
    view). A key the store does not contain reads as `None`, so the status is `None`. The views are:
    - `Binary`: `V::from_bytes`;
    - `Metadata`: `V::from_metadata` of the `MetadataRecord` (legacy metadata is a
      `conversion_error`);
    - `Value`: `DefaultValueSerializer::deserialize_from_bytes` with the metadata's
      `type_identifier` and `data_format`, the format defaulting to the key's extension.
  - **Version:** the metadata's `version`, or else `Version::from_content` of the data. The
    metadata view uses `Version::from_bytes` of the record's canonical JSON.
  - **Writes** all go through `AssetManager::set_binary`, so a live asset of the key is replaced and
    its dependents are invalidated, exactly as for any other write of the key:
    - `Binary`: the new bytes with the current metadata, or new metadata for a new key;
    - `Value`: the value serialized with `as_bytes` in the format above, and metadata naming its
      type identifier;
    - `Metadata`: the current bytes with the new record, read back from the value through
      `try_into_json_value` and serde. A key that does not exist cannot be created from the metadata
      view alone (`Error::key_not_found`): there are no data to describe.

    `set_binary` decides the status (`Source`, or `Override` when the key has a recipe), so the
    metadata view cannot set it.
  - **Info:** the status and message come from the stored metadata, and `None` when the key is
    missing. There is no progress, and the stored log is the metadata's `log`.
  - **Notifications:** none pushed in v1 (preflight). Writes through the hub notify the hub's own
    accessors at once; other writers are found on `get` and `refresh`.
- **`KeyRoot<E>`** is the root for keyed assets (AC-21).
  - **Reads** go through `AssetManager::get(key)` → `AssetRef::get`. For an expired or not yet
    evaluated asset, that read is the re-evaluation. A key for which `can_make` is false reads as
    `None`. The version comes from `AssetRef::get_metadata().version()` (`AssetInfo` has none). A
    missing version counts as *changed* on every check, never as equal. `get` builds a new
    `AssetRef` for an expired entry without sending `Removed`, so the root compares
    `AssetRef::id()` on every read and re-subscribes when it differs.
  - **Writes** use `set_state`, which gives `Override` with a recipe and `Source` without (AC-8). On
    a key that does not exist it creates the asset as `Source`. It then reads the new version back,
    and subscribes to the new asset.
  - **Info** uses `get_asset_info`, which does not evaluate: status, message and primary progress.
    `AssetNotFound` there gives status `None`. The secondary progress is the last one relayed. The
    log comes from `AssetRef::get_metadata` of the live asset, or the stored metadata otherwise.
  - **Notifications** come from `subscribe_to_notifications` of the live asset:
    - `StatusChanged`, `JobSubmitted`, `JobStarted`, `ErrorOccurred` and `Expired` are relayed as
      `Status`.
    - `PrimaryProgressUpdated` and `SecondaryProgressUpdated` are relayed as progress.
    - `LogMessage` becomes `Log`, with the new count from the asset's metadata.
    - `ValueProduced` becomes `Changed`.
    - `Removed` becomes `Changed` while `can_make(key)` is still true, because a replacement also
      sends `Removed`; the root then subscribes to the new asset. Otherwise it becomes `Removed`,
      and the status is `None`. The root accessor stays valid: a later set creates the key again.
      Projections are invalidated, because their slots are gone.
- **`AssetRefRoot<E>`** wraps an `AssetRef` the holder already has: a query asset, or an unmanaged
  one (an `apply` result, a `ClosureAsset`). It relays the same notifications, progress and log. It
  is read-only (AC-21), because the asset manager has no write for a non-keyed asset. Its `Removed`
  is final.
- **`MemoryRoot<V>`** is a variable (AC-22): a value, or none, behind a `std::sync::Mutex`, and a
  counter version. Its status is `Source` when it holds a value and `None` when it does not. It has
  no progress and no log. Holders share the variable by cloning the `Arc` that `memory_accessor`
  returns, because every clone is the same hub, and a set through one clone sends `Changed` to all
  of them. A UI element would use it for its own query (AC-10). Wiring an element to it belongs to
  the UI designs.
- **`AccessorHub<V>`** (private) holds a `tokio::sync::Mutex` *write lock*. A set holds it across
  read → derive → compare → put → write, which serialises writers on one hub. The projection tree
  sits behind a separate `std::sync::Mutex` that is never held across an `.await`. The hub collects
  the senders and sinks to fire while holding the tree lock, and fires them after releasing it.
  Driving the hub:
  - **Native:** the hub spawns one watcher task (`tokio::spawn`) for a root that offers `watch`.
    The task holds the hub weakly and ends when the hub is dropped. It is spawned only when
    `tokio::runtime::Handle::try_current()` succeeds (`cfg(not(target_arch = "wasm32"))`; wasm's
    tokio has only `sync`); otherwise `driver()` hands the future out, as on wasm.
  - **wasm:** the hub spawns nothing, because core has no wasm executor. `get`, `set` and `refresh`
    drain `poll_event` first. `AccessorHub::driver()` hands out the watcher future instead, and the
    host spawns it: `liquers-web` does so with `spawn_local` when the first JavaScript subscriber
    attaches. A holder with no driver, such as a UI runner, calls `refresh` on its own tick.

  **A value that does not exist.** When `read` gives `None`, `get` fails (the root's not-found
  error), `info` reports status `None` with no fingerprint, and projections of it are invalidated.
  A `set` on the root writes through `write`, which creates the value, provided the accessor last
  saw it absent; `set_if(…, None, …)` makes the same check explicitly, and fails as a conflict when
  the value exists. A `set` on a projection of an absent root fails with `not_available`, because
  there is no parent to put the part into.
- **Steps**, sharing one `Arc<CommandMetadataRegistry>` taken once per hub
  (`context.get_envref().command_metadata_snapshot()`, which clones the registry) where they edit
  queries. Steps see only the parent value, not its metadata, so they choose by the value's
  `identifier()` and content:
  - `RecipeStep(usize)` and `FieldStep(String)` / `ItemStep(usize)` work on a
    **`serde_yaml::Value` tree** parsed from the document's text or bytes. They keep unknown keys and
    key order, and write back in the parent's representation and format. `RecipeStep` yields
    `Value::Recipe`. Its anchor is the list length plus `Recipe::filename`, or the position when there
    is none.
    A recipe goes to and from the tree through `try_into_json_value` and serde, and back to a value
    with `from_recipe`.
  - `QueryFieldStep` and `PipelineFieldStep` take a recipe to its query or pipeline. A query is
    written back through `QueryEditor`, whose conversion is lenient about commands the registry does
    not know (the strict `Query::try_from(&PipelineEditor)` is not used).
  - `ActionStep(usize)` uses `PipelineEditor` element indices, where 0 is the resource
    (`pipeline/convert.rs`). Its anchor is the element count plus the action's `CommandKey`.
  - `ArgumentStep(ArgRef)` puts through `ActionEditor::set` / `unset`, whose refusals give AC-3.
    `describe` reports `overridden` when `Recipe.arguments` or `links` names the argument.
- **Fingerprint content:** always `Version::from_bytes` of the part's canonical JSON, so a disjoint
  edit never changes a part's token (AC-14). The root version is only a cache key: when it is
  unchanged, the hub reuses the fingerprints it has and derives nothing. A part with no JSON form
  uses the root version, which reads as *changed* and is never wrongly *unchanged*. There is no
  pointer-identity shortcut: `ValueInterface` has no identity hook.
- **Errors:** `Error::accessor_conflict(address: &Query, current: Option<&Fingerprint>)` (`None`
  when the value no longer exists: the conflict then carries no token and HTTP sends no `ETag`) is a new constructor on the
  existing **`ErrorType::StatusConflict`**, as `store-conditional-write` proposes for its
  `write_conflict`. So no `ErrorType` variant is added, and the error enums of py and web are
  untouched. The current token is carried in the message and, over HTTP, in the response's `ETag`.

**Language wrappers (Phase 1 AC-24): using existing accessors.** Python and JavaScript get one
wrapper class each over `Arc<dyn ValueAccessor<Value>>`. They follow the precedent of the UI
element wrappers (`lq.UIElement`, `LiquersUIElement`): the value bridge turns
`ExtValue::Accessor` into the wrapper, and the wrapper back into the *same* accessor, so an
accessor passed to a command or returned from one is never copied. Each wrapper keeps the
environment it came from, which `project` needs. Both are thin: every method calls the trait.
Constructing a new root or step in a language is out of scope
(`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`).

| Python `lq.Accessor` (`liquers.accessors`) | JavaScript `Accessor` | Trait call |
|---|---|---|
| `env.accessor(query)` | `liquers.accessor(query)` | evaluate; an accessor value is used as is, any other value's asset is wrapped by `asset_accessor` (read-only), as the web route does |
| `env.memory_accessor(value=None, address=None)` | `liquers.memoryAccessor(value?, address?)` | `memory_accessor` |
| `address`, `capabilities` (`.write`, `.notify`) | `address`, `capabilities` | `address`, `capabilities` |
| `get()` → the converted value | `get()` → `Promise<value>` | `get` |
| `snapshot()` → `lq.AccessorSnapshot` (`value`, converted lazily; `state`; `token`; `root_version`) | `snapshot()` → `Promise<AccessorSnapshot>` (`value()`, `state`, `token`) | `get` |
| `set(value)` → the new token | `set(value)` → `Promise<string>` | `set` |
| `set_if(value, token)`; `token=None` creates | `setIf(value, token \| null)` | `set_if` |
| `info()` → `lq.AccessorInfo` (`status`, `message`, `primary_progress`, `secondary_progress`, `log_len`, `token`) | `info()` → `Promise<AccessorInfo>` | `info` |
| `log(since=0)` → list of log entries | `log(since?)` → `Promise<LogEntry[]>` | `log` |
| `describe()` → `lq.PartDescription` | `describe()` → `Promise<PartDescription>` | `describe` |
| `refresh()` | `refresh()` → `Promise<void>` | `refresh` |
| `project(actions)` → `lq.Accessor` | `project(actions)` → `Promise<Accessor>` | evaluates the address with `actions` appended (`…/-/ns-acc/recipe-0/query`) in the wrapper's environment |
| `on(callback)` → `lq.AccessorSubscription` (`dispose()`, a context manager) | `subscribe(callback)` → `{ dispose() }` | `subscribe` |
| `events()` (async iterator), `wait_event(timeout=None)`, `process_events()` | — | `subscribe` |

- **Values.** A set converts the language value through the `VALUE` bridge, so a set accepts what
  a command's return accepts. The event and info objects are plain data: `lq.AccessorEvent` has
  `kind` (`"status"`, `"progress"`, `"log"`, `"changed"`, `"invalidated"`, `"removed"`) and the
  event's data, and the JavaScript event is the WebSocket's JSON shape (Web Endpoints).
- **Python waits and callbacks.** Each blocking method has an `_async` form (`get_async`,
  `set_async`, …), and awaiting one never blocks the caller's loop (`aio.rs`). The blocking forms
  release the GIL. Callbacks follow `lq.UI`: a background task only queues events (bounded, the
  oldest dropped), and a callback runs only on a thread that asks for it (`wait_event`,
  `process_events`, or iterating `events()` on the event loop), never under a lock. A raising
  callback goes to `sys.unraisablehook`. The hub's watcher runs on liquers-py's runtime, which
  `memory_accessor` enters (`runtime.rs`).
- **JavaScript.** Every async method returns a `Promise`, as `Asset` does, because the trait is
  async. A callback runs from the driver task (`spawn_local`), never while a `RefCell` is borrowed,
  as in `ui.rs`. Disposing the last subscription stops the driver.
- **Errors** map through each integration's error bridge. A conflict is
  `ErrorType::StatusConflict` (`lq.StatusConflictError`; `errorType === "status_conflict"` in
  JavaScript). Its message carries the current token in a fixed form (`current token: <token>`),
  which each wrapper parses into a `token` attribute, because `Error` has no field for it.
- **Per-holder tokens.** A wrapper object keeps the token it last read or wrote, and its `set`
  sends `set_if` with that token. So two Python or JavaScript holders of one accessor conflict with
  each other as AC-5 says, although they share one Rust handle.
- **Declarations.** `liquers/accessors.pyi` and the TypeScript declarations (`typescript.rs`) list
  every member above (`STUBS`).

**`ValueInterface` hooks** (`liquers-core/src/value.rs`), both defaulted:
`fn from_accessor(a: Arc<dyn ValueAccessor<Self>>) -> Result<Self, Error>` (default
`Error::not_supported`), and `fn try_into_accessor(&self) -> Result<Arc<dyn ValueAccessor<Self>>, Error>`
(default `Error::conversion_error`).
- `CombinedValue`'s generic impl (`liquers-lib/src/value/extended.rs`) forwards both through two new
  defaulted `ValueExtension` hooks:
  - `fn erased_accessor(&self) -> Option<Arc<dyn ErasedAccessor>>`;
  - `fn from_erased_accessor(a: Arc<dyn ErasedAccessor>) -> Option<Self>`.
- The generic impl wraps `AccessorBox<Self>` and downcasts it with `as_any`. That succeeds exactly
  when `Self` is lib's `Value`, which is the only accessor type `ExtValue` holds.

**Where the variant lives (Phase 1 question 1).** The traits, roots, steps and hooks are in core, as
decided. *Decided (Orest, 2026-11-16):* the accessor lives in core and is used through
**`ExtValue`**: `ExtValue::Accessor { value: Arc<dyn ValueAccessor<crate::value::Value>> }`, a
`dyn` trait object so that any custom accessor fits. Core `Value` gets no variant. It derives
`PartialEq` and untagged `Deserialize`, and a trait object would force hand-written impls of both.
The variant follows `UIElement`'s precedent for `Debug`, `Clone` and `Send`. Its `TypeInfo::new("Accessor")` (CamelCase, as `UIElement` and `RecordView`) declares no formats,
so persisting it yields `NonSerializable` and it is never stored (AC-11). Its `data_node` is opaque.

**Records (lib):** `RowStep`, `ColumnStep` and `CellStep` live in `liquers-lib/src/records/accessor.rs`,
because they need `ExtValue::RecordView`. A cell `put` clones the batch and replaces only the edited
column. These steps are Rust API only: no `acc` command covers row, column or cell yet (AC-9 is
proven through the API).

**Later template-link projections (question 8):** these would be a `ProjectionStep` from template
text to the query of one link, in `liquers-template` (core-only). Its `put` would rewrite the link
at the position the parser reports. Nothing here precludes it.

## Integration Points

- **`liquers-core`:** `accessor/` (the hub, `StoreRoot`, `KeyRoot`, `AssetRefRoot`, `MemoryRoot`, the steps,
  `read_only`, `validated`), `lib.rs`, `value.rs` (the hooks), `error.rs` (`accessor_conflict`).
- **`liquers-lib`:**
  - `value/mod.rs`: the variant, its `TypeInfo`, and an arm in every `ExtValue` match, including
    `egui/mod.rs`, `ui/web/html.rs` (`webui`) and `tests/value_type_system.rs` (`samples`);
  - `value/extended.rs`: the hooks;
  - `records/accessor.rs`;
  - a new `accessor/` module holding the `acc` commands and a `#[macro_export]
    register_accessor_commands!`, called wherever the other groups are listed:
    `commands.rs` (`register_all_commands!`), `bin/export_command_registry.rs` (a
    `Group::Accessor`), `tests/registry_export.rs`, `liquers-py/src/builtins.rs` and
    `liquers-web/src/builtins.rs`;
  - `specs/command_registry.yaml`, regenerated.
- **`liquers-axum`:**
  - a new `accessor/` module with `AccessorApiBuilder<E>`, mounted like `AssetsApiBuilder`;
  - accessor handlers map `StatusConflict` to `412` themselves (`api_core/error.rs` maps it to `409`
    for every other route);
  - the accessor WebSocket, reusing the connection handling of `assets/websocket.rs`.
- **`liquers-py`:** a new `src/accessor.rs` (`Accessor`, `AccessorSnapshot`, `AccessorInfo`,
  `AccessorEvent`, `PartDescription`, `AccessorSubscription`), registered in `lib.rs`; `value.rs`
  converts `ExtValue::Accessor` both ways; `environment.rs` gains `accessor` and
  `memory_accessor`; the Python package gains `liquers/accessors.py` and `accessors.pyi`; the
  conformance matrix in `liquers-py/README.md` gains the `ACCESSOR` rows.
- **`liquers-web`:** a new `src/accessor.rs` (the `Accessor` class and its subscriptions);
  `src/default_value.rs` converts `ExtValue::Accessor` both ways in its exhaustive matches;
  `environment.rs` gains `accessor` and `memoryAccessor`; `typescript.rs` the declarations, checked
  by `check-stubs.sh`.

## Error Handling

| Case | Constructor |
|---|---|
| Stale fingerprint (AC-5) | `Error::accessor_conflict(address, current)` (new, `ErrorType::StatusConflict`) |
| `get` of a value that does not exist: store root, keyed-asset root | `Error::key_not_found`, `Error::asset_not_found` |
| `get` of an empty memory variable | `Error::not_available().with_query(address)` |
| A create (`set_if` with `None`) when the value exists, or a set based on an absent value that now exists | `Error::accessor_conflict(address, current)` |
| A metadata-view set on a key that does not exist (AC-20) | `Error::key_not_found` |
| Projected slot gone | `Error::not_available().with_query(address)` |
| A step cannot parse its parent (not YAML, not a recipe list) | `Error::conversion_error` |
| An argument refused by the editor (AC-3) | the `ActionEditor` error, passed through unchanged |
| The state is not a key, or not an accessor | `Error::conversion_error` |
| A root write fails | the `set_state` error, with the key attached |
| A set through a read-only root or wrapper (AC-18) | `Error::not_supported` |
| A validating wrapper refuses the value (AC-18) | the check's own error, unchanged |
| A Python or JavaScript value that the value bridge cannot convert, on `set` | the bridge's `conversion_error` |
| A Python value of a non-accessor passed where an accessor is expected | `Error::conversion_error` |

## Relevant Commands

All are in `liquers-lib`, namespace `acc`. Each is cached as an asset, so one query yields one
shared handle, and none is stored. The projection commands are `async`. Each one:
- awaits `get_asset_info` of `context.get_asset_ref()` to find its own address;
- calls `project`;
- registers a sink that expires that asset (held weakly).

`accessor` and `store` register no sink: a root accessor stays valid when its key is removed, and
reports status `None` (AC-15). There is no command for a memory accessor: a variable is made in
Rust, by its owner, and shared by cloning. An accessor of any other query is made by
`asset_accessor` in Rust, or by the web API (below).

```rust
register_command!(cr, async fn accessor(state, context) -> result namespace: "acc" label: "Accessor" doc: "A settable accessor of the keyed asset given as state (-R-key/…)")?;
register_command!(cr, async fn store(state, view: String = "value", context) -> result namespace: "acc" label: "Store accessor" doc: "A settable accessor of the stored data (view: binary, metadata or value) of the key given as state")?;
register_command!(cr, async fn recipe(state, index: usize, context) -> result namespace: "acc" label: "Recipe")?;
register_command!(cr, async fn query(state, context) -> result namespace: "acc" label: "Recipe query")?;
register_command!(cr, async fn pipeline(state, context) -> result namespace: "acc" label: "Recipe pipeline")?;
register_command!(cr, async fn action(state, index: usize, context) -> result namespace: "acc" label: "Pipeline element")?;
register_command!(cr, async fn argument(state, name: String, context) -> result namespace: "acc" label: "Action argument")?;
register_command!(cr, async fn field(state, name: String, context) -> result namespace: "acc" label: "Document field")?;
register_command!(cr, async fn item(state, index: usize, context) -> result namespace: "acc" label: "List item")?;
register_command!(cr, async fn get(state) -> result namespace: "acc" label: "Accessor value" volatile: true)?;
```

For example `-R-key/reports/new.yaml/-/ns-acc/store-binary` is the binary data of a key that may
not exist yet. Building the root from `-R-key/` gives a `UseKeyValue` step with no dependency (`plan.rs`). So
writing the file never expires the accessor itself, and every root accessor of one key is one
query asset with one hub. `get` is volatile, so it reads the current value every time.

## Web Endpoints

These routes come from `AccessorApiBuilder<E>`, mounted at `/api/acc` by default:
Every route has a fixed prefix (`q/`, `info/`, `log/`, `describe/`, `ws/`), as the assets API
does, so no key is shadowed by a route name. The `{*query}` is evaluated. When its value is an accessor, that accessor is used. Otherwise the
query's own asset is wrapped by `asset_accessor`, so any query can be read and watched, read-only
(AC-21).
- `GET {base}/q/{*query}`: the part's value, in the requested format, with `ETag: <token>`. A value
  that does not exist answers `404`, with the info as the body.
- `GET {base}/info/{*query}`: the `AccessorInfo` as JSON (status, message, progress, log length,
  `etag`). It does not evaluate the value.
- `GET {base}/log/{*query}?since=<n>`: the log messages from index `n`, as JSON.
- `GET {base}/describe/{*query}`: the `PartDescription` as JSON.
- `PUT {base}/q/{*query}`: `set_if`. `If-Match: <token>` sets a value that exists, and
  `If-None-Match: *` creates one that does not (AC-20, AC-21). It answers `200` with the new `ETag`,
  `412` with the current `ETag` on a conflict, `428` when neither header is given, and `405` for a
  read-only accessor.
- `GET {base}/ws/{*query}`: a WebSocket that relays the accessor's events as JSON
  (`{"event": "status", "status": "Expired"}`, `{"event": "progress", "primary": …}`,
  `{"event": "progress", "secondary": …}`, `{"event": "log", "count": 3}`,
  `{"event": "changed", "etag": …}`, `invalidated`, `removed`). It follows the connection handling, limits and close codes of the asset event socket
  (`liquers-axum/src/assets/websocket.rs`, `ws_query_handler`). The `ui-split-elements` protocol
  can carry the same event shape.

## Documentation Architecture

**Documents** (written in Phase 5, once the code exists, because a reference must be true at
HEAD):
- `specs/reference/VALUE_ACCESSORS.md` (new reference, internal, `core/value`):
  - root versus projection: the root does I/O, a projection is a synchronous step over the root's
    value, and one hub per root keeps every projection in step;
  - the first-class accessors (store in its three views, keyed asset, other assets, memory), what
    each reads, writes, creates and relays, and the status `None` of a value that does not exist;
  - the API in detail: `ValueAccessor`, `AccessorRoot`, `ProjectionStep`, the hub, fingerprints
    and tokens, events and `info` / `log`, conflicts, the wrappers, the `acc` commands, the web
    routes, and the Python and JavaScript classes.
  It joins `affects_docs` once it exists.
- `specs/guides/VALUE_ACCESSOR_GUIDE.md` (new guide): making an accessor (from a query, in Rust, as
  a memory variable) and getting and setting its value; projecting it; listening to events and the
  holder contract (`Changed` → redraw, `Expired` → `get`, missed events → `info`); handling a
  conflict; implementing a new root, step or wrapper with the hub. Each part with Rust, Python and
  JavaScript examples where the language supports it.
- `LANGUAGE-INTEGRATION_GUIDE.md`: a new feature `ACCESSOR` (Optional; depends on `VALUE`, `EVAL`,
  `ERROR`, `RUNTIME`, and `ASYNCQ` soft) with two directions, in the way `DATANODE` has two:
  1. **Use (outbound wrapper):** an object over an existing accessor, its waits, callbacks and
     error mapping, and crossing back as the same accessor. Python and JavaScript select it.
  2. **Define (inbound adapter):** a language object as an `AccessorRoot` or `ProjectionStep`.
     Python and JavaScript mark it `NA`, with
     `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT` as the reversing condition.
  Phase 3 gives the feature its test IDs (`ACCESSOR01`…), and the guide's Appendix A its reference
  tests.
- `VALUE_TYPE_SYSTEM.md`: `ExtValue::Accessor` as a justified handle, the `accessor` identifier,
  and why it is never stored.
- `PYTHON_INTEGRATION.md`: `lq.Accessor` and its wait and callback rules. `PYTHON_GUIDE.md` and
  `JAVASCRIPT_GUIDE.md`: a section each on editing through accessors.
- `RECIPES_AND_PLANS.md`: editing `recipes.yaml` through accessors. Comments and formatting are not
  kept; unknown keys and order are.
- `WEB_API_SPECIFICATION.md`: the `/api/acc` routes.
- `UI_INTERFACE_FSD.md`: only that `MemoryRoot` is the root for an element-owned value. Binding an
  element is the UI designs' work.
- User guides: the values chapter of each guide (`docs/guides/{rust,python,web}/04-values.md`)
  gains a section on the accessor value, and `e-web-api.md` the routes. Phase 5 reviews every
  chapter `chapters-for` lists for `affects_docs`.

**API docs**
- All five parts: `liquers-core/src/accessor/mod.rs`, `liquers-lib/src/accessor/mod.rs`,
  `liquers-axum/src/accessor/mod.rs`, `liquers-py/src/accessor.rs` and
  `liquers-web/src/accessor.rs`.
- The Value module docs are brought up to `API_DOCUMENTATION_GUIDE.md`:
  - `liquers-core/src/value.rs`: the value layers, the conversion hooks with the accessor hooks
    among them, and a section on handle values (what a variant may hold, why handles are not
    stored);
  - `liquers-lib/src/value/mod.rs`: the `ExtValue` variants grouped by kind, with the accessor as a
    handle beside `UIElement` and `Widget`.
- Main components only:
  - `liquers-lib/src/value/extended.rs` (the hooks);
  - `liquers-lib/src/records/` (the steps);
  - `liquers-core/src/pipeline/editor.rs` (where used: accessors).

`liquers-core/src/data_node.rs` is unchanged, as Phase 1 question 7 decided.

## Risks

| Assessment | Finding |
|---|---|
| Compatibility / data | Additive. **Phase 1 AC-2 is narrowed (accepted for v1):** rewriting through a `serde_yaml::Value` tree keeps every key and its order, but drops comments and formatting (`YAML-DOCUMENT-EDIT-DROPS-COMMENTS`) |
| Concurrency | First-writer-wins only within one hub. Other writers are last-writer-wins until `store-conditional-write`. `watch` delivers only the latest event, so a burst of progress can hide a `Changed`: holders check the fingerprint in `info()` on every event (Overview) |
| Change detection | A store root, and a keyed-asset root with no live asset, see other writers only on `get` and `refresh` until the manager event stream exists (preflight) |
| Performance | One root read per change. Derive and hash run only for subscribed nodes, and nothing is derived when the root version is unchanged |
| Security | The `PUT` route is opt-in and for trusted servers only (preflight) |
| Build | `ExtValue` arms in lib, py and web, the `records` gating of the lib steps, and the wasm driver path: `check-build-matrix.sh`, `scripts/test-liquers-py.sh`, the liquers-web Node loop and `check-stubs.sh` |
| Tests likely to change | `registry_export` (new commands) |
| Decided at the gate (2026-11-16) | the variant is in `ExtValue`; AC-2's comment loss is accepted for v1 and tracked in `YAML-DOCUMENT-EDIT-DROPS-COMMENTS`; the four first-class roots, a missing value as status `None` with create-on-set, and the progress and message relay (second round); a metadata-view set cannot create a key or choose the status (`set_binary` decides); a memory variable has no command |
