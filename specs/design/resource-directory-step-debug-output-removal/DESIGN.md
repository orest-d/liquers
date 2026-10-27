---
id: RESOURCE-DIRECTORY-STEP-DEBUG-OUTPUT-REMOVAL
kind: design
title: The store-directory step evaluates without writing debug lines to stderr
form: compact
status: complete
gh_pr: [192]
readiness: ready
autofix: eligible
area: [core/plan]
issues: [RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT]
created: 2026-10-26
---
# The store-directory step evaluates without writing debug lines to stderr

> **Complete on 2026-10-27**: implemented by automatic fix in #192.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

Every `-R-sdir/` (`-R-store_directory/`) evaluation writes two unconditional lines to stderr, the
second carrying the whole `Debug` form of the listing. They are leftovers from developing the
instruction, not diagnostics anybody reads. They flood the server log and the agent binary's stderr
for a large folder. Remove them.

### Problem Example

Verified at HEAD (cd6d5da). `liquers-core/src/interpreter.rs` `do_step`, arm
`Step::GetResourceDirectory(key)`:

```rust
eprintln!("Getting resource directory for key {:?}", key);
let store = envref.get_async_store();
let d = store.listdir_asset_info(&key).await?;
eprintln!("Got resource directory: {:?}", d);
```

Query `-R-sdir/data/-/to_text` (`liquers-validate`: `Ok`, encoded `-R-sdir/data/-/to_text`).
Today, besides its value, it writes `Getting resource directory for key …` and
`Got resource directory: [AssetInfo { … }, …]`. Should be: nothing is written. `-R-dir/` (planned
as `GetAssetDirectory`) never printed. The source issue's title and example named it by mistake and
were corrected.

### Scope and Acceptance Criteria

- **AC-1** No debug output from the step
  WHEN `Step::GetResourceDirectory` runs
  THEN `liquers-core/src/interpreter.rs` contains no `eprintln!` in that arm, so nothing is written
  to stderr by it
- **AC-2** The listing is unchanged
  WHEN `-R-sdir/folder` is evaluated against a memory store holding `folder/a.txt` and `folder/b.txt`
  THEN the value is `Value::AssetInfo` with those two entries, as today

Non-goals: the per-step `Applying step i/n` line in `apply_plan_state` and the other library
`eprintln!` sites, which `core-tracing-migration` turns into `tracing` events; adding a trace event
for this step (that design may add one when it lands).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — removal of two debug lines in `liquers-core/src/interpreter.rs`
  plus one unit test. No `pub` item, command, format or dependency changes (rules 1-6 hold).
- **Leading issue:** None
- **Explanation:** the lines only print and have no other effect. Deleting them cannot change a
  value. The only overlap is with `core-tracing-migration`, which would otherwise migrate them.
- **Open questions:** None.

### Design Dependencies

- `core-tracing-migration` — **overlaps** (finished Phase 4, not edited). Its level table lists the
  resource-directory lines as `trace!` sites. If this lands first, that design's implementer finds
  two fewer sites and should add at most a `trace!` without the listing body. If that lands first,
  this design reduces to deleting the two `trace!` events or leaving them, as its implementer
  decides. No order is required. Overlap exclusion E1 keeps this item separate, because it is
  eligible for automatic fixing and that design is not.

### Consolidated Findings

- Only `GetResourceDirectory` prints. `GetAssetDirectory` (`-R-dir/`) does not, so the source issue's
  title and example were wrong and have been corrected in the issue.
- `liquers-core` has no unit test that evaluates `-R-sdir/`; the only end-to-end use is
  `liquers-lib/tests/records_end_to_end.rs` `file_records_lists_a_store_directory_through_a_query`.
  The new core test pins AC-2 so the deletion is not unguarded.

## Phase 2: Architecture

### Solution

Delete the two `eprintln!` calls in `do_step`'s `Step::GetResourceDirectory` arm. The arm keeps
`resolve_key_from_cwd`, `listdir_asset_info` and `ValueInterface::from_asset_info` unchanged.

Rejected: gating the lines behind `cfg!(debug_assertions)` (still unfilterable noise in every test
run); converting them to `tracing` now (that is `core-tracing-migration`'s dependency decision,
which is still open).

### Changes

- `liquers-core/src/interpreter.rs` `do_step`: two lines removed.
- `liquers-core/src/interpreter.rs` `mod tests`: new test (Phase 3).
- No documents describe the output. `specs/issues/RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT.md`
  is closed with a resolution note.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/interpreter.rs` only |
| Existing tests | none read stderr; the `test_dir*` tests use `-R-dir/` and are unaffected |
| Compatibility | none: no value, metadata or format changes |
| Concurrency / performance | slightly faster: no `Debug` formatting of the listing |
| Recovery | revert the commit |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

`-R-sdir/folder` against a memory store with `folder/a.txt`, `folder/b.txt` yields the two
`AssetInfo` entries and writes nothing to stderr.

### Tests

- `liquers-core/src/interpreter.rs` `tests::test_sdir_lists_store_entries` (AC-2): memory store with
  `folder/a.txt` and `folder/b.txt`, `SimpleEnvironment`, `envref.evaluate("-R-sdir/folder")`. The
  value is `Value::AssetInfo` with filenames `{a.txt, b.txt}`. Mirrors `test_dir_no_recipe_yaml`.
- Source check (AC-1): `grep -n 'resource directory' liquers-core/src/interpreter.rs` prints nothing.
- Existing: `liquers-lib/tests/records_end_to_end.rs`
  `file_records_lists_a_store_directory_through_a_query` (AC-2, through a command).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib test_sdir` and
`cargo test -p liquers-lib --test records_end_to_end`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/interpreter.rs` `tests` — add `test_sdir_lists_store_entries`; it passes at HEAD (behaviour pin) — `cargo test -p liquers-core --lib test_sdir`
- [x] 2. `liquers-core/src/interpreter.rs` `do_step`, `Step::GetResourceDirectory` — delete the two `eprintln!` lines — step 1 test still passes; `grep -n 'resource directory' liquers-core/src/interpreter.rs` is empty. Rollback: revert.
- [x] 3. Close `RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT` (`status: closed`, resolution note naming the test) and move this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-lib --test records_end_to_end`. Diff review: only `interpreter.rs` and the
two specs files change.
