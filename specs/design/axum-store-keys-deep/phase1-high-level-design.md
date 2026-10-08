# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — needs-decision (rule 5). The recommended "deep" answer
  changes what an HTTP route returns (rule 4); the "synonym" answer is a doc-comment fix and would
  be eligible once chosen
- **Leading issue:** **Open design question — HTTP contract of `GET {store}/keys`.** It is either
  deep (as its name, doc comment and the original `web-api-library` spec promised) or a documented
  synonym of `listdir` (current behaviour, now written into `WEB_API_SPECIFICATION.md`).
- **Explanation:** Both are trivial to implement. The design specifies the recommended deep
  answer. Choosing "synonym" reduces the work to a doc-comment fix.
- **Open questions:**
  1. **Proposed resolution — deep:** `keys` returns `listdir_keys_deep(prefix)`. Without it, the
     Store API has no single-call subtree enumeration. The Assets API's `key/listdir?deep=true` is
     not a substitute, because it includes recipe-declared keys and returns asset-level answers.
     The cost is unbounded response size on a large store. Mitigation: none in this design
     (consistent with `listdir`). A `limit` parameter can be a later feature.

## Problem

`keys_handler` (`liquers-axum/src/store/handlers.rs`) calls `store.listdir_keys(prefix)`, so it
answers what `GET {store}/listdir/{prefix}` does, while its doc comment says "List all keys,
optionally filtered by prefix".

## Expected behaviour and acceptance (deep)

1. Store with `data/a.txt`, `data/sub/b.txt`: `GET keys?prefix=data` → `["data/a.txt",
   "data/sub", "data/sub/b.txt"]` (exact set as `listdir_keys_deep` returns, directories included
   as that method does; order as returned).
2. `GET keys` (no prefix) → the deep listing of the root.
3. Invalid prefix → parse error, as today.

## Scope

One handler, plus its test and spec row.

## Design Dependencies

- `axum-assets-endpoints` — **overlaps** (its audit recorded current behaviour in the spec).

## Documentation assessment

- Reference: `WEB_API_SPECIFICATION.md` §Store API `keys` row.

## Consolidated Findings

- `listdir_keys_deep` includes directory keys and recurses through `is_dir`. Whether directories
  belong in the answer follows the trait method. The spec states it.
- Clients: no in-tree client calls `/api/store/keys` (only design documents). The behaviour change
  affects external users only.
