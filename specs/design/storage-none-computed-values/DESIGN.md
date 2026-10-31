---
id: STORAGE-NONE-COMPUTED-VALUES
kind: design
title: An inferred metadata record is not stored, and under a recipe it is not trusted as a source
form: compact
status: complete
area: [core/store, core/assets]
issues: [RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE]
created: 2026-10-20
---
# An inferred metadata record is not stored, and under a recipe it is not trusted as a source

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. **Revision 2, 2026-10-27**:
rewritten after the maintainer's decisions in the project thread "Plan status check" (summary under
*Decisions* below). Approved and implemented 2026-10-27 ("proceed all"), PR #173. The options
compared on the way are in the project file `agent-memory-mvp/recipe-results-metadata-options.md`.

## Decisions (maintainer, 2026-10-26 and 2026-10-27)

1. Revision 1's question 1 (never write a recipe result into a `storage: none` mount, or write it
   and never trust it) is replaced by a mechanism, not answered by a mode.
2. No new mount key (`computed:` / `recipe_results:`) and nothing in `recipes.yaml`: behaviour is
   configured with `store_config` queries.
3. The record carries one **fact**: whether it was **inferred** (synthesized from the data, not read
   from storage). **An inferred record is not stored.** That is a mechanical rule of the pipeline,
   not a comparison of records.
4. The **policy** is which steps a mount's chain has: a `store_config` step may mark a record
   inferred, so it is not stored. Default chains never do, so normal Liquers mounts store every
   record as today. Sparse metadata is a special configuration, made for the agent memory.
5. Per-key behaviour ("this key is kept in git, the rest is not") is a general feature with a
   design of its own (`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`); this design does not
   anticipate it.
6. A database of all records would not contain inferred ones. Accepted: they are still read
   through the store.

## Phase 1: High-Level Design

### Purpose

A mount whose chain synthesizes records cannot tell a recipe's result from a user's file once the
result's record is gone (`storage: none`, or a gitignored record tree after a fresh clone). The
result then reads back as `Source`: it is served stale, and `remove` deletes it as a document. Make
"this record was inferred" a fact the asset manager can see, make the pipeline never store an
inferred record, and let a mount's configuration decide which records it infers. Under a recipe, an
inferred record is treated as a file with no metadata, which the manager already knows how to
handle.

### Problem Example

At HEAD (`cd6d5da`), by code reading. The `memory/liquers-specs.yaml` profile mounts `specs` with
`storage: none` and `on_read: media_type/file_facts/content_hash-on_change/front_matter/…`; suppose
`specs/index.md` has a recipe (the plan of `agent-memory-derived`).

1. The first evaluation computes `index.md`; `AssetRef::save_to_store` writes it through the
   pipeline; `NoStorage::plan` writes no record, so its `Ready` status, dependencies and version
   are lost.
2. A new process reads `-R/specs/index.md`. `NoStorage::get` returns `None`, the `on_default` chain
   synthesizes a record (`defaults` sets `status: Source`), and `content_hash` gives it a version.
   `adopt_stored_status` maps only a `None` status, and `external_change_status` recognises "no
   metadata" only by an unknown version, so the record passes as a real `Source`.
   `try_fast_track` serves the old bytes, even after the recipe's inputs changed.
3. `AssetManager::remove` sees `Source` under a recipe and takes `RemoveAction::Delete`, not
   `DropComputed`; `get_asset_info` never reports `Recipe` again.

The same happens on any mount with records in a gitignored tree after a fresh clone: the result's
data is committed, its record is not.

Should be: in step 2 the record is marked inferred; the manager treats an inferred record under a
recipe as a file with no metadata (status `Ready`, recorded version unknown), and the environment's
`ExternalChangePolicy` decides as it already does for such files: `UserInput` keeps the bytes as an
`Override`, `Corrupted` deletes them so the recipe recomputes. In step 3, `remove` drops the value
back to its recipe.

### Scope and Acceptance Criteria

- **AC-1** Records carry the fact
  WHEN the pipeline returns a record built by `on_default` (no stored record)
  THEN `MetadataRecord::inferred` is `true`; WHEN it returns a record read from its storage model
  THEN it is `false`, whatever the stored bytes say
