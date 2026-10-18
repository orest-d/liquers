---
id: LANGUAGE-GUIDE-WEB-CLAIMS
kind: design
title: Correct the Language Integration Guide's stale claims about liquers-web
form: compact
status: complete
readiness: ready
autofix: eligible
area: [docs, web]
issues: [LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS]
created: 2026-10-17
---
# Correct the Language Integration Guide's stale claims about liquers-web

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

The 2026-10-17 conformance audit (`specs/archive/2026-10-17-liquers-web-guide-conformance.md` §3)
fact-checked the guide's descriptive claims about `liquers-web`. Four are wrong or stale. An
integration author following them builds a browser harness for suites that run under Node, writes
`OBJECT09` against a superseded contract, and believes the worked example is conformant with the
current inventory. Documentation only.

### Problem Example

An author designing a Python or browser integration reads the guide's harness question 2:

> all thirteen `VALUE` tests for `liquers-web` are pure ECMAScript and run under Node … while
> `RUNTIME`, `ASYNCQ` and `PACKAGE` need a real browser.

Today: they plan a WebDriver harness for `RUNTIME` and `ASYNCQ`. In fact
`liquers-web/tests/runtime_RUNTIME.rs` and `async_ASYNCQ.rs` run under Node; only
`store_local_STORE.rs` and `store_conformance_browser_CONF.rs` (behind `browser-tests`) call
`wasm_bindgen_test_configure!(run_in_browser)`, and `PACKAGE` runs in Playwright. Should be: the
guide says so.

### Scope and Acceptance Criteria

- **AC-1** Harness claim
  - WHEN a reader reads §3 harness question 2
  - THEN it says the `VALUE`, `RUNTIME` and `ASYNCQ` suites run under Node, and only browser-API
    suites (`localStorage`) and the delivery-form `PACKAGE` specs need a real browser
- **AC-2** `OBJECT09` one-liner
  - WHEN a reader reads the §OBJECT "Meaningful tests" line
  - THEN `OBJECT09` is described as "any host string round-trips through query text", matching the
    Appendix A blueprint `test_OBJECT09_any_parameter_value_round_trips` and the 2026-08-14 History row
- **AC-3** Rebuild triggers
  - WHEN a reader reads the TypeRegistry advice ("Register where your rebuild path already goes")
  - THEN the parenthetical names every rebuild trigger in `liquers-web/src/environment.rs`: command
    registration and unregistration, store configuration and store-object registration after the
    environment is shared
- **AC-4** Worked-example inventory
  - WHEN a reader reads the Related-documents entry for `specs/design/liquers-web/`
  - THEN it no longer presents the 83-test disposition as current; it says the disposition is against
    the 2026-08 inventory and points to the 2026-10-17 audit for the later additions (`ENVIRON08`,
    `RECORDS01`-`11`, `STORE14`-`18`)
- **AC-5** Records kept
  - WHEN the guide is changed
  - THEN `reviewed:` is bumped and a `## History` row cites `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`
    (`DOCS_STRUCTURE_GUIDE.md` §9.2), in the same commit

Non-goals: the same harness claim in `liquers-web/tests/value_bridge_VALUE.rs` (design
`web-docs-stale-limitations`); rewriting the guide's prescriptions — only descriptive statements
about liquers-web change.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation fix in one `specs/guides/` file, no interface change.
- **Leading issue:** None
- **Explanation:** Each statement was rechecked at HEAD (guide L106-109, L271, L621-623, L1548;
  Appendix A L1730; `environment.rs` `register_command_on`, `configure_store_on`, `rebuild_with`,
  `rebuild_without`, `register_store_object_on` → `apply_retained_store_config`). The correct text is determined by the code and the guide's own appendix.
- **Open questions:** None.

### Design Dependencies

- `web-docs-stale-limitations` — **overlaps** (the same harness claim in a test comment); independent
  order.
- `web-environ-validation-report`, `liquers-web-records-bridge`, `web-store-conformance-tests` —
  **overlaps**: each may later add its own History row to this guide. AC-4 deliberately avoids
  stating a current conformance count, so those designs do not have to revise this text.

### Consolidated Findings

Line numbers move; locate each passage by its text. The OBJECT09 fix is the one with a contract
consequence: the §5 one-liner and the appendix currently disagree, and the appendix wins because the
2026-08-14 History row records the decision.

## Phase 2: Architecture

### Solution

Four in-place edits plus the History row. Rejected: deleting the liquers-web examples (they are the
guide's only worked integration); replacing "83" with a fresh count (it would go stale again on the
next amendment).

Known-issue preflight: `PARAMETER-ESCAPING-INCOMPLETE` (closed) decided OBJECT09; no open issue
changes liquers-web's rebuild triggers (`POST-INIT-COMMAND-REGISTRATION` is accepted, and still
rebuilds).

### Changes

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` only:

1. §3 harness question 2 — rewrite the liquers-web sentence per AC-1.
2. §OBJECT "Meaningful tests" — `OBJECT09` any host string supplied as a parameter round-trips
   through query text (encode, then parse with the real parser).
3. TypeRegistry advice item 2 — "(as `liquers-web` does on command registration and unregistration,
   store configuration, and store-object registration while a configuration is installed — each once the environment is shared)".
4. Related documents — "phases 1-4 with the 83-test disposition against the 2026-08 inventory; later
   additions are dispositioned in `specs/archive/2026-10-17-liquers-web-guide-conformance.md`".
5. `reviewed: 2026-10-17` (the implementation date) and a History row.
6. `specs/issues/LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS.md` — close with a resolution note.

### Risks

A prescription could be changed by accident while editing descriptive text; the review step checks
that only the four passages and the History table differ. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; after the fix question 2 matches the test files.

### Tests

No runtime tests. Proofs:

- AC-1: `grep -n "run_in_browser" liquers-web/tests/*.rs` lists only the two store suites; the guide's
  sentence names the same set.
- AC-2: `grep -n "OBJECT09" specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — no line says "raises".
- AC-3: `grep -n "rebuild_with\|rebuild_without" liquers-web/src/environment.rs` callers match the
  triggers named.
- AC-4: `grep -n "83-test" specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` shows the qualified wording.
- AC-5: `python3 scripts/docs_index.py --check` passes; the History row is present.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — edits 1-4 of Phase 2 — AC-1..AC-4 greps
- [x] 2. Same file — bump `reviewed:`, add the History row — AC-5
- [x] 3. Close `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS` (§4.3) and regenerate — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`git diff` touches only the guide, the issue and generated index files; `docs_index.py --check`.
