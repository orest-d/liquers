# Backlog compaction, 2026-10-08

The first compaction run under `.claude/skills/liquers-project/references/compaction.md`, run
interactively, fix cap 5. It is the first run since the `autofix` column was added, so its main
work was labeling every readiness-labeled design. Dated record; not edited later.

## Counts

| | Start | End |
|---|---|---|
| Open issues and features | 120 | 119 |
| …without a design | 78 | 72 |
| …`S` without a design | 6 | 0 |
| Open designs | 67 | 72 |
| …readiness-labeled | 51 | 56 |
| …`autofix` assessed | 0 | 56 |
| …`autofix: eligible` | 0 | 10 (7 already implemented, 3 to fix) |
| Readiness `ready` / `needs-decision` / `covered` / `phase2-blocked` | 19 / 17 / 14 / 1 | 22 / 19 / 14 / 1 |

`docs_index.py --check`: 0 errors before and after (32 warnings, all pre-existing: `L` issues
without a design, overdue reviews, designs without acceptance scenarios).

## Automatic-fix labels

Every readiness-labeled open design now has `autofix` in `DESIGN.md` and an **Automatic fixing**
line naming the rule in its Design Readiness section.

- **Eligible, not yet fixed (3, all new this run):** `simple-value-untyped-and-scalar-reads` (P2),
  `manifest-chunk-error-identity` (P3), `ordered-json-orient-column-order` (P3).
- **Eligible, already implemented (7):** `axum-recipes-metadata-entry`,
  `docs-index-phase-link-targets`, `ext-value-description-completeness`,
  `manifest-over-stored-csv-test`, `metadata-only-entry-reload`,
  `register-all-commands-feature-gating`, `register-command-payload-docs`. Labeled for the record;
  they need Phase 5 approval, not a fix.
- **Not eligible (46):** 14 `covered` records; 19 `needs-decision` designs (rule 5, most also rule
  4); 13 others for an interface change (rule 4), a new structure (rule 3), more than one crate
  (rule 6) or changed rather than restored behaviour (rule 2).
- **Would become eligible once decided:** `axum-store-keys-deep` (answer "synonym"),
  `axum-store-upload-metadata`, `command-registry-impl-version-freshness`,
  `query-leading-slash-field` (answer "keep the name"), `store-guide-status-table`,
  `pyo3-python-3-13-support` (answer B), `build-sysinfo-rustc-compatibility` (see decision D3).

16 open designs carry no readiness (pre-bulk-design work, or in implementation with `gh_pr`); they
are outside this labeling.

## Designs created (step 4)

Every `S` issue without a design now has a compact design (Phases 1-4, readiness, `autofix`):

| Design | Sources | Readiness | Auto-fix |
|---|---|---|---|
| `simple-value-untyped-and-scalar-reads` | `STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ` (P2), `SIMPLE-VALUE-READS-TEXT-SCALARS-AS-TEXT` (P3) — merged, T3 (same function) | ready | eligible |
| `manifest-chunk-error-identity` | `MANIFEST-CHUNK-SCHEMA-ERROR-DOES-NOT-NAME-THE-CHUNK` | ready | eligible at the run; re-labeled not-eligible in review (rule 6: its proof un-ignores a liquers-lib test) |
| `ordered-json-orient-column-order` | `SCHEMA-LESS-ORDERED-JSON-ORIENTS-LOSE-COLUMN-ORDER` | ready | eligible |
| `pyo3-python-3-13-support` | `PY-PYO3-REJECTS-PYTHON-3-13` | needs-decision | not-eligible |
| `store-key-format-seeding` | `STORES-DISAGREE-ON-SEEDING-THE-DATA-FORMAT-FROM-THE-KEY` | needs-decision | not-eligible |

Both ignored liquers-lib tests that the first two designs un-ignore were run on 2026-10-08 and fail
exactly as their issues describe.

## Overlaps and merges (step 3)

