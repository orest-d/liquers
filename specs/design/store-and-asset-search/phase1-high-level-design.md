# Phase 1: High-Level Design — Store and asset search

> **Revision 8, 2026-10-10; amended 2026-10-11** with nested field predicates, a trait-based
> predicate with pluggable parsers, and the gaps found by checking revision 8 against revision 7's
> requirements and agent memory's needs (`DESIGN.md` §"Coverage check").
>
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
  WHEN the expression uses `"a phrase"`, `-term`, `a | b`, `(…)`, `field:value` (`*` and `?`
  allowed, so `field:*` means "present"), or `field:>v` / `>=` / `<` / `<=`
  THEN each has its usual meaning. Input that does not parse is searched as plain words, and a
  warning gives the position where parsing failed.
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
  THEN the result has one row per registered command (or per command in that namespace). Each row
  has a `query` column holding the fragment that calls the command (`ns-rec/to_json`), and
  `ns-search/commands/search-<word>` finds commands by name, namespace, realm, module, label,
  signature or documentation with no field list.
- **AC-14** Unknown field in an expression
  WHEN `nosuch:value` appears in an expression
  THEN that clause matches nothing, the search does not fail, and a warning naming the field is
  logged.
- **AC-15** Order is kept unless ranked
  WHEN `rank` is false, or the expression has no free-text terms (only field tests), or it is
  empty
  THEN rows keep their input order. For a catalog that is key order.
- **AC-16** Nested predicate on a text field
  WHEN the expression has `field:(…)`, such as `content:(expiration -draft)`
  THEN the inner predicate is applied to that field's text alone.
- **AC-17** Structured text inside a field
  WHEN `field:(…)` contains field tests, or a dotted name such as `content.status:draft` is used
  THEN the field's text is parsed only where that is reliable: as JSON (whole text, verified by
  parsing) or as Markdown front-matter (a leading `---` YAML or `+++` TOML block, closed and parsed
  to a map). The inner tests navigate the parsed value, and a list matches when any of its
  elements does. Anything else does not match, and is counted in one warning.
- **AC-23** Explicit structure format
  WHEN the format is named, as in `field:yaml(…)`, `field:json(…)`, `field:frontmatter(…)`,
  `field:fenced_yaml(…)` or `field:fenced_json(…)`
  THEN the text is parsed that way only. `fenced_*` means the first fenced code block of that
  language in a Markdown document.
- **AC-18** Budget honesty
  WHEN `limit` stops a search before every match is returned
  THEN the result's description says `N of M matches shown` (or `first N matches; more may exist`
  for a stream), and the same is logged as Info.
- **AC-19** Match evidence
  WHEN `rank` is true
  THEN a `matched` column lists the search fields in which a positive term matched, and `excerpt`
  skips a leading front-matter block.
- **AC-20** Catalog size bound
  WHEN a folder has more than `max_entries` entries (default 100 000)
  THEN `catalog` stops there, says so in its description, and logs it.
- **AC-21** Pluggable predicate language
  WHEN a Rust caller parses an expression with another language (a type implementing the
  predicate syntax trait) and passes it to the search kernel
  THEN it filters and ranks exactly as the built-in language does. The built-in parser's error
  for malformed input carries a `Position`.
- **AC-24** Extracting fields from documents
  WHEN `…/ns-search/field_extract` is evaluated (extension `md` by default)
  THEN only rows whose file has that extension are kept. Each file's structure is parsed by its
  format: `md` as front-matter (YAML `---` or TOML `+++`), `json` as JSON, `yaml` or `yml` as YAML.
  Its top-level fields (or the ones named) become columns, and files with no fields are dropped.
- **AC-25** Extraction options
  WHEN `prefix` is set, or `flatten` is true
  THEN the extracted column names start with the prefix, and nested maps become `parent_child`
  columns. Without `flatten`, a nested map is one column of JSON text, reachable as
  `column.child:value`. A list is JSON text, and a field test on it matches any element.
- **AC-26** Extracted columns replace same-named ones
  WHEN an extracted field has the same name as an input column (front-matter `status` or `title`
  in a catalog)
  THEN the extracted column replaces it, keeping that column's role, and a log entry names the
  replaced columns. The Id column is never replaced; such a field is skipped with a warning.
- ~~**AC-27** Simple filter commands~~ — moved on 2026-10-11 to
  [`record-filter-commands`](../record-filter-commands/), where it is AC-1 to AC-7.
- **AC-22** Browser build
  WHEN `liquers-lib` is built for `wasm32` with `records`
  THEN the `search` commands are registered, and the build matrix covers it.

**Non-goals:**
- a persistent index, an external engine, vector or semantic search; all of these belong to
  [`external-index-sync`](../external-index-sync/);
- a selection method on `AsyncStore` (`STORE-NO-CONTENT-OR-METADATA-SEARCH` is closed, not
  planned);
- front-matter stored as asset metadata (`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`). Front-matter
  is read from content, by `field_extract` (AC-24) or by a nested predicate (AC-17);
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
- **Commands:** a new `search` namespace with `catalog`, `commands` and `search`. An MCP tool
  surface is `agent-memory-mvp`'s; it builds these queries. The predicate's transport form is its
  syntax string, so no structured-predicate argument is added.
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
  - `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: a command that produces records for search must
    not evaluate other assets;
  - `specs/reference/RECORD_STREAMS.md`, since search is the first consumer of roles and
    `resolve_field`;
  - `specs/command_registry.yaml` (regenerated);
  - `specs/README.md`, changing the capability from designing to built.

## Open Questions

Answered by the user on 2026-10-11:

1. ~~Should the implicit catalog read content?~~ **No.** Content is read only when asked for with
   `catalog-t-t/search-…`.
2. ~~Should `catalog` supersede `ns-rec/file_records`?~~ **Keep `file_records` for now.**
3. ~~Rename `status`?~~ **Keep it** as the asset lifecycle; a document's status is reached
   through its parsed content.
4. ~~Selecting a predicate language?~~ **Not yet.** In future it may be a `search` argument naming
   a query-language dialect.
5. ~~Paging with `offset`?~~ **Not for now.**

6. ~~How is a structured text field recognised?~~ **Layered, decided 2026-10-11.**
   - Automatic recognition is used only where it is reliable: JSON and front-matter (AC-17).
   - The format can be named explicitly, including `fenced_yaml` and `fenced_json` (AC-23).
   - Document collections go through `field_extract`. It takes the format from the file
     extension, an enum of `md`, `json`, `yaml` and `yml`, defaulting to `md` (AC-24 to AC-26).
     Only `md`, `json` and `yaml` structures are supported, and TOML front-matter is included
     under `md`.
   - Simple `ns-rec` filters moved to [`record-filter-commands`](../record-filter-commands/).

7. ~~`field_extract`'s `flatten` default?~~ **False** (user, 2026-10-11).
8. ~~Roles of extracted columns?~~ Roles do not limit `search`: any column can be named in a field
   test or in the field list. Roles only choose the *default* free-text fields and matter to an
   external engine. A command that sets roles belongs to
   [`external-index-sync`](../external-index-sync/) (user, 2026-10-11).

## References

- `specs/issues/STORE-NO-CONTENT-OR-METADATA-SEARCH.md`, closed as superseded by this design
- [`use-cases.md`](./use-cases.md), [`research-questions.md`](./research-questions.md) and
  [`options-analysis.md`](./options-analysis.md): background
- [`external-index-sync`](../external-index-sync/): engines and persistent indexes
- `specs/reference/RECORD_STREAMS.md`: records
- `specs/design/agent-memory-mvp/`: the first consumer
