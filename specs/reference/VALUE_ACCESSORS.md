---
title: Value Accessors
kind: reference
audience: internal
area: [core/value, core/assets, core/query, lib/value, axum, py, web]
reviewed: 2026-11-16
---

# Value Accessors

A **value accessor** is a live handle on the place a value lives: data in the store, a keyed asset,
any asset, a variable in memory, or a part of one of these (a field of a document, a recipe of a
`recipes.yaml`, one action of its query, one argument of that action). Through it a holder reads the
value, writes it back, watches it change, and asks what it is. This document is the contract; how to
use it is [`guides/VALUE_ACCESSOR_GUIDE.md`](../guides/VALUE_ACCESSOR_GUIDE.md).

Code: `liquers-core/src/accessor/` (contract, hub, roots, steps, wrappers), `liquers-lib/src/accessor/`
(the `acc` commands), `liquers-lib/src/records/accessor.rs` (record steps), `liquers-axum/src/accessor/`
(the web routes), `liquers-py/src/accessor.rs` and `liquers-web/src/accessor.rs` (the language
wrappers). Design: `design/settable-value-accessor/`.

## 1. Root and projection

An accessor is a **root** plus a chain of **projection steps**.

| | Root (`AccessorRoot`) | Projection step (`ProjectionStep`) |
|---|---|---|
| Is | the place: where the value is read from and written to | a lens from a value to a part of it |
| Does I/O | yes, async | never: `get`, `put`, `anchor`, `describe` are synchronous |
| Knows | its own source (store, asset, memory) and how to watch it | only the parent value, never its metadata |
| Reports | status, progress, log, removal, changes (`RootEvent`) | nothing; the hub derives what changed |

A projection is never read or written on its own. A `get` of a part reads the root (or the hub's
cached root value) and applies every step from the root down; a `set` of a part reads the root,
`put`s the part into each parent in turn back to the root, and writes the new root value. A step
therefore cannot be slow, cannot block, and cannot see anything the root did not give it.

**One hub per root.** `AccessorHub::new(root, address, policy)` turns a root into a full accessor, and
every projection of it (`project`, at any depth) is a node of the same hub. The hub holds:

- the projection tree, children by weak reference and parents by strong reference, so a projection
  keeps its chain alive and an unused projection is dropped;
- the last value read from the root and its version;
- a write lock (`tokio::sync::Mutex`) held across a whole set (read, compare, put, write), so the
  writers of one hub are serialized. The tree and the cache sit behind `std::sync::Mutex`es that are
  never held across an `.await`; events and sinks are collected under them and fired afterwards.

After every change of the root (a set through any node, or a change the root reports) the hub derives
every live node again and classifies it:

| Class | Condition | Effect |
|---|---|---|
| unchanged | same anchor, same content | nothing |
| changed | same anchor, new content | the node's fingerprint is updated and it gets `Changed` |
| invalidated | the anchor changed, or the slot is gone | `Invalidated`, its `InvalidationSink` is called once, and the node stays dead |

The writer's own node gets `Changed` too: a handle is shared (an `Arc` clone is the same node), so
another holder of the writer's handle must hear of the set. Overlapping projections (a recipe and an
argument of its query) are both derived from the same root value, so both see every edit; disjoint
projections (two recipes) do not conflict, because a set is conditional on the part's own
fingerprint, not the root's.

## 2. Fingerprints, tokens and conflicts

Every read gives a `Fingerprint { anchor, content }`:

- `anchor: u64` is structural: where the part sits. A step's `anchor` (for example the element count
  and command name of an action) changes when the part moves; the hub combines the anchors of the
  chain (blake3), so a moved ancestor moves every descendant.
- `content: Version` is the hash of the part's JSON form, or the root's version when the part has no
  JSON form (bytes, a handle).

Two fingerprints are compared with `==`; `Version::unknown` is not a wildcard. The **token** is the
fingerprint as text, `{anchor:016x}-{content:032x}`, which `Fingerprint::from_token` parses back
(surrounding quotes allowed, so an HTTP `ETag` parses). It is the HTTP `ETag`, and the Python and
JavaScript `token`.

