---
id: DOC-03-APPLY-ENTRY-POINTS
kind: design
title: DOC-03's entry-point table describes apply and apply_queued as implemented
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [docs, core/assets]
issues: [DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES]
created: 2026-10-26
---
# DOC-03's entry-point table describes apply and apply_queued as implemented

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` §Public entry-point contract is the
table a reader uses to choose between `apply`, `apply_queued` and `get_asset`. Its `apply` row and
the paragraphs under it describe the API before `evaluate-path-consolidation` (2026-09-04), and
contradict the same document's §Public versus infrastructure APIs. A reference must be true at HEAD.

### Problem Example

Verified at HEAD (`cd6d5da`). The table says:

```text
| `AssetManager::apply` | Supplied | No | Schedule ad-hoc asset | Evaluate ad-hoc asset inline |
| `AssetManager::apply_immediately` | Supplied | Optional | Bypass queue and evaluate before return | … |
```

and below it "with `DefaultAssetManager`, ordinary `evaluate`, `get_asset`, `get`, and `apply` return
a handle after scheduling" and "Only the immediate path accepts a payload". The code
(`liquers-core/src/assets.rs`): `AssetManager::apply(recipe, to, payload: Option<E::Payload>)`
evaluates before returning on both managers and takes no job-queue slot
(`DefaultAssetManager::apply` runs `asset_ref.run(payload)` inline); `apply_immediately` is a
`#[deprecated]` forwarding default; `apply_queued(recipe, to)` is the variant that returns a
followable handle (queued on `DefaultAssetManager`, evaluated inline and returned even on failure on
`ImmediateAssetManager`), takes a slot, and must not be called from inside an evaluation. A reader
following the table wraps `apply` in a wait it does not need, or calls it expecting a handle to
follow.

### Scope and Acceptance Criteria

- **AC-1** The `apply` row matches the code
  WHEN a reader looks up `AssetManager::apply` in the table
  THEN it reads: input supplied, payload optional, evaluates before return on both managers, no queue slot
- **AC-2** `apply_queued` is listed
  WHEN a reader looks for a followable ad-hoc evaluation
  THEN the table has an `AssetManager::apply_queued` row: no payload; queued manager returns at once with a queued handle; inline manager evaluates and returns the asset even when it failed; not from inside an evaluation
- **AC-3** `apply_immediately` is marked deprecated
  WHEN a reader finds `apply_immediately`
  THEN the row says it is deprecated and forwards to `apply`
- **AC-4** The paragraphs agree with the table
  WHEN the two paragraphs under the table are read
  THEN `apply` is not among the operations that "return a handle after scheduling", and "only the immediate path accepts a payload" is gone
- **AC-5** Record maintenance
  WHEN the change is committed
  THEN DOC-03 has a `## History` row and a bumped `reviewed:` date, and `docs_index.py --check` passes

Non-goals: rewriting other rows (`EnvRef::evaluate`, `get_asset`, `get` were re-checked and are
accurate); code changes.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a documentation fix in one reference file (rule 2), no code,
  no interface change.
- **Leading issue:** None
- **Explanation:** the code's doc comments (`AssetManager::apply`, `apply_queued`,
  `apply_immediately`) and `ASSET_LIFECYCLE.md` §entry points already state the contract; the
  table only has to say the same.
- **Open questions:** None.

### Design Dependencies

None that orders this. `asset-event-kinds`, `cancel-point-of-no-return`, `error-with-key-field`,
`executor-neutral-core`, `external-index-sync` and others also plan DOC-03 edits in other sections
(textual rebase only).

## Phase 2: Architecture

### Solution

Edit `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` only:

1. Table rows:

   ```text
   | `AssetManager::apply` | Supplied | Optional | Evaluate before return; no queue slot | Evaluate inline before return |
   | `AssetManager::apply_queued` | Supplied | No | Queue and return a handle at once (takes a slot; not from inside an evaluation) | Evaluate inline; return the asset even when it failed |
   | `AssetManager::apply_immediately` | Supplied | Optional | Deprecated: forwards to `apply` | Deprecated: forwards to `apply` |
   ```

2. First paragraph: "ordinary `evaluate`, `get_asset` and `get` return a handle after scheduling";
   add "`apply` always evaluates before returning; `apply_queued` is the ad-hoc entry point that
   returns a handle to follow."
3. Second paragraph: "`apply` and `apply_queued` create ad-hoc assets and insert them into no key or
   query map; they are never written to the store. Only `apply` (and the deprecated
   `apply_immediately`) take a payload." Drop "installs its value but does not persist it" for
   `apply_immediately` (it is `apply`).
4. `## History` row (2026-10-26 or the implementation date): "§Public entry-point contract: `apply`
   evaluates before return with an optional payload; `apply_queued` row; `apply_immediately`
   deprecated." Source: `design/doc-03-apply-entry-points/`. `reviewed:` bumped.

Rejected: removing the `apply_immediately` row (it still exists, deprecated; out-of-tree callers
look it up).

### Changes

One file, as above. No code, registry or index change beyond the regenerated `specs/index.csv`
`reviewed` column.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| A concurrent design edits DOC-03 | different sections; rebase | high |
| The wording drifts from the code comments again | the row cites the method names; the code comments are the source | medium |

## Phase 3: Examples and Tests

### Examples

After the change, a reader choosing an entry point for "evaluate this recipe on this state and give
me the result" picks `apply`; for "start it and let a web client follow progress" picks
`apply_queued`; the table and `ASSET_LIFECYCLE.md` §entry points say the same.

### Tests

No code tests. Checks:

- Manual comparison of each row with `AssetManager::apply` / `apply_queued` / `apply_immediately`
  doc comments and both managers' implementations (`DefaultAssetManager`, `ImmediateAssetManager`)
  in `liquers-core/src/assets.rs`. Covers AC-1, AC-2, AC-3.
- `grep -n "apply" specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` shows no sentence
  claiming `apply` schedules or that only the immediate path takes a payload. Covers AC-4.
- `python3 scripts/docs_index.py --check`. Covers AC-5.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` table rows (Solution 1) — compare with `assets.rs`
- [x] 2. The two paragraphs (Solution 2-3) — the `grep` check
- [x] 3. History row and `reviewed:`; issue `status: closed` with a resolution note;
  `python3 scripts/docs_index.py` then `--check`

### Validation

`python3 scripts/docs_index.py --check`; diff review limited to DOC-03, the issue and the index.
