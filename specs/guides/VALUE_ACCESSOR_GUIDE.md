---
title: Value Accessor Guide
kind: guide
audience: internal
area: [core/value, core/assets, lib/value, axum, py, web]
reviewed: 2026-11-17
---
# Value Accessor Guide

How to make a value accessor, read and write through it, project it to a part, react to its events,
handle a conflict, and implement a new root or step. The contract is
[`reference/VALUE_ACCESSORS.md`](../reference/VALUE_ACCESSORS.md); read its §1 (root and projection)
first if the words *root*, *step* and *hub* are new.

The examples share one fixture: a store holding `config.yaml` with the text
`title: Dashboard\nrows: 3\n`.

## 1. Making an accessor

**From a query.** The `acc` commands make an accessor from a key and project it. This is the usual
way, because a query is an address every layer understands (Rust, Python, JavaScript, HTTP):

| Query | Accessor of |
|---|---|
| `-R-key/config.yaml/-/ns-acc/accessor` | the keyed asset `config.yaml` (a set is a `Source`, or an `Override` when a recipe makes the key) |
| `-R-key/config.yaml/-/ns-acc/store` | the stored data under the key, deserialized (`store-binary` and `store-metadata` for the other views) |
| `-R-key/config.yaml/-/ns-acc/accessor/field-title` | the `title` field of that document |
| `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-1/argument-n` | argument `n` of the first action after the resource, in the first recipe's query |

Any other query also gives an accessor, read-only, of the query's own asset.

```rust
use liquers_core::accessor::{resolve_accessor, SetOptions, ValueAccessor};
use liquers_core::parse::parse_query;

let title = resolve_accessor(&envref, &parse_query("-R-key/config.yaml/-/ns-acc/accessor/field-title")?).await?;
```

```python
title = env.accessor("-R-key/config.yaml/-/ns-acc/accessor/field-title")
```

```javascript
const title = await liquers.accessor("-R-key/config.yaml/-/ns-acc/accessor/field-title");
```

The environment needs the `acc` commands: `liquers_lib::register_accessor_commands!(cr)?` (with a
`type CommandEnvironment = …;` alias in scope); the Python and JavaScript environments register them
already.

**In Rust, directly.** `key_accessor(envref, key, address)`, `store_accessor(envref, key, view,
address)` and `asset_accessor(asset, address)` build the same roots without a query; `address` is the
query you want `address()` to report, and may be `Query::new()`.

**As a memory variable.** A value with no home elsewhere (an element's own state, a value shared
between two commands) lives in a memory accessor. Every clone of the handle shares the variable.

```rust
use liquers_core::accessor::memory_accessor;
let counter = memory_accessor::<Value>(Some(State::from_value_and_metadata(Value::from(1), Arc::new(Metadata::new()))), Query::new());
let shared = counter.clone();
```

```python
counter = env.memory_accessor(1, address="counter")
```

```javascript
const counter = liquers.memoryAccessor(1, "counter");
```

An empty variable (`memory_accessor(None, …)`, `env.memory_accessor()`) has status `None`: `get`
fails and the first set creates it.

## 2. Getting and setting

`get` reads the part and remembers its token; `set` writes and succeeds only if nobody changed the
part since this handle last read or wrote it; `set_if(value, token)` names the token itself.

```rust
let snapshot = title.get().await?;                       // state, fingerprint, root_version
assert_eq!(snapshot.state.try_into_string()?, "Dashboard");
title.set(Value::from("Sales"), SetOptions::default()).await?;
// config.yaml now reads "title: Sales\nrows: 3\n"
```

```python
snap = title.snapshot()               # snap.value, snap.token
title.set_if("Sales", snap.token)     # returns the new token
title.set("Report")                   # uses the token this wrapper last saw
```

```javascript
const { token } = await title.snapshot();
await title.setIf("Sales", token);
await title.set("Report");
```

To create a value that must not exist yet, pass no token: `set_if(value, None, SetOptions::default())`
in Rust, `set_if(value, None)` in Python, `setIf(value, null)` in JavaScript. A key that does not exist has status `None`
(`info().status`); creating it through `accessor` or `store` writes it.

Some accessors cannot be written: check `capabilities().write` (a query that is not an accessor, an
asset computed by a recipe through `asset_accessor`, anything wrapped by `read_only`). A set of one is
`NotSupported`; over HTTP it is `405`.

## 3. Projecting

A projection is a part of an accessor's value with the same get/set/events. Through a query, append
`acc` steps; in Rust, call `project` with a step; in Python and JavaScript, `project` appends actions
to the address and evaluates it:

```rust
use liquers_core::accessor::steps::FieldStep;
let rows = doc.project(Arc::new(FieldStep::new("rows")), parse_query("-R-key/config.yaml/-/ns-acc/accessor/field-rows")?)?;
```

```python
root = env.accessor("-R-key/config.yaml/-/ns-acc/accessor")
title = root.project("field-title")
```