- **AC-2** The flag never persists
  WHEN any storage model (`native`, `per_entry`, `grouped`, `none`, central) is asked to store a
  record
  THEN the encoded record has no `inferred` field (or `false`), and reading it back yields `false`
- **AC-3** Callers cannot set it
  WHEN `set` or `set_metadata` receives a record with `inferred: true` (a read record sent back)
  THEN the pipeline clears the flag before `on_validate` and `on_write`, and the record is stored
  as on a mount without this design
- **AC-4** An inferred record is not stored, and replaces a stored one
  WHEN a step of the `on_write` chain leaves the record marked inferred
  THEN no record is written, and a record already stored for the key is deleted in the same plan;
  the data object is written as usual
- **AC-5** The `infer` step
  WHEN a mount's `on_write` chain contains `infer-<statuses…>` (for example `infer-Source`)
  THEN a record whose status is listed is marked inferred; an empty list marks every record; the
  record's fields are not otherwise changed; an unknown status name fails at build
- **AC-6** Default behaviour is unchanged
  WHEN a mount has no `infer` step, or no `metadata` section
  THEN every write stores its record exactly as before, and the conformance suites pass unchanged
- **AC-7** An inferred record under a recipe is a file with no metadata
  WHEN the asset manager reads data whose record is inferred, at a key its recipe provider can make
  THEN it treats the stored status as `None` and the recorded version as unknown, so
  `reusable_stored_status` gives `Ready` and the version check applies the environment's
  `ExternalChangePolicy`: `UserInput` → `Override`, served; `Corrupted` → stored copy deleted,
  recipe recomputes. Without a recipe the record stays `Source`.
- **AC-8** Removal drops back to the recipe
  WHEN `remove` is called on a recipe key whose only record is inferred
  THEN the action is `DropComputed` (live asset dropped; the data object deleted; no record
  written), and `get_asset_info` reports `Recipe`
- **AC-9** `persist_defaults` is retired
  WHEN a `metadata` section sets `persist_defaults`
  THEN the build fails with the section's unknown-field error naming `persist_defaults` (no
  replacement is offered: an inferred record is never stored, so storing one on read is gone)
- **AC-10** Documented
  WHEN `STORE_SEMANTICS.md` §14, `STORE_CONFIG_GUIDE.md` §2 and §5, `STORE_CONFIG_FSD.md` (metadata
  section) or `AGENT_MEMORY_SERVICE.md` is read
  THEN they state the flag, the not-stored rule, the `infer` step, AC-7, the edit caveat (below)
  and the retirement of `persist_defaults`

Non-goals: per-key policies (decision 5); a per-mount `ExternalChangePolicy` (the environment's
applies; question 2); changing the `liquers-specs` profile or committing index assets (that is
`agent-memory-derived` Phase 2, which uses this mechanism); `AsyncFileStore` and other stores used
without a `metadata` section, which keep writing their synthesized sidecars; `liquers-web` stores.

**Edit caveat (intended).** On a mount whose chain infers `Source` records, `set_metadata` that
changes a field the data cannot carry (the title of a PDF) succeeds and is visible while the asset
is live, then reverts in the next process. Markdown titles and descriptions survive, because
`front_matter_sync` writes them into the file. Documented, not prevented (question 4).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds a `pub` field to `MetadataRecord`, a `store_config`
  entry, and retires a configuration field (rules 3-4).
- **Leading issue:** None
- **Explanation:** the maintainer settled the mechanism (decisions 1-6). The questions below have
  recommended resolutions that Phases 2-4 assume; none blocks the plan.
- **Open questions:**
  1. **Proposed resolution - field name:** `inferred: bool` on `MetadataRecord`,
     `#[serde(default, skip_serializing_if = "Not::not")]`, so API responses show it and stored
     records never carry it (the pipeline also clears it before planning, AC-2).
  2. **Proposed resolution - which `ExternalChangePolicy`:** the environment's
     (`EnvironmentConfig::external_change`, already configurable). The agent memory sets
     `corrupted`, so a result whose record is missing is recomputed. A per-mount answer
     (`AsyncStore::external_change_policy(key)`, like `version_verification`) waits for a real
     mixed case, and would likely be scoped per key with decision 5.
  3. **Proposed resolution - `persist_defaults`:** retire it (AC-9). It exists to store inferred
     records, which decision 3 forbids; the maintainer stated on 2026-10-17 that no backward
     compatibility is needed yet.
  4. **Proposed resolution - the edit caveat:** document it; no `on_validate` refusal. A mount that
     needs edits of non-Markdown fields to persist should not infer `Source` records.
  5. **Proposed resolution - `infer` changes only the flag:** it does not rebuild the record from
     the data. The record is not stored, so the next read infers it anyway; rebuilding would only
     discard the caller's fields from the live asset sooner.

