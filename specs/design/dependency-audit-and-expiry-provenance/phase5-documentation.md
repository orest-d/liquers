# Phase 5: Documentation - dependency-audit-and-expiry-provenance

## Completion Preconditions

- [x] Implementation is finished and validated (Steps 0–12, orest-d/liquers#75)
- [x] All user comments are answered or incorporated (Phase 1–4 gates)
- [x] All review comments are answered or incorporated (the PR had no reviews when this was written)
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Implementation Summary

The design grouped eight `core/assets` issues around one question: when Liquers serves a stored
value, can it tell whether that value is still right, and why it is not? All eight are implemented
and closed. They were delivered in twelve steps on one branch, each step a green commit.

- **Versions say what kind they are (Parts A, G).** Bit 127 flags a content hash
  (`Version::from_content`). A hash can be re-checked against the bytes; a timestamp cannot. `0`
  means unknown. A hash stored before the flag existed is verified against the old unflagged form,
  so existing stores keep working. `from_bytes` stays unflagged, because command metadata versions
  use it.
- **Every route into `Expired` records why (Part C).** `ExpiryReason` is `Direct{cause}` or
  `Cascaded{cause, root, via}`, over seven causes. It is stored in metadata and is meaningful only
  while `Expired`. Every expired asset's log gets one line, written through the overridable
  `AssetManager::record_expiry`.
- **Audits compare correctly, under a chosen policy (Parts A, B).** `audit_version` checks the
  current version against every recorded one, even on first observation. `DependencyAuditPolicy`
  (`Explicit` default, `OnLoad`), `AuditMode::ReportOnly` and `AuditReport.findings` are new.
- **Folder listings are real dependencies (Part D).** A `-R-dir/` version is a hash of the sorted
  entry names, refreshed after writes through Liquers.
- **The immediate manager's deadline fires (Part C).** Its lazy check tested the status, not the
  deadline, so it never fired.
- **Commands can start a dependency and wait later (Part E).** `Context::submit` and a public
  `Context::wait_for_dependency`. This also gave the stale-dependency path its end-to-end test.
- **`AssetManager` can be implemented outside core (Part F).** A from-scratch manager written
  against the public API passes the shared manager scenarios.
- **Content changed outside Liquers is detected on read (Part G).** `VersionVerification::OnRead`
  re-hashes on read. A `Source` adopts the new content. A value with a recipe becomes `Override`
  (default, `ExternalChangePolicy::UserInput`) or is deleted (`Corrupted`). Either way its
  dependents expire with `UpdatedInStore`.

**Conformance.** The implementation follows the approved Phases 2–4. Where it departs from them,
the code was right and the design text was wrong; each case is recorded in `DESIGN.md` Notes. The
four that change what a reader of the design would expect:

1. The listing step runs in its own query asset, so the listing version is carried on that asset's
   metadata and copied into the dependent's record (Step 8).
2. After a restart, an outside change cascades through `audit_version`, not `register_version`
   (Step 9).
3. `submit` is not lazy on either manager (Step 10). Phase 2 now carries a correction.
4. A plan-chained recipe records its dependency's own dependencies directly, so Phase 3 Example 1's
   two-step `via` needs a command that reads its dependency itself (Step 4). Phase 3 now carries a
   correction.

**Added beyond the design:**
- `AssetManager::publish_version` and `is_volatile_query`, which the external manager needed
  (Step 11).
- The inline manager's `wait_for_dependency` now applies the stale-dependency policy (Step 10).

**Fixed after the PR review (2026-10-04).** Codex found three defects, all confirmed by a test
that failed first:
1. After a restart, a Liquers write to a dependency that this process had not yet seen expired
   nothing: `register_version` took it as a first observation. This held under both policies.
   Writes now register through `register_written_version`, and the `on_load` check records the
   version it confirmed.
2. `makedir` and `removedir` did not refresh the parent listing.
3. The directory step built its value and its version from two different reads. It now uses one
   read, and re-reads once the version is registered.

**Omitted:** nothing from the approved scope. The immediate manager's lazy deadline expiry still
does not cascade, as Phase 2 Example 6 specified. That open decision is now an issue.

## Documentation Delivered

### New Reference Documents

None, as Phases 1–2 planned: the contracts extend `reference/DEPENDENCIES_STATUS.md` and
`reference/ASSETS.md` rather than starting a third document.

### New Guide Documents

`specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (new, as planned): how to implement and verify
an `AssetManager` outside `liquers-core`. It covers the decisions to make before writing code, what
a manager holds, required and provided methods, the lifecycle primitives and their contracts, the
key-mutation lock rule, the registration invariants, overriding `record_expiry`, providing an
`AssetManagerKind`, and running the shared scenarios. It ends with the known limits. Its snippets
come from `tests/common/minimal_manager.rs` and `tests/external_asset_manager.rs`.

### Existing Documents Reviewed or Updated

`affects_docs` is unchanged from Phase 2: the ten documents below. Phase 2 discarded the other
`core/assets` candidates (`PROJECT_OVERVIEW`, `DOC_01`, `DOC_08`, `ASSET_SET_OPERATION`,
`PAYLOAD_GUIDE`), and implementation gave no reason to revisit that. Each document was reviewed
against the code and tests, not the design, and got `reviewed: 2026-10-02` and a `phase-5`
History row.

| Document | Change |
|---|---|
| `reference/DEPENDENCIES_STATUS.md` | audits on first observation, `AuditMode` / findings, `DependencyAuditPolicy`, `-R-dir/` listing versions, unknown plan edges, `submit` in Flow B; glossary entries |
| `reference/ASSETS.md` | new §"Why an asset is `Expired`" (cause table, `record_expiry`, real log lines); new §"Content changed outside Liquers"; the `AssetManager` trait is implementable outside core |
| `reference/ASSET_LIFECYCLE.md` | routes into `Expired` with their reasons; verification and `OnLoad` in the fast track; `submit` |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` | route/reason table; the P1 "private dependency-manager type" finding removed as resolved; `remove` description corrected |
| `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | `submit` and `wait_for_dependency`; `submit` is not lazy |
| `reference/ENVIRONMENT_CONFIG.md` | `assets.dependency_audit`, `verify_versions`, `external_change` with defaults |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | new section on waiting for dependencies; the `asset.get()` example now uses `wait_for_dependency` |
| `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | manager options and policy setters; custom kinds link to the new guide; a stale "skips a dependency with no version" passage corrected |
| `guides/STORE_IMPLEMENTATION_GUIDE.md` | "see also" line to the new guide (link only) |
| `guides/UNITTEST_GUIDE.md` | write manager-contract tests once in `tests/common/manager_scenarios.rs` |

The review also corrected stale rustdoc that this design had made wrong:
- `Context::submit`, which still described laziness;
- `trigger_dependency_audit`, which still called the default policy "never";
- the policy enums, which still said "today's behaviour";
- `run_inline` and `expire_without_cascade`;
- the headers of the scenario test modules;
- two broken intra-doc links.

### Links and Capability Map

In `specs/README.md` §"Assets and their lifecycle", the "designing" line is replaced by three
`documented` lines:
- audit and policy → `DEPENDENCIES_STATUS.md`;
- expiry reasons and outside changes → `ASSETS.md`;
- asset managers outside core → the new guide.

Each line names the design in parentheses. The "Versions for computed keyed assets" line now also
points to the audit policies, and the paragraph on designs folded into references names this one.

## Issues Filed

During implementation:
- `SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON`: a caller can store `Expired` with no reason.
- `DEPENDENCY-FAILURE-ERRORS-NAME-ASSET-IDS`: these errors name runtime ids instead of keys.
- `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`: each link records every upstream
  link (40 links take 80 s). Not caused by this design.
- `MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES`: a metadata-only entry in the memory store
  reads back as empty bytes.
- `ANY-STATUS-READ-MISSES-STORED-VALUE-OF-UNLOADED-LIVE-ASSET`: an any-status read misses the stored
  value of a live asset that is not loaded.
- `SUBMIT-IS-NOT-LAZY-ON-ANY-MANAGER`: Phase 2's laziness claim did not hold.
- `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET`: the limits of a manager written outside core.

At Phase 5:
- `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-DOES-NOT-CASCADE`: the open half of
  `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`.

From the PR review (2026-10-04):
- `DEPENDENCY-EDGE-RECORDED-AGAINST-SUPERSEDED-VERSION-IS-NOT-EXPIRED`: the window that remains
  after the directory-step fix below.

Already open and seen again: `WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT`, a `main` failure in
the wasm suite.

## Important Learning

- **A version is a claim, and only a hash can be checked.** Before this design, every check trusted
  the recorded version. The flag bit is what turns "version" into something Liquers can verify
  against bytes. Any new version source must decide whether it is a hash; see `ASSETS.md`.
- **First observation is not "no change".** `register_version` is right for evaluation and wrong
  for audits and restarts. The original audit bug (Step 6) and the outside-change cascade after a
  restart (Step 9) both came from using it where `audit_version` belonged.
- **Lock order decides where side effects run.** Both the outside-change handling (Step 9) and the
  listing refresh (Step 8) had to move their effects after `key_mutation_lock` or the asset data
  lock was released. The rule is in `KeyMutationAccess`'s contract and in the manager guide.
- **Unsealing needs a from-scratch proof.** Writing a manager with only the public API found two
  missing primitives that a review of the trait had not. The shared scenarios module
  (`tests/common/manager_scenarios.rs`) is now the conformance suite for every manager.

## Conformance and Remaining Work

The requested scope, the approved scope and the implemented scope match. Nothing from the design
remains. The follow-up items are the issues above, chiefly
`IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-DOES-NOT-CASCADE` and
`EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET`.

## Validation

- `python3 scripts/docs_index.py --check`: 0 errors.
- The one new example query (`-R-cwd/proj/-/concat_siblings`) was checked with `liquers-validate`.
- `cargo doc -p liquers-core` with `-D rustdoc::broken_intra_doc_links`: clean.
- `cargo test -p liquers-core --lib --tests`, after the rustdoc edits: green.
- The code validation for the implementation is in `DESIGN.md` Notes (Step 12).
