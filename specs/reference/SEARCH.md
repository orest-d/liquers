---
title: Search
kind: reference
audience: internal
area: [records, lib/commands]
reviewed: 2026-10-17
---

# Search

The `ns-search` commands and the search engine under them: what each command takes and returns, the
expression grammar, matching and ranking rules, and the extension point for other query languages.
Task-oriented usage is in [`guides/SEARCH_GUIDE.md`](../guides/SEARCH_GUIDE.md); the design is
`design/store-and-asset-search/`.

| Part | Where |
|---|---|
| Predicate trait, built-in language, kernels, BM25, structure parsing, `FilteredSource` | `liquers-records/src/search/` (no feature gate; wasm-clean) |
| The five commands, `folder_of`, `search_input`, `read_text`, `register_search_commands!` | `liquers-lib/src/search/` (feature `records`) |

## Commands

Namespace `search`, registered by `register_search_commands!` inside `register_all_commands!`
(a no-op without `records`).

```
catalog(state, recursive: bool = true, content: bool = false, max_bytes: i64 = 262144, max_entries: i64 = 100000)
commands(namespace: String = "")
search(state, query: String = "", rank: bool = true, case_sensitive: bool = false, limit: i64 = 50, fields: Vec<String>...)
field_extract(state, extension: String = "md" (md|json|yaml|yml), prefix: String = "", flatten: bool = false, fields: Vec<String>...)
md_split(state, unit: String = "section" (section|line))
```

`limit = 0`, `max_bytes = 0` and `max_entries = 0` mean no limit; negative values are errors.

**`ns-rec/filter-<expr>`** is `search-<expr>` with `rank=false` and no limit, over any record input.
The `ns-rec` single-column filters (`eq`, `starts_with`, `is_in`, `is_null`, …) build one node of
this language in code — `SearchExpr::field(column, FieldTest::…)`, with the literal constructors
`FieldTest::prefix` / `suffix` / `substring` (regex-escaped) — so nothing is parsed or escaped; they
wrap it in `StrictPredicate`, which turns a missing column into an error instead of a warning
([`RECORD_STREAMS.md`](RECORD_STREAMS.md) §"`ns-rec` commands").

## Inputs

**`folder_of`** — the folder a state denotes (`catalog`, and the folder case of the others):

| Input state | Folder |
|---|---|
| no value and no key | the root key |
| a `dir` state, or an `AssetInfo` listing (`-R-dir`, `-R-sdir`) | the key in its metadata |
| a `Key` value (`-R-key/…`), or a `Query` value that is a plain key, naming a directory | that key |
| anything else | none — `catalog` fails with a conversion error |

**`search_input`** — the records `search` works on: a folder → its catalog (recursive, no
content); a `RecordView` or `RecordSource` → itself; a `Key` or `Query` value → evaluated, then
converted again; anything else → `records::to_record_source` (tables in their declared format, JSON,
manifests), a non-manifest result materialized into a view.

**`field_extract` and `md_split`** take a folder (walked like `catalog`, without content) or
records with a `key` column (optionally `filename` and `content`), taken to be a catalog. Text comes
from `content` when present, otherwise from `read_text`.

## Non-evaluation

Listings go through `Context::record_listing` (the listing half of `-R-dir/<folder>`, built on
`AssetManager::listdir`) and `get_asset_info`, which never start an asset, so a key only a recipe
declares is listed with status `Recipe`. `read_text` reads **stored bytes only**:
a non-directory entry whose status is `Ready`, `Source` or `Override` and whose media type is text
(`text/*`, `application/json`, `application/x-yaml`, `application/x-toml`, `application/xml`, or a
`+json` / `+xml` type; from `AssetInfo.media_type`, else the file extension). Bytes are cut at a
UTF-8 boundary after `max_bytes`; non-UTF-8 bytes are not text. The only evaluation is of an
explicit `Key` or `Query` value given to `search`.

## Dependencies

A result records what it read, so it is reused while that stays the same and recomputed when it
changes through the asset manager (a write, `set_binary`, `set_state`, a removal):