### Design Dependencies

- `agent-memory-derived` (draft) — **required-by**: its index recipes run in the `liquers-specs`
  mounts, and its Phase 2 chooses their chains (`per_entry-root` with `infer-Source`, or
  `storage: none`) and sets `external_change: corrupted` in the agent's environment.
- `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` — **follow-up**: scoping `infer` (and the
  external-change policy) to keys.
- `dependency-audit-and-expiry-provenance` (complete) — origin of `ExternalChangePolicy`,
  `external_change_status` and the "file with no metadata" rule this design reuses unchanged.
- `store-conditional-write` — **overlaps**, weak: both touch pipeline `set`; no order.
- `agent-memory-mvp` (complete, E4) — origin of the issue; not extended.

## Phase 2: Architecture

### Solution

1. **The fact.** `MetadataRecord::inferred: bool` (question 1), with `Metadata::is_inferred()` and
   `set_inferred()`. `LegacyMetadata` answers `false` and ignores the setter.
2. **Read path** (`store/pipeline.rs`). Where `get` / `get_metadata` / listings build a record with
   the `on_default` chain because `storage.get` returned `None`, set `inferred = true` (after
   `on_read`, so no step can clear it). Where the record came from `storage.get` or `get_folder`,
   set `inferred = false` (AC-1, guards against a record stored by an older build).
3. **Write path** (`set`, `set_metadata`). Clear the incoming flag first (AC-3); run `on_validate`
   and `on_write`; then, if the record is inferred, call `storage.plan(backend, key, None)` (which
   already means "so that it has none", deleting an existing record) instead of
   `plan(.., Some(record))`, and clear the flag on the record handed to `plan` otherwise (AC-2,
   AC-4). The data write and the write strategy are unchanged.
4. **The step** (`store/handlers.rs`, registered in `register_builtin_store_config`). `infer` with
   one `multiple` parameter `statuses` (status names as `Status` serializes them; empty = all).
   Stage: `Write` only; any other stage fails at build, as `require` does outside `Validate`.
5. **The manager** (`assets.rs`). A helper `stored_status_and_version(&Metadata) -> (Status,
   Version)` returns `(None, unknown)` for an inferred record and the record's own values
   otherwise. `adopt_stored_status`, the version check before reuse (`external_change_status` /
   `external_change_action` callers) and `remove`'s `decisive` status use it, so an inferred record
   under a recipe follows the existing "file with no metadata" path (AC-7, AC-8). Under
   `DropComputed` with an inferred record there is no record to rewrite: the data object is
   removed (`store.remove`).
6. **`persist_defaults` retired** (`store_config.rs` `MetadataConfig`, `pipeline.rs`): the field is
   removed; `deny_unknown_fields` refuses it; the error text is extended to say an inferred record
   is never stored (AC-9).

Rejected: a mount key (`computed:` / `recipe_results:`), recipe flags, and policy fields in the
record (`persist`, `trusted`, `generated`) — see *Decisions* and the options file; comparing the
inferred and the written record to decide storage (decision 3); the store deciding trust (it cannot
see recipes).

Known-issue preflight: `STORE-WRITE-HAS-NO-PRECONDITION` (pipeline `set` again; no conflict).
`MetadataRecord` is `deny_unknown_fields`: the new field is defaulted, so records stored before it
still load. `liquers-py` and `liquers-web` see a new optional field in serialized records only.

### Changes

