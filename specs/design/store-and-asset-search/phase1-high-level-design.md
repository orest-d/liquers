# Phase 1: High-Level Design — Store and asset search

> **Revision 8, 2026-10-10.** Refocused on three commands over records. This supersedes the
> revision-7 Phase 1 approved on 2026-09-18. That version is archived at
> `specs/archive/2026-10-10-store-and-asset-search-rev7-phase1-high-level-design.md`, and
> `DESIGN.md` §"Revision 8" records what changed. Needs re-approval.

## Feature Name

Store and asset search

## Purpose

Let a person, a UI search field or an agent ask a Liquers environment *which entries match* in one
query, instead of listing a subtree and filtering in their own code.

Search is built from records (`liquers-records`):
- one command turns a store folder into records;
- one turns the command registry into records;
- one filters records by a small search syntax and optionally ranks them by relevance.

Each piece is an ordinary command, so it composes with every other record consumer.

## Problem Example

Find the open documents about expiration safety in `specs/issues/`. Today no query can do this. A
caller must list the folder (`-R-sdir/specs/issues`), fetch every entry, and match text in its own
code. The repository's `scripts/docs_index.py` does exactly that for `specs/`, and agent memory
would need to do it a second time.

Once this design is built:

```
-R-key/specs/issues/-/ns-search/catalog-t-t/search-expiration~.safety
```

This query returns a table with at most 50 rows: the entries under `specs/issues/` whose filename,
title, description or content contain both words. The rows are sorted by relevance, and each
carries `key`, `status`, `title`, `score` and `excerpt`. The query was validated with
`liquers-validate`; the decoded parameter is `expiration safety`.

## Scope and Acceptance Criteria

- **AC-1** Catalog of a folder
  WHEN `-R-key/<folder>/-/ns-search/catalog` is evaluated
  THEN the result is a `RecordView` with one row per entry under `<folder>`, recursively. Its
  columns are `key`, `filename`, `extension`, `parent`, `is_dir`, `status`, `type_identifier`,
  `media_type`, `title`, `description`, `size` and `updated`, and `key` is the Id.
- **AC-2** Catalog of the whole store
  WHEN `ns-search/catalog` is evaluated with no input
  THEN the catalog starts at the root key.
- **AC-3** Folder only
  WHEN `catalog-f` is evaluated
  THEN only the folder's direct entries are listed, subfolders as rows with `is_dir = true`.
- **AC-4** Nothing is evaluated
  WHEN a listed key exists only as a recipe, or its asset is volatile or unfinished
  THEN its row shows that status (e.g. `Recipe`), its `content` is null, and no asset is started.
- **AC-5** Content
  WHEN `catalog-t-t` (recursive, content) is evaluated
  THEN the `content` column holds the stored text of every entry with a text media type, truncated
  to `max_bytes`, and null for everything else.
- **AC-6** Ranked search
  WHEN `…/ns-search/search-<terms>` is evaluated
  THEN only rows that match every term in the search fields are kept, sorted by descending
  `score`, at most `limit` (50) rows, with `score` and `excerpt` columns appended.
- **AC-7** Syntax
  WHEN the expression uses `"a phrase"`, `-term`, `a | b`, `(…)` or `field:value` (`*` and `?`
  allowed in the value)
  THEN each has its usual meaning, and input that is not syntax is taken as a literal term.
- **AC-8** Case sensitivity
  WHEN `case_sensitive` is true
  THEN terms, phrases and text `field:value` tests match case-sensitively; otherwise they ignore
  case.
- **AC-9** Explicit fields
  WHEN field names follow the other arguments (`search-x-t-f-50-title-content`)
  THEN free-text terms are matched only in those columns, and a name the schema lacks is an error.
- **AC-10** Default fields
  WHEN no fields are given
  THEN terms are matched in whichever of `filename`, `title`, `description`, `doc` and `content`
  exist. If none exist, they are matched in the schema's text-role fields, and failing those, in
  every Text column.
- **AC-11** Inputs search accepts
  WHEN `search` receives a `RecordView` or `RecordSource`, a folder key (or no input), a key or
  query value, or CSV-like bytes or text whose metadata names a table format
  THEN it searches, respectively, those records, the folder's catalog, the evaluated value, or the
  parsed table.