- **Merged:** the two `lib/value` sources above (T3, both edit `SimpleValue::deserialize_from_bytes`;
  both eligible, so E1 does not apply).
- **Linked, not merged:** `ordered-json-orient-column-order` overlaps the implemented
  `json-table-column-order` (same change site; finished designs are not reopened).
  `store-key-format-seeding` overlaps `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` and the
  untyped-file source (weak).
- **Merge proposals (maintainer decision, `DOCS_STRUCTURE_GUIDE.md` §5.1.1):**
  1. `argument-info-description` + `command-metadata-command-hints` — both designs already
     recommend it (mirror-image metadata asymmetry, one macro grammar change, one registry
     regeneration). Leading source: `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`.
  2. `type-info-write-only-formats` into `DATA-FORMAT-CONSTANTS-AND-TOOLING` (L, no design) — T2;
     the design's own scope question (D8).
- **Design-less clusters proposed for one design each, not created this run** (`M`/`L`, beyond this
  run's budget):
  - `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` + `DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION`
    (T3: where and how often plan dependency analysis runs; both filed from
    `dependency-chain-analysis-cost`). `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION`
    is weak overlap (same evaluation path, different cause).
  - `CORE-ASSET-GC` + `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` — weak only (both
    memory policy, different objects); keep separate, link.
  - `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` + `CORE-SESSION-AND-KEY-ACL` — weak (both
    restrict a `Context`, different purposes); keep separate, link.
- **No duplicates found.**

## Fixes (step 5)

Three eligible, `ready`, `P2`/`P3` designs, fixed as spin-offs (mechanism A, child sessions), one
branch and one PR each, each based on this run's branch so the design is present. No eligible
`P0`/`P1` item exists. The design-only PR is https://github.com/orest-d/liquers/pull/88.

| Order | Design | Crate | Must not run in parallel with |
|---|---|---|---|
| 1 | `simple-value-untyped-and-scalar-reads` (https://github.com/orest-d/liquers/pull/91) | liquers-lib | — (touches one `#[ignore]` line in `records_manifest_over_csv_files.rs`, as does 2) |
| 2 | `manifest-chunk-error-identity` (https://github.com/orest-d/liquers/pull/89) | liquers-records (+ one `#[ignore]` in liquers-lib) | — (different line of the same test file as 1) |
| 3 | `ordered-json-orient-column-order` (https://github.com/orest-d/liquers/pull/90) | liquers-records | — (different files from 2) |

Also closed on evidence: `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` (the fix and the memory-store
prerequisite were already merged; all three tests pass).

## Decisions needed (step 6)

Ordered by what they unlock. Each names the recommendation.

- **D1 — Merge `argument-info-description` and `command-metadata-command-hints`?** Recommended: yes,
  leading source the in-progress feature. Unlocks one coherent design; both stay not-eligible
  (rule 4).
- **D2 — `axum-store-keys-deep`: is `GET {store}/keys` deep or a synonym of `listdir`?**
  Recommended: deep. "Synonym" makes it an eligible doc-comment fix.
- **D3 — `build-sysinfo-rustc-compatibility`: pin, or declare an MSRV?** New finding: `Cargo.lock`
  is git-ignored and no `rust-version` exists, so the proposed pin cannot be done. Recommended:
  declare `rust-version` in the workspace `Cargo.toml` and close the issue. Then eligible
  (build-settings fix).
- **D4 — `pyo3-python-3-13-support`: upgrade PyO3 (A) or document the range (B)?** Recommended: B
  now, A as its own `M` design. B is eligible.
- **D5 — `store-guide-status-table`: build the generator test or close as documented?**
  Recommended: build it. Either answer is eligible.
- **D6 — `command-registry-impl-version-freshness`: compare `impl_version` exactly in a separate
  test, and forbid `version: now` in exported groups?** Recommended: yes to both. Then eligible.
- **D7 — `axum-store-upload-metadata`: declare an upload's `media_type` only when the extension
  cannot tell?** Recommended: yes. Then eligible.
- **D8 — `type-info-write-only-formats`: add `write_only_data_formats` now or defer to
  `DATA-FORMAT-CONSTANTS-AND-TOOLING`?** Recommended: now (`RecordView`'s `html` claim is false
  today).
- **D9 — `query-leading-slash-field`: rename `Query::absolute` to `rooted` (wire name kept) or keep
  and document?** Recommended: rename. "Keep" makes it eligible.
- **D10 — `store-key-format-seeding`: seed the filename from the key in `finalize_metadata`?**
  Recommended: yes.
- **D11 — remaining single-question designs**, each with its recommendation in its Phase 1:
  `configuration-error-kind` (add `ErrorType::ConfigurationError`, HTTP 500),
  `context-title-predecessor-inheritance` (inherit command-set fields),
  `external-manager-replacement-surface` (expose both primitives, guarded),
  `metadata-error-traceback` (one optional string), `record-timestamp-utc` (UTC everywhere),
  `register-command-option-value` (reject unsupported `Option<T>`; add `Option<String>` and
  `Option<bool>` impls), `state-argument-serde-default` (omission means a transforming command),
  `web-liquers-error-constructor` (`new LiquersError(errorType, message)`).

## Implementation plan (step 7)

1. **Now, in parallel:** the three spin-off fixes above, each its own PR, merged after this run's
   PR.
2. **Approvals, no code:** 16 designs whose implementation has landed and whose Phase 5 awaits
   approval (`phase: documentation`). `context-title-description`, `docs-index-phase-link-targets`
   and `metadata-only-entry-reload` are implemented with their issues closed but are still at
   `phase: implementation` with no Phase 5 record; write those records first.
3. **After D2-D7 and D9 ("keep"):** each answered design becomes an eligible fix for the next run,
   one PR each, no ordering constraints between them (different crates or files).
4. **Human-gated after their decisions:** the D1, D8, D10 and D11 designs. `configuration-error-kind`
   and `metadata-error-traceback` both change `ErrorPayload`/`ErrorType`, the contract every
   binding observes; do them in sequence, not in parallel.
5. **Stale records for a human:** `expiration-integration-suite-repair` (draft, Phase 3; its issue
   is closed) and `asset-manager-insert-key-asset-semantics` (draft, Phase 5; its issue is closed
   and covered by `COVERED-01`) look finished or abandoned; their status is a maintainer's call.

## Same-day follow-up (before this record landed)

The maintainer answered part of the decision list on 2026-10-08; each answer is recorded in its
design's Phase 1.

- D1 merged the two command-metadata designs into `command-metadata-descriptions-and-hints`
  (needs-decision on the two macro spellings). D2 deep, D3 declare `rust-version` (dependency floor
  1.95; a minimum, not a pin), D4 option B, D6 as recommended: those four designs became ready and
  eligible. D8 folded `type-info-write-only-formats` into `DATA-FORMAT-CONSTANTS-AND-TOOLING`.
- Phase 5 approved and `status: complete` for the seven already-implemented eligible designs.
- Two more spin-offs started within the cap of 5: `build-sysinfo-rustc-compatibility` (branch
  `claude/declare-rust-version`) and `command-registry-impl-version-freshness` (branch
  `claude/registry-impl-version-freshness`). Deferred by the cap: `axum-store-keys-deep`,
  `pyo3-python-3-13-support`.
- Review of the design PR corrected four designs: `manifest-chunk-error-identity` re-labeled
  not-eligible (rule 6); `simple-value-untyped-and-scalar-reads` keeps `toml` reading as `Text`;
  `pyo3-python-3-13-support` keeps the `>=3.8` lower bound; `store-key-format-seeding` handles
  `LegacyMetadata` explicitly and puts its rules in the shared conformance inventory.

