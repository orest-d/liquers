# Phase 1: High-Level Design — Settable value accessor

## Purpose

Code that changes a value — a command, a binding, a service, an editor — often needs a handle to
*the place a value lives*, not a copy of the value: a stored file, an asset, a field of a document, a cell of a table, or one argument of one action of a query. A
**value accessor** is that handle. It can read the value, write it, and report changes made by
others (a new version, expiry, removal). Narrower accessors are made from wider ones by
**projection**. The contract is not UI-specific. Interactive editors are the motivating consumer
(VISION: desktop application, web UI and dashboards), and the problem example below uses one: a
query and pipeline editor that shows each step and edits its arguments with the GUI hints of the
command metadata registry, wherever the query comes from.

## Problem Example

`reports/recipes.yaml` in the store:

```yaml
recipes:
  - query: "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv"
    title: "Top EU and US rows"
```

A query editor should show three steps, `from_csv`, `is_in` (`column` = `region`, `values` =
`EU`, `US`) and `head` (`n` = `10`, an `IntegerField` from `head`'s `gui_info`), and let the user
change `n` to 20.

**Today.** `QueryEditor` (`liquers-core/src/pipeline/editor.rs`) can change `n` on a `Query` in
memory, and `DataNode` (`liquers-core/src/data_node.rs`) can read the YAML by field. Nothing joins
them to the file, though. The caller must read the bytes, parse the YAML, find the recipe, parse
its query, edit it, re-encode it, rewrite the YAML and write it back. Nothing tells the caller when
another editor of the same recipe, a YAML text editor on the same file, or another writer changes
the file, or when its asset expires or is removed. `QueryConsoleElement` holds its query as a plain
`String`, and the egui `text_editor` never writes its text back (`TEXT-EDITOR-EDITS-ARE-NEVER-SAVED`).

**Wanted.** An accessor of `reports/recipes.yaml` is projected to recipe 0, then to its query, then
to its `head` step, then to argument `n`. Setting `n` to 20 rewrites the file with
`…/head-20/top.csv` and leaves the rest of the file as it was. Every other accessor that overlaps
that part, such as an editor of the whole query or of the YAML text, is told about the change and
reads the new value.

## Scope and Acceptance Criteria

- **AC-1** Projection reads a part, with its editing hints
  WHEN an accessor of `reports/recipes.yaml` is projected to recipe 0, its query, the `head` step, argument `n`
  THEN it reads `10`, and it describes the part as an `int` argument of `pl/head` with gui hint
  `IntegerField`
- **AC-2** Setting a part writes through to its root
  WHEN that accessor sets `n` to `20`
  THEN `reports/recipes.yaml` holds the query `-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-20/top.csv`,
  and its other content (the title, other recipes) is unchanged
- **AC-3** A refused set changes nothing
  WHEN a set is invalid for the part (`n` = `abc`, an argument the command does not have, an edit
  `QueryEditor` refuses)
  THEN the set returns an error and the root is not written
- **AC-4** Overlapping accessors stay in step
  WHEN two accessors overlap the same part (the whole query, and its `head` step) and one of them sets it
  THEN the other receives a change notification and reads the new value
- **AC-5** The first writer wins
  WHEN two accessors set the same root, each based on the same version
  THEN the first set applies, the second fails with a conflict that names the current version, and
  both accessors then read the first writer's value
- **AC-6** Changes from other writers are detected
  WHEN the root changes through the environment's store or asset manager without going through an
  accessor
  THEN every accessor on it receives a change notification, and a read returns the new content
- **AC-7** Expiry and removal are reported
  WHEN the asset behind an accessor expires, or the key is removed
  THEN the accessor reports `Expired` or `Removed`, and a read after removal is a not-found error
- **AC-8** Writing an asset accessor overrides the asset
  WHEN an accessor of a key that has a recipe is set
  THEN the asset's status becomes `Override` (`specs/reference/ASSET_SET_OPERATION.md`), and its
  dependents are invalidated as for any set
- **AC-9** Parts of documents and tables
  WHEN an accessor of a JSON or YAML document is projected to a field path, or an accessor of a
  record table is projected to a row, a column or a cell
  THEN reads and sets apply to that part only
- **AC-10** One query editor for every query source
  WHEN a query accessor is made from a UI element's field or from a recipe
  THEN the same action and argument projections (AC-1 to AC-5) work on each. A further source, such
  as a link in a template, needs only its own root-to-query projection
- **AC-11** Accessors pass through commands
  WHEN a query creates an accessor of `reports/recipes.yaml` and passes it to projection commands
  THEN the result is a live accessor of the projected part. It is never stored, and evaluating the
  query again gives a live accessor, not a cached copy
- **AC-12** An accessor can be named across a boundary
  WHEN an accessor's address (root and projection path) is serialized and resolved in another
  environment that has the same root
  THEN the resolved accessor reads and sets the same part
- **AC-13** Accessors over the web API
  WHEN a client reads an accessor defined by a query (for example the `head` argument of AC-1)
  through the web API, then writes it back with the version token it read
  THEN the read returns the part and its token, the write applies it, and a write with a stale token
  for a part that has changed since is refused as a conflict
- **AC-14** Disjoint edits do not conflict
  WHEN two accessors of different recipes in one `recipes.yaml` each set their part, both based on
  the same root version
  THEN both sets apply. A set conflicts only when the part it was based on has changed since
- **AC-15** A content change keeps a projection, a structural change invalidates it
  WHEN a specialised editor holds `-R-key/some/recipes.yaml/-/recipe_query_accessor-1/action_accessor-2`
  and a query editor holds `-R-key/some/recipes.yaml/-/recipe_query_accessor-1`, and one of them
  changes an argument of action 2
  THEN the other receives `Changed` and both accessors stay valid. When a step is instead inserted
  before action 2, the action accessor is invalidated (its asset expires), and evaluating its query
  again gives an accessor of whatever is step 2 now. The root accessor expires only when its key is
  removed

- **AC-16** An accessor relays its source's lifecycle
  WHEN the asset behind an accessor of an asset-manager value expires, and the holder reacts by
  calling `get`
  THEN the holder first receives the `Expired` status, the `get` re-evaluates the asset, and the new
  value arrives as `Changed`. With the reload policy, the accessor makes that `get` itself
- **AC-17** Notifications over the web API
  WHEN a client subscribes to an accessor defined by a query through the web API, and the part
  changes or expires
  THEN the client receives the same events (status, changed, invalidated, removed) that an
  in-process holder receives
- **AC-18** Accessors can restrict and check what they accept
  WHEN a value is set through a read-only accessor, or through a validating accessor whose check
  refuses it
  THEN the set fails with an error, and nothing is written
- **AC-19** A custom source gets the shared machinery
  WHEN an application implements only a root for its own source and wraps it in the hub
  THEN projections, conflicts and notifications work on it as they do for an asset root

Non-goals: locking (a later design, which this one must not preclude: `VALUE-ACCESSOR-CANNOT-LOCK`);
query accessors over template links (they must remain implementable as a projection); the editor widgets and
composite editors themselves (separate UI designs that use accessors); detecting a change made
outside the environment, for example a file edited on disk (`ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS`);
merging concurrent edits (a conflicting set fails); the transport of addresses and edits between
client and server (`ui-split-elements`).

## Core Interactions

- **Value types**: an accessor is a handle value, like `ExtValue::Widget`. That is the justified
  exception that `VALUE_TYPE_SYSTEM.md` §"What a value variant may hold" requires. It is never
  serialized as data, only by address.
- **Store / Assets**: store-data and asset roots read and write through the asset manager
  (`set_binary` / `set_state`), and watch its per-asset notifications (`AssetNotificationMessage`:
  `ValueProduced`, `Expired`, `Removed`).
- **Query / Pipelines**: query, pipeline, action and argument projections apply edits through
  `QueryEditor` / `PipelineEditor`, and describe arguments from the `CommandMetadataRegistry`.
- **Data nodes**: document projections read through `DataNode`. This design needs a write side,
  which `store-handler-field-access` also proposes (`DataNodeMut`).
- **Commands**: an accessor-creating command and projection commands.
- **Web/API**: `liquers-axum` routes that read and write an accessor defined by a query, with the
  version token as an entity tag.
- **UI**: elements bind fields to accessors, and editors are built from projections.
- **Records**: table projections over record views.

## Crate Placement

The accessor contract, addresses, versions, change notifications, and the store, asset, document,
query, pipeline and recipe accessors go in `liquers-core` (a new `accessor` module). That way axum,
web and py can resolve addresses without `liquers-lib`. Record-table projections go in
`liquers-records`, which depends on core only. A later template-link projection would go in
`liquers-template` the same way.
The value variant and the commands go in `liquers-lib`, along with polars projections if they are
in scope.

## Documentation Intent

- Reference: new `specs/reference/VALUE_ACCESSORS.md` (the contract: get, set, projection,
  versions, notifications, conflicts); extend `VALUE_TYPE_SYSTEM.md` (the handle exception) and
  `UI_INTERFACE_FSD.md` (binding a field to an accessor).
- Guide: none in this design; a UI guide section comes with the editor designs.
- Other documents: none.
- Documents to update: `VALUE_TYPE_SYSTEM.md`, `UI_INTERFACE_FSD.md`, `RECIPES_AND_PLANS.md`
  (recipes edited through accessors), `specs/command_registry.yaml` (new commands).
- API docs: the new `liquers-core/src/accessor/` modules; `liquers-core/src/pipeline/editor.rs`
  and `liquers-core/src/data_node.rs` (their role under accessors); `liquers-lib/src/value/mod.rs`
  (the new variant).

## Open Questions

1. **Value variant.** *Decided (Orest, 2026-11-15):* the accessor belongs in `liquers-core`. The
   trait and the core accessors live there. *Decided (2026-11-16):* the variant is in `ExtValue`
   (a `dyn` trait object), not in core `Value`.
2. **Version of a root.** *Decided (Orest, 2026-11-15):* the asset's version where there is an
   asset, otherwise a counter held by the root. Every set carries the version it was based on, and
   a stale one is a conflict (AC-5).
3. **Conflict rule.** *Chosen with Orest, 2026-11-15, subject to Phase 2's cost check:* option C.
   A set conflicts only when the part it was based on has changed (AC-14). There is no forced set in
   v1. Projections are arbitrary commands, so there is no general path. The proposed mechanism is a
   fingerprint of the part's content that each accessor remembers in place of a copy. A pointer-equality
   check comes first (an unchanged part keeps its `Arc`), and a content hash is the fallback.
   Structural steps (field, index, action argument) may also declare a path, so a whole subtree can
   be skipped without deriving it.
4. **Structural edits.** *Decided (Orest, 2026-11-15):* an accessor is an asset named by its
   query, and a projection is a longer query. After the root changes, each projection is
   **unchanged** (same fingerprint, nothing happens; this includes the editor that made the edit),
   **changed** (same slot, new content: `Changed`, the accessor stays valid) or **invalidated** (its
   slot is gone or its anchor changed: the asset expires and its holders re-evaluate the query,
   which resolves by position). There is no re-basing in v1 (AC-15). Each step has an **anchor**, a
   cheap structural fingerprint of where it sits in its parent: an index step's anchor is the list's
   length plus the identity of the element (for an action, its command), and a field step's anchor
   is that the field exists. A changed anchor means invalidated, and a changed content fingerprint
   with the same anchor means changed. The root tracks its live projections (by weak reference) as a
   tree, the way a signal-reactivity system does: a change marks the tree dirty top down, subscribed
   nodes are recomputed lazily, and a child of an invalidated node is invalidated without being
   derived.
5. **Notification granularity.** *Decided (Orest, 2026-11-15)* with question 4: the root
   re-derives only subscribed projections, walking them as a tree so a shared prefix is derived
   once. Projections without subscribers check when they are next read.
6. **Locking.** *Decided (Orest, 2026-11-15):* out of scope. Phase 2 reserves a hook so a lock can
   be added without changing the contract. What is known (asset-level locks held by a user or
   session, which require `CORE-SESSION-AND-KEY-ACL`; simpler accessor-level locks, where only one
   projection accessor may write) is recorded in `VALUE-ACCESSOR-CANNOT-LOCK`.
7. **One write contract.** *Decided (Orest, 2026-11-15):* there is one write contract, the
   accessor's set. This design is about whole value accessors and does not design a writable
   `DataNode`. A document projection may implement its set on top of whatever data-node write
   `store-handler-field-access` ends up with. That design should adopt the accessor's contract
   rather than define a second one.
8. **Template links.** *Decided (Orest, 2026-11-15):* not in this design. Phase 2 checks that a
   template-link query projection can be added later as one more root-to-query projection.
9. **Command namespace.** *Decided (Orest, 2026-11-15):* `acc`, for example
   `-R-key/some/recipes.yaml/-/ns-acc/recipe_query-1/action-2`.

## Scope Changes

- **2026-11-16, at the Phase 2 gate (Orest).** Added:
  - Accessors are source-agnostic, with first-class roots for assets (managed and unmanaged) and
    for resources, aligned with the asset lifecycle.
  - Accessors relay their source's notifications, and the holder's reactions are defined
    (AC-16).
  - The web API gains notifications (AC-17).
  - Accessors may validate, convert or be read-only (AC-18).
  - The hub is a reusable building block for custom accessors (AC-19).

  It stays one design. Phases 1 and 2 are updated, and approval is pending at the Phase 2 gate.

## Design Dependencies

- **Supersedes:** `value-accessor` (abandoned 2026-10-20). This design keeps its goal, and replaces
  `Arc<Mutex>` mutation of a shared value with sets applied by the root, plus versions and
  notifications.
- **Requires:** none to start. AC-6 and AC-7 use the per-asset notifications that exist today.
- **Overlaps:** `store-handler-field-access` (shared contract, question 7); `command-parameter-editor`
  (its `set_argument_parameter` duplicates `ActionEditor::set`; the form should edit through an
  argument accessor); `text-editor-save` (its save is a store-data accessor set); `VALUE-ACCESSOR-CANNOT-LOCK` (the later
locking design);
  `query-editor-live-validation`; `ui-events` and `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`
  (a field-change event); `ui-split-elements` (addresses and edits are what crosses the wire,
  AC-12); `asset-event-kinds` and `asset-events-over-websocket` (out-of-process change detection);
  `remote-store-and-asset-manager` (remote roots); `value-conversion-registry` (converting a set
  value to the part's type).

## References

- `specs/design/value-accessor/` — the earlier design.
- `specs/reference/VALUE_TYPE_SYSTEM.md`, `ASSET_SET_OPERATION.md`, `PIPELINES.md`,
  `RECIPES_AND_PLANS.md`, `UI_INTERFACE_FSD.md`.
