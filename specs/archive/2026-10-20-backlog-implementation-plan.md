# Backlog design pass and implementation plan, 2026-10-20

A bulk design pass over every open issue, P3 included, under
`specs/guides/autonomous_bulk_design.md`. It was run on the maintainer's request: "bulk-design
solutions for all open issues and assess readiness, review also existing designs … try to find
synergies … create an implementation plan … treat the issues related to liquers-py, liquers-web or
agent-memory-system separately, but create plan for those too". This is a dated record; it is not
edited later. The designs, issues and `specs/index.csv` are the live state.

Baseline: `main` at `18732dd`. Nothing was implemented. Every "reproduces" verdict is from reading
the code at that commit, with file and symbol named in the issue or design; the few cargo runs are
listed per item. Queries in designs were checked with `liquers-validate`.

## Contents

1. [Counts](#1-counts)
2. [How the pass was run](#2-how-the-pass-was-run)
3. [Records changed](#3-records-changed)
4. [Look at these first](#4-look-at-these-first)
5. [Ready designs](#5-ready-designs)
6. [Decision register](#6-decision-register)
7. [Cross-cutting synergies](#7-cross-cutting-synergies)
8. [Implementation waves](#8-implementation-waves)
9. [Review of the existing designs](#9-review-of-the-existing-designs)
10. [Project groups](#10-project-groups)
11. [Issues still without a design](#11-issues-still-without-a-design)
12. [What is not done](#12-what-is-not-done)

## 1. Counts

| | Start (`18732dd`) | End |
|---|---|---|
| Documents in the index | 675 | 820 |
| Open issues and features | 143 | 200 (60 filed, 3 closed) |
| …by priority P1 / P2 / P3 | 0 / 53 / 90 | 2 / 63 / 135 |
| …without a design | 95 | 76 (see §11) |
| Open designs | 36 | 114 (85 created, 7 closed out) |
| …`ready` | 7 | 24 |
| …`needs-decision` | 19 | 82 |
| …`phase2-blocked` / `covered` / no readiness | 2 / 0 / 8 | 4 / 1 / 3 |
| …`autofix: eligible` and not implemented | 3 (all already merged) | 15 |

`docs_index.py --check` at the end: **0 errors**, 32 warnings. 29 of the warnings are "complexity
L/XL has no design" (§11, mostly the follow-ups this pass split off on purpose), the rest are the
long-standing "no acceptance scenarios" and an unticked Phase 4 in the complete
`build-matrix-disk-budget`.

## 2. How the pass was run

- **Eighteen cluster agents** (C1a-C11), each owning the open issues of one area: triage at HEAD
  (still there? partly fixed? wider than filed?), overlap analysis under `references/overlap.md`,
  then Phases 1-4 of a design per item (compact for S/M, full for L/XL), readiness and auto-fix
  label. Every new design passes `validate_phase.py` for its phases.
- **Two review agents** (R1, R2) re-read the 23 existing designs with a finished Phase 4 against
  HEAD, read-only (the procedure forbids editing them), and listed corrections.
- **Three project agents** planned the liquers-py, liquers-web/language-integration and
  agent-memory issues as milestones, without design folders, because that work belongs to its own
  project (liquers-web is fixed in another project).
- The coordinator closed out finished records (§3), regenerated the index and wrote this plan.

Readiness values: `ready` (Phases 1-4 done, no open question), `needs-decision` (Phases 1-4 written
against the *recommended* answers; approving them makes the design ready), `phase2-blocked` (a
question blocks the architecture itself, so only Phases 1-2 exist), `covered` (another design's
step fixes it).

## 3. Records changed

### Issues closed (3)

| Issue | Why |
|---|---|
| `ASSETS-FIX1` | The FIXME work it tracked is done: keyed delegation waits through the claim-based `wait_for_dependency` (designs `dependency-management`, `keyed-recipe-ownership`, `keyed-delegation-hand-off`, all complete); remaining markers are owned by `EXTENDED-FAST-TRACK` or need no tracking |
| `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` | Fixed by runtime volatility (#142): 0/280 failures at HEAD against 16/20 on `2bc4228`. Its design `evaluation-session-volatile-test-flake` is `abandoned`. The race behind the old flake is real for non-volatile keys and is filed as P1 (§4) |
| `PY-ENVIRONMENT-CANNOT-EVALUATE` | Filed from a stale branch; `lq.Environment` evaluates since `design/liquers-py` (#150/#163) |

### Designs closed out (7)

| Design | New status | Evidence |
|---|---|---|
| `inline-waiter-recovers-dropped-run` | complete | #167 merged, source closed |
| `parquet-read-declared-schema` | complete | #169 merged, source closed |
| `polars-command-tests-via-queries` | complete | #170 merged, source closed |
| `axum-assets-endpoints` | complete | #73 merged, all steps ticked; its two open sources now have their own designs |
| `register-command-enum` (sourceless, 2026-03-02) | complete | the `enum:`/`enum_ref:` DSL shipped without a tracked PR; the remainder is `global-enum-registration` |
| `value-list-support` (sourceless, 2026-03-02) | complete | `handles_to_value` and the `From<Vec<i64>>` impls |
| `value-accessor` (sourceless, 2026-03-02) | abandoned | the read side is `DataNode`; `Value::Accessor(Arc<Mutex>)` contradicts VALUE_TYPE_SYSTEM; conversion is `value-conversion-registry` |

`external-index-sync` was resumed and **narrowed** to its source issue (manager-level expiration
events); the reconciliation layer moved to the new `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`
(XL), with the folder's analysis documents as its input. The maintainer should confirm the
narrowing (decision in §6.3).

### Issues re-scoped

About 40 issues were re-scoped, re-sized or widened, each with a dated triage section in the issue
and the original text kept below it as history. The ones that change what a reader expects:

- **Raised:** `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` P3 → **P1** (silent wrong value).
- **Smaller than filed:** `COMBINED-EXPIRES` L → S (only the cross-time-zone reduction is left),
  `ASSETS-IMPROVEMENTS` L → M (only the HTTP body limit), `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`
  L → M, `COMMAND-METADATA-ENHANCEMENTS` L → M (global enums only; three follow-ups filed),
  `QUERY-AST-DISCARDS-ENTITIES` L → S, `WORKSPACE-SERDE-DERIVE-UNDECLARED` L → S,
  `NO-SQL-QUERY-CAPABILITY-…` L → M, `CORE-STATE-LOCK-API-CLEANUP` M → S (the lock is gone),
  `AGENT-WORKBENCH-SURFACE` L → M, `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` M → S,
  `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` M → S (root cause is in liquers-web).
- **Larger than filed:** `POST-INIT-COMMAND-REGISTRATION` M → L, `CORE-STORE-OPENBIN-MISSING`
  M → L, `TEMPLATE-HAS-NO-HIGHLIGHTER` M → L, `ASSETS-API-ADMIN-OPERATIONS` M → L,
  `WEB-CANCELLATION-INERT` M → L (a core lifecycle change), `LANGUAGE-STORE-CONFIG-FUNCTION-…`
  M → L, `EXPRESSION-ATTRIBUTION-MAPS-ARE-NEVER-CLEANED` S → M.
- **Premise corrected:** `CORE-TOKIO-REMOVAL` (the coupling is 9 `tokio::spawn` and 3 `tokio::time`
  sites, not `tokio::sync`, which is runtime-agnostic), `CORE-MULTI-REALM-INTERPRETER` (realm
  resolution and planning already shipped), `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING`
  (two of its three symptoms do not reproduce), `VALIDATE-PHASE-EMPTY-SECTION-…` (half the
  diagnosis was wrong; a second defect was found).
- **Wider than filed:** `RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION` (also leaks through
  `RecordStreamExt::materialize`), `YAML-SCHEMA-FILE-CANNOT-BE-LINKED-AS-SCHEMA-TEXT` (`.json` too),
  `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` (every `AssetRef::get()` deep-copies the record and
  log), `MACRO-QUERY-VALIDATION-AND-HINTS` (a bad literal leaves a half-registered command).
- Seven issues had their `design:` moved off a **complete** origin design onto the new design that
  owns the remainder (`record-streams`, `agent-memory-mvp`, `liquers-mustache`,
  `axum-assets-endpoints`, `variadic-arguments-declaration`, `variadic-metadata-tail-check`); the
  origin is named in each body.

### Issues filed (60)

Split-off remainders and follow-ups (L/XL ones deliberately have no design yet), and defects found
while reading code. The ones that are defects rather than features:

| Issue | P/C | Found by |
|---|---|---|
| `REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE` | P1 M | C1b |
| `EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD` | P2 M | C1a |
| `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` | P2 M | C1a |
| `CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED` | P2 M | C1b |
| `METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH` | P2 S (designed) | C6 |
| `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` | P2 S | C4c |
| `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE` | P3 M | C3a |
| `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE` | P3 S | C3a |
| `SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64` | P3 S | C5a |
| `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` | P3 S | C5c |
| `RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT` | P3 S | C7 |
| `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER` | P3 S | P-py |
| `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` | P3 S | P-py |
| `LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY` | P3 M | C5a |
| `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` | P3 S (doc example does not compile) | C5c |
| `DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES` | P3 S (docs) | C3a |
| `WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY` | P3 S (docs) | C11 |
| `BUILD-MATRIX-CI-SKIPS-NEWER-CRATES` | P3 S (CI) | C10 |
| `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI` | P3 S (CI) | C10 |

The remaining 41 are split-off scope (for example `VALUE-NOT-CONVERTED-AUTOMATICALLY`,
`REMOTE-ASSET-MANAGER-AND-SYSTEM-API`, `STORED-DERIVED-VALUES-ARE-NEVER-PRUNED`) or source issues
for sourceless designs (`PY-ASYNC-EVALUATION-NOT-SUPPORTED`,
`AGENT-MEMORY-HAS-NO-INDEXES-OR-CONTEXT-PACKS`); `specs/index.csv` lists them all with
`created: 2026-10-20`.

## 4. Look at these first

Correctness gaps, highest first. Two are P1.

1. **`STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN`** (P1) — an `I64` stored at an extensionless
   key reads back as `Bytes "42"`. Design `stored-value-default-data-format` is **ready and
   eligible**: do it in wave 1.
2. **`REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`** (P1, **no design**) — a queued
   manager's background save still in flight after `remove` / `set_state` / `set_binary` writes the
   old value back, so a removed value reappears or a user's value is overwritten in the store. It is
   the same race the closed flake test exposed for volatile keys. Inferred from the old failing log
   and the code; not re-run with a non-volatile key. **Next design to write**, together with the
   two siblings below ("late write-back" family; they share `uncached-write-back-rechecks-dependency-versions`'
   version-check idea but have different change sites).
3. **`CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED`** (P2) — `mark_expired_status` refuses
   in-flight statuses and the error is ignored, so a dependent finishes `Ready` on a superseded
   input. Needs a decision against the existing "tolerated in-flight race" test.
4. **`EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD`** and
   **`RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`** (P2) — after a reload, or after a recipe edit,
   dependents are not expired. Both inferred from reading.
5. **`METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH`** (P2) — the immediate manager panics without a
   tokio runtime when a command logs. Design `inline-metadata-save-without-runtime` is ready and
   eligible; its first step reproduces the panic and stops if it does not.
6. **`QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII`** (P2, S, no design) — `Łoad-x` parses as an action
   because the parser casts `c as u8`. A one-crate bug fix; write a compact design and auto-fix.

## 5. Ready designs

24 designs are marked `ready`. 23 can be implemented now; the 24th, `core-value-enum-oversized`, is
listed after the table because it must be corrected first. **Eligible** means it may go to an automatic fix under
`references/auto-fix.md`; the others are ready but need a maintainer's eye on the PR (public API,
several crates, or a registry change).

| Design | Source P/C | Auto-fix | Notes |
|---|---|---|---|
| `stored-value-default-data-format` | P1 S | eligible | wave 1; land before `key-without-recipe-answer` (same function) |
| `inline-metadata-save-without-runtime` | P2 S | eligible | prerequisite of `executor-neutral-core` |
| `inline-run-claim-repair-under-lock-contention` | P2 M | eligible | same region as `cancel-point-of-no-return`; one implementer, in sequence |
| `cancel-point-of-no-return` | P3 M | eligible | rewrites COMMAND_DESIGN_GUIDE §Sync versus async, which documents the bug |
| `temporary-asset-unique-id` | P3 S | eligible | |
| `uncached-write-back-rechecks-dependency-versions` | P3 M | eligible | |
| `runtime-dependency-record-superseded-check` | P3 M | eligible | run the full lib suite (risk: `ns-search` false positives) |
| `expires-cross-timezone-reduction` | P2 S | eligible | |
| `reserved-instruction-command-names` | P2 S | eligible | also fixes three docs that describe the defect as open |
| `core-value-json-identifier-dispatch` | P2 S | eligible | **hidden prerequisite of `core-value-enum-oversized`** (its AC-3 fails without it) |
| `state-lock-marker-cleanup` | P3 S | eligible | comment only |
| `core-wasm-warning-free` | P3 S | eligible | proof needs the wasm32 matrix row |
| `validate-phase-section-match` | P3 S | eligible | docs tooling; do first in the docs-tooling PR |
| `phase5-skeleton-unticked` | P3 S | eligible | docs tooling |
| `front-matter-yaml-agreement` | P3 S | eligible | docs tooling; quoting and unquoting in one commit |
| `serde-derive-feature-declared` | P2 S | not eligible | three manifests; fold the `serde` lines with `core-value-enum-oversized` |
| `lib-build-warning-cleanup` | P3 S | not eligible | regenerates the registry (`version: auto`) |
| `root-listing-dependency-key-spelling` | P3 S | not eligible | **add the `restore_or_remove_edge` normalisation site** (R1) |
| `environment-manager-reference-cycle` | P2 M | not eligible | **land early** (§7); audit must include `MetadataSaver::save_immediately` |
| `payload-requirement-skips-dependency-analysis` | P3 M | not eligible | ~691 → ~271 lookups in `chain_20_links_smoke` (estimate) |
| `recipe-preview-shares-plan-analysis` | P3 S | not eligible | |
| `polars-dataframe-ipc` | P3 S | not eligible | after `ipc-reader-polars-text-columns` if polars-written IPC must read back in records |
| `column-mut-zero-copy-validity` | P3 M | not eligible | after `column-slice-shared-buffers` (both touch `into_mut`) |

**Not implementable as written: `core-value-enum-oversized`** (P2 M, not eligible). Its front matter
says `ready` and every decision is taken, but its step 4 / AC-5 assume liquers-py and liquers-web
build unmodified, which stopped being true when liquers-py began binding `SimpleValue` payloads, and
it names a method that does not exist (§9). The design would stop at step 4. Apply the §9
corrections and re-run `validate_phase.py core-value-enum-oversized 4` before scheduling it; this
pass could not, because finished designs are not edited by bulk design.

`temporary-asset-never-run-leak` is `covered` by `environment-manager-reference-cycle` step 5. The
maintainer should add `TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP` to that design's
`issues:` (finished designs were not edited by this pass), or lift step 5 out as an S fix.

## 6. Decision register

Every `needs-decision` design has Phases 1-4 written against the recommended answer below, so a
plain "approve the recommendations" makes it ready. Where a design has more questions than listed,
the rest are *proposed resolutions* (the design states them; approving the design accepts them).
Full context, alternatives and worked examples are in each design's Phase 1 "Design Readiness".
Questions that should be answered together are in §7.

### 6.1 Dependency graph and asset runs (C1a, C1b)

| Design | Question → recommended answer |
|---|---|
| `keyed-run-replaces-dependency-edges` | Drop a keyed asset's old incoming edges **at the start of its run**; keep expired-never-rerun assets in `expression_dependents` (accept over-expiry); keep the O(graph) scan unless `chain_evaluation_scales` regresses >10% (step 5 stops) |
| `set-binary-replaces-dependency-edges` | Add a provided `AssetManager::track_installed_records` for external managers: **yes** |
| `dependency-manager-registered-version` | Public read-only `DependencyManager::registered_version`: **yes** (not a `test-support` feature) |
| `inline-manager-flat-dynamic-dependency-runs` | A session-scoped run driver (`FuturesUnordered`, no executor); orphaned dependency runs are **drained before the top level returns**. Riskiest design of the pass (hang/deadlock rated medium) |

### 6.2 Resolving a key and its errors (C2)

| Design | Question → recommended answer |
|---|---|
| `key-without-recipe-answer` (MISSING + DEGRADED + DIRECTORY) | "No value and no recipe" is `KeyNotFound` via new `Error::resource_not_found`; a bare directory key with no recipe becomes a `Directory` state; a stored refusal is reported only when no recipe exists; the degraded error stays `General`. Un-xfails a liquers-py test |
| `recipe-folder-names-listing-contract` | **A:** document that `assets_with_recipes` lists only folders the store lists (B, climbing to recipe-only folders, after `root-listing-dependency-key-spelling`) |
| `storage-none-computed-values` | **A:** never write a computed value into a `storage: none` mount; new `AsyncStore::keeps_metadata(key)`. Needed before `agent-memory-derived` Phase 2 |
| `error-with-key-field` | Ordered `ErrorPayload.context: Vec<ErrorContextFrame>`, innermost first; flat fields name where raised; roles `Asset/Link/Action/Resource`; bound 16 frames; cancellation root cause moves to `key`; bindings expose frames as plain records. **Decide with the error-schema batch (§7.1)** |

### 6.3 Asset manager features, ownership, lifecycle (C3a, C3b)

| Design | Question → recommended answer |
|---|---|
| `http-request-body-limit` | Keep axum's plain-text 413 and its 2 MiB default, add `with_body_limit`; drop the core `max_binary_size` idea |
| `command-execution-class` (phase2-blocked) | **Blocking:** is a fast lane wanted at all? Recommended: confirm with an egui/dashboard use case first, then build (b) a reserved fast lane with its own capacity |
| `asset-registration-contract` | Defer protected registration to `REGISTERED-ASSET-CANNOT-BE-PROTECTED-FROM-UNREGISTRATION`: **yes** — then the design is docs + tests only and becomes eligible |
| `asset-garbage-collection` | Reachable = only the manager holds it, finished, not Source/Override; first slice bounds count and idle time (bytes wait for `VALUE-HAS-NO-MEMORY-SIZE-ESTIMATE`); never deletes stored data |
| `environment-orderly-shutdown` | Modes `Drain` and `Cancel`, no timeout argument; one provided `Environment::on_shutdown`; reads never refused; bindings unchanged |
| `asset-manager-store-sync` | One conservative sequence parameterised by root and `AuditMode`; no global lock; a deleted stored copy only unmaps |
| `external-index-sync` | New per-manager `broadcast` of `AssetEvent` (expirations only, in process, not `#[non_exhaustive]`); **confirm the narrowing** of the folder (§3) |

### 6.4 Commands, macro, recipes, diagnostics (C4a, C4b)

| Design | Question → recommended answer |
|---|---|
| `variadic-link-array-expansion` | Expand an array-valued link into elements at materialisation, one level deep |
| `macro-argument-hints-and-query-literals` | No compile-time parsing of `query "…"` literals (would make the macro depend on core); register atomically with the command name in the error; implement argument-level `hint` |
| `global-enum-registration` | A dangling `enum_ref` is an `Error` at `build()`; re-adding replaces; the metadata version hashes the resolved enum |
| `composite-variadic-arguments` | Approve now, **schedule when a command needs pairs**; metadata and retrieval only; `ArgumentType::Tuple`, arity 2-4 |
| `post-init-command-registration` | (a) `Snapshot<'_, T>` returns from the two registry getters (breaks out-of-tree `Environment` impls); each step takes the current snapshot; removed commands expire dependents. Option (c), keep the rebuild and close not-planned, is legitimate — the web/py rebuild works today |
| `recipe-override-addressing` | Qualified names inside `Recipe.arguments` (`1.values`, `@key`), 0-based; a key hole appends at a segment end; no predecessor boundary may contain an overridden action (**correctness finding**: steps 4 and 5 ship together) |
| `query-cache-directives` | Final result follows the mode at its last command; the directive wins over `cached: false`; `cache` becomes a reserved instruction; warning, not error, in a volatile region. The maintainer had deferred the source: implement on demand |
| `validate-through-recipe-providers` | The core CLI may open a read-only file store with `--store-root`; caller names sample keys; library in `liquers_core::validate::provider` |
| `diagnostics-carry-their-origin` | `LogSource { Command, Plan, Engine }` as `Option`; `add_log_entry` fills `Command`; `Issue::Plan { severity, query, position, message }` |

### 6.5 Query syntax and keys (C4c)

| Design | Question → recommended answer |
|---|---|
| `resource-name-unicode` | **Option C**: Unicode alphanumerics and tilde entities in names; the decoded name is the key and the physical name (new `Key::path()`); forbidden: `/`, C0, DEL; canonical spelling keeps every key that parses today byte-identical; no normalisation; `ResourceName::try_new`, `Key::try_join`. Option A = close not planned. Changes key encoding: `PROJECT_OVERVIEW.md` update; one PR |
| `query-builder` | New `liquers-encode` binary; `ParameterError`; `resource([...])` refuses `.`/`..`; JS `encodeParam("")` returns `""` (changes documented behaviour) |
| `store-backend-absolute-key` | Split the trait: backends implement `StoreBackend(AbsoluteKey<'_>)`, callers keep `AsyncStore(&Key)` by a blanket impl; seal `AsyncStore`; borrowed `AbsoluteKey` |
| `styled-query-entity-tokens` | Canonical-spelling highlighting is acceptable (no AST redesign) |
| `one-based-error-positions` | User-data and query-text positions 1-based; Rust indices labelled "index"; chunk numbers stay 0-based as identities. Its records part becomes eligible once decided |

### 6.6 Values: serialization, runtime, type system (C5a-C5c, R1, R2)

| Design | Question → recommended answer |
|---|---|
| `data-format-vocabulary` | This design = vocabulary + tooling + write-only formats (three new issues take the rest); `TypeInfo::write_only_data_formats`; no alias normalisation on the data path; unknown format = validator warning; canonical spellings are Liquers' defaults |
| `deserialization-with-metadata` | Write side separate (`RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE`); attribute `records.schema`; a bad schema fails the read. Step 4 requires `metadata-application-attributes` |
| `shared-metadata-and-bytes` | `bytes::Bytes` in core; `from_metadata(Arc<…>)`, `from_bytes(Bytes)`; keep `set(&[u8])`; copy-on-write inside `Metadata`. Stage A (metadata) is the cheapest high-value piece: every `AssetRef::get()` deep-copies the record and log |
| `value-serialization-streaming` | `serialize_stream(self: Arc<Self>, fmt) -> SerializedBytes { Whole, Stream }`; whole cached, stream never; entry routes collect; persistence stays whole. **Requires** `store-streaming-read-write` and shared-bytes stages B/C |
| `serialization-lossy-write-warnings` + `value-serialization-writer` | **One question, answered once:** now, or inside the streaming redesign? Recommended: **now, standalone** (both survive the redesign). Lossy: a `LogEntry::warning`, every kept serialization, deduplicated. Writer: `&mut dyn Write`; overrides for core `Value` json and polars DataFrame |
| `serialized-binary-disposal-policy` | Drop the bytes after a successful persist, no configuration; reader-cached bytes stay; fast-track bytes are never released |
| `metadata-application-attributes` | Recomputed values do not inherit attributes; one flat `BTreeMap<String, serde_json::Value>` with `<app>.<name>` convention; `AssetInfo` carries them; HTTP returns the record as stored |
| `search-catalog-single-metadata-read` | Default trait method `AssetManager::get_asset_info_and_version`; accept that a store overriding `get_asset_info` is not consulted on that branch |
| `combined-value-shared-data-node` | `ValueInterface::as_data_node(&self) -> Option<&dyn DataNode>`, default `None` |
| `template-non-scalar-text-form` | **Stay deferred** (maintainer's call of 2026-10-17) |
| `value-interface-capability-split` | Extract only `ValueType` (identity + description); keep the serializer supertrait; rename `identifier` → `type_identifier` (and the JS getter) |
| `value-type-definition-macro` | Generator first, scalars moved to `VALUE-EXTENDED-SCALAR-TYPES-MISSING`; accessors normalised to `from_X(Arc<T>)` (29 call sites); `ExtValue` only; a proc macro. Revisits the maintainer decision of 2026-08-18 |
| `value-conversion-registry` | Explicit conversion now, automatic later (`VALUE-NOT-CONVERTED-AUTOMATICALLY`); one root command `convert-TARGET`; classes `Identical/Exact/Widening/Structural/Lossy/Fallible` — **the class table is a public promise, review it explicitly** |
| `value-summary` | `{summary, properties}` named `ValueSummary`; a typed field on `MetadataRecord`/`AssetInfo`; O(1)/O(schema) cost rule. P3, last |

### 6.7 Realms and distribution (C6)

| Design | Question → recommended answer |
|---|---|
| `realm-type-registry` | An imported registry's realm is the importer's name for that host; import default-realm entries only; transfer `Serialize` / `ConvertTo` / `Untransferable` |
| `remote-store-and-asset-manager` | Slice 1 as a `client` feature of `liquers-axum`; static headers with `${VAR}` until sessions exist; type name `liquers_api` |
| `executor-neutral-core` (phase2-blocked) | **Blocking:** executor-agnostic (an `AsyncRuntime` spawn/timer seam, keep `tokio::sync`) — recommended — or tokio-free; executor supplied on the manager and `EnvironmentBuilder::with_runtime` |
| `multi-realm-dispatch` (phase2-blocked) | **Blocking:** prefix delegation (recommended) vs per-action RPC; the binding owns the realm rename; bindings in config and builder |

### 6.8 Stores (C7)

| Design | Question → recommended answer |
|---|---|
| `store-config-uri` | Expand `${VAR}` on the URI string, percent-encoding each value (overturns the 2026-08-29 answer, which OpenDAL makes impossible); `config:` beside `uri` for secrets; `file://` core, `fs://` OpenDAL; `type` and `uri` exclusive; a narrow hand-written parser |
| `store-handler-field-access` | Settable: six text fields, `type_name`, `message`, `attributes.<name>`; empty = null/""/[]/{}; after `metadata-application-attributes` |
| `store-config-key-scoped-handlers` | `-R-key/k/` guards k and everything below; every matching guard runs in spec order |
| `central-storage-model` | Records at `<mount>/__metadata__/<key>`, one JSON object per key; a read-only central mount refused unless the router section is read-only |
| `store-command-namespace` | Reads in `store`, writes in `store_write` (volatile), so an MCP allow-list can name `store` safely; authorisation as for `-R/` |
| `store-database-records` | `sqlx` SQLite (**choose together with `relational-database-access`**); drop the `PreparedWrite` half; type `sqlite_kv`, feature `sqlite-store` off by default |
| `store-streaming-read-write` | Chunks are `bytes::Bytes` (shared with the value layer); `set_stream` pulls a stream; a failed stream removes data and sidecar; `get_stream`/`set_stream` |
| `store-conditional-write` | Conflict as `StatusConflict` via `Error::write_conflict` (M) — or a new `ErrorType::WriteConflict` (L; decide with §7.1); version token = `Version::from_content`; default check-then-write |
| `store-conformance-check-tool` | Required `--claims <yaml>`; a scratch-recipe table in the tool rather than `StoreFactory::create_fixture` (**reverses a decision recorded in the accepted issue**); print config with `${VAR}` unexpanded |

### 6.9 Records and SQL (C8a, C8b, R2)

| Design | Question → recommended answer |
|---|---|
| `schema-argument-structured-link` | Change both core and lib `TryFrom<…> for String`; general rule: an Object/Array bound to any `String` argument becomes compact JSON text (error → success for every command) |
| `column-slice-shared-buffers` | Unsliced buffers stay 64-byte aligned, a window is element-aligned (arrow-rs does the same); offsets copied and rebased. **Answer before designing the Arrow C Data export** |
| `record-batch-private-fields` | `pub(crate)` fields plus accessors; a materialised batch carries the view's own `chunk_id` |
| `polars-bridge-vector-columns` | Enable polars `dtype-array`; accept `Array(Float32, d)` only |
| `ipc-reader-polars-text-columns` | Read both Large\* and view layouts, converted to `i32` offsets, refused above `i32::MAX` |
| `sql-over-tables` | `polars/sql` from the existing `polars` feature; `ns-pl/sql` over table `self`; single statement, relations limited to `self` and CTEs (a **security** requirement: polars table functions would read the server filesystem) |
| `relational-database-access` (phase2-blocked) | **Blocking:** how a query names a database (store mount / connection value / `databases:` config) and freshness (volatile / TTL / backend change marker). No recommendation strong enough to proceed; new crate `liquers-sql` recommended for placement |

### 6.10 UI (C9, R2)

| Design | Question → recommended answer |
|---|---|
| `ui-asset-snapshot-boundary` | No `UiAssetAdapter` trait (**overrides the accepted issue**); `text_editor` becomes async and returns a `UIElement`; delete `AssetStatus` and `from_asset_ref` without deprecation |
| `ui-events` | One `UpdateMessage::User(UserEvent)` with `UserEventKind { Submit, Named }`; events declared in markup; Enter → menu shortcuts → browser; deliver to the owner only |
| `query-editor-live-validation` | Source-span `SyntaxToken` in core; `UIContext::with_command_registry` (snapshot); first error only; browser highlights at render time |
| `template-syntax-highlighter` | Byte-range tokens wrapping core `SyntaxKind`; `template/highlight(style = "html")`; shared `lq-tok-<kind>` classes |
| `template-editor-ui` | One `TextEditorElement` with `EditorSyntax`; preview shows the saved template |
| `text-editor-save` | Typed `UpdateMessage::AssetSet`; write through `AssetManager::set_state`; unmounted editor still saves |
| `console-mounts-nested-element` | Mount as a child through a new `AppMessage::ReplaceChildren`; remount only when the value `Arc` changes |
| `variadic-argument-slots` | Pad omitted earlier arguments with their defaults; core `plan.rs`; renderer moves to `NO-COMMAND-PARAMETER-EDITOR` |

### 6.11 Build, docs tooling, server, access (C10, C11, R1)

| Design | Question → recommended answer |
|---|---|
| `workspace-rustfmt-check` | Its own PR **after the small-fix wave**, merged with a merge commit (keeps `.git-blame-ignore-revs` valid); the CI job fails the PR; no wrapper script. Drift grew 39 → 213 files in seven weeks |
| `core-benchmark-suite` | `criterion`, native-only dev-dependency; baselines not in git; core only first |
| `library-no-panic-lint` | Lint in `check-build-matrix.sh` (clippy in every row, wasm too); `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`; no exceptions needed today. **Share the attribute line and the CI switch with `core-tracing-migration`** |
| `core-tracing-migration` | `tracing`, unconditional in core/records/lib; silent without a subscriber; the CLI bins, py init and web init install one — **plus `liquers-agent`, whose stdout is an MCP stdio stream** (R1). Candidate to split into a core-only first slice |
| `axum-websocket-connection-limits` | Defaults on (1024 connections, ping 30 s, idle 90 s, write 30 s); refuse over-cap with close code 1013; idle ≥ 2 × ping |
| `assets-api-admin-operations` | `AssetManager::remove_computed`, `POST key/remove_computed/{*key}`; `GET manager` at the base; a key without a recipe → 409 |
| `unmanaged-asset-registry` | Opt-in now with UUID v4 bearer ids (capacity 1024, kept 10 min), `id/` route family |
| `access-policy-at-entry-points` | **Check identity at the entry points, never on `Context`** (contradicts the issue's "Context exposes the user", for shared caching); reuse `User`; new `ErrorType::AccessDenied` → 403 (decide with §7.1); `with_access_policy` per builder; check only what the request names; no policy = allow all |
| `pure-command-context` | Keep `get_envref()`/`get_asset_ref()` infallible, rely on `pure: true`; derive purity from the signature with `pure: false` opt-out; one mode `ContextAccess::Pure` |
| `registry-query-default-text-form` | Write text too (`!Query '-R-key/.'`): bumps 11 commands' `metadata_version` once |
| `cwd-default-link-silent-root` | Silence only a `DefaultLink` that is exactly `-R-key/.` |
| `cwd-less-recipe-analysed-at-root` | The analysis resolves a cwd-less recipe against the root, as its run does (then **eligible**) |
| `context-title-predecessor-inheritance` | Inherit command-set title/description across `Step::Evaluate`. **Fails structural validation** (no AC-n, no checklist): fix before implementing |
| `external-manager-evaluation-session-sharing` | `pub async fn AssetRef::join_evaluation_session_of(&self, parent)` |

## 7. Cross-cutting synergies

### 7.1 One error-schema decision

Five designs and issues change the serialized `Error` or add `ErrorType` variants. Decide them in
one sitting so py, web, axum and stored logs change once:

- `error-with-key-field` — context frames, `key` vs `query`;
- `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING` and the class half of
  `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (language segments, exception class);
- `access-policy-at-entry-points` — `ErrorType::AccessDenied`;
- `store-conditional-write` Q1 — `ErrorType::WriteConflict` or reuse `StatusConflict`;
- `key-without-recipe-answer` — `Error::resource_not_found` (a constructor, no new variant).

Every exhaustive `ErrorType` match (core `error.rs`, the `assets.rs` persistence match, axum
`api_core/error.rs`, py `error.rs`/`_errors.py`, web `error.rs`) gets its arms in one PR; the
`configuration-error-kind` PR is the precedent.

### 7.2 Land `environment-manager-reference-cycle` early

It is ready, and it unblocks the most: the liquers-py and liquers-web conformance rows held at
BLOCKED/PARTIAL (`RUNTIME05` strict xfail), `environment-orderly-shutdown`,
`asset-garbage-collection`, and the covered temporary-asset leak. Together with
`key-without-recipe-answer` (DEGRADED) it flips five liquers-py matrix rows with no py code change
beyond removing xfails. Land it after the claim-area fixes of wave 1 (it adds a pin to
`InlineRunClaim`).

### 7.3 Serialization chain

`core-value-json-identifier-dispatch` → `core-value-enum-oversized` (corrected; fold in
`serde-derive-feature-declared`) → `serialization-lossy-write-warnings` →
`serialized-binary-disposal-policy` → `value-serialization-writer` → `shared-metadata-and-bytes`
stage A, then B, then C → `value-serialization-streaming`. `store-streaming-read-write` supplies the
`Bytes` chunk type and stream alias the value layer uses (consider a neutral module rather than
`store.rs`). `data-format-vocabulary` and `deserialization-with-metadata` rebase onto whichever
lands first; `RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE` should share the "what serialization
adds to the metadata" hook with lossy warnings.

### 7.4 Store trait chain

`store-conditional-write` → `store-conformance-check-tool` (ships with `cond01`-`cond04`) →
`store-streaming-read-write` → `store-backend-absolute-key` (the `StoreBackend` split; whatever
lands after it writes against `StoreBackend`) → `resource-name-unicode` (benefits from the backends'
path mapping having just been touched). `store-config-uri`, `central-storage-model` →
`store-database-records`, `store-config-key-scoped-handlers` and `store-handler-field-access` (after
`metadata-application-attributes`) are independent of the trait chain. The SQLite engine choice is
shared by `store-database-records` and `relational-database-access`.

### 7.5 Value type system

`value-interface-capability-split` → `value-type-definition-macro` (same files; one implementer);
`value-conversion-registry` and `value-summary` are separate PRs after the split. The macro changes
CLAUDE.md §Adding a Value Type (maintainer-owned; the PR proposes the edit).

### 7.6 UI message variants

`ui-asset-snapshot-boundary` → `ui-events` → `text-editor-save` / `console-mounts-nested-element`
→ `query-editor-live-validation` → `template-syntax-highlighter` → `template-editor-ui`. Four of
these add an `UpdateMessage`/`AppMessage` variant and touch the same seven exhaustive-match sites:
one PR sequence. `UI_INTERFACE_FSD.md` is stale (describes `ExtValue::UiWidget`); rewrite it against
HEAD before or with the first. The browser tests need a `cargo clean` session of their own.

### 7.7 Registry-regenerating designs land one at a time

`lib-build-warning-cleanup`, `registry-query-default-text-form`, `pure-command-context`,
`core-tracing-migration`, `ui-asset-snapshot-boundary`, `sql-over-tables`,
`macro-argument-hints-and-query-literals`, `global-enum-registration` and shared-metadata stage A
each regenerate `specs/command_registry.yaml`. Merge them sequentially and regenerate on the rebase.

### 7.8 Access policy is a hub

`access-policy-at-entry-points` steps 1-2 (the error variant and the `User` derives, inert) unblock
`unmanaged-asset-registry`, `remote-store-and-asset-manager` (which must take peer identity from
it), `MCP-SERVER-APPLIES-NO-ACCESS-POLICY` and the agent workbench's general evaluate tool.
`store-command-namespace` commands are authorised only as commands.

### 7.9 Smaller pairings

- Query token predicates: `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` → `query-builder` (reuses the
  predicates; `liquers-template`'s private copies delegate) → `resource-name-unicode` widens them.
- Plan analysis: `payload-requirement-skips-dependency-analysis` and
  `recipe-preview-shares-plan-analysis`, either order; `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-…`
  is blocked on `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`.
- Edge-lifecycle trio for one reviewer: `keyed-run-replaces-dependency-edges` →
  `set-binary-replaces-dependency-edges` → `runtime-dependency-record-superseded-check`, after
  `root-listing-dependency-key-spelling`.
- CWD trio in `plan.rs`: `cwd-less-recipe-analysed-at-root`, `cwd-default-link-silent-root`, then
  `registry-query-default-text-form`.
- Records internals: `column-slice-shared-buffers` → `column-mut-zero-copy-validity`;
  `record-batch-private-fields` before `polars-bridge-vector-columns` (both edit `polars.rs`);
  `ipc-reader-polars-text-columns` before `polars-dataframe-ipc`.
- Late write-back family: `uncached-write-back-rechecks-dependency-versions`, the P1
  `REMOVED-OR-REPLACED-KEY-REVIVED-…` and `CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED`.

## 8. Implementation waves

Each wave lists what can start once the previous one has merged; within a wave, items in different
bullets can run in parallel threads. "After decisions" means the §6 recommendations are approved.

**Wave 1 — ready, eligible, small (auto-fix candidates).** Independent PRs:

- `stored-value-default-data-format` (P1).
- Asset run harness, one implementer in sequence: `temporary-asset-unique-id` →
  `inline-run-claim-repair-under-lock-contention` → `cancel-point-of-no-return` →
  `uncached-write-back-rechecks-dependency-versions`.
- `inline-metadata-save-without-runtime`, `expires-cross-timezone-reduction`,
  `runtime-dependency-record-superseded-check`.
- `reserved-instruction-command-names`, `core-value-json-identifier-dispatch`,
  `state-lock-marker-cleanup`.
- Docs-tooling PR: `validate-phase-section-match` → `phase5-skeleton-unticked` →
  `front-matter-yaml-agreement` → `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI` (needs a compact design).
- Build-hygiene PR: `core-wasm-warning-free`, `BUILD-MATRIX-CI-SKIPS-NEWER-CRATES` (compact design).

**Wave 2 — ready, not eligible (maintainer reviews the PR).**

- `lib-build-warning-cleanup`, `root-listing-dependency-key-spelling` (with the extra site),
  `payload-requirement-skips-dependency-analysis`, `recipe-preview-shares-plan-analysis`.
- `core-value-enum-oversized` (after correction, with `serde-derive-feature-declared`).
- `environment-manager-reference-cycle` (after the wave-1 claim fixes).
- Records: `polars-dataframe-ipc`; `column-mut-zero-copy-validity` once
  `column-slice-shared-buffers` is decided and landed.
- Compact designs then fixes for the S defects in §11 (`QUERY-TOKEN-CLASSES-…`,
  `RESOURCE-DIRECTORY-STEP-PRINTS-…`, `SIMPLE-VALUE-I32-…`, `FOREIGN-VALUE-…`,
  `TO-OVERRIDE-KEEPS-…`, `POLARS-DATAFRAME-JSON-…`, the three doc fixes).
- **Design** the P1/P2 late-write-back and dependency-versioning defects (§4).

**Barrier — `workspace-rustfmt-check`** (after its decision): one reformat PR, merge commit, every
open branch runs `cargo fmt --all` before rebasing.

**Wave 3 — first decisions, contained designs.** In rough priority order:
`key-without-recipe-answer`, the error-schema batch (§7.1) starting with `error-with-key-field`,
`asset-registration-contract`, `storage-none-computed-values`, `set-binary-…` /
`keyed-run-…` / `dependency-manager-registered-version`, `diagnostics-carry-their-origin`,
`one-based-error-positions`, `styled-query-entity-tokens`, `schema-argument-structured-link`,
`record-batch-private-fields`, `ipc-reader-polars-text-columns`, `sql-over-tables`,
`http-request-body-limit`, `axum-websocket-connection-limits`, `library-no-panic-lint` (clean
crates first), `metadata-application-attributes`, `search-catalog-single-metadata-read`,
`cwd-less-recipe-analysed-at-root`, `cwd-default-link-silent-root`,
`combined-value-shared-data-node`, `external-manager-evaluation-session-sharing`,
`context-title-predecessor-inheritance` (after its structural fix), `variadic-argument-slots`,
`console-mounts-nested-element`.

**Wave 4 — the chains of §7**, each a PR sequence: serialization (§7.3), store trait (§7.4), value
type system (§7.5), UI (§7.6), access policy (§7.8) with `unmanaged-asset-registry` and
`assets-api-admin-operations`, lifecycle (`environment-orderly-shutdown` →
`asset-garbage-collection`; `asset-manager-store-sync`, `external-index-sync`), recipes
(`recipe-override-addressing` → `validate-through-recipe-providers`), commands
(`variadic-link-array-expansion`, `macro-argument-hints-and-query-literals`,
`global-enum-registration`, `pure-command-context`, `registry-query-default-text-form`,
`core-tracing-migration`), stores (`store-config-uri`, `store-handler-field-access`,
`store-config-key-scoped-handlers`, `central-storage-model` → `store-database-records`,
`store-command-namespace`), query (`query-builder` → `resource-name-unicode`).

**Wave 5 — large or on demand.** `post-init-command-registration`, `composite-variadic-arguments`,
`query-cache-directives`, `inline-manager-flat-dynamic-dependency-runs`, `realm-type-registry`,
`remote-store-and-asset-manager`, `core-benchmark-suite`, `value-summary`,
`template-non-scalar-text-form` (deferred), and the phase2-blocked four
(`command-execution-class`, `executor-neutral-core`, `multi-realm-dispatch`,
`relational-database-access`) once their blocking question is answered.

## 9. Review of the existing designs

All 23 existing designs with a finished Phase 4 were re-read against HEAD. **None is stale**: every
source issue still reproduces and nothing has been implemented. Six are valid as written, the rest
drifted slightly. Corrections were not applied (§1 of the bulk-design guide forbids editing a
finished design); the implementer applies them in the implementing PR.

| Design | Correction |
|---|---|
| `core-value-enum-oversized` | **Blocks as written.** AC-5 / step 4 assume liquers-py builds unmodified; it now builds `SimpleValue::{Metadata, Recipe, CommandMetadata, Query}` payloads directly (8 sites in `liquers-py/src/value.rs`) and `liquers-web/tests/value_bridge_VALUE.rs:283` does too — add both, reword AC-5 to "the Python and JavaScript APIs are unchanged". Phase 1 says `needs-decision` while the front matter says `ready` (ready is right). Calls `serialize_to_bytes` (the method is `as_bytes`). Its AC-3 requires `core-value-json-identifier-dispatch`. The PyO3 forward-compatibility hint is obsolete. New match sites in both `data_node.rs` files |
| `root-listing-dependency-key-spelling` | Add `restore_or_remove_edge` (`dependencies.rs`) as a normalisation site; add a root case to the ancestor-listing test |
| `environment-manager-reference-cycle` | Counts are now 9 spawn sites, ~86 / 25 envref call sites; step 4's audit must include `MetadataSaver::save_immediately` |
| `core-tracing-migration` | Inventory misses `commands.rs`, `store/realm.rs`, `store_factory.rs`; covers neither `liquers-mcp`, `liquers-template` nor `liquers-agent` (a stdio binary: its subscriber must write to stderr); test name collides with `environ08_*`; re-size the issue to L |
| `context-title-predecessor-inheritance` | Fails `validate_phase.py` Phases 1 and 4 (no AC-n, no checklist); execution site moved to `do_step_state`, which now checks `state.value()?` first |
| `registry-query-default-text-form` | 11 query defaults now (2 template, 9 `ns-mem`), not 2 |
| `cwd-default-link-silent-root` | Its reach grew to the 9 `ns-mem` commands; optionally add an `ns-mem` test case |
| `external-manager-evaluation-session-sharing` | `minimal_manager.rs` already overrides `get_dependency_asset_with_payload` |
| `combined-value-shared-data-node` | "AC-3 gets cheaper" is not so (`ContextHost::input_state` still deep-clones) |
| `metadata-application-attributes` | Obsolete PyO3 override; stale `PY-VALUE-SERIALIZER-IS-A-STUB` remark; AC-3 / `attributes05` must name a real retag path (there is no asset-level one) |
| `serialization-lossy-write-warnings` | Stale liquers-py remark; say that `ExtValue::Foreign` reports nothing |
| `value-serialization-writer` | liquers-py no longer implements the serializer; `ExtValue::write_to` must list `Foreign` explicitly |
| `store-conditional-write` | `agent-memory-mvp` is complete: it is a follow-up consumer (`mem/write.rs` → `set_if`), not an ordering constraint |
| `ui-events` | Front matter: `created` contradicts the body |
| `lib-build-warning-cleanup`, `cwd-less-recipe-analysed-at-root`, `pure-command-context`, `ipc-reader-polars-text-columns`, `search-catalog-single-metadata-read`, `serialized-binary-disposal-policy`, `store-conformance-check-tool`, `template-non-scalar-text-form`, `ui-asset-snapshot-boundary` | None (`pure-command-context` is ~275 characters over the size guide) |

Two recurring decisions span several of these: "now vs the streaming redesign" (lossy Q1 = writer
Q1) and "replace what an accepted issue proposed" (`ui-asset-snapshot-boundary` drops the
`UiAssetAdapter` trait; `store-conformance-check-tool` drops `create_fixture`). Both need an
explicit yes because they contradict a recorded decision.

Defects noticed during review and not yet filed: `PY-VALUE-ENUM-OVERSIZED`'s resolution claims lib
`Value` payloads are behind `Arc` (false until `core-value-enum-oversized` lands);
`ContextHost::input_state` deep-clones a `Value` (minor, possibly intended).

## 10. Project groups

Planned as milestones, without design folders. Each project's own thread designs and implements
them.

### 10.1 liquers-py

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| M1 | API shape | `PY-METADATA-FIELDS-ARE-METHODS`, `PY-REPRS-SHOW-RUST-DEBUG-FORMAT`, `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION` | — | 3 × S, one PR |
| M2 | Declarations and configuration | `PY-COMMAND-DECORATOR-CANNOT-SET-REALM`, `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`, `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` (+ lib `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`) | — | 3 × S |
| M3 | API reference | `PY-API-REFERENCE-NOT-GENERATED` | M1, M2 | M |
| M4 | Matrix to COMPLETE | (core) `environment-manager-reference-cycle`, `key-without-recipe-answer` | core | py side XS |
| M5 | Records reach Python | `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` (copy via IPC) | — | M |
| M6 | Zero-copy Arrow | `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` | M5; a design; `column-slice-shared-buffers` Q1 | L |
| M7 | Async | `PY-ASYNC-EVALUATION-NOT-SUPPORTED` (design `liquers-py-async`, resume at Phase 2) | — | L |

Decisions, with recommended answers: metadata fields become properties **without** a deprecation
period (PyO3 cannot keep both forms under one name; the package is unreleased); reprs as
`Class('<encoded>')` so `eval(repr(x)) == x`; `pdoc` into `target/` with a docstring-coverage test;
`arguments=` as a dict by name plus a `declaration=` escape hatch; a configured recipe choice still
re-appends the manifest provider; Arrow export through the **PyCapsule interface** with `unsafe`
confined to one feature-gated `liquers-records` module (amends "no `unsafe` in liquers-records" —
the maintainer's call); `liquers-py-async` answers as its Phase 1 proposes. No CI job runs the py
test loop: add one before the core value refactors.

### 10.2 liquers-web and language integration

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| W1 | Correct failures and JS stores | `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL` (concurrent-waiter path), `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` (no `finalize_metadata` in `JsStore::get`) | — | S + S |
| W2 | Error transport, web half | `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | W1 | S |
| W3 | Command context | `JS-COMMAND-CANNOT-ACCESS-CONTEXT` (`COMMAND07` is vacuous; README overstates) | — | M |
| W4 | Configuration through core | `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` | — | M |
| W5 | Page-defined store types | `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | W4 | M |
| W6 | Documentation | `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`, `WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE` | W1-W3 for the web half | M + M |
| W7 | Deferred evaluation | `WEB-CANCELLATION-INERT` — a **core** design | wave-1 claim fixes | L |
| W8 | IndexedDB store | `WEB-NATIVE-IO-TIER2` | W4 | L |
| W9 | Core extension points (park) | `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`, `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` | §7.1; a user | L + L |

Decisions, with recommended answers: a JS command receives `context` positionally by the core
naming convention, as a `Context` class mirroring liquers-py; a sync command may call
`context.evaluate` but not await it; deferral is an opt-in mode of `ImmediateAssetManager` used by
`getAsset` only (design now, implement when a fetching command exists); the stack moves to
`traceback`, `jsClass` stays null until §7.1; `registerStoreType(info, create)` with a synchronous
`create`; boxed store-config builders but park the issue; integration docs follow the py
precedent (TypeDoc over the `.d.ts`); `configureStore` stays the only JS entry point; a page record
without `status` reads as `Source`; IndexedDB designed first, built when quota matters. W4 and W5
both touch the rebuild path — in sequence, not in parallel.

### 10.3 Agent memory

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| M1 | Workbench surface | `AGENT-WORKBENCH-SURFACE` (G1 evaluate, G2 validate, G4 receipts) | — | M |
| M2 | Markdown navigation | `NO-MARKDOWN-NAVIGATION-COMMANDS` (first slice) | — | M |
| M3 | Derived memory | `AGENT-MEMORY-HAS-NO-INDEXES-OR-CONTEXT-PACKS` (design `agent-memory-derived`, resume at Phase 2) | `storage-none-computed-values`, `front-matter-yaml-agreement` | L |
| M4 | Migrate | `AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND` | `store-conditional-write` | M |
| M5 | Code navigation | `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` (Rust via `syn`) | — | M |
| M6 | Governance | `MCP-SERVER-APPLIES-NO-ACCESS-POLICY`, `AGENT-CANNOT-PROPOSE-A-COMMAND` | access policy, `pure-command-context`, post-init | L |

Decisions, with recommended answers: until `pure-command-context` lands, the general evaluate tool
uses an allow-list of non-writing namespaces, not `*`; the validate tool returns the existing
`liquers-validate` report with a `schema_version`; receipts read the status before evaluation (no
core change); migrations declared in `_schema/memory.yaml`, dry run by default, after
`store-conditional-write`; `pulldown-cmark` for read-only Markdown navigation in a new `md`
namespace (keep the line scanner for byte-preserving edits); `syn` for Rust only; no Python in the
agent binary for toolsmithing v0; derived indexes live at a memory key, not over `specs/index.csv`;
context packs v1 without code items; `mem` depends on `templates`.

## 11. Issues still without a design

76 open issues have no design. **Every issue that was open at the start and is not a project-group
issue now has a design**: the 19 pre-existing issues without one are all project-group issues
(§10). The other 57 were filed during this pass, after their clusters had been designed, and were
out of its scope by construction; they are the input of the next pass. By kind:

- **Project groups (28)** — the py, web/language and agent issues of §10. Planned, deliberately
  undesigned here.
- **Split-off follow-ups (27, mostly L/XL)** — scope moved out of a design this pass so the design stayed
  bounded, for example `VALUE-NOT-CONVERTED-AUTOMATICALLY`, `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`,
  `STORED-DERIVED-VALUES-ARE-NEVER-PRUNED`, `REMOTE-ASSET-MANAGER-AND-SYSTEM-API`,
  `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT`, `NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE`.
  They wait on their parent design; `--check` warns about them by design (§4.5).
- **Defects to design next (8)** — the P1 and three P2 correctness gaps of §4 (M), and
  `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE`, `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-…`,
  `LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY`, `RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE`.
- **Small fixes (13, all S)** — `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` (P2),
  `RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT`, `SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64`,
  `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED`, `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE`,
  `POLARS-DATAFRAME-JSON-NOT-SERIALIZABLE`, `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS`,
  `DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES`, `WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY`,
  `BUILD-MATRIX-CI-SKIPS-NEWER-CRATES`, `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI`,
  `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI`, `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`.
  Most look auto-fix shaped; each needs a compact design first, which the next compaction run
  (`references/compaction.md` step 4) produces.

## 12. What is not done

- **Nothing implemented**, by design of the request. The 15 eligible designs are candidates for
  automatic fixing, not started.
- **Reproduction is by reading code.** Cargo was run only where a report says so (the flake bisect,
  the inline-without-runtime test, `cargo fmt --check`, `cargo tree`). Three new defects are
  inferred, not executed (the P1 late save with a non-volatile key, the two P2 reload/versioning
  gaps); their designs should start by reproducing them. liquers-web and the wasm32 rows were not
  built.
- **Corrections to finished designs** (§9) are recorded here, not applied.
- **Shared-target incident:** one agent briefly built an old commit into the shared `target/` and
  restored it; any liquers-core test result from 21:35-21:42 on 2026-10-20 inside this session
  should be disregarded. No result in this plan depends on one.
- **Validator false positive** `VALIDATE-PHASE-EMPTY-SECTION-MATCHES-ACROSS-LINES` was hit by
  several agents and worked around by rewording; `validate-phase-section-match` fixes it.
- **README placement.** The 85 new designs appear in the generated "Not yet placed" block of
  `specs/README.md`; placing them in the capability map is left to the PRs that implement them.
- **Not filed, noted only:** `register_command!` stubs named `ns` are accepted by
  `liquers-validate --command ns` (harmless); `assets.rs` uses `assert!(…try_insert_key_asset…)` in
  library code and `egui/widgets.rs` uses `Color32::from_hex(..).unwrap()` (both within
  `library-no-panic-lint`'s scope); about 175 textual `panic!`/`unreachable!` (mostly tests).
