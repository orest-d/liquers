---
id: CENTRAL-STORAGE-MODEL
kind: design
title: The central(mount) storage model keeps a router's records in one of its mounts
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/config]
issues: [STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED]
created: 2026-10-20
---
# The central(mount) storage model keeps a router's records in one of its mounts

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Let a router-level `metadata` section keep every member's records in one writable mount of the
same router (`storage: central-<mount>`), so read-only checkouts, HTTP mounts and object stores can
have edited metadata without a sidecar beside their data. Today only the test-only
`memory_central` can serve central storage, and it loses everything on restart.

### Problem Example

```yaml
metadata: { storage: central-meta, on_read: media_type-fill/file_facts }
stores:
  - type: filesystem
    prefix: docs
    config: { path: ./docs }
    read_only: true
  - type: filesystem
    prefix: meta
    config: { path: ./meta }
```

Today: `StoreRouterBuilder::build_store` resolves the mount `meta` through `ComponentContext`, then
`build_central` (`liquers-core/src/store/storage.rs`) returns `not_supported`: "central storage in
mount 'meta' is declared but not implemented yet". Should be: the store builds; `set_metadata` of
`docs/a.md` with a title writes nothing under `./docs`, writes the record to
`./meta/__metadata__/docs/a.md`, and a rebuilt store reads the title back.

### Scope and Acceptance Criteria

- **AC-1** Records go to the mount
  WHEN a store built with `storage: central-meta` writes the record of `docs/a.md`
  THEN the member `meta` holds it at `meta/__metadata__/docs/a.md`, the member `docs` gains no
  object, and `get_metadata` returns it
- **AC-2** Records survive a rebuild
  WHEN the same configuration is built again over the same `meta` backend
  THEN `get_metadata("docs/a.md")` returns the record written before
- **AC-3** The record space is hidden
  WHEN a client lists `meta`, or reads or writes any key with a `__metadata__` segment
  THEN the listing omits `__metadata__` and the access is refused with `KeyNotSupported`
- **AC-4** Record-only keys are enumerable
  WHEN `set_metadata` records a key that has no data
  THEN `contains` is true for it and its parent's `listdir` names it (`STORE_SEMANTICS.md` §8)
- **AC-5** Conformance
  WHEN conformance suite `A5c` runs `central-meta` over memory members, with a member keeping a
  `per_entry` backup
  THEN it passes with no allowed failure, as `A5` does for `memory_central`
- **AC-6** Unusable mounts are refused at build
  WHEN the named mount is missing, offers no data-only access, or is `read_only` while the router
  is not
  THEN the build fails with a `configuration_error` naming the mount
- **AC-7** Member sections still refuse it
  WHEN a member's own section names `central-meta`
  THEN the build fails as it does today ("belongs in the router-level metadata section")

