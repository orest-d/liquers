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

*Added at the Phase 2 gate: `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES` and
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` (see `DESIGN.md`). The two sections below
were added on 2026-09-29 at the owner's request. This document is longer than the usual 30 lines
because of them.*

## Terms used in this design

All examples use two stored files: `data/a.csv`, and `data/report.txt`, whose recipe reads
`a.csv` (for example `-R/data/a.csv/-/summarize`).

| Term | Meaning, on the example |
|---|---|
| **Version** | A fingerprint (a hash) of a stored value's bytes, kept in its metadata. When Liquers stores `a.csv` it records `version: V1`. A different content gets a different version (`V2`). *Unknown* (`Version(0)`) means "no fingerprint was taken". |
| **Dependency** | `report.txt` depends on `a.csv`, because its recipe read it. Dependencies are named by *dependency keys*: `-R/data/a.csv` for a stored value, `-R-dir/data` for the list of names in a folder, `ns-dep/command_impl-…` for a command's code. |
| **Recorded version** (the design also says *recorded expectation*) | When `report.txt` is computed, its metadata stores the version of every input it read: `dependencies: [{key: -R/data/a.csv, version: V1}]`. It means "`report.txt` is valid as long as `a.csv` is still `V1`". The same fact is kept in memory as an edge `a.csv → report.txt` labelled `V1`. |
| **Current version** (the design also says *durable version*) | What `a.csv`'s version is *now*, read from metadata without computing anything: from the live asset if this process holds one, otherwise from the store's metadata. "Durable" means it survives a restart, because it is in the store. `None` means `a.csv` has no stored metadata at all. |
| **Version map** | The asset manager's in-memory table `dependency key → version` of what *this process* has seen. It is empty after a restart and fills as assets are loaded or computed. |
| **Stale** | `report.txt` is stale when the current version of an input differs from its recorded version (current `V2`, recorded `V1`). |
| **Cascade** | When `a.csv` changes, everything that depends on it is expired, and everything that depends on *those*, transitively. |
| **Audit** | An explicit check, on request: for each dependency whose version the map does not know, read its current version from metadata and compare it with the recorded versions. Expire what is stale. |
| **Fast track** | Loading `report.txt` straight from the store instead of recomputing it, when its stored status is `Ready`. |
| **Expired** | Status meaning "this value is out of date; the next request recomputes it". |

## The problems, on examples

**1. An audit after a restart misses a changed input** (`AUDIT-CANNOT-EXPIRE-…`). On Monday
`report.txt` is computed from `a.csv` at `V1`, and both are stored. The server stops. A batch job
stores a new `a.csv` through Liquers, so its metadata now says `V2`. On Tuesday the server starts
and serves `report.txt` from the store: the version map is empty, so there is nothing to compare
against. An operator runs `trigger_dependency_audit("-R/data/report.txt")`. The audit reads `V2`
for `a.csv` and puts it in the map. Because the map had *no previous entry*, it treats this as
"first seen", not as "changed", and expires nothing: `AuditReport { checked: [a.csv], expired: [] }`.
`report.txt` stays `Ready` and wrong. **After:** the audit compares `V2` with `report.txt`'s
recorded `V1`, which differ, so `report.txt` is expired.

**2. Nobody can say when to check** (`DEPENDENCY-AUDIT-POLICY-…`). A production service that
restarts nightly wants "never serve `report.txt` built on an old `a.csv`", so it wants a check every
time a stored result is loaded. A researcher who deleted the 20 GB intermediate `data/big.parquet`
to save disk wants the final `report.html` built from it to stay usable. A strict check would see
`big.parquet` gone and recompute `report.html`, which needs `big.parquet` again. Today only explicit
calls exist, with no setting and no "just tell me" mode. **After:**
`assets: {dependency_audit: on_load}` for the service, and the default (`explicit`) for the
researcher. A report-only audit lists what is stale without changing anything.

**3. An expired asset cannot say why** (`EXPIRY-RECORDS-NO-REASON`). `report.txt`'s stored
metadata says `status: Expired`, and its log ends with the entries from its last evaluation. There is no way to
tell whether its time limit ran out, `a.csv` changed three steps upstream, someone called
`expire()`, or an audit found it stale. The one message that exists says *"Dependency asset 17
expired…"*, where 17 is an in-memory counter that means nothing after a restart. **After:**
`expiry_reason: {kind: cascade, trigger: "-R/data/a.csv"}` in the metadata, plus the log line
*"data/report.txt expired: dependency data/a.csv changed"*.

**4. A result built from a folder listing never updates** (`DIRECTORY-LISTING-…`).
`data/index.txt` is computed by `-R-dir/data/-/index_files`: it reads the *list of names* in
`data/` and indexes them. The plan records the dependency `-R-dir/data`, but nothing ever gives
that listing a version, so the dependency is silently dropped. Store `data/new.csv`, and `index.txt`
keeps its old list forever, including after a restart. **After:** the listing gets a version (a
fingerprint of the sorted names). Adding `new.csv` through Liquers changes that version, so
`index.txt` is expired and recomputed.

**5. The "use the old input" rule is never tested through a real command**
(`STALE-DEPENDENCY-PATH-…`). If `a.csv` expires *while* `report.txt` is being computed, the rule
is: finish with the value already read, and mark `report.txt` `Expired` so the next request
recomputes it. Every test calls the internal function directly, because an ordinary command cannot
pause between asking for `a.csv` and receiving it. This path has already been silently dead once
(`DEPENDENCY-EXPIRED-STALE-VALUE-UNREACHABLE`). **After:** a command can `submit` `a.csv`, do other
work, and `wait_for_dependency` later. A test pauses the command in between, expires `a.csv`, and
checks that `report.txt` ends up `Expired`.

**6. Time limits never fire on the in-browser manager**
(`IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`). `report.txt`'s command declares
`expires: "in 1 sec"`. On the immediate manager (used by `liquers-web`), request it, wait two
seconds, and request it again: you get the old value, still `Ready`. The check that should compare
the clock with the deadline compares the status with itself. **After:** the second request
recomputes.

**7. No one outside `liquers-core` can write an asset manager**
(`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE`). Suppose you want a manager that runs
evaluations on a cluster. `impl AssetManager<E> for ClusterManager` in your own crate does not
compile, because the trait requires a private trait. **After:** it compiles, using a documented set
of public building blocks, and a test in `liquers-core/tests/` proves it by doing exactly that.

Not solved here, and filed as `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`: a version is
recorded only when *Liquers* writes a value. If another program overwrites `data/a.csv` directly,
its metadata still says `V1`, and no check, however strict, can see the change.

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

## Documentation Intent

*Added 2026-09-29, after this phase was approved, when the design moved to the `liquers-project`
workflow. It records intent only; Phase 2 §"Documentation Architecture" has the details.*

**Reference:** Extend existing ones, with no new reference. `reference/DEPENDENCIES_STATUS.md` already
owns the dependency and audit contract; it gains audit policy, report-only audits and directory-listing
dependencies. `reference/ASSETS.md` §"The one meaning of `Expired`" gains expiry reasons. A new
reference would split one contract across two documents.

**Guide:** Create `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (added with Part F). Implementing an
asset manager outside core becomes a repeatable task with its own rules, the counterpart of
`STORE_IMPLEMENTATION_GUIDE.md`. Extend `guides/COMMAND_REGISTRATION_GUIDE.md` with the
`submit` / `wait_for_dependency` pattern for commands.

**Other documents to create:** None. The summary is Phase 5 of this folder.

**Specific documents to update:** `ENVIRONMENT_CONFIG.md` (the new `assets.dependency_audit` field),
`api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (the new Context methods),
`api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` and `ASSET_LIFECYCLE.md` (expiry routes and reasons),
`ENVIRONMENT_CONSTRUCTION_GUIDE.md` (pointer to the new guide), `UNITTEST_GUIDE.md` (the shared manager
scenarios), and the capability map in `specs/README.md`.

Audience: internal developers and coding agents working on assets, and authors of external asset
managers or commands that start several dependencies.

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
