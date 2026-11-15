# Phase 2: Solution & Architecture — Settable value accessor

## Overview

An accessor is **a root plus a chain of synchronous projection steps**, shared through one **hub**
per root.
- **Root.** The root is asynchronous. It reads and writes the value where it lives: a key through
  the asset manager, or an in-memory value owned by its holder.
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
| `UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS` | draft | P3 | An element holding an invalidated accessor keeps it | No | No | Left to the UI designs that consume accessors |
| `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`, `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET` | draft | P3 | `ArgumentStep` relies on `ActionEditor` refusals (AC-3) | No | No | Phase 3 edge cases avoid both |
| `CORE-TOKIO-REMOVAL`, `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS`, `STORE-HANDLER-FIELD-ACCESS-HARDCODED` | — | P3 | Same `watch` primitive as `AssetRef`; out-of-process changes are a non-goal; question 7 overlap | No | No | None |

Command namespaces involved: `acc` (new). No existing namespace changes.

## Interfaces

**Core, `liquers-core/src/accessor/`** (`mod.rs`, `hub.rs`, `root.rs`, `steps.rs`). It is generic
over the value `V`, because the value variant cannot name an environment. Every `async_trait` below
uses the `cfg_attr` pair of `data_node.rs` (`?Send` on wasm).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint { pub anchor: u64, pub content: Version }
impl Fingerprint { pub fn token(&self) -> String; pub fn from_token(s: &str) -> Result<Self, Error>; }
pub struct AccessorSnapshot<V> { pub state: State<V>, pub fingerprint: Fingerprint, pub root_version: Version }
pub enum AccessorEvent { Initial, Changed(Fingerprint), Invalidated, Expired, Removed }
#[derive(Default)] #[non_exhaustive] pub struct SetOptions {}
#[derive(Serialize, Deserialize)]
pub struct PartDescription { pub step: String, pub type_identifier: String,
    pub argument: Option<ArgumentInfo>, pub command: Option<CommandKey>,
    pub overridden: bool /* a recipe argument or link override supersedes the query's value */, pub read_only: bool }

