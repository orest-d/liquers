---
title: Search Guide
kind: guide
audience: both
area: [lib/commands, records]
reviewed: 2026-10-17
---

# Search Guide

How to find things in a Liquers store with the `ns-search` commands: files by folder or name,
documents by their words or their front-matter, lines by a regular expression, sections of
Markdown, and commands. The guide is for people and agents alike; [§6](#6-for-agents-and-tool-builders)
adds what an agent or a tool that builds queries needs. The exact rules (grammar, ranking constants,
schemas) are in [`reference/SEARCH.md`](../reference/SEARCH.md).

Every query below is run against a fixture store by
`liquers-lib/tests/search_commands.rs::guide_queries_run`, so this guide cannot drift from the
commands.

## 1. The idea in one paragraph

Search works on **records** (tables). Five commands make or filter them:

| Command | Makes |
|---|---|
| `ns-search/catalog` | One row per entry of a folder: `key`, `filename`, `extension`, `status`, `title`, `description`, … and, with `content=true`, the text |
| `ns-search/field_extract` | One row per document with its front-matter (or JSON / YAML) fields as columns |
| `ns-search/md_split` | One row per Markdown section, or per line of any text file |
| `ns-search/commands` | One row per registered command |
| `ns-search/search` | The rows of its input that match an expression, ranked by relevance |

A query chains them: *list, (extract fields), search*. **Nothing is evaluated** while listing or
reading: a key that only a recipe declares appears with status `Recipe` and is never run.

## 2. Recipes

The folder comes from the input: `-R-key/<folder>/-/…`. With no input, the whole store is used.

**List a folder** (recursively; `catalog-f` for one level):

```
-R-key/specs/issues/-/ns-search/catalog
```

**Find files by path** — a glob on `key` (`*` crosses `/`), unranked, all matches:

```
-R-key/specs/-/ns-search/catalog/search-key~ncolon~specs~/issues~/~nast~.md-f-f-0
```

decoded: `key:specs/issues/*.md`, `rank=false`, `case_sensitive=false`, `limit=0`.

**Full-text search, best first** (`catalog-t-t` reads the content):

```
-R-key/specs/issues/-/ns-search/catalog-t-t/search-expiration~.safety
```

**Filter by front-matter** — `field_extract` reads it straight from the folder; its fields become
columns, and a front-matter `status` replaces the catalog's own:

```
-R-key/specs/issues/-/ns-search/field_extract/search-status~ncolon~draft-f-f-0
```

A list field matches when any element does: `area:core/assets`

```
-R-key/specs/issues/-/ns-search/field_extract/search-area~ncolon~core~/assets-f-f-0
```

**Words and front-matter together** — open issues about expiration:

```
-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract/search-expiration~.~_status~ncolon~closed~nast~
```

decoded: `expiration -status:closed*`.

**Matching lines with line numbers** (`grep -n`) — `md_split-line` reads any text file:

```
-R-key/src/-/ns-search/md_split-line/search-~/fn~.get_asset_info~/-f-f-0-text
```

decoded: `/fn get_asset_info/` matched in the `text` column; each row has `key`, `filename` and
`line`.

**A section of a Markdown document** — by heading path:

```
-R-key/specs/design/-/ns-search/md_split/search-heading~ncolon~~nast~Purpose-f-f-0
```

**Which command does X?** — and one namespace's commands:

```
ns-search/commands/search-record
ns-search/commands-rec
```

Each row's `query` column is the fragment that calls the command (`ns-rec/to_record`), and
`description` is its signature.

**Search a table** — any records, a stored CSV included:

```
-R/data/table.csv/-/ns-search/search-france-f-f-0
```

## 3. Writing an expression

| Write | Means |
|---|---|
| `word` | the word appears (case-insensitive) in a search field |
| `a b` | both |
| `a \| b` | either |
| `-a` | not |
| `( … )` | grouping |
| `"a phrase"` | the words in order |
| `/re/` | a regular expression |
| `field:value` | the column equals `value`; `*` and `?` are wildcards; `field:*` means "has a value" |
| `field:>v` `>=` `<` `<=` | comparison (numbers, dates; text compares alphabetically) |
| `field:/re/` | the column matches the regex |
| `field:( … )` | the inner expression, restricted to that column |
| `attr.status:draft` | a path inside a JSON column, as `field_extract` writes lists and maps |

**Search fields** — where plain words are looked for — are the schema's text fields
(`filename`, `title`, `description`, `content` in a catalog), or the fields you name at the end of
the `search` arguments: `search-expiration-t-f-50-title-content`.

Arguments of `search`, in order: `query`, `rank` (`t`), `case_sensitive` (`f`), `limit` (`50`;
`0` = all), then the field names. To set `limit`, give the two before it: `search-x-f-f-0`.

## 4. Writing it into a query

Only letters, digits, `_`, `+` and `.` survive unescaped in a query parameter. The rest is escaped
(`guides/QUERY_ESCAPING_GUIDE.md`):

| Character | Escape | Character | Escape |
|---|---|---|---|
| space | `~.` | `-` | `~_` |
| `:` | `~ncolon~` | `/` | `~/` |
| `*` | `~nast~` | `"` | `~nquot~` |
| `(` `)` | `~nlpar~` `~nrpar~` | `\|` | `~nverbar~` |
| `>` `<` | `~ngt~` `~nlt~` | `\` | `~nbsol~` |

Check a query before using it — the decoded expression is in the output:

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- '<query>'
```

## 5. Reading results

- **Ranked results** (`rank=true`, the default) gain three columns: `score` (BM25), `matched` (the
  fields a word was found in) and `excerpt` (text around the first hit, skipping front-matter).
- **The description** of the result says `N of M matches shown`, so a cut-off list is never
  silent. A lazily filtered stream says `first N matches; more may exist`.
- **Fetch a hit** by its key: `-R/specs/issues/A-EXPIRY.md`. A `md_split` row adds `line` and
  `end_line` for reading part of the file.
- **Field names** of any result: `…/ns-rec/records_schema`.

## 6. For agents and tool builders

- **Build queries with `liquers_core::query::encode_token`** (or `ActionRequest`), never by hand:
  an expression with spaces, colons or slashes needs the escapes of §4.
- **Validate with `liquers-validate`** before sending; compare its decoded parameter with what you
  meant. Positional arguments are the usual mistake: `search-x-f-0` sets `case_sensitive`, not
  `limit`.
- **Keep limits small and read in tiers**: rank, take the top few rows' `title`, `description` and
  `excerpt`, and fetch the full text only of what you choose.
- **Exhaustive filters**: `rank=false` with `limit=0` returns every match in input (key) order.
- **Reading content is the cost**: `catalog-t-t` reads every text file in the folder. Narrow the
  folder first, or search names and titles (`catalog` without content) when that is enough.
- **Malformed expressions do not fail**: the words are searched and a warning with the position is
  logged — check the log when results look wrong.
- **Duplicate check before filing**: rank titles only — `…/field_extract/search-<words>-t-f-5-title`.

## 7. Limits and pitfalls

- `status` in a plain catalog is the **asset** status (`Ready`, `Recipe`, …). A document's own
  status comes from `field_extract`.
- A result is cached, and recomputed when a folder it walked or a file it read changes through
  Liquers (a write or removal through the asset manager), chains and recipe keys built on it
  included. Not seen: edits made behind Liquers' back while it runs, and a file written into a
  **new** subfolder (`issues/NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING.md`). Details:
  `reference/SEARCH.md` §Dependencies.
- A recomputed search reads the folder again: there is no index yet
  (`design/external-index-sync/`). Semantic search is not available.
- A column name with spaces or `-` cannot be named in an expression.
- `md_split` is a proof of concept; richer Markdown navigation is
  `issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md`.
- Over HTTP the expression goes into the path, escaped (`issues/QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH.md`).
- `-R/<folder>/-/ns-search/catalog` fails until `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE`
  is fixed; use `-R-key/<folder>`.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-17 | §7: results are cached and recomputed when what they read changes. | `design/search-read-dependencies/` |
| 2026-10-11 | Created with the `ns-search` commands. | phase-5, `design/store-and-asset-search/` |
