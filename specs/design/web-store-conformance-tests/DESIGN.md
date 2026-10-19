---
id: WEB-STORE-CONFORMANCE-TESTS
kind: design
title: Close liquers-web's STORE test gaps and record its STORE dispositions
form: compact
status: complete
gh_pr: [155]
readiness: ready
autofix: eligible
area: [web, core/store]
issues: [WEB-STORE-CONFORMANCE-TEST-GAPS]
created: 2026-10-17
---
# Close liquers-web's STORE test gaps and record its STORE dispositions

Bulk design (`specs/guides/autonomous_bulk_design.md`). Implemented: the tests and the README
`## Conformance` section are in place, and the issue is closed. The browser half of AC-1 is landed
gated; it was not run where the design was implemented (chromedriver 147 against Chromium 141).

## Phase 1: High-Level Design

### Purpose

`liquers-web` selects all three `STORE` directions, so `STORE01`-`STORE17` are required and
`STORE18` is `NA` (`LANGUAGE-INTEGRATION_GUIDE.md` §STORE disposition table). Its suites pin five
contracts more narrowly than the Appendix A blueprints do, and one disposition is written nowhere:

1. `STORE05b` (traversal after `makedir`) has no liquers-web test, and the dotted negatives
   (`d/.hidden`, `d/a..b`, `d/...`, `d/..x`, `d/v1.2.3`) are absent from
   `tests/store_pure_STORE.rs` `keyguard04_ordinary_keys_accepted`.
2. `STORE11`'s overlap half ("`data` shadows `data/scratch`" in document order) is untested from a
   configuration document; `store_js_STORE.rs` `store11_configuration_routes_by_prefix` uses
   disjoint prefixes. The guide asks the integration to "say whether it warns"; nothing says.
3. `STORE14` (`store14_read_only_js_store_refuses_writes`) checks only `set`, against an object
   that has no write methods anyway, and reads `is_read_only` through the Rust trait rather than
   the JavaScript `Store.isReadOnly()` (`src/store/wrapper.rs` `LiquersStore::is_read_only`).
4. `STORE17` sends no `precision`, so `JsStore`'s `TimePrecision::from_name` path
   (`src/store/js_store.rs`, the `dataFacts` parser) and its refusal are untested.
5. `STORE18` is legitimately `NA` but unrecorded, against "absence is not an `NA` decision".