**First writer wins.** `set_if(value, expected)` writes only when the part's current fingerprint is
`expected`; `expected = None` is a conditional create, which succeeds only when the part does not
exist. A mismatch writes nothing and is `Error::accessor_conflict(address, current)`, of type
`ErrorType::StatusConflict`, whose message ends with `current token: <token>` (or `none` when the
part no longer exists). `conflict_token(&error)` reads the token back; this is how the web API answers
`412` with the current `ETag`, and how the Python and JavaScript errors get their `token`.

`set(value)` is `set_if` with the fingerprint *this node* last saw. A node is shared by every holder
of the handle (one query asset is one handle), so `set` catches other handles and outside writers,
not another holder of the same handle. A holder that needs that keeps its own token and calls
`set_if`; the Python and JavaScript wrappers do exactly this, one token per wrapper.

A refused write (a step's `put` fails, the root refuses, a wrapper's check fails) writes nothing and
leaves every fingerprint as it was.

## 3. The contract

### `ValueAccessor<V>`

| Method | Does |
|---|---|
| `address() -> &Query` | the query that names the accessor; empty for one made in code |
| `get() -> AccessorSnapshot<V>` | reads the part: `state`, `fingerprint`, `root_version`. A part that does not exist is the root's not-found error, or the first step's error when a step failed |
| `set(value, SetOptions)` | `set_if` with this node's last-seen fingerprint |
| `set_if(value, Option<Fingerprint>, SetOptions)` | compare-and-set (§2); returns the snapshot as written |
| `info() -> AccessorInfo` | status, message, progress, log length and fingerprint, without evaluating or reading anything |
| `log(since) -> Vec<LogEntry>` | the log messages from index `since` |
| `describe() -> PartDescription` | what the part is, for an editor |
| `capabilities() -> Capabilities` | `write` (a set can succeed) and `notify` (changes are pushed) |
| `refresh()` | pulls the root's pending events; for a root that cannot notify, checks its version. On wasm, and wherever no watcher runs, this is how events arrive |
| `subscribe() -> watch::Receiver<AccessorEvent>` | the event channel (§4) |
| `project(step, address)` | a projection through `step`, named by `address` |
| `set_invalidation_sink(sink)` | called once when a structural change invalidates the accessor |
| `driver() -> Option<BoxFuture>` | the watcher future, handed out once, for a host that spawns it itself (wasm); `None` when nothing is to be watched or it already runs |

`SetOptions` is empty and `#[non_exhaustive]`: it is reserved for locking
(`VALUE-ACCESSOR-CANNOT-LOCK`).

`AccessorInfo { status, message, primary_progress, secondary_progress, log_len, fingerprint }`:
`status` is `Status::None` when the value does not exist, and `fingerprint` is then `None`.
Because `info` never reads the root, its `fingerprint` is also `None` until the accessor has been
read once.

`PartDescription { step, type_identifier, argument, command, overridden, read_only }`: `step` is the
step's name as a query action (`field-title`, `argument-n`) or `root`; `argument` and `command` are the
command metadata of an argument part; `overridden` says a recipe's `arguments` or `links` names the
argument, so its value in the query is not the one evaluation uses.

### `AccessorRoot<V>`

What a custom source implements; the hub supplies everything else.

| Method | Default | Contract |
|---|---|---|
| `read() -> Option<(State<V>, Version)>` | — | the value and its version, `None` when it does not exist |
| `write(state) -> (State<V>, Version)` | refuses (`NotSupported`) | writes, creating the value when missing; returns the state *as written*, with the status the root chose (`Source`, `Override`), and the new version. The hub passes a state whose status it reset to `None` |
| `info()` | — | status, message, progress, log length; the fingerprint is the hub's |
| `log(since)` | empty | |
| `capabilities()` | — | |
| `poll_event() -> Option<RootEvent>` | `None` | the next pending event, without waiting |
| `watch() -> Option<BoxFuture>` | `None` | a future that completes when `poll_event` has something; it owns what it waits on |
| `not_found()` | `not_available` | the error of a `get` of a value that does not exist |