Non-goals: central storage named from a member's own section; keeping records in a mount of a
*different* router; a record layout other than the `__metadata__` subtree; migrating records from
sidecars into a central mount.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` struct `CentralStorage` and a `pub` method on
  `ComponentContext` (rules 3 and 4).
- **Leading issue:** **Open design - record layout in the mount:** where in the mount records live.
- **Explanation:** every part the model needs exists: `ComponentContext` carries the mounts,
  `MetadataStorage::object_store` routes writes there, and the strategies already stage and journal
  across stores (`JournalWrite::commit`, `writes.rs`). One layout choice remains; Phases 3-4 assume
  the recommendation.
- **Open questions:**
  1. **Open design - record layout:** records at `<mount>/__metadata__/<key>` (recommended: reuses
     the reserved exact name `__metadata__` that the `root` layout and the file stores already
     reserve, so no new reservation mechanism; the mount stays usable for ordinary data), or at
     `<mount>/<key>` with the whole mount reserved (needs a key-prefix reservation that
     `ReservedNameSet` cannot express, and wastes the mount).
  2. **Proposed resolution - record encoding:** one JSON object per key, `encode_metadata` /
     `decode_metadata` as every model does, no `.json` suffix (recommended: the key is the name; the
     media type is not read from these objects).
  3. **Proposed resolution - read-only mount:** refused at build unless the router-level section is
     itself read-only (recommended: a write would otherwise land the data and then fail the record,
     leaving them inconsistent under `direct`). Needs `ComponentContext::read_only()`.

### Design Dependencies

- `store-database-records` — **required-by**: records in a database are this model over a
  database-backed mount; that design reuses `CentralStorage` unchanged.
- `store-adapters` (complete, E4) — origin: declared the entry and `ComponentContext`. Not
  extended.
- `store-conditional-write` — **overlaps**, weak: a pipeline `set_if` serializes per group lock,
  which central storage keeps unchanged. No order.
- `store-config-key-scoped-handlers` — **overlaps**, weak: both are `store_config` features of a
  router-level section; independent change sites.

### Consolidated Findings

- No new reservation machinery: `__metadata__` is already an exact reserved name of
  `PerEntryStorage` `root` and of the file stores; the pipeline refuses it router-wide.
- `object_store` returns the member's `Arc` from `ComponentContext`, not the router, so record
  writes bypass the router's routing and the pipeline's own reserved-name refusal by construction.
- Cross-store staging and journalling already work (`memory_central` + `transactional` is covered
  by `A5`/`A6`); the new suite proves it for a real mount.
- The only interface addition beyond the struct is `ComponentContext::read_only()` (Q3).
- `STORE_CONFIG_GUIDE.md` §5 and §6, and `STORE_SEMANTICS.md` §13 describe `central` as declared
  only; both change.

## Phase 2: Architecture

### Solution

`CentralStorage` is a `MetadataStorage` holding the mount prefix and the member store taken from
`ComponentContext::mount`. A record of key `k` lives in the member at
`record_key(k) = <mount>/__metadata__/<k>` (each segment of `k` appended to
`<mount>.join("__metadata__")`):

| Method | Behaviour |
|---|---|
| `name` | `central-<mount>` |
| `reserved_names` | exact `__metadata__` |
| `get` | `get_data(record_key(k))` on the member; `KeyNotFound` → `None`; `decode_metadata` |
| `plan` | `ObjectWrite::put` / `delete` of `WriteTarget::Metadata` at `record_key(k)` |
| `record_names(folder)` | `listdir_data(<mount>/__metadata__/<folder>)` on the member, minus the strategy's staged and journal names (as `list_objects` does) |
| `record_folder(folder)` | `Some(<mount>/__metadata__/<folder>)` |
| `object_keys(k)` | `[record_key(k)]` |
| `object_store` | `Some(member)` |
| `group_of` | default `None` (one object per key) |

`build_central` keeps its two checks, then also refuses a member whose
`data_capabilities().data_access` is false, and one whose `is_read_only()` is true while
`components.read_only()` is false (Q3); it returns `StoreComponent::Storage(Arc::new(CentralStorage::new(prefix, member.clone())))`.

`StoreRouterBuilder::build_store_reporting_without_env_expansion` creates the context with
`ComponentContext::with_mounts(&mounts).with_read_only(self.config.read_only)`.

Rejected: a whole reserved mount (Q1); a new `ReservedNameSet` key-prefix form (new structure for
no gain); writing records through the router (the pipeline would refuse its own `__metadata__`
keys); `PreparedWrite` (`store-database-records` shows the cross-store strategies suffice).

Known-issue preflight: `STORE-WRITE-HAS-NO-PRECONDITION` (P2, design `store-conditional-write`)
holds as for every pipeline: one writer process per store. Not blocking. No command namespace.

### Changes

`liquers-core/src/store/storage.rs`:

```rust
/// `central-<mount>`: every record in one member of the router, under `<mount>/__metadata__/`.
pub struct CentralStorage { mount: Key, store: Arc<dyn AsyncStore> }
impl CentralStorage {
    pub fn new(mount: Key, store: Arc<dyn AsyncStore>) -> Self;
    fn record_key(&self, key: &Key) -> Key;          // private
}
impl MetadataStorage for CentralStorage { /* table above */ }
```

`build_central`: the refusals and the construction above; doc comment no longer says "not
implemented". `register_builtin_storage`: the `central` doc text.

`liquers-core/src/store/realm.rs` `ComponentContext`: a `read_only: bool` field, `with_read_only(self,
bool) -> Self`, `read_only(&self) -> bool` (all `pub`; `none()` and `with_mounts` set `false`).
Errors: `Error::configuration_error` for the build refusals; run time uses the existing store
errors. Async as `MetadataStorage`; no blocking I/O. `specs/command_registry.yaml`: the `central`
doc string changes, so regenerate.

Documents: `STORE_SEMANTICS.md` §13 (models table row, central storage paragraph),
`STORE_CONFIG_GUIDE.md` §5 storage table row and §6 ("only `memory_central`…" sentence),
`STORE_CONFIG_FSD.md` §Metadata section example; History rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Record keys of very deep keys exceed a backend's path limit | same depth as the `root` layout, already shipped | high |
| A file-store member refuses `__metadata__` in data-only mode | its data-only path builders reserve `.__lock__` only (§11) — `A5c` over a temp file store proves it | high |
| `listdir` of the mount shows staged record objects | the pipeline already drops strategy-reserved names | high |
| Existing `central_storage_resolves_its_mount_through_the_router` expects the refusal | rewritten to expect a built store (step 3) | high |

Existing tests: that one test changes; others unchanged. Recovery: revert; `memory_central` is
untouched.

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-1/AC-2. A writable variant keeps records in a database-backed mount
(`store-database-records`):

```yaml
metadata: { storage: central-records, writes: transactional }
stores:
  - { type: filesystem, prefix: docs, config: { path: ./docs } }
  - { type: memory, prefix: records }
