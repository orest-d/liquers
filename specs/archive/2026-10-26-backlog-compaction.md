# Backlog compaction and design pass, 2026-10-26

A backlog compaction (`.claude/skills/liquers-project/references/compaction.md`) combined with a bulk
design pass (`specs/guides/autonomous_bulk_design.md`) over every open issue that had no design. It
was run on the maintainer's request: "compactify issues, bulk design designs for issues lacking
design, evaluate readiness and eligibility for autofixing", and, mid-run, "there have been some more
issues added, please pull and process new issues too". Issues of liquers-py, liquers-web and the
agent-memory system get a milestone plan (§8), not design folders: that work is done in another
project. Nothing was fixed (step 5 of the compaction procedure was not requested). This is a dated
record; it is not edited later. The designs, issues and `specs/index.csv` are the live state.

Baseline: `main` at `cd6d5da`, then `aaebb17` merged in (PR #182, nine new liquers-web issues).
Every "still true" verdict comes from reading the code at that commit, with file and symbol named in
the issue or design. No cargo build or test ran except building `liquers-validate`, which checked
every query in the new designs and issue bodies.

## Contents

1. [Counts](#1-counts)
2. [How the pass was run](#2-how-the-pass-was-run)
3. [Records changed](#3-records-changed)
4. [Look at these first](#4-look-at-these-first)
5. [Ready designs and automatic fixing](#5-ready-designs-and-automatic-fixing)
6. [Decision register](#6-decision-register)
7. [Implementation order](#7-implementation-order)
8. [Project groups](#8-project-groups)
9. [Issues still without a design](#9-issues-still-without-a-design)
10. [What is not done](#10-what-is-not-done)

## 1. Counts

| | Start (`cd6d5da`) | End |
|---|---|---|
| Open issues and features | 208 | 227 (12 filed by this pass, 9 by PR #182, 2 closed by PR #182) |
| …by priority P1 / P2 / P3 | 1 / 63 / 144 | 1 / 68 / 158 |
| …without a design | 90 | 40 (§9) |
| Open designs | 109 | 175 (66 created) |
| …`ready`, `autofix: eligible` | 14 | 33 |
| …`ready`, not eligible | 8 | 9 |
| …`needs-decision` | 81 | 123 |
| …`phase2-blocked` | 4 | 8 |
| …no readiness (other workflows) | 2 | 2 |

The 66 new designs: **20 ready (19 eligible for automatic fixing), 42 needs-decision, 4
phase2-blocked, 0 blocked, 0 covered.** `docs_index.py --check` at the end: 0 errors, 12 warnings.
`validate_phase.py` passes for every phase written (size warnings only; the phase2-blocked designs
have no Phase 3-4 by design).

## 2. How the pass was run

- **Preflight.** 90 open issues had no design. 29 belong to the py/web/agent project groups (§8);
  61 were split into eleven clusters by area and shared change sites: A asset lifecycle, B1 asset
  events and the remote API, B2 storage and memory lifecycle, C recipes, D planner and query, E value
  serialization, F data formats and scalars, G1 command type declarations, G2 enums and realms, H
  errors, I records and SQL, J UI, docs and build.
- **Per cluster** (one agent each): triage at HEAD (still true, partly fixed, wider than filed),
  tighten the issue body (accurate at HEAD, a `## Problem Example` everywhere), overlap under
  `references/overlap.md` against the cluster and every open item, then Phases 1-4 (compact for S/M,
  full for L/XL), readiness and the automatic-fix label with the failing rule named.
- **Second round.** The twelve issues the clusters filed got designs too (clusters K and L), except
  the last one filed (§9).
- **Project groups**: one agent re-triaged the 29 py/web/agent issues and refreshed the 2026-10-20
  milestone plan; after merging main, the nine new liquers-web issues were placed in it.
- Existing designs were not re-reviewed: those with a finished Phase 4 are out of scope by
  procedure, and readiness of the older `needs-decision` designs is unchanged (their decisions are
  in the 2026-10-20 register, §6 of `2026-10-20-backlog-implementation-plan.md`). Two new designs
  were edited after creation by the second round (`sql-several-tables`, `replaced-asset-write-gate`,
  dependency lines only).

## 3. Records changed

### Proposed close (status left for the maintainer)

| Issue | Evidence |
|---|---|
| `EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD` | Fixed before it was filed: `Context::wait_for_dependency_recording` copies a non-keyed dependency's records into the waiting asset (`91f1810`, 2026-10-17, `search-read-dependencies`). A triage note and a suggested regression test were added to the issue |

### Merges (strong overlap, one design owning several sources)

| Design | Sources | Test |
|---|---|---|
| `recipe-list-set-cwd-validation` | `RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE`, `RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING` | T3, both rewrite `RecipeList::set_cwd`; both eligible, so E1 does not apply |
| `error-type-taxonomy` | `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED`, `LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY` | T2 + T3, the constructors and the `SerializationError` split are the taxonomy's decisions |
| `generic-serde-data-formats` | `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS`, `NO-COMPACT-BINARY-DATA-FORMAT` | T2 + T4 |

Kept apart on purpose: the G1 trio (`requires` chain; merging would be XL, E2), the two
`FOREIGN-VALUE-*` items (E1), `PLANNER-INSTRUCTION-NAMES-DUPLICATED` versus
`reserved-instruction-command-names` (E1, finished Phase 4), the two CI items (different workflow
files), `LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL` versus `replaced-asset-write-gate`
(E1). No merge of existing designs is proposed.

### Issue corrections

Every designed issue now has `design:` and a `## Problem Example` checked at HEAD. Beyond that:

- **Retitled:** `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` (narrowed to Decimal; UUID/time/interval split
  off), `VALUE-NOT-CONVERTED-AUTOMATICALLY` (state and link arguments only; UI and HTTP split off),
  `RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT` example (only `-R-sdir/` prints).
- **Wider than filed:** `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED` (no edge from a key to its own
  recipe; a restart serves the stale copy), `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE` (`+HH:MM` parse is a
  stub, short negative offsets lose their sign, unknown zones accepted silently),
  `QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII` (five productions, not four),
  `NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS` (four divergences).
- **Narrower than filed:** `CASCADE-TO-AN-IN-FLIGHT-DEPENDENT-IS-DROPPED` (its original example is
  caught once `runtime-dependency-record-superseded-check` lands), `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`
  (web half only), `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL` (concurrent waiter only),
  `AGENT-WORKBENCH-SURFACE`, `PY-METADATA-FIELDS-ARE-METHODS` (only `Metadata` is inconsistent).
- **Recounted:** `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` (27 variants; ~636 `General` sites
  outside tests: lib 221, records 205, core 145, template 18, web 15, mcp 8, py 8, agent 3, store 2),
  `LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY` (19 real sites, not ~50).
- **Complexity:** S→M `CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER`,
  `LIB-RECIPE-PROVIDERS-HAVE-NO-VALIDATION-CLI`, `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`,
  `PLAN-EXPIRATION-REACHES-THE-CONTEXT-ONLY-THROUGH-APPLY-RECIPE`; S→L
  `PLANNER-INSTRUCTION-NAMES-DUPLICATED`; M→S `EXPIRES-TEXT-FORM-DROPS-TIME-ZONE`,
  `PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT`, `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`,
  `WEB-HAS-NO-TEMPLATE-COMMANDS`; L→M `STORE-WHOLE-WRITE-COPIES-CALLER-BYTES`,
  `UNKEYED-ASSET-HAS-NO-VERSION`; M→L `RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL`. No priority
  changed. Suggested, not applied: raise `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS` to P2 (a
  prerequisite of the P2 remote asset manager).

### Issues filed by this pass (12, draft)

| Issue | P | C | Design |
|---|---|---|---|
| `LATE-SAVE-OF-AN-UNREGISTERED-KEYED-ASSET-SURVIVES-REMOVAL` | P2 | M | `unregistered-keyed-write-gate` |
| `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI` | P2 | S | project groups (§8) |
| `SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT` | P3 | S | `soft-consistency-refined-format` |
| `RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-POLARS-ACCEPTS` | P3 | S | `record-table-csv-refinements` |
| `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY` | P3 | S | `template-diagnostics-namespace-scope` |
| `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE` | P3 | S | `variadic-key-query-link-text` |
| `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES` | P3 | S | none yet (§9) |
| `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` | P3 | M | `ui-display-conversion` |
| `UNKEYED-ASSET-HAS-NO-VERSION` | P3 | M | `unkeyed-asset-derived-version` |
| `HTTP-ACCEPT-HEADER-NOT-NEGOTIATED` | P3 | L | `http-accept-negotiation` |
| `RECORD-FIELD-TYPE-HAS-NO-UUID-TIME-OR-INTERVAL` | P3 | L | `record-uuid-time-duration-types` |
| `EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES` | P3 | M | project groups (§8) |

## 4. Look at these first

1. **P1 `replaced-asset-write-gate`** (`REMOVED-OR-REPLACED-KEY-REVIVED-BY-A-LATE-BACKGROUND-SAVE`)
   is ready and passes every automatic-fix rule, but it is P1, so it needs the maintainer's go-ahead.
   Its unregistered-asset sibling `unregistered-keyed-write-gate` (P2) lands after it.
2. **A deliberate behaviour would be reversed.** `VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE`
   is pinned by a test and by `plan-relative-resolution` ("not silently stringified"). Its design
   proposes stringifying Key and Query values only; `sql-several-tables` now requires that decision.
3. **Encode failures are silent today.** `classify_persistence_error` turns every
   `SerializationError` into a silent `NonSerializable`. `error-type-taxonomy` decision 3 fixes it.
4. **`error-type-taxonomy` (P2 L)** is the error-schema decision the web traceback and store-config
   items have parked on (§8, W13).
5. **G1 contract.** `command-io-type-declarations` is the root of `automatic-value-conversion`,
   `command-specialization-by-input-type`, `ui-display-conversion`, `http-accept-negotiation` and
   (via `produces`) `cross-realm-transfer-planning`. One decision (where declarations live) unlocks
   five designs.
6. **Data inconsistency, not fixed:** `AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND` and
   `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` carry `design:` pointing at complete designs
   (`agent-memory-mvp`, `record-streams`) that do not implement them. Each needs its own slug when
   its project designs it.

## 5. Ready designs and automatic fixing

### New, ready and eligible (19)

| Design | Source | P | Crate | What it does |
|---|---|---|---|---|
| `replaced-asset-write-gate` | REMOVED-OR-REPLACED-KEY-REVIVED-… | **P1** | core | Value-write gate in `MetadataSaver`; replacement closes the saver of finished assets too. **Needs authorization (P1)** |
| `query-token-classes-ascii` | QUERY-TOKEN-CLASSES-TRUNCATE-NON-ASCII | P2 | core | Five `c as u8` productions become ASCII predicates |
| `recipes-yaml-read-errors-propagate` | RECIPES-YAML-READ-ERRORS-READ-AS-NO-RECIPES | P2 | core | Only KeyNotFound/KeyNotSupported read as "no recipes" |
| `recipe-list-set-cwd-validation` | RECIPE-CWD-ERROR-… + RECIPE-LIST-SET-CWD-… | P3 | core | Validate the whole list first; error names each `recipes[i]` |
| `mid-query-q-arity` | MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS | P3 | core | `check_q_arity` in both branches |
| `inherited-volatility-bounds-predecessor-cut` | DEPENDENCY-VOLATILITY-INVISIBLE-TO-PREDECESSOR-WALK | P3 | core | Predecessor cut never spans a step reading a volatile upstream |
| `resource-directory-step-debug-output-removal` | RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT | P3 | core | Delete two `eprintln!`, add a test |
| `plan-serialization-internal-contract` | PLAN-SERDE-HAS-NO-COMPATIBILITY-CONTRACT | P3 | docs | Serialized `Plan` is a same-build inspection format (relies on the no-backward-compatibility policy) |
| `soft-consistency-refined-format` | SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT | P3 | core | Look up media type by `data_format_base` |
| `doc-03-apply-entry-points` | DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES | P3 | docs | Correct the entry-point table |
| `next-presets-follow-planner-rules` | NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS | P3 | lib | `find_next_presets` resolves the last command as the planner does |
| `foreign-value-text-json-delegation` | FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED | P3 | lib | `Foreign` arms call the trait hooks |
| `simple-value-i32-json-read` | SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64 | P3 | lib | Strict `I32` arm in the JSON read |
| `type-identified-in-docs` | TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS | P3 | docs | Docs say no type implements it yet |
| `record-table-csv-refinements` | RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-… | P3 | records | Accept `csv:semicolon`, `csv:pipe`, `csv:<c>` |
| `template-diagnostics-namespace-scope` | TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-… | P3 | template | Per-segment namespace scope |
| `web-api-spec-read-only-mount` | WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY | P3 | docs | Replace the stale sentence |
| `build-matrix-path-filters` | BUILD-MATRIX-CI-SKIPS-NEWER-CRATES | P3 | CI | `'liquers-*/**'` path filters |
| `docs-tooling-tests-in-ci` | DOCS-TOOLING-TESTS-NOT-RUN-IN-CI | P3 | CI | Run the docs-index and skill-script tests |

### New, ready, not eligible (1)

`foreign-value-data-node` (P2): adds a `pub` default method `ForeignValue::data_node` (rule 4).

### Earlier ready and eligible, still not implemented (14)

`cancel-point-of-no-return`, `core-value-json-identifier-dispatch`, `core-wasm-warning-free`,
`expires-cross-timezone-reduction`, `front-matter-yaml-agreement` (open PR #173),
`inline-metadata-save-without-runtime`, `inline-run-claim-repair-under-lock-contention`,
`phase5-skeleton-unticked`, `reserved-instruction-command-names`,
`runtime-dependency-record-superseded-check`, `state-lock-marker-cleanup`,
`temporary-asset-unique-id`, `uncached-write-back-rechecks-dependency-versions`,
`validate-phase-section-match`. Checked against `git log` since 2026-10-20: none has landed.

### One decision from eligible

`override-ignores-recipe-deadline`, `unnamed-recipe-diagnostics`, `plan-expiration-set-at-finalization`
and `variadic-key-query-link-text` become eligible once their decisions (§6) are taken as
recommended.

## 6. Decision register

Every design below has Phases 1-4 written against the **bold** answer; approving it makes the design
ready. Decisions on designs older than this pass are in the 2026-10-20 register.

### Asset lifecycle

- **`cascade-marks-in-flight-dependent-stale`** — a running dependent reached by a cascade: **takes
  the stale-dependency route (Expired at birth)**, honoured only if the run recorded `via`.
- **`recipe-dependency-versions`** — version over **every declared recipe field except the circular
  fields** (a title edit recomputes); **a removed recipe expires the key**; **each keyed asset records
  its own recipe dependency**; **keep the per-write scan**, add an index only on a >10% regression.
- **`override-ignores-recipe-deadline`** — a recipe deadline **does not apply to an `Override`**.
- **`expires-text-form-time-zone`** — an unknown zone token **is an error**; canonical spelling
  `UTC` or `±HH:MM`; keep the `tz_offset` fields.
- **`unregistered-keyed-write-gate`** — a key mutation **stops only the late write**, not the
  evaluation; reach unregistered assets through **a new `AssetManager::unregistered_key_writers`
  with an empty default**.
- **`asset-event-kinds`** — **no `ListingChanged`**; `Updated` carries `origin`; `Removed` only on a
  real delete; release together with `external-index-sync`.
- **`asset-events-over-websocket`** — **off by default** (`with_event_stream(true)`); a separate
  `ws/events`; one scope per connection; clients ignore unknown `type`s.
- **`remote-asset-manager`** (phase2-blocked) — **blocking: the server owns every key under a
  delegated prefix**; no system API in the first slice; in-memory mirror only; keys only.
- **`registered-asset-protection`** (phase2-blocked) — **build nothing until a consumer exists**,
  then prefer a key-scoped subscription; refuse `remove`/`set_state` with `StatusConflict`.
- **`stored-derived-value-pruning`** — last use = **record `updated` time + not registered**; keep
  the record; orphans **reported, removed only with `remove_orphans`**; explicit pruning only.
- **`streamed-keyed-persistence`** — **readable after the streamed write and its hash complete**.
- **`store-shared-buffer-write`** — **a one-chunk `set_stream`** kept by the memory store.
- **`value-memory-size-budget`** — unknown size **counts as 0**, reported; per-asset counting; names
  `estimated_memory_size`, `max_cached_key_bytes`, `max_cached_query_bytes`.
- **`unkeyed-asset-derived-version`** — **computed after evaluation from the recorded
  dependencies**; not distinguishable from other versions; unversioned commands left out.

### Recipes

- **`recipe-circular-dependency-fields-removal`** — **remove** `has_circular_dependencies` /
  `circular_dependency_key` (they came from a maintainer review comment, so this is your call).
- **`unnamed-recipe-diagnostics`** — **keep valid entries and report the rest**; report through
  **the validator and the provider's not-found errors** (no API change); severity **Error**.
- **`kind-aware-recipe-provider-choice`** — **a defaulted `AssetManagerKind::recipe_provider_for`
  hook**; `trivial` under `LibKind` = **no recipes at all**. Unblocks `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`.
- **`records-recipe-validation-cli`** — **host in `liquers-records`** with the committed registry;
  binary `liquers-validate-records`. Requires `validate-through-recipe-providers`.

### Planner and query

- **`plan-expiration-set-at-finalization`** — apply the expiration **inside
  `finalize_plan_expanded`**. Then eligible.
- **`dependency-summary-reuse`** — **a crate-private map in `DependencyManager`**; accept a stale
  summary on a recipe edit until `recipe-dependency-versions` lands.
- **`planner-instruction-names-public`** — **`PLANNER_INSTRUCTIONS` + `is_planner_instruction` +
  `ActionRequest::is_planner_instruction` / `is_v`** in `liquers_core::query`.
- **`variadic-key-query-link-text`** — **reverse "not silently stringified" for Key and Query
  only**, as a fallback inside `materialize_link_json`. Then eligible.

### Values and formats

- **`polars-dataframe-json`** — **implement** JSON/NDJSON (not remove the variants); **one object
  per row**; spellings `json`, `ndjson`, `jsonl`.
- **`record-schema-attribute-on-write`** — **a separate `serialization_attributes` hook**; it
  **overwrites** an existing `records.schema`; only formats with no embedded schema.
- **`query-data-format-command`** — **`data_format`, root namespace**; scope **the whole result**
  (must come last); validated **when the command runs**.
- **`generic-serde-data-formats`** — **YAML, CBOR, MessagePack; not TOML**; **CBOR** the
  recommended compact format; core features `cbor` / `msgpack` **off by default**, lib
  `serde-binary` **on**; payload read by identifier.
- **`extended-scalar-value-types`** — `u64`/`i128`/`u128`/decimal **as JSON strings**, ISO 8601
  temporals; **`rust_decimal`**; **one `ext-scalars` feature, on**; add `I8…Uuid` to the closed
  bare-identifier set.
- **`record-decimal-field-type`** — **Decimal only**; JSON cell **as a string**; browser
  descriptor `"Decimal128"`; precision 1-38, never rounded.
- **`record-uuid-time-duration-types`** — **`Uuid`, `Time`, `Duration` as types**, interval and
  zone-less timestamp as `Text`; forms as `extended-scalar-value-types`; UUID via `arrow.uuid` /
  Parquet `UUID`.
- **`error-type-taxonomy`** — (1) **add 8 types** `ItemNotFound`, `OutOfRange`, `InvalidData`,
  `FormatNotSupported`, `RecipeNotFound`, `DependencyFailed`, `AlreadyExists`, `PayloadMissing`;
  (2) **add 5 categories** `InvalidInput`, `Unsupported`, `Conflict`, `Dependency`, `Data`;
  (3) **only `FormatNotSupported` is a silent `NonSerializable`**, a persisting `SerializationError`
  is a logged `NotPersisted`; (4) HTTP **422/415/404/424/409/400** as tabled; (5) **keep `General`
  for foreign code, guarded by a CI script**; (6) **`Error::new` private**; (7) remove
  `CacheNotSupported`.

### Command types, enums and realms

- **`command-io-type-declarations`** — **`accepts` on `ArgumentInfo` + `produces` on
  `CommandMetadata`** (not a new `ArgumentType` variant); an ordered identifier list, first is
  principal; a mismatch is **a plan warning only**. Unlocks five designs (§4.5).
- **`automatic-value-conversion`** — a refused or failed conversion **passes the value through with
  a warning** (declarations are a hint, not a guarantee); state and link arguments only.
- **`command-specialization-by-input-type`** — **an explicit generic entry listing variants**;
  chosen **at plan time when decidable, else by `Step::Dispatch`**; principal type first; ship only
  the root `head`.
- **`ui-display-conversion`** — **no "table" purpose**: the UI lists the identifiers it renders and
  asks for an automatic edge to one.
- **`http-accept-negotiation`** — `Accept` overrides **only when the value's own media type is not
  acceptable**; a query filename wins; `406` as an axum `NotAcceptable` response.
- **`enum-alternative-labels`** — macro syntax **`"alias" (label: …, description: …)`**; MCP
  **`oneOf`/`const`/`title` only when labelled**; a label counts in `metadata_version`.
- **`computed-enum-alternatives`** — **check only, never rewrite**; a failing provider **warns and
  accepts**; environment scope at plan time, input scope before execution; global enums only.
- **`registry-publication`** — **resolve global enums inline on export**; the route answers **the
  bare document**; the importer names the realm; an occupied realm is refused.
- **`cross-realm-transfer-planning`** (phase2-blocked) — blocked on `multi-realm-dispatch` Q1 (the
  dispatch unit).

### Records, SQL and indexes

- **`external-index-reconciliation`** — projection identity **on the sink**; no content policy in
  slice 1; versionless chunks **re-fed by time**; first deliverable **the contract + `MemorySink`**.
- **`record-source-row-streaming`** — **an encoder registered in `CommandRegistry`**; the source
  with a row filename streams; csv/tsv/ndjson/jsonl only; whole-byte callers keep the refusal.
- **`sql-several-tables`** — **explicit `name, link` pairs** on `ns-pl/sql`. Requires
  `variadic-key-query-link-text`.
- **`relational-database-writes`** (phase2-blocked) — **blocking: a mirror sink only, no write
  commands**; database identity inherited from `relational-database-access` Q1.

### UI, templates

- **`command-parameter-editor`** — **a toggled Parameters panel in `QueryConsoleElement`**; the
  widget comes **from the argument type** when `gui_info` is the default (410 of 423 entries);
  `set_argument_parameter` **in core**; an edit **re-encodes the whole query**.
- **`web-query-editor-live-preview`** — **patch the DOM from an `input` listener**; no debounce.
- **`template-render-in-env`** — **one function using existing types**; no state-plus-data form;
  warnings **as `LogEntry`**.

## 7. Implementation order

**Wave 1 — eligible, P2/P3, independent PRs** (automatic fixing on the maintainer's go-ahead):

- core planner, one implementer in sequence (same files): `mid-query-q-arity` →
  `inherited-volatility-bounds-predecessor-cut` → `resource-directory-step-debug-output-removal`;
  `query-token-classes-ascii` separately (`parse.rs`).
- core recipes: `recipes-yaml-read-errors-propagate`, `recipe-list-set-cwd-validation` (different
  functions of `recipes.rs`, can run in parallel).
- core assets: `soft-consistency-refined-format`.
- lib values: `foreign-value-text-json-delegation`, `simple-value-i32-json-read`,
  `next-presets-follow-planner-rules`.
- records `record-table-csv-refinements`; template `template-diagnostics-namespace-scope`.
- One docs PR: `doc-03-apply-entry-points`, `web-api-spec-read-only-mount`, `type-identified-in-docs`,
  `plan-serialization-internal-contract`.
- One CI PR: `build-matrix-path-filters`, `docs-tooling-tests-in-ci` (with the 2026-10-20 docs-tooling
  batch: `validate-phase-section-match`, `phase5-skeleton-unticked`, `front-matter-yaml-agreement`).
- The 14 earlier eligible designs (§5), in the 2026-10-20 wave-1 order.

**Authorize separately:** `replaced-asset-write-gate` (P1), then `unregistered-keyed-write-gate`.

**Wave 2 — ready, not eligible:** `foreign-value-data-node`, plus the nine from 2026-10-20.

**Wave 3 — after decisions, ordered by what they unlock:** `error-type-taxonomy` (6 PRs; Stage 4
regenerates the command registry); `command-io-type-declarations` → `automatic-value-conversion` /
`command-specialization-by-input-type` / `http-accept-negotiation` steps 1-4;
`recipe-dependency-versions` → `dependency-summary-reuse`; `kind-aware-recipe-provider-choice` (frees
py P3); `variadic-key-query-link-text` → `sql-several-tables`; `generic-serde-data-formats`;
`expires-text-form-time-zone` after `expires-cross-timezone-reduction`.

**Waits on unimplemented designs:** `asset-event-kinds` / `asset-events-over-websocket` on
`external-index-sync`; `stored-derived-value-pruning` on `assets-api-admin-operations`;
`streamed-keyed-persistence`, `store-shared-buffer-write`, `record-source-row-streaming` on the
streaming designs; `value-memory-size-budget` part B on `asset-garbage-collection`;
`extended-scalar-value-types` on `value-type-definition-macro`, `value-conversion-registry`,
`value-interface-capability-split`; `records-recipe-validation-cli` on
`validate-through-recipe-providers`; `web-query-editor-live-preview` on `query-editor-live-validation`.

## 8. Project groups

Planned as milestones, without design folders; each project's own thread designs and implements
them. Successor to §10 of `2026-10-20-backlog-implementation-plan.md`. Triage at `cd6d5da`; nothing to
close.

**Done since 2026-10-20:** py M4 (CONFORMANT baseline, 53a1768); `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE`
(83644c1); `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` and `WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE`
(PR #182). No core design these plans wait on has been implemented.

### 8.1 liquers-py

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| P1 | CI for the binding loops | `LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI` (new; shared with web) | — | S |
| P2 | API shape | `PY-METADATA-FIELDS-ARE-METHODS`, `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION`, `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | — | 3 × S, one PR |
| P3 | Declarations and configuration | `PY-COMMAND-DECORATOR-CANNOT-SET-REALM`, `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`, `PY-RECIPE-PROVIDER-NOT-CONFIGURABLE` | recipe provider: `kind-aware-recipe-provider-choice` | 3 × S |
| P4 | API reference | `PY-API-REFERENCE-NOT-GENERATED` | P2, P3 | M |
| P5 | Records reach Python | `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` (Arrow IPC + `pyarrow`) | — | M |
| P6 | Zero-copy Arrow | `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` | P5; its own design slug | L |
| P7 | Async | `PY-ASYNC-EVALUATION-NOT-SUPPORTED` (`liquers-py-async`, open PR #181) | — | L |
| P8 | Follow core changes | `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS`, `QUERY-BUILDER-NOT-IN-BINDINGS`, `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`, `EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES` (new) | `global-enum-registration`, `query-builder`, `post-init-command-registration`, `extended-scalar-value-types` | S each (py half) |
| P9 | Error taxonomy mirror | (core) `error-type-taxonomy` | P1 | XS–S per core PR |

Decisions, recommended: metadata fields become **properties now, no deprecation**; reprs as
**`Class('<encoded>')`**; a failing property **raises the typed `LiquersError`**; `arguments=` as **a
dict by name** plus a `declaration=` escape hatch; **add `realm=`** to the decorators; the configured
recipe choice keeps the manifest provider **through `kind-aware-recipe-provider-choice`**; **`pdoc`**
into `target/` with a docstring-coverage test; records cross as **`pyarrow.RecordBatch` / an
iterator of batches**; zero-copy through the **PyCapsule interface** with `unsafe` confined to one
feature-gated `liquers-records` module (amends "no `unsafe` in liquers-records", your call).

### 8.2 liquers-web and language integration

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| W1 | Correct failures and bridge values | `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL` (concurrent waiter), new: `WEB-NESTED-NULL-CROSSES-AS-UNDEFINED` (P2), `WEB-NESTED-BYTES-REFUSED-NOT-DEGRADED`, `WEB-METADATA-STATUS-CASE-DIFFERS` | — | 4 × S |
| W1b | Command API addressing | new: `WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE` (P2), `WEB-DESCRIBE-COMMAND-SHAPE-DIFFERS-FROM-DECLARATION` (P2), `WEB-UNREGISTER-DEPENDS-ON-FIRST-EVALUATION` | the unregister rule shares the rebuild path with W8 | 3 × S |
| W1c | Minimal JS store works | new: `WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED` (P2) | — | S |
| W2 | Error transport, web half | `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (now web-only, S) | W1 | S |
| W3 | Command context | `JS-COMMAND-CANNOT-ACCESS-CONTEXT` | — | M |
| W4 | Template commands | `WEB-HAS-NO-TEMPLATE-COMMANDS` | in-command rendering after W3 | S |
| W5 | Configuration through core | `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG`, new: `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` (P2) | — | M + M |
| W6 | Page-defined store types | `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | W5 (same rebuild path, in sequence) | M |
| W7 | API reference | new: `WEB-API-REFERENCE-NOT-GENERATED` (TypeDoc + export coverage) | — | M |
| W8 | Follow core changes | `GLOBAL-ENUMS-…`, `QUERY-BUILDER-NOT-IN-BINDINGS`, `LANGUAGE-INTEGRATIONS-REBUILD-…` | core designs as in P8 | S + S + M |
| W9 | Remote server store | `WEB-HAS-NO-READ-WRITE-REMOTE-STORE` | `remote-store-and-asset-manager` slice 1, `registry-publication`, `access-policy-at-entry-points` | M |
| W10 | Deferred evaluation | `WEB-CANCELLATION-INERT` (a core design; area now `core/assets, web`) | — | L |
| W11 | IndexedDB store | `WEB-NATIVE-IO-TIER2` | W5 | L |
| W12 | SQL in wasm (park) | `NO-SQL-ENGINE-IN-WASM-BUILDS` | a user | L |
| W13 | Core extension points (park) | `MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING`, `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` | `error-type-taxonomy`; a user | L + L |

Decisions, recommended: a JS command gets **`context` positionally** as a `Context` class mirroring
`lq.Context`; a sync command may call `context.evaluate` but cannot await it; the JS stack goes to
**`Error::traceback`**; `templates` **opt-in** in liquers-web; **retain the whole
`EnvironmentConfig`** on migration; **`registerStoreType(info, create)`** with a synchronous
`create`; deferral as **an opt-in mode of `ImmediateAssetManager` used by `getAsset`**; park
store-config handlers and wasm SQL; **one rule for unregistering a built-in** (refuse it on both
paths; check what liquers-py does first); a minimal JS store **derives `contains` from `get` and
answers `isDir` as false**; store configuration on an instance `Environment` **is refused with
`not_supported`** until instances can rebuild.

### 8.3 Agent memory

| # | Milestone | Issues | Depends on | Size |
|---|---|---|---|---|
| A1 | Workbench surface | `AGENT-WORKBENCH-SURFACE` (validate, receipts, tool set; evaluate-by-allow-list exists) | — | M |
| A2 | Markdown navigation | `NO-MARKDOWN-NAVIGATION-COMMANDS` (first slice) | — | M of L |
| A3 | Derived memory | `AGENT-MEMORY-HAS-NO-INDEXES-OR-CONTEXT-PACKS` (`agent-memory-derived`) | `storage-none-computed-values`, `front-matter-yaml-agreement` | L |
| A4 | Migrate | `AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND` (needs its own slug) | `store-conditional-write` | M |
| A5 | Code navigation | `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` (Rust via `syn`) | — | M of L |
| A6 | Access policy on MCP | `MCP-SERVER-APPLIES-NO-ACCESS-POLICY` | `access-policy-at-entry-points` | M |
| A7 | Toolsmithing v0 | `AGENT-CANNOT-PROPOSE-A-COMMAND` | `post-init-command-registration`, `pure-command-context`, A1 | L |

Decisions, recommended (unchanged from 2026-10-20 apart from A6): an allow-list for the general
evaluate tool until `pure-command-context`; the validate tool returns the `liquers-validate` report
with a `schema_version`; receipts read the status before evaluation; `pulldown-cmark` in a new `md`
namespace; `syn` for Rust only; migrations declared in `_schema/memory.yaml`, dry run by default; no
Python in the agent binary for toolsmithing v0; **keep both** the tool allow-list and the access
policy on the MCP server.

## 9. Issues still without a design

40 open issues. **38 are project-group issues** (§8), deliberately planned rather than designed
here. The other two:

- `EXPRESSION-DEPENDENCY-RECORD-IS-LOST-ON-RELOAD` — proposed close (§3).
- `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES` (P3 S) — filed by the last cluster of this pass
  after the design rounds closed; input of the next pass.

## 10. What is not done

- **Nothing implemented** and no fix started: the 19 eligible new designs are candidates, not work in
  progress.
- **Reproduction is by reading code.** The P1 late-save defect and the deliberate variadic failure
  were established from code and existing tests, not by running them.
- **Older designs were not re-reviewed.** Their readiness is as recorded on 2026-10-20; none of the
  14 eligible ones has landed (`git log`).
- **The nine new liquers-web issues** were placed in the plan from their bodies (filed the same day
  with measured evidence), not re-triaged against code.
- **README placement.** The new designs sit in the generated "Not yet placed" block of
  `specs/README.md`.
