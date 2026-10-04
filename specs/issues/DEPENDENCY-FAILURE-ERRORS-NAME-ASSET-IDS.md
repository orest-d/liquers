---
id: DEPENDENCY-FAILURE-ERRORS-NAME-ASSET-IDS
kind: issue
title: Dependency failure errors name the dependency by runtime asset id
status: draft
priority: P3
complexity: S
area: [core/assets]
design: 
created: 2026-10-02
github:
---
## Problem

`AssetManager::wait_for_dependency` (`liquers-core/src/assets.rs`) builds two errors that name the
dependency by its runtime asset id: "Dependency asset {id} did not produce a value (status …)" and
"Dependency asset {id} expired and was evicted before its value could be used". Both reach the
dependent's metadata through `fail_due_to_dependency` → `with_error`, so they are persisted in its
log, where a process-local id means nothing.

Step 4 of `dependency-audit-and-expiry-provenance` moved the two *log* lines of the same path
(`note_expired_dependency`, `enter_dependencies`) to keys/queries; these two error messages were
outside its scope.

## Impact

Diagnostics only: a stored error says "asset 1001" instead of `-R/data/b.txt`, so a reader of a
persisted record cannot tell which dependency failed.

## Expected behaviour

Name the dependency by key, or by query when it has none (the `AssetData::provenance_key` /
`expiry_subject` helpers added in Step 4 give exactly that).

## Discovery

Found 2026-10-02 while writing `log_line_names_keys_not_asset_ids` for Step 4 of
`dependency-audit-and-expiry-provenance`.