| Read | Recorded | Expires the result when |
|---|---|---|
| Each folder walked (`catalog`, and every command given a folder) | `-R-dir/<folder>` at its listing version (`Context::record_listing`) | an entry is added to or removed from that folder |
| Each file a catalog lists, its text read or not | `-R/<key>` at `AssetManager::version` (metadata only), read before its info, so the recorded version is never newer than the columns | the file is written or removed |
| Each file whose text is read (`read_text`) | `-R/<key>` at the version its bytes were read at: a live asset's bytes and version from one snapshot in memory (it may be `Ready` before its write reaches the store); otherwise from the store, read again if the version moved, at most three times, else recorded as unknown | as above |
| An explicit `Key` or `Query` value given to `search` | its query, through `Context::get_dependency_state` | it is recomputed |

A recipe-only entry records the version its metadata gives, and nothing evaluates it. Records
reach a chain built on the command (`…/catalog-t-t/search-x`) through core: a non-keyed dependency
hands its dependent the records its run added (`DEPENDENCIES_STATUS.md`, row `Evaluate(query)`), and
a keyed result stores them, so a stored index is checked against them when it is loaded.

Not covered: edits made behind the asset manager's back while it runs
(`ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE`); a file written into a **new** subfolder
(`NEW-FOLDER-WRITE-DOES-NOT-REFRESH-THE-PARENT-LISTING`); a recipe-only entry that later becomes
`Ready` (a first registration is not a change); a write that lands between reading a version and
recording it (`RUNTIME-DEPENDENCY-RECORD-AGAINST-SUPERSEDED-VERSION-IS-NOT-CHECKED`). Results stored
before 2026-10-17 carry no records and are recomputed only once something else expires them.

Cost (`design/search-read-dependencies/performance.md`): one version read per entry, one listing
per folder. At 5 000 files (debug build, cold) a catalog takes about 1.5x its earlier time on a
memory store and on a file store, where the version read is most of it
(`SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE`); the first load of a stored keyed index in a
process reads the metadata of every file it recorded.

## Grammar

```text
expr       = ws , or_expr , ws ;
or_expr    = and_expr , { ws , "|" , ws , and_expr } ;
and_expr   = unary , { ws1 , unary } ;
unary      = "-" , unary | atom ;
atom       = group | field_test | phrase | regex | term ;
group      = "(" , expr , ")" ;
field_test = field_path , ":" , ( group | regex | [ cmp_op ] , ( phrase | value ) ) ;
field_path = name , { "." , name } ;            (* name: letter or _, then letters, digits, _ *)
cmp_op     = ">=" | "<=" | ">" | "<" ;
phrase     = '"' , { char | '\"' | "\\" } , '"' ;
term, value = text up to whitespace, "(", ")", "|" or '"' ;
```

- An empty expression matches everything. Juxtaposition binds tighter than `|`; `-` tightest.
- **When `/` is a regex:** only an atom or value that *begins* with `/`, and only when an
  unescaped `/` is followed by a delimiter (whitespace, `)`, `|`, end); the body ends at the
  **first** such `/`. So `area:core/store` is a value, `/a/ /b/` is two regexes, `/a/b/` is the
  regex `a/b`, `path:/usr/bin` and `/unclosed` are plain text.
- An empty value (`f:`) is not a field test; it is the term `f:`.
- `SearchExpr::try_from` fails on malformed input with `Error::parse_error(..).with_position(..)`;
  positions are offsets into the decoded expression. `search` parses leniently: on failure it
  searches the whitespace-separated words (operator characters stripped) and logs the error as a
  warning.

Escapes for writing an expression into a query: `guides/SEARCH_GUIDE.md` §4.

## Matching

- **Search fields** (where terms, phrases and regexes are matched): the explicit `fields`; else the
  schema's text-role fields (`RecordSchema::text_fields`); else whichever of `filename`, `title`,
  `description`, `doc`, `content` exist; else every Text column. An explicit name not in the schema
  is an error.
- **Terms** match as substrings, **phrases** as whitespace-normalised substrings, both lower-cased
  unless `case_sensitive`. **Regexes** use the `regex` crate (linear time), with `(?i)` unless
  `case_sensitive`.
- **Field paths:** for `a.b`, the column is the longest prefix `RecordSchema::resolve_field`
  finds (with its `meta.` / `attr.` / `key.` expansion); the rest is a JSON path. A field test on a
  missing column is a warning and matches nothing.