#[async_trait] pub trait ValueAccessor<V>: MaybeSend + MaybeSync + Debug {
    fn address(&self) -> &Query;
    async fn get(&self) -> Result<AccessorSnapshot<V>, Error>;
    async fn set(&self, value: V, options: SetOptions) -> Result<AccessorSnapshot<V>, Error>;  // vs. last-seen fingerprint
    async fn set_if(&self, value: V, expected: Fingerprint, options: SetOptions) -> Result<AccessorSnapshot<V>, Error>;
    async fn describe(&self) -> Result<PartDescription, Error>;
    async fn refresh(&self) -> Result<(), Error>;            // pull root events (wasm has no watcher task)
    fn subscribe(&self) -> watch::Receiver<AccessorEvent>;
    fn project(&self, step: Arc<dyn ProjectionStep<V>>, address: Query) -> Result<Arc<dyn ValueAccessor<V>>, Error>;
    fn set_invalidation_sink(&self, sink: Arc<dyn InvalidationSink>);
}
pub trait ProjectionStep<V>: MaybeSend + MaybeSync + Debug {
    fn name(&self) -> Cow<'_, str>;
    fn get(&self, parent: &V) -> Result<Option<V>, Error>;            // None: the slot is gone
    fn put(&self, parent: &V, part: V) -> Result<V, Error>;
    fn anchor(&self, parent: &V) -> Result<Option<u64>, Error> { Ok(self.get(parent)?.map(|_| 0)) }
    fn describe(&self, parent: &V) -> Result<PartDescription, Error>;
}
#[async_trait] pub trait InvalidationSink: MaybeSend + MaybeSync { async fn invalidated(&self); }
#[async_trait] pub trait AccessorRoot<V>: MaybeSend + MaybeSync + Debug {
    async fn read(&self) -> Result<(State<V>, Version), Error>;
    async fn write(&self, state: State<V>) -> Result<Version, Error>;
    async fn poll_event(&self) -> Option<RootEvent>;                    // non-blocking
}
pub enum RootEvent { Changed(Version), Expired, Removed }
pub fn key_accessor<E: Environment>(envref: EnvRef<E>, key: Key, address: Query) -> Arc<dyn ValueAccessor<E::Value>>;
pub fn memory_accessor<V: ValueInterface>(initial: State<V>, address: Query) -> Arc<dyn ValueAccessor<V>>;
/// Type-erased carrier, so a value can return its accessor through generic impls.
pub struct AccessorBox<V>(pub Arc<dyn ValueAccessor<V>>);
pub trait ErasedAccessor: MaybeSend + MaybeSync + Debug { fn as_any(&self) -> &dyn std::any::Any; }
```

`V: ValueInterface` bounds go on the impls, not on the traits. Concrete core types:
- **`KeyRoot<E>`** reads with `AssetManager::get(key)` → `AssetRef::get`, and gets the version
  from `get_asset_info`. A missing version counts as *changed* on every check, never as equal. It
  writes with `set_state`, which gives `Override` with a recipe and `Source` without (AC-8), then
  reads the new version back. Asset notifications reach it through `subscribe_to_notifications`.
  `Removed` with `can_make(key)` still true becomes `Changed`, because a replacement also sends
  `Removed`. Otherwise it becomes `Removed`.
- **`MemoryRoot<V>`** holds a value and a counter version. A UI element would use it for its own
  query (AC-10). Wiring an element to it belongs to the UI designs.
- **`AccessorHub<V>`** (private) holds a `tokio::sync::Mutex` *write lock*. A set holds it across
  read → derive → compare → put → write, which serialises writers on one hub. The projection tree
  sits behind a separate `std::sync::Mutex` that is never held across an `.await`. The hub collects
  the senders and sinks to fire while holding the tree lock, and fires them after releasing it.
  Driving the hub:
  - **Native:** `key_accessor` spawns one watcher task (`tokio::spawn`), which holds the hub weakly
    and ends when the hub is dropped.
  - **wasm:** there is no task. `get`, `set` and `refresh` drain `poll_event` first. A holder that needs
    push-style updates, such as a UI runner, calls `refresh` on its own tick.
- **Steps**, each capturing `Arc<CommandMetadataRegistry>` (`Context::command_metadata_snapshot`) at
  construction where it edits queries:
  - `RecipeStep(usize)` and `FieldStep(String)` / `ItemStep(usize)` work on a
    **`serde_yaml::Value` tree** parsed from the document's text or bytes. They keep unknown keys and
    key order, and write back in the parent's representation and format. `RecipeStep` yields
    `Value::Recipe`. Its anchor is the list length plus `Recipe::filename`, or the position when there
    is none.
  - `QueryFieldStep` and `PipelineFieldStep` take a recipe to its query or pipeline.
  - `ActionStep(usize)` uses `PipelineEditor` element indices, where 0 is the resource
    (`pipeline/convert.rs`). Its anchor is the element count plus the action's `CommandKey`.
  - `ArgumentStep(ArgRef)` puts through `ActionEditor::set` / `unset`, whose refusals give AC-3.
    `describe` reports `overridden` when `Recipe.arguments` or `links` names the argument.
- **Fingerprint content:** the root version when the root is unchanged (no derive). Otherwise
  `Version::from_bytes` of the part's canonical JSON. A part with no JSON form uses the root version,
  which reads as *changed* and is never wrongly *unchanged*. There is no pointer-identity shortcut:
  `ValueInterface` has no identity hook.
- **Errors:** `Error::accessor_conflict(address, current: Fingerprint)` is a new constructor on the
  existing **`ErrorType::StatusConflict`**, as `store-conditional-write` proposes for its
  `write_conflict`. So no `ErrorType` variant is added, and the error enums of py and web are
  untouched. The current token is carried in the message and, over HTTP, in the response's `ETag`.

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
decided. The variant goes in **`ExtValue` only**: `ExtValue::Accessor { value: Arc<dyn ValueAccessor<crate::value::Value>> }`.
This follows the `UIElement` variant's precedent for `Debug`, `Clone` and `Send`. Core `Value`
derives `PartialEq` and untagged `Deserialize`, and a trait-object variant would force hand-written
impls of both. *For confirmation at the gate.* Its `TypeInfo::new("accessor")` declares no formats,
so persisting it yields `NonSerializable` and it is never stored (AC-11). Its `data_node` is opaque.

**Records (lib):** `RowStep`, `ColumnStep` and `CellStep` live in `liquers-lib/src/records/accessor.rs`,
because they need `ExtValue::RecordView`. A cell `put` clones the batch and replaces only the edited
column. These steps are Rust API only: no `acc` command covers row, column or cell yet (AC-9 is
proven through the API).

**Later template-link projections (question 8):** these would be a `ProjectionStep` from template
text to the query of one link, in `liquers-template` (core-only). Its `put` would rewrite the link
at the position the parser reports. Nothing here precludes it.

## Integration Points

- **`liquers-core`:** `accessor/`, `lib.rs`, `value.rs` (the hooks), `error.rs`
  (`accessor_conflict`).
- **`liquers-lib`:**
  - `value/mod.rs`: the variant, its `TypeInfo`, and an arm in every `ExtValue` match;
  - `value/extended.rs`: the hooks;
  - `records/accessor.rs`;
  - a new `accessor/` module holding the `acc` commands, registered beside the other namespace
    modules (`mem/`, `markdown/`);
  - `specs/command_registry.yaml`, regenerated.
- **`liquers-axum`:**
  - a new `accessor/` module with `AccessorApiBuilder<E>`, mounted like `AssetsApiBuilder`;
  - `api_core/error.rs`: conflicts from accessor routes map to `412`.
- **`liquers-web`:** `src/default_value.rs` gets an arm in its exhaustive `ExtValue` match.
  `liquers-py` changes only if an exhaustive `ExtValue` match exists there; Phase 4 checks.

## Error Handling

| Case | Constructor |
|---|---|
| Stale fingerprint (AC-5) | `Error::accessor_conflict(address, current)` (new, `ErrorType::StatusConflict`) |
| Root key removed | `Error::asset_not_found` with the key attached |
| Projected slot gone | `Error::not_available().with_query(address)` |
| A step cannot parse its parent (not YAML, not a recipe list) | `Error::conversion_error` |
| An argument refused by the editor (AC-3) | the `ActionEditor` error, passed through unchanged |
| The state is not a key, or not an accessor | `Error::conversion_error` |
| A root write fails | the `set_state` error, with the key attached |

## Relevant Commands

All are in `liquers-lib`, namespace `acc`. Each is cached as an asset, so one query yields one
shared handle, and none is stored. The projection commands are `async`. Each one:
- awaits `get_asset_info` of `context.get_asset_ref()` to find its own address;
- calls `project`;
- registers a sink that expires that asset (held weakly).

`accessor` also registers a sink on its root, so the root accessor expires when the key is
`Removed` (AC-15).

```rust
register_command!(cr, async fn accessor(state, context) -> result namespace: "acc" label: "Accessor" doc: "A settable accessor of the key given as state (-R-key/…)")?;
register_command!(cr, async fn recipe(state, index: usize, context) -> result namespace: "acc" label: "Recipe")?;
register_command!(cr, async fn query(state, context) -> result namespace: "acc" label: "Recipe query")?;
register_command!(cr, async fn pipeline(state, context) -> result namespace: "acc" label: "Recipe pipeline")?;
register_command!(cr, async fn action(state, index: usize, context) -> result namespace: "acc" label: "Pipeline element")?;
register_command!(cr, async fn argument(state, name: String, context) -> result namespace: "acc" label: "Action argument")?;
register_command!(cr, async fn field(state, name: String, context) -> result namespace: "acc" label: "Document field")?;
register_command!(cr, async fn item(state, index: usize, context) -> result namespace: "acc" label: "List item")?;
register_command!(cr, async fn get(state) -> result namespace: "acc" label: "Accessor value" volatile: true)?;
```

Building the root from `-R-key/` gives a `UseKeyValue` step with no dependency (`plan.rs`). So
writing the file never expires the accessor itself, and every root accessor of one key is one
query asset with one hub. `get` is volatile, so it reads the current value every time.

## Web Endpoints

These routes come from `AccessorApiBuilder<E>`, mounted at `/api/acc` by default:
- `GET {base}/{*query}`: the part's value, in the requested format, with `ETag: <token>`.
- `GET {base}/describe/{*query}`: the `PartDescription` as JSON.
- `PUT {base}/{*query}` with `If-Match`: `set_if`. It answers `200` with the new `ETag`, `412` with
  the current `ETag` on a conflict, or `428` when `If-Match` is missing.

## Documentation Architecture

**Documents**
- `specs/reference/VALUE_ACCESSORS.md` (new reference, internal, `core/value`): roots, steps, hub,
  fingerprints, events, conflicts, commands, routes. It joins `affects_docs` once it exists.
- `VALUE_TYPE_SYSTEM.md`: `ExtValue::Accessor` as a justified handle, and the `accessor` identifier.
- `RECIPES_AND_PLANS.md`: editing `recipes.yaml` through accessors. Comments and formatting are not
  kept; unknown keys and order are.
- `WEB_API_SPECIFICATION.md`: the `/api/acc` routes.
- `UI_INTERFACE_FSD.md`: only that `MemoryRoot` is the root for an element-owned value. Binding an
  element is the UI designs' work.

**API docs**
- All five parts: `liquers-core/src/accessor/mod.rs`, `liquers-lib/src/accessor/mod.rs` and
  `liquers-axum/src/accessor/mod.rs`.
- Main components:
  - `liquers-core/src/value.rs` (the hooks);
  - `liquers-lib/src/value/mod.rs` (the variant);
  - `liquers-lib/src/records/` (the steps);
  - `liquers-core/src/pipeline/editor.rs` (where used: accessors).

`liquers-core/src/data_node.rs` is unchanged, as Phase 1 question 7 decided.

## Risks

| Assessment | Finding |
|---|---|
| Compatibility / data | Additive. **Phase 1 AC-2 is narrowed:** rewriting through a `serde_yaml::Value` tree keeps every key and its order, but **drops comments and formatting**. This is a gate decision (below) |
| Concurrency | First-writer-wins only within one hub. Other writers are last-writer-wins until `store-conditional-write`. `watch` delivers only the latest event |
| Performance | One root read per change. Derive and hash run only for subscribed nodes, and nothing is derived when the root version is unchanged |
| Security | The `PUT` route is opt-in and for trusted servers only (preflight) |
| Build | `ExtValue` arms in lib and web, the `records` gating of the lib steps, and the wasm watcher path: `check-build-matrix.sh` |
| Tests likely to change | `registry_export` (new commands) |
| Open, for the gate | (a) the variant in `ExtValue` only; (b) AC-2's comment loss. A lossless rewrite needs a YAML editor that preserves comments, which is not a dependency today |