- **AC-12** Streaming filter
  WHEN `rank` is false and the input is a `RecordSource`
  THEN the result is a `RecordSource` filtered lazily, chunk by chunk, never materialized, without
  `score` or `excerpt`, and stopping after `limit` matches.
- **AC-13** Command catalog
  WHEN `ns-search/commands` or `ns-search/commands-<namespace>` is evaluated
  THEN the result has one row per registered command (or per command in that namespace), and
  `ns-search/commands/search-<word>` finds commands by label, signature or documentation with the
  default fields.
- **AC-14** Unknown field in an expression
  WHEN `nosuch:value` appears in an expression
  THEN that clause matches nothing, the search does not fail, and a warning naming the field is
  logged.
- **AC-15** Filter-only queries keep order
  WHEN an expression has no free-text terms (only `field:value` tests), or is empty
  THEN rows keep their input order. For a catalog that is key order.

**Non-goals:**
- a persistent index, an external engine, vector or semantic search; all of these belong to
  [`external-index-sync`](../external-index-sync/);
- a selection method on `AsyncStore` (`STORE-NO-CONTENT-OR-METADATA-SEARCH` is closed, not
  planned);
- front-matter fields;
- passing arguments by name over HTTP (`QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`).

## Core Interactions

- **Assets:** `catalog` lists entries through `AssetManager`'s `listdir_asset_info` and
  `listdir_keys_deep`, so recipe-declared keys are included. It describes each entry with
  `get_asset_info`, which no longer starts evaluation. Content comes straight from the store's
  bytes, never from the asset manager. *A search never evaluates*; only an explicit key or query
  input is evaluated, because the caller asked for it.
- **Records:**
  - The predicate, the syntax parser, matching kernels, BM25 scoring and a filtering
    `RecordSource` are added to `liquers-records`.
  - Results are views built with the existing `filter`, `take` and `with_columns`.
  - Search is the first consumer of `FieldRole` and of `RecordSchema::resolve_field`.
- **Commands:** a new `search` namespace with `catalog`, `commands` and `search`.
- **Query and Store:** no change.
- **Web/API:** no change. Over HTTP the path carries the escaped expression. Passing arguments by
  name is filed separately.

## Crate Placement

- **`liquers-records`** gets the search engine itself: syntax, predicate, kernels, scoring and the
  filtering source. It is pure, depends on `liquers-core` only, and builds for wasm, so the
  browser build gets search too.
- **`liquers-lib`**, behind the `records` feature, gets the three commands and the state-to-records
  conversion. They need `Value`, `Context` and the asset manager.

## Documentation Intent

- Reference: new `specs/reference/SEARCH.md`. It covers the syntax, the matching and ranking
  rules, the default fields, the catalog and command-catalog schemas, the accepted inputs, and the
  non-evaluation rule.
- Guide: none. The reference's examples are enough for a syntax this small.
- Documents to update:
  - `specs/reference/RECORD_STREAMS.md`, since search is the first consumer of roles and
    `resolve_field`;
  - `specs/command_registry.yaml` (regenerated);
  - `specs/README.md`, changing the capability from designing to built.

## Open Questions

1. When `search` receives a folder, should its implicit catalog read content? Recommended: **no**.
   Reading content costs a store read per entry, so asking for it should be explicit
   (`catalog-t-t/search-…`), and the reference says so.
2. Should `catalog` supersede `ns-rec/file_records`, which lists one folder with four columns?
   Recommended: keep `file_records` unchanged and share the listing code.
3. The `status` column is the **asset** lifecycle (`Ready`, `Recipe`, …), not a document's
   front-matter status. Recommended: keep the name. Front-matter fields arrive later under an
   `attr.` prefix, once `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` exists.

## References

- `specs/issues/STORE-NO-CONTENT-OR-METADATA-SEARCH.md`, closed as superseded by this design
- [`use-cases.md`](./use-cases.md), [`research-questions.md`](./research-questions.md) and
  [`options-analysis.md`](./options-analysis.md): background
- [`external-index-sync`](../external-index-sync/): engines and persistent indexes
- `specs/reference/RECORD_STREAMS.md`: records
- `specs/design/agent-memory-mvp/`: the first consumer
