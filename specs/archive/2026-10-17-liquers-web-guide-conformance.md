# liquers-web ↔ LANGUAGE-INTEGRATION_GUIDE conformance audit — 2026-10-17

A bidirectional verification: the `liquers-web` implementation against the guide's requirements
(the guide front matter says `reviewed: 2026-10-16`), and the guide's descriptive claims about
`liquers-web` against the code. Statuses below are **structurally verified** — test presence,
contracts read against the blueprints, and the design folders' recorded runs — not re-executed in
this session (no wasm test runner in the environment).

Everything actionable found here is filed; this document is the dated snapshot and is not updated.
Issues filed by this audit: `WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH`,
`WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED`, `WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED`,
`WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX`, `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`,
`WEB-STORE-CONFORMANCE-TEST-GAPS`, `WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED`.

## 1. Consolidated feature matrix

The guide's checklist item 1 asks for one matrix with level, status, limitations and test links;
until now the status was spread across `specs/design/liquers-web/`,
`specs/design/liquers-web-store/`, `specs/design/store-adapters/`,
`specs/design/record-streams/` and `liquers-web/README.md`. Levels are for the Browser JavaScript
profile (§4: baseline + `ASYNCQ`; the rest "selected as needed").

| Feature | Level | Status | Tests present | Limitations / gaps |
|---|---|---|---|---|
| `OBJECT` | Essential | COMPLETE | 01–09 (`tests/objects_OBJECT.rs`) | — |
| `ERROR` | Essential | COMPLETE | 01–05; 06 in `store_js_STORE.rs:403` (page-constructed `LiquersError`, PR #123) | `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (accepted): class/stack arrive inside `message` |
| `RUNTIME` | Essential | COMPLETE | 01 native (`liquers-lib/src/value/extended.rs:725`); 02–06 under Node | `RUNTIME05` needs `--features debug-handles` |
| `VALUE` | Essential | COMPLETE* | 01–13 under Node | *outbound i64 > 2^53 unconfirmed/untested — `WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED` |
| `VALUE`/RECORDS | (sub-feature) | **PARTIAL** | only 05, 06 (`records_RECORDS.rs`) | `RECORDS10` violated (single-cell crosses as 1×1 batch); `RecordSource` unmapped; bridge one-way; 01/02/04/07/08 only as Rust counterparts; NA records for 03/09/11 missing — two issues filed |
| `ENVIRON` | Essential | **PARTIAL** | 01–07 | `ENVIRON08` (added 2026-09-05) unimplemented: no language-visible validation report — `WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED` |
| `EVAL` | Essential | COMPLETE | 01–07 (07 added by `keyed-recipe-ownership`) | `EVAL06` asserts the immediate manager's inert cancellation (`WEB-CANCELLATION-INERT`, accepted) |
| `COMMAND` | Essential | COMPLETE | 01–14 + DECL01–03 | post-share registration rebuilds the environment (`POST-INIT-COMMAND-REGISTRATION`, accepted) |
| `ASYNCQ` | Profile (required in browser) | COMPLETE | 01–06, 08; **07 `NA`** (JS has an async model; condition cited in `async_ASYNCQ.rs:3`) | — |
| `ASYNCCMD` | Optional, selected | COMPLETE | 01–06 | — |
| `STORE` (dirs 1+2+3) | Optional, selected | COMPLETE with test gaps | 01–07, 09–12, 14–17 by ID; 08 = the parameterised conformance suite; 13 native (`factory04`); e2e `STORE07` live (keyed `-R/` evaluation works since `keyed-recipe-ownership`) | `STORE05b`, dotted negatives, `STORE11` overlap order, `STORE14` breadth, `STORE17` explicit precision, `STORE18` `NA` unrecorded — `WEB-STORE-CONFORMANCE-TEST-GAPS` |
| `RECIPE` | Optional | NS (deferred by Phase 1 decision) | — | now unblocked: `-R/` evaluates in the browser |
| `MODULE`, `POLYGLOT`, `WEBSERV`, `WEBAPI` | Optional | `NA` for the shipped milestone (recorded in `liquers-web/phase1`) | — | `POLYGLOT03`/`04` are pre-satisfied by `ForeignValue::origin()` + checked downcast |
| `UIUSE`, `UIDEF` | Optional | NS (deferred) | — | — |
| `STUBS` | Optional, selected | COMPLETE | 01–07 (02/04/06 via `check-stubs.sh`; 07 in e2e) | — |
| `PACKAGE` | Optional, selected | COMPLETE | 01–05, 07 (02/03/04/07 Playwright); **06 `NA`** (no optional extra; reversing condition recorded) | — |

The two features that keep the integration from claiming its selected set `CONFORMANT` against the
current guide are `ENVIRON` (ENVIRON08) and the RECORDS sub-feature of `VALUE`.

## 2. Implementation vs guide (direction A)

The original design closed with "every prescribed conformance ID satisfied" against the
83-test inventory of 2026-08. The guide has since grown (`ENVIRON08` 2026-09-05, `RECORDS01`–`11`
2026-09-20/24, `STORE14`–`18` 2026-10-15), and the amendments the earlier review produced
(`OBJECT09`, `ENVIRON07`, `COMMAND12`–`14`, `ERROR06`, the amended `VALUE02`/`COMMAND05`/
`COMMAND08`/`ERROR03`) **are all implemented** — the drift is confined to:

1. **`ENVIRON08`** — no validation-report exposure at all (core has the report; liquers-web never
   reaches it).
2. **RECORDS** — `RECORDS10` actively violated (`default_value.rs:49-52` materializes every view,
   pre-empting liquers-lib's own single-cell scalar path); `RecordSource` refused with a generic
   JSON `ConversionError`; `from_js_custom` always `Ok(None)` so no record value can cross back;
   JS-side tests exist only for 05/06.
3. **STORE test breadth** — five narrower-than-blueprint spots (see the issue), none behavioural
   so far as core's own tests show.
4. **Dispositions** — `STORE18`, `RECORDS03/09/11` are legitimately `NA` but recorded nowhere,
   against "absence is not an `NA` decision".

## 3. Guide vs implementation (direction B)

Fact-checking the guide's ~30 descriptive claims about liquers-web found most TRUE (the JsStore
sentinel and once-at-construction protocol, the data-only method rules, `localstorage`/`http`
data access, `build_store` against the environment's registry, `sync` unexposed, the
first-wins factory chain, the dropped `liquers-store` dependency, the single `serialize_to_js`
wrapper, zero-argument `arguments: []`, `js.Value` identifier semantics). Four claims are wrong or
stale (filed as `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`):

1. **L106-109**: "`RUNTIME`, `ASYNCQ` and `PACKAGE` need a real browser" — RUNTIME and ASYNCQ run
   under Node; only the `browser-tests`-gated store suites and the Playwright specs need one.
2. **L271**: the `OBJECT09` one-liner still describes the superseded "raises" contract;
   Appendix A and the 2026-08-14 History entry define the round-trip contract.
3. **L621-623**: rebuild happens on store configuration, store-object registration and
   unregistration too, not only command registration.
4. **L1548**: the "full 83-test disposition" no longer describes conformance with the current
   inventory.

Code-side documentation rot found by the same pass (filed as
`WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX`): the README still declares `-R/` broken in the browser and
`encodeParam` partial — both fixed; an orphaned `Cargo.toml` comment; "Four stores" over a
three-row table; the stale harness claim duplicated in `value_bridge_VALUE.rs`.

## 4. Implementation plan

Ordered by value over effort; stages are independent except where noted.

- **Stage 0 — documentation truth (S, auto-fix eligible, no design):**
  `WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX` and `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`. Pure doc
  edits; the guide fix takes a History row and `reviewed:` bump.
- **Stage 1 — `RECORDS10` fix (S):** `WEB-RECORDS-SINGLE-CELL-VIEW-CROSSES-AS-BATCH` — a
  single-cell check in `to_js_custom` before materializing, plus the two-halves test. Small,
  behavioural, independent.
- **Stage 2 — `ENVIRON08` (M, compact design):** `WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED`.
  Decide whether liquers-web adopts `EnvironmentBuilder` or surfaces the report from its own
  path; then the accessor and the test.
- **Stage 3 — STORE test closure (M, no design):** `WEB-STORE-CONFORMANCE-TEST-GAPS` — test-only
  except the recorded dispositions; also write the `STORE18`/`RECORDS03/09/11` `NA` entries into
  the conformance record (see Stage 5).
- **Stage 4 — records bridge completion (L, full design):**
  `WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED` — the one substantial feature: a
  `RecordSource` JS wrapper (`chunks()`, async iterator, bounded `materialize`), the JS→Rust
  direction decision, the clear refusal message meanwhile, and the JS-side RECORDS tests.
  Suggested slug `liquers-web-records-bridge`.
- **Stage 5 — a living conformance record:** §1's matrix placed where it can be maintained
  (`liquers-web/README.md` or a `specs/reference/` document), so the next guide amendment has one
  place to diff against. Do this with whichever stage lands first.
- **Optional next capability (user decision):** `RECIPE` is the natural next selection now that
  keyed evaluation works in the browser; `WEB-NATIVE-IO-TIER2` (IndexedDB) remains the accepted
  storage follow-on. Neither is a conformance gap.

## 5. Verification method

Prescribed inventory extracted from the guide (all 3,125 lines) and
`specs/reference/CONFORMANCE_TERMS.md`; test IDs inventoried across `liquers-web/tests/`,
`src/`, `scripts/check-stubs.sh`, `tests/e2e/`, and the native `RUNTIME01` guard in liquers-lib;
issue statuses cross-checked in `specs/index.csv`; ~30 descriptive guide claims fact-checked
against `src/store/*`, `src/value.rs`, `src/bridge.rs`, `src/default_value.rs`,
`src/environment.rs`, `src/records.rs` and the test files, with file:line evidence recorded in
the audit session.
