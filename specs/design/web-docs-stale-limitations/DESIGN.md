---
id: WEB-DOCS-STALE-LIMITATIONS
kind: design
title: Bring liquers-web's documentation up to date with the fixed limitations
form: compact
gh_pr: [151]
status: complete
readiness: ready
autofix: eligible
area: [web, docs]
issues: [WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX]
created: 2026-10-17
---
# Bring liquers-web's documentation up to date with the fixed limitations

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`liquers-web/README.md` "Known limitations" still describes two limitations that are fixed, and
three code comments rotted with it. A reader planning around them misses the crate's headline
capability (resource queries evaluated in a page). Documentation only.

### Problem Example

A page author reads `liquers-web/README.md` § Known limitations:

> **`-R/` queries do not evaluate in the browser yet.** … `env.evaluate("-R/…")` hangs … The blocked
> end-to-end tests are kept as `fixme` in `tests/e2e/store.spec.ts`

Today: the author avoids `env.evaluate("-R/data/input.csv")` and reads through `env.store()` instead.
Should be: the README says `-R/` evaluates (fixed by `specs/design/keyed-recipe-ownership/`, which
closed `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` and `IMMEDIATE-MANAGER-NO-FAST-TRACK`), and
`tests/e2e/store.spec.ts`, which contains no `fixme`, is not described as blocked.

### Scope and Acceptance Criteria

- **AC-1** No fixed limitation is listed
  - WHEN a reader opens `liquers-web/README.md` § Known limitations
  - THEN neither the `-R/` paragraph nor the `encodeParam` / `PARAMETER-ESCAPING-INCOMPLETE` row is
    present, and every remaining row names an issue whose status in `specs/index.csv` is open
    (`WEB-CANCELLATION-INERT`, `POST-INIT-COMMAND-REGISTRATION`,
    `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`)
- **AC-2** Stale code comments are corrected
  - WHEN a reader opens `liquers-web/Cargo.toml`, `src/store/mod.rs` and
    `tests/value_bridge_VALUE.rs`
  - THEN no comment describes the removed `liquers-store` dependency, the store table is introduced
    with the number of rows it has, and the harness note says that only the `browser-tests`-gated
    store suites and the Playwright specs need a browser
- **AC-3** Nothing but documentation changes
  - WHEN the diff is reviewed
  - THEN it touches only comments, doc comments, `README.md` and spec records, and
    `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` is
    unaffected

Non-goals: the guide's own claims about liquers-web (`LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`,
design `language-guide-web-claims`); a consolidated conformance matrix (the audit's Stage 5).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation fix (README and comments) confined to `liquers-web`,
  no interface change.
- **Leading issue:** None
- **Explanation:** Every claim was checked against HEAD: `tests/e2e/store.spec.ts` has no `fixme`,
  `src/encode.rs` delegates to the total `encode_token`, `Cargo.toml:18-20` comments a dependency
  that is not there, `src/store/mod.rs:6` says "Four stores" above three rows, and only
  `store_local_STORE.rs` / `store_conformance_browser_CONF.rs` configure `run_in_browser`.
- **Open questions:** None.

### Design Dependencies

- `language-guide-web-claims` — **overlaps**: the same stale harness claim appears in the guide and
  in `value_bridge_VALUE.rs`. Kept separate (different crates and documents; both eligible); either
  can land first.
- `web-store-conformance-tests`, `liquers-web-records-bridge` — **overlaps**: both may add a
  conformance record to `liquers-web/README.md`. This design edits only § Known limitations, so the
  changes do not conflict.

### Consolidated Findings

Pure documentation. The one judgement call is the `-R/` paragraph: delete it rather than rewrite it
as history, since `keyed-recipe-ownership` and the closed issues record the history.

## Phase 2: Architecture

### Solution

Edit the five spots in place. Rejected: moving the limitations table into `specs/` (the README is
where page authors look), and keeping the `-R/` paragraph as a "fixed in" note (history belongs to the
design records).

Known-issue preflight: the three remaining rows' issues are open; no open design edits these lines.

### Changes

- `liquers-web/README.md` § Known limitations — delete the `-R/` paragraph and the `encodeParam` row;
  keep the four remaining rows. If an "Evaluating" or store section elsewhere repeats the `-R/`
  caveat, correct it too (`grep -n "\-R/" liquers-web/README.md`).
- `liquers-web/Cargo.toml` — remove the orphaned three-line comment about configuration without
  OpenDAL above `base64`.
- `liquers-web/src/store/mod.rs` module doc — "Four stores" → "Three stores".
- `liquers-web/tests/value_bridge_VALUE.rs` module doc — replace the `RUNTIME*`/`ASYNCQ*`/`PACKAGE*`
  sentence: the `browser-tests`-gated store suites configure `run_in_browser`; `PACKAGE` runs in
  Playwright (`tests/e2e/`); everything else, `RUNTIME` and `ASYNCQ` included, runs under Node.
- `specs/issues/WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX.md` — close with a resolution note.

### Risks

None behavioural. Comment-only edits in a test file and `Cargo.toml` cannot change the build.
Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; after the fix the README lists only open limitations.

### Tests

No new tests: documentation has nothing to assert at runtime. Proofs:

- AC-1: `grep -nE "fixme|do not evaluate|PARAMETER-ESCAPING-INCOMPLETE" liquers-web/README.md`
  returns nothing, and each issue ID left in the table is open in `specs/index.csv`.
- AC-2: `grep -n "Four stores" liquers-web/src/store/mod.rs` and
  `grep -n "RUNTIME\*" liquers-web/tests/value_bridge_VALUE.rs` return nothing; the Cargo comment is
  gone.
- AC-3: `git diff --stat` lists only the files above;
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test value_bridge_VALUE`
  still passes where a wasm runner is available.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-web/README.md` § Known limitations — delete the `-R/` paragraph and the `encodeParam` row; fix any repeat of the caveat elsewhere in the README — AC-1 grep
- [x] 2. `liquers-web/Cargo.toml`, `src/store/mod.rs`, `tests/value_bridge_VALUE.rs` — the three comment fixes in Phase 2 — AC-2 greps
- [x] 3. Close `WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX` (§4.3) and regenerate the index — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`git diff --stat` shows documentation only; `python3 scripts/docs_index.py --check`. The wasm test run
in Phase 3 is optional because no code changes.