Projections of one root are always in step, however they overlap: a set through `field-title` is a
`Changed` for the root accessor and for every other projection whose content it changed. Editing two
different parts never conflicts. `describe()` tells an editor what a part is: its type, and for a
query argument the command and argument metadata, and whether a recipe overrides it.

A document part is written back in the document's own representation (YAML text stays YAML text).
Comments and formatting are not kept; keys, their order and unknown keys are.

## 4. Listening to events

Events are wake-ups, not a record: a slow holder sees only the latest one. React to them by reading:

| On | Do |
|---|---|
| `changed` | redraw from `get` |
| `status` `Expired` | call `get`, which re-evaluates and then gives `changed` |
| `log` | read new messages with `log(since)` |
| `invalidated` | drop the accessor; the part moved (a step inserted before an action, a recipe removed). Evaluate the query again for a fresh one |
| `removed` | the value is gone |
| anything, when you may have missed one | compare `info()`'s token with what you drew |

```rust
let mut events = title.subscribe();
tokio::spawn(async move {
    while events.changed().await.is_ok() {
        let event = events.borrow_and_update().clone();
        // AccessorEvent::Changed(fp) => redraw; AccessorEvent::Invalidated => drop; …
    }
});
```

Python delivers events on the caller's thread, never on a runtime thread. A callback runs only while
you call `process_events()` or `wait_event(timeout)`, or iterate `events()`:

```python
with title.on(lambda e: print(e.kind, e.token)):
    title.set("Sales")
    title.wait_event(10)          # runs the callback for each event it takes

async for event in title.events():
    if event.kind == "changed":
        break
```

JavaScript calls back from a task of its own:

```javascript
const sub = title.subscribe(e => { if (e.event === "changed") redraw(e.etag); });
sub.dispose();
```

A store root cannot push changes made behind Liquers' back (`capabilities().notify` is false): call
`refresh()` to check. Writes through Liquers are seen at once either way. The web socket refreshes
such an accessor once a second.

### Binding an editor

A UI editor is bound to where its value lives by an accessor query. Ending the query with an `lui`
editor command makes the element:

```
-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor
```