- **Text columns:** `f:v` is an anchored wildcard match (`*`, `?`), case-folded unless
  `case_sensitive`; `f:>v` compares lexicographically and ignores `case_sensitive`; `f:/re/` is a
  regex.
- **Other columns:** `f:*` means present; any other wildcard, or a regex, is an error; a value is
  parsed with `FieldValue::parse_text` for the column's type and compared with `Column::compare`.
  A date-only value against a Timestamp column means midnight UTC.
- **JSON columns** — Text columns whose `FieldSchema::media_type` is `application/json`
  (`field_extract` writes lists and, without `flatten`, maps this way): a glob, compare or regex on
  a cell holding an array matches when any element matches; `f:(…)` with field tests, or a dotted
  path into the column, parses each cell (once per column per evaluation) and walks it — an array
  matches when any element does, a term matches any string leaf. An invalid cell does not match
  and is counted in one warning. Nested field tests on a column not declared JSON are an error.
- **`f:(…)` without field tests** is the inner expression with `f` as its only search field.
- A null never matches; `-x` matches where `x` does not, nulls included.

## Ranking and results

`search_view` filters, then — with `rank` — scores the surviving rows with BM25 and sorts them
stably by descending score, then applies the limit.

| Setting | Value |
|---|---|
| BM25 | k1 = 1.2, b = 0.75 |
| Statistics | from the rows that survive the filter |
| Tokens (document length) | split on non-alphanumerics |
| Term frequency | occurrence count; match count for a regex |
| Field weights | `title` 3; `filename`, `name`, `description` 2; others 1 |
| Phrase | counts double |