`RootEvent` is `Status`, `PrimaryProgress`, `SecondaryProgress`, `Log(count)`, `Changed(Version)` or
`Removed`. The hub races `watch()` against its own end, so a watcher blocked on a quiet source does
not outlive its accessors. `ExpiryPolicy` says what the hub does on `Status(Expired)`: `Relay` (the
default: the holder calls `get`, which re-evaluates) or `Reload` (read the root at once, so `Changed`
follows with no holder action).

### `ProjectionStep<V>`

| Method | Default | Contract |
|---|---|---|
| `name()` | — | the step as a query action, for `describe` and addresses |
| `get(parent) -> Option<V>` | — | the part; `None` when the slot is gone |
| `put(parent, part) -> V` | — | the parent with the part replaced; must not mutate `parent` |
| `anchor(parent) -> Option<u64>` | `0` when the part exists | where the part sits; a different anchor invalidates the projection |
| `describe(parent)` | — | |
| `overridden_arguments(part)` | empty | argument names the part supersedes, so a projection further down reports them `overridden` |

A step chooses by the parent's `identifier()` and content. Document steps parse text or bytes (JSON
when it starts with `{` or `[`, otherwise YAML) or take the value's JSON form, and write back in the
parent's own representation: a YAML text stays YAML text. Comments and formatting of a parsed
document are not kept; unknown keys and key order are.

## 4. Events and the holder contract

`subscribe` gives a `tokio::sync::watch` receiver of `AccessorEvent`. It is a **wake-up, not a
record**: it keeps only the latest event, so a burst of events coalesces and a slow holder sees the
last one. `Status`, the two progress events, `Log(count)` and `Removed` are relayed from the root;
`Initial`, `Changed(Fingerprint)` and `Invalidated` come from the hub.

| Event | Holder does |
|---|---|
| `Changed` | redraw from `get` |
| `Status(Expired)` | call `get`, which re-evaluates an asset and then produces `Changed` |
| `Log(count)` | read new messages with `log(since)` |
| `Invalidated` | drop the accessor; evaluate its query again for a fresh one |
| `Removed` | the value is gone; `get` fails, a set creates it again where the root allows |
| any, after a possible miss | compare `info().fingerprint` with what was drawn |

`AccessorEvent::kind()` names an event in one lowercase word (`initial`, `status`, `progress`, `log`,
`changed`, `invalidated`, `removed`) and `to_json()` gives `{"event": kind, …}` with `status`,
`primary` / `secondary`, `count` or `etag` (the new token). The WebSocket, Python and JavaScript
events use this form; JavaScript names a status as `LiquersAsset.status` does (`"source"`).

## 5. The first-class accessors

| Constructor | Root | Reads | Writes | Missing value | Relays |
|---|---|---|---|---|---|
| `memory_accessor(initial, address)` | `MemoryRoot` | the variable | replaces it; a counter version | status `None`; `get` fails; a set creates | nothing: `notify` is true, every change is a set |
| `store_accessor(envref, key, view, address)` | `StoreRoot` | the store entry, never evaluating a recipe | `AssetManager::set_binary`: a live asset of the key is replaced and its dependents expire | status `None`; `get` fails; a set creates | nothing: `notify` is false, `refresh` compares the stored version |
| `key_accessor(envref, key, address)` | `KeyRoot` | the keyed asset, evaluated when needed | `AssetManager::set_state`: an `Override` when the key has a recipe, else a `Source` | status `None` when nothing can make the key; a set creates it | status, progress, log, expiry, removal of the live asset |
| `asset_accessor(asset, address)` | `AssetRefRoot` | an asset the holder already has | read-only: no asset-manager write for a non-keyed asset | — | status, progress, log; removal is final |
| `resolve_accessor(envref, query)` | — | evaluates `query`; its value when that is an accessor, otherwise `asset_accessor` of its asset | | | |

The store root has three **views** (`StoreView`, parsed from `binary`, `metadata`, `value`), all
read-write: the stored bytes as a bytes value, the stored metadata record as a metadata value, or the
bytes deserialized with the metadata's type identifier and data format. A new key written through the
value view uses the key's extension for its format; through the binary view it is the bytes value's
own type and default format (`Bytes`, `b`), since opaque bytes cannot be checked against the
extension.

