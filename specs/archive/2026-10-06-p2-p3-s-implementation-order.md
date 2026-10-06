# P2/P3 size-S designs: merge candidates, decisions and implementation order

*True on 2026-10-06, after the bulk-design run on branch `claude/p3-s-issue-designs-oc1dy6` (merged
with `main` at `013c4a8`). It records a plan, not a decision. The maintainer decides merges and
open questions, and `specs/index.csv` is authoritative for current readiness.*

## 1. Scope of the run

- **33 new designs**, one per open P3/S issue that had none, produced under
  [`guides/autonomous_bulk_design.md`](../guides/autonomous_bulk_design.md): 18 `ready`,
  15 `needs-decision`, 0 `blocked`, 0 `phase2-blocked`, 0 `covered`.
- **8 existing P3/S designs excluded without review**, because their Phase 4 was already
  substantive (procedure §4): `command-metadata-command-hints`, `configuration-error-kind`,
  `query-leading-slash-field`, `command-registry-impl-version-freshness`,
  `register-command-payload-docs`, `core-dead-code-hygiene`, `opendal-derived-store-arguments`,
  `web-liquers-error-constructor`.
- **P2/S designs considered for ordering only** (not modified): `metadata-serde-partialeq`,
  `queued-manager-conditional-eviction`, `register-command-payload-docs`,
  `save-to-store-skip-outcome`, `text-value-markdown-format`,
  `web-object06-error-type-exhaustiveness` (all `ready`); `build-sysinfo-rustc-compatibility`,
  `metadata-error-traceback`, `state-argument-serde-default` (`needs-decision`).
  `context-title-description` was implemented on `main` during the run and drops out.
- Seven issues filed during `record-streams` pointed their `design:` at that frozen folder. They
  now point at their own designs.

## 2. Recommended merges (maintainer decision, `DOCS_STRUCTURE_GUIDE.md` §5.1.1)

The run did not merge anything itself, so each pair below is a recommendation.

| # | Designs | Strength | Why |
|---|---|---|---|
| M1 | `memory-store-metadata-only-entry` + `metadata-only-entry-reload` | **Must land together** | After the memory store reports `KeyNotFound` for a metadata-only key, `try_fast_track`'s `store.get(..)?` propagates it and the asset's `get` fails instead of recomputing. That already happens on file stores today (latent defect found during design). Neither change is safe alone. |
| M2 | `immediate-set-state-status-match` + `supplied-expired-status-reason` | Strong | Both edit the same four `final_status` sites. The second changes one row of the first's shared function. Once M2's decision is made, they are one small PR. |
| M3 | `argument-info-description` + `command-metadata-command-hints` | Strong | Mirror-image gaps (prose per argument, hints per command) in the same struct family and the same macro grammar. The issue itself asks for them to be settled together. Leading source: `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` (already `in_progress`). |
| M4 | `text-value-markdown-format` (P2) → `simple-value-serializer-parity` | Optional | Same `SimpleValue::as_bytes` match. Sequencing them (P2 first) is enough. Merging saves one rebase. |

Not merges, but **batch into one PR each** for convenience, since they are independent:
the three axum designs (`axum-recipes-metadata-entry`, `axum-store-keys-deep`,
`axum-store-upload-metadata`), and the docs tooling pair (`docs-link-check-code-spans`,
`designer-init-argument-parsing`).

Conflict hotspots, to be serialized rather than merged:

- `liquers-core/src/assets.rs`: 8 designs touch it.
- `liquers-records/src/formats/csv.rs`: `csv-physical-lines-short-rows`, and
  `rec-id-iso-date-parsing`, which moves `parse_date`/`parse_timestamp` out of it.
- `CommandMetadata` serialization: M3 plus `command-cache-flag`.

## 3. Decisions needed

Each `needs-decision` design states its recommendation in its Phase 1 "Design Readiness" section.
Phases 3–4 are written around that recommendation, so accepting it makes the design `ready`.

**System-design decisions** (change a contract, data or observable behaviour):