```

`central-meta` is a component spec, checked at build; `liquers-validate` accepts the filled form
`-store_config/central-meta` (validated).

### Tests

`liquers-core/src/store/storage.rs` `mod tests`:

- `central_storage_maps_keys_under_the_mount` — `record_key`, `record_folder`, `object_keys`,
  `name`: AC-1
- `central_storage_reads_what_it_planned` — `plan` then a `direct` commit into a memory member,
  `get` returns it; `record_names` lists it: AC-1, AC-4

`liquers-core/tests/store_adapters.rs`:

- `central_mount_keeps_records_out_of_the_members` — Problem Example over memory members: AC-1
- `central_mount_records_survive_a_rebuild` — two builds over shared members (`type: shared`, as
  `CENTRAL` uses): AC-2
- `central_mount_hides_its_record_space` — listing of `meta` and `meta/__metadata__/x` refused: AC-3
- `central_mount_lists_a_record_only_key` — AC-4
- `central_storage_resolves_its_mount_through_the_router` — rewritten: missing mount and member
  section still refused, present mount builds: AC-6, AC-7
- `central_mount_refuses_a_read_only_or_dataless_mount` — `read_only: true` member (refused),
  same with router `read_only` (built), a member without data access: AC-6

`liquers-core/tests/store_conformance_CONF.rs`: `a5c_central_mount` — `metadata: { storage:
central-meta }` over memory members `mem` (with `per_entry` backup) and `meta`, plus the same over a
temp file-store `meta`, no allowed failures: AC-5.

Run: `cargo test -p liquers-core --features store-conformance --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/store/realm.rs` `ComponentContext` — `read_only` field, `with_read_only`, `read_only()` — `cargo test -p liquers-core --lib store::realm`. Rollback: revert (additive).
- [ ] 2. `store/storage.rs` — `CentralStorage` and its unit tests — `cargo test -p liquers-core --lib central_storage`. Depends on 1. Rollback: revert.
- [ ] 3. `store/storage.rs` `build_central` and the `central` doc; `store_factory.rs` passes `with_read_only`; `tests/store_adapters.rs` tests above — `cargo test -p liquers-core --test store_adapters central`. Depends on 2. Rollback: restore the refusal.
- [ ] 4. `tests/store_conformance_CONF.rs` `a5c_central_mount` (memory and file-store `meta`) and its row in the report table — `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF a5c`. Depends on 3.
- [ ] 5. Regenerate `specs/command_registry.yaml` (the `central` doc) with a CHANGELOG line — `cargo test -p liquers-lib --test registry_export`.
- [ ] 6. Documents: `STORE_SEMANTICS.md` §13, `STORE_CONFIG_GUIDE.md` §5-§6, `STORE_CONFIG_FSD.md`; History rows, `reviewed:`; close `STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --features store-conformance --lib --tests`;
`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (wasm32 rows:
`CentralStorage` is not gated); diff review for edits outside `liquers-core/src/store*` and the
three documents.
