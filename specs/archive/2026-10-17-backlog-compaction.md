# Backlog compaction, 2026-10-17

The second run under `.claude/skills/liquers-project/references/compaction.md`, run interactively
on the maintainer's request ("compactify issues"), fix cap 5. Its main work was closing out designs
that had shipped but never been closed, and designing every `S` issue that had no design. Dated
record; not edited later.

## Counts

| | Start | End |
|---|---|---|
| Open issues and features | 125 | 127 (2 filed this run) |
| …without a design | 98 | 80 |
| …`S` without a design | 20 | 1 (`TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS`, folded into `DATA-FORMAT-CONSTANTS-AND-TOOLING` by decision D8 of 2026-10-08) |
| Open designs | 68 | 33 |
| …`ready` / `needs-decision` / `covered` / `phase2-blocked` | 35 / 2 / 14 / 1 | 14 / 9 / 0 / 2 |
| …`autofix: eligible` and not yet implemented | 1 | 9 |

`docs_index.py --check`: 0 errors before and after; 36 warnings, all pre-existing (`L` issues
without a design, overdue reviews, designs without acceptance scenarios).

## Close-out of finished work (new step this run)

Most of the open-design count was work that had already shipped. The procedure had no step for it,
so this run added one.

- **Complete (21):** designs whose implementing PRs merged (checked on GitHub) and whose source
  issues are closed, with no phase owed: `query-leading-slash-field` (#119),
  `command-registry-impl-version-freshness` (#94), `web-liquers-error-constructor` (#123),
  `axum-store-keys-deep` (#109), `axum-store-makedir-test-unignore` (#105),
  `axum-store-upload-metadata` (#108), `command-metadata-descriptions-and-hints` (#116),
  `configuration-error-kind` (#117), `external-manager-replacement-surface` (#118),
  `json-orient-index-column-collision` (#107), `metadata-error-traceback` (#113),
  `ordered-json-orient-column-order` (#90), `pyo3-python-3-13-support` (#110),
  `record-timestamp-utc` (#121), `register-command-option-value` (#124),
  `simple-value-untyped-and-scalar-reads` (#91), `state-argument-serde-default` (#114),
  `store-conformance-warning-cleanup` (#106), `store-guide-status-table` (#120),
  `store-key-format-seeding` (#122), `expired-binary-read-safety` (#22).
- **Complete (14 coverage records):** `COVERED-01`…`COVERED-14`, whose covered issue is closed.
- **Abandoned (1):** `expiration-integration-suite-failing-at-head`; its issue was closed at triage
  on 2026-08-12 because the suite already passed.
- **Complete after Phase 5 approval (18):** five-phase designs with merged code, closed sources
  and a written Phase 5, approved by the maintainer during this run (decision D1): `opendal-derived-store-arguments`,
  `asset-cancellation-outcome`, `command-cache-flag`, `csv-physical-lines-short-rows`,
  `js-store-effective-media-type`, `json-table-column-order`, `markdown-empty-text-and-tables`,
  `null-cell-string-option`, `rec-id-iso-date-parsing`, `simple-value-serializer-parity`,
  `text-value-markdown-format`, `web-object06-error-type-exhaustiveness`,
  `evaluate-path-consolidation`, `stale-dependency-status-finalization`,
  `foreign-value-type-registration`, `asset-manager-insert-key-asset-semantics`, `plan-cwd-freeze`,
  `context-title-description`. `evaluate-path-consolidation` still lists `ASSETS-FIX1` and
  `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`, which stay open on their own.

## Designs created (step 4)

Every `S` issue without a design now has a compact design (19 designs for 19 issues, Phases 1-4 unless blocked).
None was already fixed at HEAD, and none strongly overlapped an open design.

| Design | Source | P | Readiness | Auto-fix |
|---|---|---|---|---|
| `axum-query-api-base-slash` | `AXUM-QUERY-API-QUERY-KEEPS-LEADING-SLASH` + `AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR` (filed this run, merged: T2) | P3 | ready | eligible |
| `build-matrix-disk-budget` | `BUILD-MATRIX-EXHAUSTS-SESSION-DISK` | P3 | ready | eligible |
| `command-registry-header-feature-names` | `COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES` | P3 | ready | eligible |
| `step-volatility-impl-removal` | `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED` | P3 | ready | eligible |
| `variadic-empty-element-conversion` | `VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES` | P3 | ready | eligible |
| `dependent-asset-weak-reference-pruning` | `DEPENDENT-ASSET-WEAK-REFERENCES-ARE-NEVER-PRUNED` | P3 | ready | eligible |
| `replacing-write-drops-stale-dependency-edges` | `REPLACING-WRITE-KEEPS-DROPPED-DEPENDENCY-EDGES` | P3 | ready | eligible |
| `new-folder-write-ancestor-listing-refresh` | `NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING` | P3 | ready | eligible |
| `root-listing-dependency-key-spelling` | `ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS` | P3 | ready | not (rule 4: stored record string) |
| `lib-build-warning-cleanup` | `LIB-BUILD-EMITS-UNUSED-AND-UNHANDLED-RESULT-WARNINGS` | P3 | ready | not (rule 4: two `version: auto` commands change) |
| `py-value-arc-payloads` | `PY-VALUE-ENUM-OVERSIZED` (re-sized M) | P3 | ready | not (rules 2, 4) |
| `registry-query-default-text-form` | `REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT` | P3 | needs-decision | not (rule 4) |
| `cwd-less-recipe-analysed-at-root` | `DEPENDENCY-ANALYSIS-USES-CALLER-CWD-FOR-A-CWD-LESS-RECIPE` | P3 | needs-decision | not (rule 5) |
| `cwd-default-link-silent-root` | `CWD-DEFAULT-LINK-WARNS-WITHOUT-WORKING-DIRECTORY` | P3 | needs-decision | not (rule 5) |
| `external-manager-evaluation-session-sharing` | `EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION` | P3 | needs-decision | not (rule 4) |
| `search-catalog-single-metadata-read` | `SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE` | P3 | needs-decision | not (rules 4, 6) |
| `combined-value-shared-data-node` | `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE` | P3 | needs-decision | not (rules 3, 4, 6) |
| `template-non-scalar-text-form` | `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM` (re-sized M) | P3 | needs-decision | not (rules 4, 6) |
| `evaluation-session-volatile-test-flake` | `EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT` | P2 | phase2-blocked | not |

Findings from the designs, recorded in them:
- `lib-build-warning-cleanup`: two of the warnings are real bugs. The egui dataframe table never
  sorts (columns taken from the unsorted frame), and `img/thumb` ignores its `method` filter.
- `variadic-empty-element-conversion`: the cause is in the planner (`ParameterValue::pop_value`
  converts an empty element with the whole list's `ArgumentInfo`), so every non-string `multiple`
  argument rejects an empty slot. No registered command is affected yet.
- `command-registry-header-feature-names`: `template` vs `templates` is wrong the same way as
  `image` vs `image-support`.
- `evaluation-session-volatile-test-flake`: the issue's suggested cause contradicts the code (each
  request already builds a fresh session); two hypotheses and a diagnosis command are recorded.
  Re-check on top of open PR #142.

**Filed:** `AXUM-API-BASE-WITH-TRAILING-SLASH-DOUBLES-THE-SEPARATOR` (P3 S, attached to
`axum-query-api-base-slash`), `EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES` (P3 M, no design: the
evaluation-path half that `replacing-write-drops-stale-dependency-edges` scopes out).

## Overlaps and merges (step 3)

- **Merged:** the two axum base-path sources above (T2, shared contract; both eligible, one crate).
- **Weak, linked:** `py-value-arc-payloads` with `core-value-enum-oversized` (same remedy, different
  files); `template-non-scalar-text-form` with `combined-value-shared-data-node` (same `SimpleValue`
  `DataNode` impls); `new-folder-write-ancestor-listing-refresh` with
  `root-listing-dependency-key-spelling`; `cwd-less-recipe-analysed-at-root` and
  `evaluation-session-volatile-test-flake` with open PR #142; `step-volatility-impl-removal` with
  `core-tracing-migration` (both touch the same two `eprintln!` lines).
- **Carried over from 2026-10-08, still open:** `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` +
  `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION` as one design (both `M`, not created).
- **No duplicates found.**

## Fixes (step 5)

Eight eligible, `ready`, `P3` designs exist; the ninth (`default-asset-manager-recipe-opt-payload-check`)
has open PR #132 and is skipped. This host cannot start child sessions, and the session may push only
its own branch, so no fix was started; the maintainer was asked to authorize the first five:

| Order | Design | Crate | Must not run in parallel with |
|---|---|---|---|
| 1 | `axum-query-api-base-slash` | liquers-axum | — |
| 2 | `build-matrix-disk-budget` | scripts | — |
| 3 | `command-registry-header-feature-names` | liquers-lib (bin) | — |
| 4 | `step-volatility-impl-removal` | liquers-core `plan.rs` | 5 (same file) |
| 5 | `variadic-empty-element-conversion` | liquers-core `plan.rs` | 4 (same file) |
| next | `dependent-asset-weak-reference-pruning`, `replacing-write-drops-stale-dependency-edges`, `new-folder-write-ancestor-listing-refresh` | liquers-core `assets.rs` | each other (same file) |

## Decisions needed (step 6)

Ordered by what they unlock. Each names the recommendation recorded in its design.

- **D1 — Phase 5 approval for the 18 shipped designs above.** Decided during the run: approved,
  all 18 are `complete`.
- **D2 — `registry-query-default-text-form`:** also *write* a query default as text, not only read
  it? Recommended: yes (one-time `metadata_version` change for `template/render` and
  `template/expand`).
- **D3 — `cwd-less-recipe-analysed-at-root`:** analyse a cwd-less recipe against the root, or let
  the run inherit the caller's cwd? Recommended: the root (a keyed asset is one asset per key).
- **D4 — `cwd-default-link-silent-root`:** which links stop warning? Recommended: only a declared
  default link naming the working directory (`-R-key/.`).
- **D5 — `external-manager-evaluation-session-sharing`:** API shape. Recommended:
  `AssetRef::join_evaluation_session_of(&self, parent)`, keeping `EvaluationSession` crate-private.
- **D6 — `search-catalog-single-metadata-read`:** a new `AssetManager::get_asset_info_and_version`
  default method, or a `version` field on `AssetInfo`? Recommended: the method, documenting that a
  store overriding `get_asset_info` for files is bypassed.
- **D7 — `combined-value-shared-data-node`:** hook shape. Recommended:
  `ValueInterface::as_data_node(&self) -> Option<&dyn DataNode>`, default `None`.
- **D8 — `context-title-predecessor-inheritance`:** inherit command-set title/description across a
  predecessor cut, or keep per-asset semantics? Recommended: inherit.
- **D9 — `core-tracing-migration`:** `tracing` facade, unconditional dependency, accept silence
  without a subscriber. Recommended: yes to all three.
- **D10 — `template-non-scalar-text-form`:** deferred by the maintainer earlier today; recommended:
  keep deferred (three sub-questions recorded in the design).
- **D11 — stale 2026-03 drafts with no source issue:** `register-command-enum`, `value-accessor`,
  `value-list-support`. Recommended: abandon, or file an issue for each one still wanted.
- **D12 — `axum-assets-endpoints`:** PR #73 merged and 12 of its 14 sources are closed; close it as
  complete and let `AXUM-WEBSOCKET-HARDENING` and `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` stand alone?
  Recommended: yes.
- **Not decisions, still blocked:** `error-with-key-field` (needs a design session on the
  error-context model); `evaluation-session-volatile-test-flake` (needs the test run, which a fix
  session can do).

## Implementation plan (step 7)

1. **Now, after authorization:** fixes 1-5 above, one PR each; 4 and 5 in sequence.
2. **Next batch:** the three `assets.rs` fixes, in sequence.
3. **Human-gated, ready:** `core-value-enum-oversized` and `py-value-arc-payloads` (same remedy; do
   core first), `environment-manager-reference-cycle`, `root-listing-dependency-key-spelling`,
   `lib-build-warning-cleanup` (needs a registry regeneration).
4. **One decision away from ready:** D2-D9, each naming its design. D3 and D4 both edit
   `plan.rs`'s cwd handling; do them in sequence.
5. **In implementation:** `default-asset-manager-recipe-opt-payload-check` (PR #132),
   `agent-memory-mvp` (PR #140), runtime volatility (PR #142).
