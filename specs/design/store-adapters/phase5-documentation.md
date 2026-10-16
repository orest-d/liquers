# Phase 5: Documentation - store-adapters

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #137)

## Implementation Summary

Implemented in 17 commits on PR #137 (`a8e457f` … step 15 `9022a38`), one per Phase 4 step:

- **Planner realm fix** (step 0): a segment's realm header applies to every action in it, which a
  `store_config` spec needs (user decision 1).
- **Records and data-only access** (steps 1–3, 13, 15): `created`, `etag` and `BackendTime` on
  records; data-only methods and `DataCapabilities` / `DataFacts` on `AsyncStore`, implemented by
  the memory, file, router, OpenDAL and JavaScript stores.
- **Read-only** (step 4): `ReadOnlyStore`, `read_only: true` on a mount, HTTP 501 at the API.
- **The `store_config` realm** (step 5): handler chains, storage models and write strategies are
  queries in a realm that is addressable and plannable but not executable, compiled at build into
  closures. Built-in handlers include `defaults` (user decision 2), `media_type`, `file_facts`,
  `content_hash`; `front_matter` and `front_matter_sync` live in `liquers-lib` behind `records`
  (decision 4). Command overviews omit the realm (decision 3).
- **Storage models and write strategies** (steps 6–7): `native`, `per_entry` (four layouts),
  `grouped`, `none`; `direct`, `atomic`, `transactional` (journal and recovery).
- **`MetadataPipelineStore`** (step 8) and the asset manager's per-mount `version_verification`
  (step 9).
- **Configuration** (step 10): `metadata` sections with the defaults of Phase 2, `build_store` for
  router-level (central) sections; `EnvironmentBuilder` and the web environment use it.
- **Conformance** (step 11): suites `A1`–`A7` over adapted stores, all conformant.

It conforms to the request and the approved design, with the deviations recorded in
[`phase4-implementation.md`](./phase4-implementation.md) §Deviations: the most visible are
front-matter write-back only in `set_metadata`, `record_names` for metadata-only keys, OpenDAL
capabilities reported conservatively, and no `PreparedWrite`. Every acceptance criterion AC-1 to
AC-22 has its Phase 3 test; AC-12 (central metadata) is met with the test model `memory_central`
only, because `central(mount)` is declared but not built (filed below).

## Documentation Delivered

### New Reference Documents
- [`reference/REALMS.md`](../../reference/REALMS.md) — what a realm is, the default realm, and the
  questions a foreign realm raises, with `store_config` as the worked case.

### New Guide Documents
- [`guides/STORE_CONFIG_GUIDE.md`](../../guides/STORE_CONFIG_GUIDE.md) — writing `metadata`
  sections (stages, catalogue, defaults, validation) and defining a new handler, storage model or
  write strategy.

### Existing Documents Reviewed or Updated
All of `affects_docs`, each with a `## History` row and `reviewed: 2026-10-15`:
`reference/STORE_SEMANTICS.md`, `reference/STORE_CONFIG_FSD.md`, `reference/ASSETS.md`,
`reference/PROJECT_OVERVIEW.md`, `guides/STORE_IMPLEMENTATION_GUIDE.md`,
`guides/STORE_FACTORY_GUIDE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md` (the ten-point plan of
Phase 2, tests `STORE14`–`STORE18`). Also updated: `CLAUDE.md` ("Adding a Store Backend"),
`liquers-web/README.md`, `.claude/skills/liquers-validate/SKILL.md`, and
`specs/command_registry.yaml` (regenerated with CHANGELOG lines).

### Links and Capability Map
`specs/README.md`: a task row for `STORE_CONFIG_GUIDE.md`; a Realms entry under "Query and
evaluation"; the store-adapters entry moved from designing to documented.

## Issues Filed

Closed by this work: `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, `STORE-NO-READ-ONLY-ADAPTER`,
`STORE-METADATA-HANDLING-NOT-COMPOSABLE`.

Filed, each a scope the design deferred or a limit found while implementing, and triaged with the
user on 2026-10-16 (the rest stay draft):
- `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` — `-R-key/…` guards, analysed in Phase 2.
- `STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED` — `central(mount)` refuses to build; the interface is in place
  (`ComponentContext` gives a router-level storage builder the router's mounts).
- `STORE-NO-DATABASE-STORAGE-MODEL` — no database model and no `PreparedWrite`.
- `WEB-BUILT-IN-STORES-HAVE-NO-DATA-ACCESS` — **closed**: `localstorage` and `http` now take a section.
- `STORE-OPENDAL-DATA-FACTS-CAPABILITIES-UNKNOWN` — **known limitation**; `created` falls back to the write time.
- `STORE-BUILD-WARNINGS-LOST-IN-THE-BROWSER` — **closed**: warnings returned, shown in the console.
- `STORE-PIPELINE-CACHE-MISSES-EXTERNAL-RECORD-EDITS` — **known limitation**: Liquers is the only metadata writer.
- `STORE-CONFIG-COMPONENT-SPEC-DROPS-PLANNER-DIAGNOSTICS` — **closed**: warnings reported, errors fail.
- `PLAN-ERRORS-DO-NOT-NAME-THE-REALM` — **closed**: not-found errors name the command keys tried.
- `STORE-CONFORMANCE-SUITE-NAME-C6-USED-TWICE` — **closed**: the core router suite is `C8`.

Fixed during Phase 5 review: an `on_default` handler that needs bytes (`content_hash`,
`front_matter`) now gets them through `get_metadata` and listings, not only through `get`; the
`DataCapabilities::data_access` doc comment; a regression test that `native` storage over a file
store refuses a sidecar-colliding key before writing.

Already filed during the design: `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` (handlers defined
in another language; `STORE18` is `NA` until it is done).

## Important Learning

- A realm can be useful without an interpreter: being addressable and plannable is enough to get
  parsing, validation, registry export and documentation for configuration, while execution stays
  in purpose-built closures. [`REALMS.md`](../../reference/REALMS.md) records what that costs.
- The default realm had a planner defect (only the first action took the segment's realm) that
  nothing exercised until a second realm existed.
- Data-only access is the seam that makes adapters composable: a store that separates its bytes
  from its records can take any storage model, and one that cannot is refused at build with the
  mount named, never at the first read.
- Metadata-only keys need explicit support in listings and directory removal once records live
  outside the data objects (`record_names`, `remove_entries`).

## Conformance and Remaining Work

Requested and approved scope is implemented except the parts filed above, all of which were
deferred or interface-only in Phases 1–2. Nothing remains in this design.

## Validation

- `cargo test -p liquers-core --features store-conformance` (`store_adapters`,
  `store_conformance_CONF`), `liquers-store` conformance with OpenDAL, `cargo test -p liquers-lib
  --lib --tests`, `liquers-axum` `store_api_routes`, the `liquers-web` Node loop (27/27 in
  `store_js_STORE`), `check-stubs.sh` and the quickstart build: all pass.
- `bash scripts/check-build-matrix.sh`: all 32 configurations OK (after step 12).
- Every spec in the new documents validated with `liquers-validate` (`-store_config/…`).
- `python3 scripts/docs_index.py --check` and `validate_phase.py store-adapters 5`.
