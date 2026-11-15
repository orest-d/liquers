# Phase 1: High-Level Design — Settable value accessor

## Purpose

An editor needs a handle to *the place a value lives*, not a copy of the value: a stored file, an
asset, a field of a document, a cell of a table, or one argument of one action of a query. A
**value accessor** is that handle. It can read the value, write it, and report changes made by
others (a new version, expiry, removal). Narrower accessors are made from wider ones by
**projection**. This is what interactive editors need (VISION: desktop application, web UI and
dashboards), above all a query and pipeline editor that shows each step and edits its arguments with
the GUI hints of the command metadata registry, wherever the query comes from.

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
  WHEN a query accessor is made from a UI element's field, from a recipe, or from a link in a
  template
  THEN the same action and argument projections (AC-1 to AC-5) work on each
- **AC-11** Accessors pass through commands
  WHEN a query creates an accessor of `reports/recipes.yaml` and passes it to projection commands
  THEN the result is a live accessor of the projected part. It is never stored, and evaluating the
  query again gives a live accessor, not a cached copy
- **AC-12** An accessor can be named across a boundary
  WHEN an accessor's address (root and projection path) is serialized and resolved in another
  environment that has the same root
  THEN the resolved accessor reads and sets the same part

Non-goals: locking (a later design, which this one must not preclude); the editor widgets and
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
- **UI**: elements bind fields to accessors, and editors are built from projections.
- **Records / Templates**: table projections over record views, and query projections over template
  links.

## Crate Placement

The accessor contract, addresses, versions, change notifications, and the store, asset, document,
query, pipeline and recipe accessors go in `liquers-core` (a new `accessor` module). That way axum,
web and py can resolve addresses without `liquers-lib`. Record-table projections go in
`liquers-records`, and template-link projections in `liquers-template`. Both depend on core only.
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

1. **Value variant.** Should the accessor be in core `Value`, or only in lib `ExtValue`?
   *Recommended:* the trait in core and the variant in `ExtValue`, so core's `Value` stays plain data.
2. **Version of a root.** *Recommended:* the asset's version where there is an asset, otherwise a
   counter held by the root. Every set carries the version it was based on, and a stale one is a
   conflict (AC-5).
3. **Unconditional set.** Should a set without a base version be allowed (last writer wins) for
   simple UI fields? *Recommended:* yes, as an explicit separate call.
4. **Paths across structural edits.** Inserting a step before `head` shifts its index.
   *Recommended:* index paths that the root re-bases for edits it applies itself. After an outside
   rewrite, projections are re-resolved and report `Moved` or `Removed` when the part is gone.
5. **Notification granularity.** *Recommended:* a change carries the path and the operation, so an
   accessor can tell whether it overlaps. An outside change is reported as a change of the whole
   root.
6. **Locking.** *Recommended:* none now. Phase 2 reserves a lease hook, so a lock can be added
   without changing the contract.
7. **The write side of `DataNode`.** This design and `store-handler-field-access` both need it.
   *Recommended:* one contract, owned by this design, with `MetadataRecord` as one implementation.
8. **Template links in v1 (AC-10).** *Recommended:* yes, but only for links whose position the
   template parser reports. If you'd rather keep v1 smaller, they can be deferred.
9. **Command namespace** for the accessor and projection commands. *Recommended:* `acc`.

## Design Dependencies

- **Supersedes:** `value-accessor` (abandoned 2026-10-20). This design keeps its goal, and replaces
  `Arc<Mutex>` mutation of a shared value with sets applied by the root, plus versions and
  notifications.
- **Requires:** none to start. AC-6 and AC-7 use the per-asset notifications that exist today.
- **Overlaps:** `store-handler-field-access` (shared contract, question 7); `command-parameter-editor`
  (its `set_argument_parameter` duplicates `ActionEditor::set`; the form should edit through an
  argument accessor); `text-editor-save` (its save is a store-data accessor set);
  `query-editor-live-validation`; `ui-events` and `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS`
  (a field-change event); `ui-split-elements` (addresses and edits are what crosses the wire,
  AC-12); `asset-event-kinds` and `asset-events-over-websocket` (out-of-process change detection);
  `remote-store-and-asset-manager` (remote roots); `value-conversion-registry` (converting a set
  value to the part's type).

## References

- `specs/design/value-accessor/` — the earlier design.
- `specs/reference/VALUE_TYPE_SYSTEM.md`, `ASSET_SET_OPERATION.md`, `PIPELINES.md`,
  `RECIPES_AND_PLANS.md`, `UI_INTERFACE_FSD.md`.
