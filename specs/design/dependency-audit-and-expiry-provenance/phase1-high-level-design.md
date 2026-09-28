# Phase 1: High-Level Design - Dependency Audit and Expiry Provenance

## Feature Name

Dependency audit and expiry provenance

## Purpose

Makes the dependency-verification part of the expiry system correct, configurable and explainable.
An audit that finds a dependency has moved actually expires its dependents. The environment
configuration says when audits run. Every transition into `Expired` records why it happened. A
dependency on a directory listing actually triggers invalidation. Closes
`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`, `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`,
`EXPIRY-RECORDS-NO-REASON`, `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` and
`STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`. `stale-dependency-status-finalization` left the
last three explicitly open for this follow-up.

## Core Interactions

### Query System
No syntax change. `-R-dir/<key>` dependency keys (already produced by `Step::GetAssetDirectory`)
become live dependencies instead of being dropped.

### Store System
No trait change. The audit and the directory-listing version read `get_metadata` / `listdir` only,
never values, which keeps "delete intermediates, keep results" working.

### Command System
No new commands. Commands see no API change. Reaching the stale-dependency path from a recipe may
need a test-only seam near `Context::wait_for_dependency`.

### Asset System (the core of the work)
- **Audit correctness:** a new audit entry point in `DependencyManager` compares the dependency's
  durable version against each dependent's recorded expectation, even for a first observation.
  `register_version` stays unchanged on the evaluation path.
- **Audit policy:** a setting in `AssetManagerOptions` (`EnvironmentConfig.assets`) chooses when
  audits run: never (the default, which is today's behaviour), at startup or on keyed load. A
  report-only mode returns an `AuditReport` without expiring anything.
- **Provenance:** a typed optional expiry reason (deadline, cascade from a named dependency,
  explicit request, stale dependency) goes into `MetadataRecord`. `AssetInfo` exposes it, and a
  log entry names both participants by key or query rather than by runtime id. It is written under
  the same lock as the status, before the status is persisted. `Status` gets no new variant (the
  analysis in `EXPIRY-RECORDS-NO-REASON` settles this).
- **Directory dependencies:** a listing gets a version computed from its entries. That version is
  registered when the listing is produced and resolved by audit and fast-track. A plan dependency
  that resolves to no version stops being skipped silently.
- **End-to-end stale-dependency test:** one test reaches the expired arm of
  `wait_for_dependency` through an ordinary recipe evaluation.

### Value Types
None. The expiry reason is metadata, not a value.

### Web/API / UI
Not affected directly. The new optional field in `AssetInfo` reaches existing consumers without
any code change (the field is additive to serde). `liquers-axum` is out of scope.

## Crate Placement

`liquers-core` only: `dependencies.rs`, `assets.rs`, `metadata.rs`, `environment_config.rs`, `plan.rs`.
`liquers-py` and `liquers-web` need only to pass the new `AssetInfo` field through, if they do so
explicitly.

## Open Questions

1. What is a directory listing's version computed from: entry names only, or names plus each entry's
   version? Is it recursive? Is it refreshed only when listed, or also on a store write under that
   key?
2. Audit policy scope: per environment only, or also per key or per recipe? How deep is a
   transitive audit by default?
3. Stale-dependency test: add a `#[cfg(test)]`/feature-gated seam between scheduling a dependency
   and waiting for it (option 1), or document that the path is reachable only by a race (option 2)?
4. Log level for each expiry route: `info` for a deadline, `warning` for a stale dependency. What
   about cascade and explicit expiry?
5. Should `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` be included? Its fix (checking
   dependency versions again at write-back) uses the same comparison as the audit, but it also
   touches the persistence ordering.

## References

- `specs/issues/` — the five issues above
- `specs/design/stale-dependency-status-finalization/` (hand-off of issues 3–5, ordering precedent)
- `specs/design/keyed-expiry-cascade-fix/` (introduced `trigger_dependency_audit`, `AuditReport`)
- `specs/design/store-and-asset-search/options-analysis.md` §E2 (consumer of directory dependencies)
- Code at HEAD: `dependencies.rs:158` `register_version`, `:227` `report_no_version`;
  `assets.rs:3280` `mark_expired_status`, `:1613` `note_expired_dependency`, `:4295` `audit_gaps`,
  `:4452` `register_plan_dependencies`, `:1076` `dependency_blocks_fast_track`;
  `plan.rs:2663` `from_dir_key`