The runner evaluates the part before `ns-lui`, watches the accessor, and gives the editor a snapshot
on every `changed`; each edit is a `set_if` from the fingerprint the editor last loaded, so two
editors of one recipe follow each other and the second writer of the same version gets a conflict
(§5) and reloads. A set through the accessor expires the recipe's asset, so a view of that recipe's
result recalculates when it next asks for it; nothing is evaluated by the editor itself. An
accessor made in code has no address: the `lui` command adopts it for the session only. The element
side is `reference/UI_INTERFACE_FSD.md` [Editors](../reference/UI_INTERFACE_FSD.md#editors).

## 5. Handling a conflict

A conflict means someone else wrote first. Nothing was written. The error carries the current token,
so a holder can re-read, merge, and retry:

```rust
match title.set_if(Value::from("Mine"), Some(old), SetOptions::default()).await {
    Err(e) if e.error_type == ErrorType::StatusConflict => {
        let current = liquers_core::accessor::conflict_token(&e); // Option<Fingerprint>
        let latest = title.get().await?;                          // re-read, merge, retry
    }
    other => { other?; }
}
```

```python
try:
    title.set_if("Mine", old)
except lq.StatusConflictError as e:
    latest = title.snapshot()      # e.token is the current token
```

```javascript
try { await title.setIf("Mine", old); }
catch (e) { if (e.errorType === "status_conflict") { const latest = await title.snapshot(); } }
```

Over HTTP the same is a `412` with the current `ETag`.

## 6. Over HTTP

Enable the API (`apis: [..., accessors]` in the server config, or `Api::Accessors` with
`LiquersServerBuilder::with_api`), then:

```text
GET  /api/acc/q/-R-key/config.yaml/-/ns-acc/accessor/field-title          → "Dashboard", ETag: "…"
PUT  /api/acc/q/-R-key/config.yaml/-/ns-acc/accessor/field-title          If-Match: "…"   body "Sales"
GET  /api/acc/ws/-R-key/config.yaml/-/ns-acc/accessor/field-title         → {"event":"changed","etag":"…"}
```

A `PUT` needs a precondition: `If-Match: <etag>`, `If-Match: *` (whatever the value is now) or
`If-None-Match: *` (create). The routes are in
[`reference/WEB_API_SPECIFICATION.md`](../reference/WEB_API_SPECIFICATION.md).

## 7. Implementing a new root

Implement `AccessorRoot` for the source and hand it to `AccessorHub::new`; the hub supplies the
fingerprints, conflicts, projections and events. A root does the I/O; keep it to reading and
writing whole values.

```rust
use liquers_core::accessor::{AccessorHub, AccessorInfo, AccessorRoot, Capabilities, ExpiryPolicy, RootEvent};

#[derive(Debug)]
struct SettingsRoot { /* a connection, a file path, … */ }

#[async_trait::async_trait]
impl AccessorRoot<Value> for SettingsRoot {
    async fn read(&self) -> Result<Option<(State<Value>, Version)>, Error> {
        // Ok(None) when the value does not exist; the version must change with every write.
    }
    async fn write(&self, state: State<Value>) -> Result<(State<Value>, Version), Error> {
        // Write, creating when missing; return the state as written (its status yours) and the
        // new version.
    }
    async fn info(&self) -> Result<AccessorInfo, Error> {
        Ok(AccessorInfo::with_status(Status::Source))
    }
    fn capabilities(&self) -> Capabilities { Capabilities { write: true, notify: false } }
}

let settings = AccessorHub::new(Arc::new(SettingsRoot { /* … */ }), Query::new(), ExpiryPolicy::Relay);
```

Rules:

- **Versions.** `read` and `write` return a version that changes whenever the value does; the hub
  compares it to find outside changes on `refresh`.
- **Status.** `write` sets the status it means (`Source`; `Override` for a value that replaces a
  recipe's); the hub resets it to `None` before calling you.
- **Not found.** Return `Ok(None)` from `read` and `Status::None` from `info` for a value that does not
  exist, and override `not_found` for a better error than `not_available`.
- **Events.** A source that can tell when it changes implements `watch()` (a future that owns what it
  waits on and completes when `poll_event` has something) and `poll_event()`, returning
  `RootEvent::Changed(version)`, `Status`, progress, `Log` or `Removed`. Declare `notify: true` then.
  Without them, declare `notify: false`: changes are found on `get` and `refresh`.
- **Environment.** Hold an environment weakly (`Weak<E>`), as the built-in roots do, so a cached
  accessor does not keep it alive.
- **wasm.** Nothing is spawned there: the host spawns `driver()` (liquers-web does when a handle is
  created). Use the `?Send` form of `async_trait` on `wasm32`, as the built-in roots do with
  `cfg_attr`.

To make the root reachable from a query, add a command that returns
`Value::from_accessor(accessor)` (see `liquers-lib/src/accessor/mod.rs` for `accessor` and `store`).
Python and JavaScript cannot implement a root (`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`).

## 8. Implementing a new step

A step is a synchronous lens: `get` the part from the parent value, `put` it back. It never does I/O
and never sees metadata, so choose by the value's `identifier()` and content.

```rust
use liquers_core::accessor::{PartDescription, ProjectionStep};

#[derive(Debug)]
struct UpperStep;   // a part that is the parent's text, upper-cased (a toy)

impl ProjectionStep<Value> for UpperStep {
    fn name(&self) -> Cow<'_, str> { Cow::Borrowed("upper") }
    fn get(&self, parent: &Value) -> Result<Option<Value>, Error> {
        Ok(Some(Value::from(parent.try_into_string()?.to_uppercase())))
    }
    fn put(&self, _parent: &Value, part: Value) -> Result<Value, Error> {
        Ok(Value::from(part.try_into_string()?.to_lowercase()))
    }
    fn describe(&self, _parent: &Value) -> Result<PartDescription, Error> {
        Ok(PartDescription { step: "upper".into(), type_identifier: "Text".into(), ..Default::default() })
    }
}
```

Rules:

- `get` returns `Ok(None)` when the slot is gone, and an error only when the parent cannot be read.
- `put` builds a new parent; never mutate the one you were given (a record step shares every column
  it did not edit).
- `anchor` says *where* the part sits. Override it whenever the part can move: the default (`0` when
  the part exists) means "never moves". Include whatever an insertion before the part would change
  (an item's index and the list length, an action's element count and command). A different anchor
  invalidates the projection, which is how a holder learns its part moved instead of silently editing
  a different one.
- Refuse an edit in `put` before anything is written: the hub writes only after every `put` of the
  chain succeeded.
- A step that replaces arguments of a query further down (a recipe's `arguments`) returns their names
  from `overridden_arguments`, so `describe` marks them.

To make the step reachable from a query, add a projection command whose state is an accessor and
which returns `state_accessor.project(Arc::new(step), address)` with an invalidation sink that expires
its own asset, as the `acc` projection commands do.

## 9. Wrapping an accessor

`read_only(accessor)` and `validated(accessor, check)` wrap any accessor; their projections keep the
rule. A language wrapper (Python, JavaScript, another integration) wraps the trait rather than
reimplementing it: keep one token per wrapper, map `StatusConflict` to the language's conflict error
with `conflict_token`, run callbacks where the language expects them, and let an accessor value cross
back as the same `Arc`. The checklist is the `ACCESSOR` feature of
[`LANGUAGE-INTEGRATION_GUIDE.md`](LANGUAGE-INTEGRATION_GUIDE.md).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-17 | §4: binding an editor (`ns-lui/query_editor` over an accessor query). | `design/query-pipeline-editor/` |
| 2026-11-16 | Created: making, reading and writing, projecting, events, conflicts, HTTP, new roots, steps and wrappers. | `design/settable-value-accessor/` |