All five contracts are implemented (core's tests cover the behaviour); the gap is that a regression
in the wasm path would pass the liquers-web suite.

### Problem Example

A change to `JsStore::is_supported` that lets `d/a/../../etc` reach the page object after the
object created `d/a` would still pass `store05_unsupported_key`, which never calls `makedir`.
Likewise, reversing `AsyncStoreRouter::find_store`'s order would pass every liquers-web test,
because no test configures nested prefixes. Should: both regressions fail a named liquers-web test.

### Scope and Acceptance Criteria

- **AC-1** Traversal after makedir is still refused
  - WHEN a `JsStore` (Node) and a `LocalStorageStore` (browser) have `makedir("d/a")` and then
    `get("d/a/../../etc")` is called directly on the store
  - THEN it fails with `ErrorType::KeyNotAbsolute`, and the page object holds no entry for that key
- **AC-2** Dotted names are ordinary
  - WHEN `check_key` and `JsStore::is_supported` see `d/.hidden`, `d/a..b`, `d/...`, `d/..x`,
    `d/v1.2.3`
  - THEN each is accepted
- **AC-3** Overlapping prefixes follow document order
  - WHEN a configuration document lists `js` stores at `data` then `data/scratch`, and
    `data/scratch/x` is written through the built router
  - THEN the `data` object holds `data/scratch/x` and the `data/scratch` object does not; with the
    two entries listed in the opposite order, the `data/scratch` object holds it
- **AC-4** A read-only mount refuses every write
  - WHEN a `js` mount with `read_only: true` wraps a fully writable page object
  - THEN `set`, `set_metadata`, `remove`, `removedir` and `makedir` each fail with `NotSupported`,
    the page object stays empty, and `Store.isReadOnly()` on a `LiquersStore` wrapping the mount
    returns `true`
- **AC-5** An explicit precision is applied
  - WHEN `dataFacts` reports `modified: "2026-10-15T12:00:00.123456Z", precision: "millis"`
  - THEN `created` reads back as `2026-10-15T12:00:00.123Z`
- **AC-6** An unknown precision is refused
  - WHEN `dataFacts` reports `precision: "hours"`
  - THEN the operation that reads the facts fails with an error naming `'hours'`
- **AC-7** Dispositions are recorded
  - WHEN a reader opens `liquers-web/README.md` `## Conformance`
  - THEN it lists `STORE18` as `NA` linking `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` with its
    reversing condition, and states that overlapping prefixes are accepted silently (no warning)

Non-goals: a shadowing warning (see Design Readiness); `STORE18` itself; RECORDS dispositions,
which the records-bridge design records in the same section.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tests in `liquers-web/tests/` and a README section; no production
  code, binding, signature or dependency change; one crate.
- **Leading issue:** None
- **Explanation:** Every assertion pins behaviour core already tests; the README records today's
  behaviour. The shadowing answer keeps what the router does.
- **Open questions:** None blocking. The guide's "does the integration warn about shadowing?" is
  answered with today's behaviour, **accepted silently**: the router has no warning channel at
  build, a warning would belong in core's `StoreRouterConfig` for every integration, and nesting is
  sometimes deliberate. A maintainer who wants a warning files it against core; this design's test
  is unaffected.

### Design Dependencies

- `overlaps` `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` (no design): the reversing condition of
  the `STORE18` `NA`. If it lands for JavaScript, `STORE18` becomes required.
- `overlaps` the records-bridge design (`WEB-RECORDS-BRIDGE-ONE-WAY-AND-SOURCE-UNMAPPED`) and
  `WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX`: all three edit `liquers-web/README.md`. Whichever lands first
  creates `## Conformance` (audit Stage 5); the others add rows. The docs fix edits only
  `## Known limitations`. No ordering.

### Consolidated Findings

- **Where dispositions live:** `liquers-web/README.md` `## Conformance`, seeded from the matrix in
  `specs/archive/2026-10-17-liquers-web-guide-conformance.md` §1 updated to HEAD. Chosen over a
  `specs/reference/` page because the README is already the crate's maintained status page, sits in
  the same crate as the tests (keeping this fix one-crate), and needs no History row per edit.
- AC-1's browser half runs only under `--features browser-tests`; the Node half (`JsStore`) carries
  the routine loop. A `JsStore` has no real directories, so AC-1's value there is the guard-before-
  object check, not the ENOENT trap itself, which no liquers-web store (none is filesystem-backed)
  can show.
- AC-4 must use a writable page object: the current test's object has only `get`, so its refusal
  could come from the missing method rather than the mount.
- Where AC-6's error surfaces (`set` stamping `created`, or `get_metadata`) is established when the
  test is written; the assertion is on the message, not the operation.

## Phase 2: Architecture

### Solution

Test additions in the existing suites, under their guide IDs, reusing the existing fixtures
(`FULL_STORE`, `DATA_STORE`, `built`, `data_store_factory`, `object`, `key`), plus one README
section. Rejected: a separate `store_gaps_STORE.rs` file (scatters one ID across files); asserting
overlap through `AsyncStoreRouter` internals (`find_store` is private, and the page objects' own
`_data` maps observe the routing directly); adding a shadowing warning (core concern, see readiness).

Known-issue preflight: designs touching these suites (`liquers-web-store`, `keyed-recipe-ownership`,
`js-store-*`) are complete or superseded. No open issue blocks.

### Changes

- `liquers-web/tests/store_pure_STORE.rs` `keyguard04_ordinary_keys_accepted`: add the five dotted
  names (AC-2).
- `liquers-web/tests/store_js_STORE.rs`:
  - `store05b_traversal_after_makedir_is_refused` (AC-1, AC-2 for `is_supported`);
  - `store11_overlapping_prefixes_follow_document_order` (AC-3), two `FULL_STORE` objects, both
    orders, reading each object's `_data` `Map`;
  - `store14_read_only_js_store_refuses_writes` rewritten over `FULL_STORE` with every write, plus
    `LiquersStore::new(store.clone()).is_read_only()` (AC-4);
  - `store17_explicit_precision_is_applied` and `store17_unknown_precision_is_refused` (AC-5, AC-6),
    a `DATA_STORE` variant whose `dataFacts` returns the given `modified` and `precision`.
- `liquers-web/tests/store_local_STORE.rs`: `store05b_traversal_after_makedir_is_refused` (AC-1,
  browser half).
- `liquers-web/README.md`: `## Conformance` (AC-7), created if absent, with the `STORE18` row and
  the shadowing statement.

No production code, signature, command or dependency.

### Risks

| Risk | Recovery | Certainty |
|---|---|---|
| A new test fails, exposing a real wasm-path defect | Keep the test as reproduction, file the defect, stop the auto-fix | medium-high |
| `parse_key` refuses a dotted name (`d/...`) | Core's `store_key_absolute.rs` parses them; if not, drop that name and file | high |
| README edits collide with the other two designs | Separate sections; re-read before editing | high |

## Phase 3: Examples and Tests

### Examples

AC-3: document `stores: [{type: js, prefix: data, config: {object: broad}}, {type: js, prefix:
data/scratch, config: {object: narrow}}]`; `set("data/scratch/x")`; `broad._data.has("data/scratch/x")`
is `true`, `narrow._data.has(...)` is `false`.

### Tests

- `keyguard04_ordinary_keys_accepted` (extended): AC-2
- `store05b_traversal_after_makedir_is_refused` in `store_js_STORE.rs`: AC-1, AC-2
- `store05b_traversal_after_makedir_is_refused` in `store_local_STORE.rs`: AC-1
- `store11_overlapping_prefixes_follow_document_order`: AC-3
- `store14_read_only_js_store_refuses_writes` (rewritten): AC-4
- `store17_explicit_precision_is_applied`: AC-5
- `store17_unknown_precision_is_refused`: AC-6
- AC-7 is checked by review of the README section in step 6.

Run: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (Node) and
`CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown
--features browser-tests --test store_local_STORE`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `tests/store_pure_STORE.rs` `keyguard04_ordinary_keys_accepted` — add the dotted names — `cargo test -p liquers-web --target wasm32-unknown-unknown --test store_pure_STORE`
- [x] 2. `tests/store_js_STORE.rs` — add `store05b_traversal_after_makedir_is_refused` and `store11_overlapping_prefixes_follow_document_order` — `cargo test -p liquers-web --target wasm32-unknown-unknown --test store_js_STORE`
- [x] 3. `tests/store_js_STORE.rs` — rewrite `store14_read_only_js_store_refuses_writes` over `FULL_STORE` (AC-4) — same command
- [x] 4. `tests/store_js_STORE.rs` — add the two `store17_*precision*` tests with a parameterised `DATA_STORE` variant — same command
- [x] 5. `tests/store_local_STORE.rs` — add `store05b_traversal_after_makedir_is_refused` — the browser-tests command in Phase 3. Rollback: if no matching chromedriver is available, land the test (it is gated) and say so in the PR
- [x] 6. `liquers-web/README.md` — create `## Conformance` if absent (matrix from the audit §1, updated to HEAD), add the `STORE18` `NA` row with its reversing condition and the "overlapping prefixes accepted silently" statement — review against AC-7
- [x] 7. Close `WEB-STORE-CONFORMANCE-TEST-GAPS` with a resolution note — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

For any step whose new test fails on unmodified code: stop, file the defect (`DOCS_STRUCTURE_GUIDE.md`
§4.8), and hand the item back as `not-eligible`.

### Validation

The Node loop and the browser-tests `store_local_STORE` run pass; `cargo fmt -p liquers-web --check`;
`python3 scripts/docs_index.py --check`; diff contains only `liquers-web/tests/`, `liquers-web/README.md`
and the issue file.