| Design | Decision | Recommendation |
|---|---|---|
| `finished-asset-progress-contract` | Progress of a finished asset: terminal `done` entry, or none | Terminal `done` (keep the command's own message). Make it deterministic by finalizing after the service loop drains. |
| `immediate-lazy-expiry-cascade` | Should lazy deadline expiry on the immediate manager cascade? Should a dependent read before its root be checked upstream? | Cascade on access. Document the dependent-first residual case (audit is the remedy). |
| `supplied-expired-status-reason` | Accept a supplied `Expired` with a reason, or refuse it | Accept, and record `Direct { Explicit }` unless a reason is supplied |
| `external-manager-replacement-surface` | Widen the external-manager surface (reverses a recorded "not exposed, deliberately") | Expose `notify_removed`, and add a guarded `AssetRef::new_installed` |
| `command-cache-flag` | Remove / wire / reserve `CommandMetadata.cache`. Accept a one-time `metadata_version` change for every command (expires every stored computed asset once). Drop the Python getter. | Remove, accept the one-time recomputation, drop the getter |
| `argument-info-description` | Field name and macro spelling, plus merge M3 | `description`, as an argument option `(label: …, description: …)` |
| `register-command-option-value` | Reject unsupported `Option<T>` or support `Option<Value>`. Also add `Option<String>`/`Option<bool>`? | Reject at expansion with a clear message. Add `Option<String>`/`Option<bool>` only if `ArgumentType` validation allows it cleanly. |
| `axum-store-keys-deep` | `GET {store}/keys`: deep or a documented synonym of `listdir` | Deep |
| `axum-store-upload-metadata` | What an upload declares as `media_type` under the level model | Declare the part's `Content-Type` only when the extension does not already imply it |
| `csv-physical-lines-short-rows` | Refuse short rows, with or without a lenient option | Refuse, with no option yet |
| `markdown-empty-text-and-tables` | Written form of empty text; multi-table documents | `<!---->`; read the first table only, documented |
| `record-timestamp-utc` | Timestamp is a UTC instant everywhere, or naive everywhere | UTC (change IPC + polars bridge; read any zone without shifting) |
| `json-table-column-order` | Workspace `serde_json/preserve_order` or a local ordered parse | Local parse. `preserve_order` would reorder hashed JSON (`metadata_version` of commands with several hints). |
| `type-info-write-only-formats` | Representation; do it now or under `DATA-FORMAT-CONSTANTS-AND-TOOLING` | `write_only_data_formats` subset list, now |
| `store-guide-status-table` | Build a generator, or close (the guide text is already honest) | Build a small `#[ignore]` generator test |

Already-open decisions from earlier runs (unchanged here): `command-metadata-command-hints`,
`configuration-error-kind`, `query-leading-slash-field`,
`command-registry-impl-version-freshness`, `web-liquers-error-constructor` (P3), and
`build-sysinfo-rustc-compatibility`, `metadata-error-traceback`, `state-argument-serde-default` (P2).

## 4. Implementation order for the `ready` designs

Order rule: P2 before P3 within a wave. Prerequisites first. Changes to the same file in sequence.
Cheap tooling that removes friction for later work goes first. Each line is one PR unless marked.

### Wave 0 — tooling and hygiene (no runtime risk)

1. `docs-link-check-code-spans`, then `designer-init-argument-parsing` (one PR is fine)
2. `core-dead-code-hygiene` (P3, existing)
3. `stubs01-class-detection` (needs the web build; pair with Wave 5 if a web session is planned)

### Wave 1 — core assets and stores (`assets.rs` serialized in this order)

4. `queued-manager-conditional-eviction` (P2)
5. `save-to-store-skip-outcome` (P2)
6. `metadata-serde-partialeq` (P2, `metadata.rs`; can run in parallel with 4–5)
7. `immediate-set-state-status-match` (P3; with `supplied-expired-status-reason` if M2 is decided)
8. **M1 as one PR:** `metadata-only-entry-reload` + `memory-store-metadata-only-entry`
9. `recovery-read-defers-placeholder`
10. `dependency-failure-error-subject`
11. `store-router-directory-above-members` (`store.rs`; parallel with 9–10)
12. `submit-eagerness-documentation` (docs + one test; any time after 4)

### Wave 2 — value types

13. `text-value-markdown-format` (P2)
14. `simple-value-serializer-parity` (after 13; mirrors core including `md`)
15. `ext-value-description-completeness` (test-only; any time)
16. `null-cell-string-option`

### Wave 3 — commands, registration, stores config

17. `register-command-payload-docs` (P2 docs; covers both payload and expires/version issues)
18. `register-all-commands-feature-gating` (run the build matrix)
19. `opendal-derived-store-arguments` (P3, existing; `liquers-store`)

### Wave 4 — records

20. `rec-id-iso-date-parsing` (moves the ISO parsers; land before `csv-physical-lines-short-rows`
    if that is decided)
21. `manifest-over-stored-csv-test` (test-only; may surface defects, which are filed rather than fixed inline)

### Wave 5 — web and HTTP

22. `web-object06-error-type-exhaustiveness` (P2; wasm loop after `cargo clean`)
23. `js-store-effective-media-type` (same web session as 22, and 3)
24. `axum-recipes-metadata-entry` (batch with the two decided axum designs, if decided by then)

### After decisions

Each `needs-decision` design that is accepted slots into its wave by file:

- `assets.rs` → Wave 1 after item 10: `finished-asset-progress-contract`,
  `immediate-lazy-expiry-cascade`, `external-manager-replacement-surface` (after 7).
  `type-info-write-only-formats` goes after 8, because it shares the fast-track function.
- Command metadata → Wave 3. Do M3 and `command-cache-flag` in one release, so the registry is
  regenerated once.
- Records → Wave 4: `record-timestamp-utc`, `json-table-column-order`,
  `markdown-empty-text-and-tables`, `csv-physical-lines-short-rows`.
- Axum → item 24's batch.

## 5. Validation note for implementers

Every design's Phase 4 names its own commands. Across the plan, the CLAUDE.md constraints apply:
`cargo test -p liquers-lib --lib --tests` as the default loop; `bash scripts/check-build-matrix.sh`
after any `cfg`, `ExtValue` match or `CommandMetadata` change; web suites only after `cargo clean`;
and `python3 scripts/docs_index.py --check` before every push.