Environment-backed roots hold the environment weakly, so an accessor kept in a cached asset does not
keep the environment alive. An asset-backed root relays the latest message when it is subscribed
again, because the asset may have moved on in between.

**Wrappers** restrict an accessor: `read_only(inner)` refuses every set with `NotSupported` and
reports `write: false`; `validated(inner, check)` runs a `ValueCheck` before every set. A wrapper's
projections are `WrapperProjection`s, which put the part into the wrapper's value and set that
through the wrapper, so the rule holds for every part.

## 6. Projection steps

Core (`liquers-core/src/accessor/steps.rs`):

| Step | Part | Notes |
|---|---|---|
| `FieldStep(name)` | a field of a mapping | anchor: the field exists |
| `ItemStep(index)` | an item of a list | anchor: the index and the list length, so an insertion or a removal invalidates the item projections of that list |
| `RecipeStep(index)` | a recipe of a `recipes.yaml` (a `recipes:` list or a bare list), as `Value::Recipe` | reports the recipe's `arguments` and `links` as overridden |
| `QueryFieldStep` | a recipe's query, as a query value | |
| `PipelineFieldStep` | a recipe's pipeline | |
| `ActionStep(index)` | one action of a query or pipeline, as a one-element pipeline in the parent's realm | element 0 is a query's resource; anchor: element count and command, so an inserted step or a renamed command invalidates it |
| `ArgumentStep(name)` | one argument of the first action, as its JSON value | an argument the action does not give reads as `none`, and setting `none` unsets it; a set goes through `ActionEditor::set`, so a wrong type or an unknown argument is refused and nothing is written |

The query steps edit through `QueryEditor` / `PipelineEditor` and the command registry, so every edit
they make is one the editor would accept.

Records (`liquers-lib/src/records/accessor.rs`, feature `records`): `RowStep(i)` (a row, as an object
of field values), `ColumnStep(name)` (a column, as a list; a list of the wrong length is refused) and
`CellStep(row, column)` (a scalar). A `put` builds a new batch sharing every column except the edited
one, with the schema unchanged. These steps are Rust API only:
`RECORD-ACCESSOR-STEPS-HAVE-NO-COMMANDS`.

## 7. The accessor value and the `acc` commands

An accessor travels through Liquers as a value: `ValueInterface::from_accessor` and
`try_into_accessor` (defaulted to refuse), and in `liquers-lib` the variant
`ExtValue::Accessor { value: Arc<dyn ValueAccessor<Value>> }`, type identifier `Accessor`. Its
`TypeInfo` declares no formats, so it is never stored: persisting it is `NonSerializable`
([`VALUE_TYPE_SYSTEM.md`](VALUE_TYPE_SYSTEM.md)). Sending one to a client is
`ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`.

`register_accessor_commands!(cr)` registers namespace `acc` (`liquers-lib/src/accessor/mod.rs`; the
expansion needs a `CommandEnvironment` type alias in scope):

| Command | State | Gives |
|---|---|---|
| `accessor` | a key (`-R-key/…`) | `key_accessor` of the key |
| `store-<view>` | a key | `store_accessor`, view `binary`, `metadata` or `value` (default) |
| `recipe-<i>`, `query`, `pipeline`, `action-<i>`, `argument-<name>`, `field-<name>`, `item-<i>` | an accessor | its projection through the step of §6 |
| `get` (volatile) | an accessor | its current value |

Each command is cached as an asset, so one query is one shared handle. A projection command sets an
invalidation sink that expires its own asset, so after `Invalidated` the next evaluation of the query
builds a fresh accessor. Example: `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-1/argument-n`
is the argument `n` of the first action after the resource in the first recipe.

## 8. Web API

Opt-in: `Api::Accessors`, mounted at `{prefix}/api/acc` only when the server config's `apis` lists
`accessors` (`Api::defaults()` does not, `Api::all()` does), or by `AccessorApiBuilder::new(base)` in
code (`read_only()` drops `PUT`, `without_websocket()` drops `ws/`). `bind` refuses
`Api::Accessors` with a configuration error when the environment's type registry has no `Accessor`
type. Every route resolves its query with `resolve_accessor`, which evaluates it.