With `rank`, three columns are appended (an input's old ones are dropped first): `score` (Float),
`matched` (Text: the search fields where a positive term matched, comma-separated), `excerpt` (Text:
about 160 characters around the first hit in the first matching field, skipping a leading
front-matter block). An expression with no positive term, and every `rank=false` search, keeps the
input order.

The result's description is `N of M matches shown` (also logged as Info). With `rank=false` over a
`RecordSource`, the result is a lazily filtered `RecordSource` described as `first N matches; more
may exist` (or `all matches, filtered lazily`). Ranking a source materializes it; past
`DEFAULT_MATERIALIZE_MAX_ROWS` that fails with the advice to use `rank=false`.

## Schemas

**`catalog`** (rows sorted by key; directories included):

| Column | Type | Role |
|---|---|---|
| `key` | Text | Id |
| `filename`, `title`, `description` | Text | text + stored |
| `extension`, `parent`, `status` (the asset status: `Ready`, `Recipe`, …), `type_identifier`, `media_type` | Text | keyword |
| `is_dir` | Bool | keyword |
| `size` | Int, nullable | numeric |
| `updated` | Timestamp, nullable | numeric |
| `content` | Text, nullable; only with `content=true` | text |

A `.md` entry whose metadata has no title gets its first `# ` heading as `title` when content is
read. The walk is breadth-first and stops at `max_entries` (the description and an Info entry say
so); `get_asset_info` runs 16 calls at a time.

**`commands`** (sorted by namespace, name, realm): `id` (Id; `CommandKey` display), `query`
(`ns-<namespace>/<name>`, or the bare name for the empty or `root` namespace), `realm`,
`namespace`, `name`, `module`, `title` (label), `description` (the signature), `doc` — all text —
plus `volatile` (Bool) and `output_filename`.

**`field_extract`**: the input's columns for the files kept, then one column per field:
- files without the extension, or without fields (with `fields`: without any of them), are dropped
  (counted in an Info entry);
- `md` reads front-matter — YAML between `---` and `---`/`...`, or TOML between `+++` lines;
  `json` and `yaml`/`yml` read the whole file; the result must be a non-empty map;
- with `fields`, only those dotted paths (named with `_` for `.`); otherwise every top-level key;
- names keep `[A-Za-z0-9_]` (others become `_`; a leading digit gets `_`), after `prefix`; when two
  fields of a file get the same name (`a-b` and `a_b`), the first in key order is kept and a
  warning names the column;
- `flatten` turns nested maps into `parent_child` columns; lists stay JSON;
- types unify per column: Int, UInt (integers above `i64::MAX`), Float, Bool when every value is
  one, else Text — a column mixing an integer above `i64::MAX` with negatives or floats is Text, so
  no value is rounded;
- a field named like an input column **replaces** it, keeping its role (an Info entry names the
  replaced columns); the Id column is never replaced (a warning names the skipped field);
- YAML keys that are not scalars are skipped and counted; TOML datetimes become ISO strings.

**`md_split`**: `id` (Id, `key#n`), `key`, `filename`, `n` (section number from 0, or line
number), `line` and `end_line` (1-based), `heading` (`H1 > H2` path; sections only), `level`
(sections only), `text`. Sections: `.md` files only; a section runs from its heading to the next
heading of the same or a higher level, so it contains its subsections; text before the first heading
is section 0; lines in fenced code blocks are never headings. Lines: every text file.

## The predicate trait

```rust
pub trait Predicate: Debug + MaybeSend + MaybeSync + 'static {
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error>;
    fn matches_json(&self, value: &serde_json::Value, options: &SearchOptions) -> Result<bool, Error>;
    fn has_field_tests(&self) -> bool;
    fn rank_terms(&self) -> Vec<RankTerm>;
    fn to_syntax(&self) -> String;
}
pub trait PredicateSyntax: Predicate + Sized + for<'a> TryFrom<&'a str, Error = Error> {
    const LANGUAGE: &'static str;
}
pub trait BoundPredicate: MaybeSend + MaybeSync {
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>;
    fn evidence(&self, _view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error>; // default: none
    fn take_warnings(&self) -> Vec<String>;                                           // default: none
}
```

**Adding a query language:** implement `Predicate` (and `PredicateSyntax` to parse with
`TryFrom<&str>`) and pass it to `search_view` or `FilteredSource`; ranking, evidence and the limit
work unchanged (`source.rs::custom_predicate_type_filters_and_ranks_like_builtin`). A command
argument choosing the dialect is future work. `to_syntax` is the transport form (logs, recipes,
external engines).

## `FilteredSource`

A `RecordSource` filtering an inner source lazily: each view passes through `filter(mask)`
(zero-copy), empty views are skipped, and the stream stops after `limit` rows — a stop part-way
through a chunk cuts that view. The predicate is re-bound when a view's schema differs. `chunks`
and `describe_chunk` describe the **unfiltered** input. `truncated()` is the inner value or a limit
stop. Bind and evaluation warnings are kept; `warnings()` reads them. `ns-rec/head` over a source
is a `FilteredSource` with `SearchExpr::always()` and `limit = n`.

## Errors and warnings

| Situation | Outcome |
|---|---|
| Malformed expression or invalid regex (`try_from`) | `Error::parse_error` with the position |
| The same in `search` | a warning; the words are searched |
| Input that is not a folder (`catalog`) or not records (`field_extract`, `md_split`) | `Error::conversion_error_with_message` |
| Explicit search field missing; ambiguous field name | `Error::general_error`, or `resolve_field`'s error |
| Field test on a missing column | a warning; no match (under `StrictPredicate`, as the `ns-rec` filters use: `Error::general_error` naming the available fields) |
| Wildcard or regex on a non-Text column; nested field tests on a non-JSON column; unparseable compare value | `Error::general_error` naming the field and position |
| Invalid cell in a JSON column | no match; one warning with the count |
| Negative `limit` / `max_bytes` / `max_entries` | `Error::general_error` |

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-17 | `## Dependencies`: results record the folders and files they read and expire when those change; listings through `Context::record_listing`. | `design/search-read-dependencies/` |
| 2026-10-13 | `ns-rec/filter` as the unranked form; programmatic `SearchExpr` / `FieldTest` constructors and `StrictPredicate` used by the `ns-rec` filters; `FilteredSource` behind `ns-rec/head`. | `design/record-filter-commands/` |
| 2026-10-11 | `field_extract`: a file with none of the requested fields is dropped; same-name collisions keep the first and warn; integers above `i64::MAX` are UInt or Text, never rounded (PR review). | phase-5, `design/store-and-asset-search/` |
| 2026-10-11 | Created with the `ns-search` commands and `liquers-records` search. | phase-5, `design/store-and-asset-search/` |