`liquers-core/src/metadata.rs` (field, accessors), `store/pipeline.rs` (read and write paths,
`persist_defaults` removed), `store/handlers.rs` and `store/realm.rs` (`infer`),
`store_config.rs` (`MetadataConfig`) and `store_factory.rs` (`persist_defaults` removed),
`liquers-web/src/typescript.rs` (the `persist_defaults?` declaration removed), `assets.rs` (helper and its three callers),
`specs/command_registry.yaml` (realm `store_config` gains `infer`; regenerated). Documents:
`STORE_SEMANTICS.md` §14, `STORE_CONFIG_GUIDE.md` §2 and §5, `STORE_CONFIG_FSD.md` (metadata
section and the defaults table), `AGENT_MEMORY_SERVICE.md` (a recipe result in a checkout mount),
with History rows and `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A caller relies on `persist_defaults` | Build error names the field and the reason; nothing in-tree sets it except tests | high |
| `Corrupted` deletes a committed result file, then the recompute fails | The recipe's error is reported; `git checkout` restores the file; documented | medium |
| A storage model encodes the flag anyway | AC-2 test over every model; the pipeline clears it before `plan` | high |
| An inferred record of a Ready result reaches a dependent's version check | Its version is the content hash, as for any file with no metadata | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example with the environment's `external_change: corrupted`: the second process logs
"no status was stored for specs/index.md; read as Ready", the version check deletes the stale bytes,
and the recipe recomputes `index.md`. With `per_entry-root` and `on_write: …/infer-Source`, a user
document writes no record, while `index.md` keeps its `Ready` record under
`specs/__metadata__/` (gitignored).

### Tests

- `liquers-core/src/metadata.rs` `mod tests`: `inferred_is_not_serialized_and_defaults_false` —
  AC-2 (encoding), AC-1 (an old record loads as `false`)
- `liquers-core/src/store/pipeline.rs` `mod tests`:
  `default_record_is_inferred_and_stored_record_is_not` — AC-1;
  `every_storage_model_drops_the_flag` (`native`, `per_entry` each layout, `grouped`, `none`,
  `memory_central`) — AC-2; `incoming_inferred_flag_is_cleared` — AC-3;
  `inferred_record_is_not_stored_and_deletes_the_old_one` — AC-4;
  `persist_defaults_is_refused_at_build` — AC-9
- `liquers-core/src/store/handlers.rs` `mod tests`: `infer_marks_listed_statuses`,
  `infer_without_statuses_marks_all`, `infer_refuses_unknown_status_and_other_stages` — AC-5
- `liquers-core/tests/store_conformance_CONF.rs`: the existing suites pass unchanged — AC-6
- `liquers-core/tests/storage_none_computed_values.rs` (new; a router with a `storage: none`
  pipeline mount over `AsyncMemoryStore`, `DefaultRecipeProvider`, a counting command):
  `inferred_result_under_user_input_policy_becomes_override` (counter 1, status `Override`) and
  `inferred_result_under_corrupted_policy_is_recomputed` (counter 2) — AC-7;
  `inferred_record_without_recipe_stays_source` — AC-7;
  `remove_drops_an_inferred_result_back_to_its_recipe` (`get_asset_info` status `Recipe`) — AC-8;
  `sparse_mount_stores_only_computed_records` (`per_entry-root`, `infer-Source`) — AC-4, AC-5
- `liquers-lib/tests/mem_commands.rs`: `recipe_results_drop_back_to_recipe_in_a_storage_none_mount`
  — AC-8, the issue's original scenario
- Documentation (AC-10): the four documents, checked in the final diff review.

Run: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --test mem_commands`;
`cargo test -p liquers-agent`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `metadata.rs`: field and accessors — `cargo test -p liquers-core --lib inferred`. Rollback: revert (defaulted field).
- [x] 2. `pipeline.rs` read and write paths; `persist_defaults` removed from `pipeline.rs`, `store_config.rs`, `store_factory.rs`, `tests/store_adapters.rs` and the `liquers-web` declaration — AC-1 to AC-4, AC-9 unit tests; `cargo test -p liquers-core --test store_conformance_CONF` unchanged. Depends on 1.
- [x] 3. `handlers.rs` / `realm.rs`: `infer`; regenerate `specs/command_registry.yaml` with a CHANGELOG line — AC-5 tests; `cargo test -p liquers-lib --test registry_export`. Depends on 2.
- [x] 4. `assets.rs`: `stored_status_and_version` and its callers — `storage_none_computed_values.rs`. Depends on 2.
- [x] 5. `mem_commands.rs` scenario test — `cargo test -p liquers-lib --test mem_commands`. Depends on 4.
- [x] 6. Documents of Phase 2; close the issue with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-agent` (its profiles use `storage: none`); the store rows of
`scripts/check-build-matrix.sh` one at a time (disk); diff review.
