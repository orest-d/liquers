---
id: ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE
kind: feature
title: The asset and dependency managers cannot be explicitly synchronized with the store
status: draft
priority: P3
complexity: L
area: [core/assets]
design: asset-manager-store-sync
created: 2026-10-08
github:
---

## Problem

While an asset manager runs, the asset and dependency managers are the source of truth for the
store's content, and nothing is meant to modify the store behind their back. Between runs, loading
re-establishes trust: stored versions are verified on read, and recorded dependency versions are
compared with what the dependency manager knows (more or less deeply, by audit policy). There is
no operation that brings a *running* manager and its store into a state where both are known to
agree. Such an operation would:
- close all writes;
- make versions identical in the store and the dependency manager, as far as that can be
  guaranteed;
- run all audits.

The pieces exist separately: `trigger_dependency_audit_all_registered`, Part G version
verification on read, and `ManifestRecipeProvider::clear_cache`. Nothing composes them, and
nothing reloads the metadata the managers already hold.

## Problem Example

A running server holds `-R/data/report.csv` (registered, `Ready`). An operator restores `data/`
from a backup. The host must know to call `ManifestRecipeProvider::clear_cache`,
`verify_stored_versions`, `trigger_dependency_audit_store` and
`trigger_dependency_audit_all_registered`, in that order. Even then, a registration whose stored
copy the restore deleted keeps being served from memory, because nothing compares the live key map
with the store.

## Impact

- **Who it affects.** A host that knows the store changed outside Liquers (a restore, a manual
  edit, another process) has no single call to make the running system consistent again.
- **Workaround.** Restart the process, which re-verifies on load, or call the individual audit
  and cache-clearing operations by hand.

## Expected behaviour

A `sync` (or `reload`) operation on the asset manager:
1. Waits for, or closes, in-flight writes, locking the manager if that is necessary.
2. Reloads the known metadata from the store.
3. Reconciles versions with the dependency manager and expires what no longer holds.
4. Runs all audits, and drops the recipe-provider caches.

**Open design question:** should `sync` be configurable (levels of checking and guarantees), or
should it always run the most conservative checks? This needs a design (`complexity: L`).

## Discovery

Raised by the maintainer on 2026-10-08 while reviewing `dependency-chain-analysis-cost` Phase 4
(question B1, per-key audit depth). The decision there is that a running manager trusts its own
versions, and that the store is not modified behind its back. Making the store consistent again
after an outside change is this separate operation.

## Update, 2026-10-21 (bulk design triage)

Still reproduces at HEAD (`18732dd`): no `sync` / `reload` on `AssetManager`. The pieces exist
(`AsyncStore::sync`, `verify_stored_versions`, `trigger_dependency_audit_store`,
`trigger_dependency_audit_all_registered_with`, `refresh_command_versions_and_expire`,
`ManifestRecipeProvider::clear_cache`), and nothing compares live registrations with the store.
Design: [`asset-manager-store-sync`](../design/asset-manager-store-sync/).