| Route | Answers |
|---|---|
| `GET {base}/q/{*query}` | the value, in `?format=` or its effective data format, with `Content-Type` and `ETag` (the quoted token) |
| `PUT {base}/q/{*query}` | sets the value; body format from `?format=`, else `Content-Type` (`application/json`, `text/plain`, YAML, `application/octet-stream`), else JSON; `?type=` names its type identifier. Answers `{etag}` and the new `ETag` |
| `GET {base}/info/{*query}` | `AccessorInfo` as JSON, with `etag` once the accessor has been read |
| `GET {base}/log/{*query}?since=n` | the log entries from `n` (`400` when `n` is not a number) |
| `GET {base}/describe/{*query}` | `PartDescription` |
| `GET {base}/ws/{*query}` | a WebSocket of events as `to_json()` frames: the current event, then each change; closes with `1000` after `invalidated` and `1001` when the server stops. For a root that cannot notify it refreshes once a second |

`PUT` preconditions: `If-Match: <token>` sets a value whose token matches; `If-Match: *` sets the value
as it is now (`412` when it does not exist); `If-None-Match: *` creates. No precondition is `428`; a
mismatch is `412` with the current `ETag`; a read-only accessor is `405`. Details:
[`WEB_API_SPECIFICATION.md`](WEB_API_SPECIFICATION.md).

## 9. Language wrappers

Both wrap an existing accessor (direction 1 of the `ACCESSOR` feature of
[`LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md)); neither lets the language
define a root or a step (`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`). In both, an
accessor value crosses into the language as a wrapper and back as the **same** Rust `Arc`, and each
wrapper keeps its own token, so its `set` is `set_if` with what that wrapper last read or wrote.

| | Python (`liquers.accessors`, `lq.Accessor`) | JavaScript (`liquers-web`, `Accessor`) |
|---|---|---|
| Make | `env.accessor(query)`, `env.memory_accessor(value=None, address=None)` | `liquers.accessor(query)`, `liquers.memoryAccessor(value?, address?)`, and both on `Environment` |
| Read | `get()`, `snapshot()` → `AccessorSnapshot(value, state, token, root_version)` | `get()`, `snapshot()` → `AccessorSnapshot` (`value()`, `state`, `token`, `rootVersion`) |
| Write | `set(value)`, `set_if(value, token)` → new token | `set(value)`, `setIf(value, token)` → `Promise<string>` |
| Inspect | `info()`, `log(since=0)`, `describe()`, `capabilities`, `address`, `refresh()` | `info()`, `log(since?)`, `describe()`, `capabilities`, `address`, `refresh()` |
| Project | `project("field-title")`: the address with actions appended, evaluated | `project("field-title")` |
| Events | `on(callback)` (a context manager that disposes), run only inside `process_events()` / `wait_event(timeout)` or the `events()` async iterator; a queue of at most 256, oldest dropped | `subscribe(callback)` → `AccessorSubscription.dispose()`; callbacks run from a `spawn_local` task |
| Async | `get_async`, `snapshot_async`, `set_async`, `set_if_async`, `info_async`, `refresh_async`, `project_async`, `wait_event_async` | every method that reaches the accessor returns a `Promise` |
| Conflict | `StatusConflictError` with `token` (the current one; `None` for a conflict from elsewhere) | rejects with `LiquersError`, `errorType === "status_conflict"`, `token` |
| Statuses | `lq.Status` (`"Source"`) | lowercase names (`"source"`, `"none"`) |

A JavaScript handle spawns the accessor's `driver()` when it is created, since wasm has no runtime to
spawn it on construction. Contracts of each integration: [`PYTHON_INTEGRATION.md`](PYTHON_INTEGRATION.md),
[`guides/JAVASCRIPT_GUIDE.md`](../guides/JAVASCRIPT_GUIDE.md).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-16 | Created: roots and projections, the hub, fingerprints and conflicts, events, the first-class accessors, the steps, the `acc` commands, the web routes and the Python and JavaScript wrappers. | `design/settable-value-accessor/` |
