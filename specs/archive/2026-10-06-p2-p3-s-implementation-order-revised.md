# P2/P3 size-S designs: implementation order, revised after the maintainer's decisions

*True on 2026-10-06, later the same day as, and superseding,
[`2026-10-06-p2-p3-s-implementation-order.md`](2026-10-06-p2-p3-s-implementation-order.md) (kept
unedited, per the archive rule). Branch `claude/p3-s-issue-designs-oc1dy6`. `specs/index.csv` is
authoritative for current readiness.*

## 1. What changed since the first plan

**Maintainer decisions applied** (each design quotes its decision in Phase 1):

| Design | Decision | Readiness now |
|---|---|---|
| `finished-asset-progress-contract` | Progress that was started is done once the asset finishes (any terminal status). Progress never started stays absent. A final done is allowed where it simplifies the flow. | ready |
| `immediate-lazy-expiry-cascade` | Lazy expiry cascades: laziness is how expiry is discovered, and the consequences then follow | ready |
| `supplied-expired-status-reason` | A value written already expired logs a warning "Asset expired", then after-the-fact diagnostics. The structured reason stays as supplied (interpretation stated for review). | ready |
| `command-cache-flag` | Remove `CommandMetadata.cache`. The one-time `metadata_version` change is acceptable. | ready |
| `csv-physical-lines-short-rows` | Short rows are padded (null, or `""` for non-nullable text), with one aggregate warning per CSV in the asset log | ready |
| `markdown-empty-text-and-tables` | Follow the recommendation after a CommonMark check: `<!---->`, valid in CommonMark 0.30 and 0.31.2 §6.6 | ready |
| `json-table-column-order` | Unspecified column order is irrelevant, so sort column names for stability (today's order is first-appearance, not sorted, so code changes) | ready |
| `rec-id-iso-date-parsing` | Date arguments are `YYYYMMDD` or `YYYY~MM~DD` (`YYYY-MM-DD` after expansion). Timestamps follow, as basic or RFC 3339 with `~ncolon~`. | ready |

**Implemented on the branch:** the file-store half of `metadata-only-entry-reload` (fast track
re-derives a metadata-only entry instead of failing; new closed issue
`FAST-TRACK-FAILS-ON-METADATA-ONLY-FILE-STORE-ENTRY`). Found while doing it: the memory store
**serves `""`** for a metadata-only text entry, so `MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES`
is raised to P2. Its design can now land on its own.

**Review of the eight existing P3/S designs:**

| Design | Verdict |
|---|---|
| `core-dead-code-hygiene`, `opendal-derived-store-arguments`, `register-command-payload-docs` | Still valid, unchanged |
| `command-metadata-command-hints` | Updated: the registry test compares signatures, not bytes; merge M3 and the cache removal recorded |
| `configuration-error-kind` | Updated: `environment_config.rs` added to scope; five crates match `ErrorType` exhaustively |
| `query-leading-slash-field` | Rewritten: `Query::absolute` now *has* meaning (rooted resource segments), and the module doc saying otherwise is false. Recommended name `rooted`, wire name kept. |
| `command-registry-impl-version-freshness` | Rewritten: the committed registry is fresh today (verified), and detection is still missing; `version: now` handled |
| `web-liquers-error-constructor` | Rewritten: the named test file did not exist, and `LiquersError::new` is taken in Rust |

**New designs for newly filed issues** (no merges proposed for them, since they are size M):
`dependency-chain-analysis-cost` (P2 M, needs-decision), `dependency-edge-superseded-version`
(P3 M, ready), `context-title-predecessor-inheritance` (P3 M, needs-decision).

**Dropped from the plan:** `context-title-description` and `docs-index-phase-link-targets` are
implemented (their issues are closed), although their design records still say `in_review`.

## 2. Recommended merges (maintainer decision, size-S designs only)

| # | Designs | Why |
|---|---|---|
| M2 | `immediate-set-state-status-match` + `supplied-expired-status-reason` | The same four write sites in `assets.rs` |
| M3 | `command-metadata-command-hints` + `argument-info-description` | Mirror-image gaps, same macro grammar change |
| M4 (optional) | `text-value-markdown-format` → `simple-value-serializer-parity` | The same `SimpleValue::as_bytes` match |

M1 (the metadata-only pair) is no longer needed, because the fast-track half has landed.

## 3. Decisions still needed

| Design | Question | Recommendation |
|---|---|---|
| `external-manager-replacement-surface` | Widen the external asset-manager surface? (A plain-language explanation is now in its Phase 1.) | Expose `notify_removed` and a guarded `new_installed` |
| `argument-info-description` | Field name, macro spelling, merge M3 | `description`; `(label: …, description: …)` |
| `command-metadata-command-hints` | Command-level `hint key: "value"` spelling | As proposed |
| `register-command-option-value` | Reject unsupported `Option<T>`, or support `Option<Value>` | Reject with a clear message |
| `axum-store-keys-deep` | `GET {store}/keys`: deep or synonym of `listdir` | Deep |
| `axum-store-upload-metadata` | What an upload declares as media type | Only what the extension does not imply |
| `record-timestamp-utc` | Timestamp = UTC instant everywhere | Yes |
| `type-info-write-only-formats` | Representation, now or later | Subset list, now |
| `store-guide-status-table` | Generator, or close | Small generator test |
| `configuration-error-kind` | New public `ErrorType` | Add it, `ParseError` for malformed environment documents |
| `query-leading-slash-field` | Rename `Query::absolute` | `rooted`, wire name kept. The doc fix goes first regardless. |
| `command-registry-impl-version-freshness` | Exact `impl_version` freshness | Separate exact test |
| `web-liquers-error-constructor` | Constructor arguments | `(errorType, message)` |
| `dependency-chain-analysis-cost` (P2 M) | Keep transitive dependency records? Acceptable bound? | Keep (restart freshness depends on them); 40 links < 2 s |
| `context-title-predecessor-inheritance` (M) | Inherit command-set title/description across a predecessor cut | Inherit |
| P2 earlier: `build-sysinfo-rustc-compatibility`, `metadata-error-traceback`, `state-argument-serde-default` | (unchanged) | see each design |

## 4. Implementation order for the `ready` designs

Same rules as before: P2 before P3 within a wave, prerequisites first, changes to the same file in
sequence.

### Wave 0 — tooling and hygiene

1. `docs-link-check-code-spans` + `designer-init-argument-parsing` (one PR)
2. `core-dead-code-hygiene`
3. `stubs01-class-detection` (in the same web session as Wave 5)

### Wave 1 — core assets and stores (`assets.rs` in this order)

4. `memory-store-metadata-only-entry` (**P2 now**, wrong value; independent since the fast-track fix)
5. `queued-manager-conditional-eviction` (P2)
6. `save-to-store-skip-outcome` (P2)
7. `metadata-serde-partialeq` (P2, `metadata.rs`, parallel with 4–6)
8. `immediate-set-state-status-match` + `supplied-expired-status-reason` (M2, one PR)
9. `finished-asset-progress-contract`
10. `immediate-lazy-expiry-cascade`
11. `recovery-read-defers-placeholder`
12. `dependency-failure-error-subject`
13. `dependency-edge-superseded-version` (M)
14. `store-router-directory-above-members` (`store.rs`, parallel with 9–13)
15. `submit-eagerness-documentation`

### Wave 2 — value types

16. `text-value-markdown-format` (P2), then 17. `simple-value-serializer-parity`
18. `ext-value-description-completeness`, 19. `null-cell-string-option`

### Wave 3 — commands and registration

20. `register-command-payload-docs` (P2 docs)
21. `command-cache-flag` (regenerates the registry, so batch it with M3 once that is decided)
22. `register-all-commands-feature-gating`
23. `opendal-derived-store-arguments`

### Wave 4 — records (`csv.rs` in this order)

24. `rec-id-iso-date-parsing`, then 25. `csv-physical-lines-short-rows`
26. `json-table-column-order`, 27. `markdown-empty-text-and-tables`
28. `manifest-over-stored-csv-test` (after 25, so it sees the final CSV behaviour)

### Wave 5 — web and HTTP

29. `web-object06-error-type-exhaustiveness` (P2), 30. `js-store-effective-media-type`
31. `axum-recipes-metadata-entry` (batch with the decided axum designs)

### After decisions

- `assets.rs` → Wave 1 after 12: `external-manager-replacement-surface` (after 8);
  `type-info-write-only-formats`.
- P2 M `dependency-chain-analysis-cost` → Wave 1 after 13 (measure first).
- Command metadata → Wave 3 with 21 (one registry regeneration): M3,
  `register-command-option-value`, `command-registry-impl-version-freshness`.
- `configuration-error-kind` → after 29 (`web-object06` first), then
  `web-liquers-error-constructor`.
- `query-leading-slash-field` step A (doc fix) can go into Wave 0 now, without the decision.
- `context-title-predecessor-inheritance` → after Wave 1.
- Records → Wave 4: `record-timestamp-utc`. Axum → 31's batch.

## 5. Validation note

Every design's Phase 4 names its commands. Across the plan: `cargo test -p liquers-lib --lib --tests`
by default; `bash scripts/check-build-matrix.sh` after any `cfg`, `ExtValue` match or
`CommandMetadata` change; web suites only after `cargo clean`; and `python3 scripts/docs_index.py
--check` before every push.
