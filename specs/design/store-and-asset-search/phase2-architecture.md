# Phase 2: Solution & Architecture — Store and asset search

> **Revision 8, 2026-10-10; amended 2026-10-11. Draft.** It is reviewed at the Phase 2 gate, after
> Phase 1 is re-approved. It replaces revision 7, which is archived at
> `specs/archive/2026-10-10-store-and-asset-search-rev7-phase2-architecture.md`. The 2026-10-11
> amendment adds:
> - a `Predicate` **trait**, with parsing behind `TryFrom<&str>`;
> - a formal grammar and a `nom` parser that reports positions;
> - nested and structured field predicates;
> - evidence, budget and size-bound columns and messages.
>
> A second amendment the same day adds:
> - reliable-only structure recognition, with explicit formats;
> - `field_extract`;
> - simple `ns-rec` filter commands, which later moved to `record-filter-commands`.
>
> A third amendment, also on 2026-10-11, adds regular expressions, `split` (a proof of concept) and
> Markdown title fallback. It also simplifies the design:
> - nested predicates parse JSON only, and the syntax names no formats;
> - front-matter is `field_extract`'s alone;
> - the trait loses its run-time language registry;
> - `field_extract` reads a folder itself.

## Overview

Search is a **predicate over records**: a trait object made by a parser from a syntax string, and
applied by the `search` command.

Records come from three places:
- `catalog` makes them from a store folder;
- `commands` makes them from the command registry;
- any other input is converted (a table, a stored CSV, an evaluated key or query).

Everything that does not need `Value` lives in `liquers-records`: the predicate trait, the built-in
language (its AST, grammar and `nom` parser), the matching kernels, BM25 ranking and a lazily
filtering `RecordSource`. The commands and the state-to-records conversion live in `liquers-lib`.
There is no store-trait change and no new `Value` variant.

Rejected alternatives:
- **`AsyncStore::select`.** Producing the records is what reads the corpus, so a store method would
  only push the filter down; `STORE-NO-CONTENT-OR-METADATA-SEARCH` is closed.
- **A filter pipeline.** It has no OR, and the predicate never exists as a whole.
- **`Value::Predicate`.** The syntax string is the transport form (see the conditions below).
- **A closed `enum Predicate` as the public type.** It cannot be replaced or extended by another
  language. The enum survives as the *built-in language's* AST, so inside that language every
  match is still exhaustive.

## Known-Issue Preflight

| Issue | Status | Pri | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | closed | — | The former blocker, resolved by `record-streams` | — | no | none |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | closed | — | AC-4 relies on the fix | — | no | AC-4's test guards it |
| `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` | draft | P3 | Its filtering wrapper is `FilteredSource` | no | no | Taken in; closed when built |
| `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` | draft | P3 | `-R/<folder>/-/ns-search/catalog` fails until fixed | no | no | Designed around: examples use `-R-key/<folder>`, and the `dir` state is accepted for later |
| `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH` | draft | P2 | Over HTTP the expression must be escaped into the path | no | no | Independent |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | No `attr.*` columns | no | no | Front-matter is reached through `content.…` (AC-17) |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Non-evaluation is a contract, not enforced | no | no | Tests assert it (AC-4) |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | closed_not_planned | — | Superseded | — | no | none |

## Data Structures

### The predicate trait — `liquers-records/src/search/predicate.rs`

```rust
/// A predicate over records. Object-safe: `search` holds an `Arc<dyn Predicate>` and never
/// names the language it was written in.
pub trait Predicate: Debug + MaybeSend + MaybeSync + 'static {
    /// Resolve names against one schema. Called once per distinct schema; a `FilteredSource`
    /// re-binds when a chunk's schema differs. Unknown field names are warnings, not errors.
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error>;
    /// Test one JSON value. Used when this predicate has field tests and is nested in a field
    /// holding JSON text (`attr:(status:draft)`).
    fn matches_json(&self, value: &serde_json::Value, options: &SearchOptions) -> Result<bool, Error>;
    /// Whether any clause is a field test. If none is, a nested predicate is evaluated as a
    /// search restricted to that one column, and nothing is parsed.
    fn has_field_tests(&self) -> bool;
    /// Positive terms and phrases (not under a negation), for BM25 and excerpts.
    fn rank_terms(&self) -> Vec<RankTerm>;
    /// The predicate in its own language's syntax. This is the transport form: what is logged,
    /// stored in a recipe, or handed to an external engine (`external-index-sync`).
    fn to_syntax(&self) -> String;
}

/// A language: a predicate type that can be parsed from text. It extends `TryFrom<&str>`, so
/// `SearchExpr::try_from("a -b")?` is how a language parses. `Sized` keeps it off the object.
pub trait PredicateSyntax: Predicate + Sized + for<'a> TryFrom<&'a str, Error = Error> {
    const LANGUAGE: &'static str;
}

// A run-time language registry (`PredicateLanguage`) is deferred until `search` gains the
// dialect argument (Phase 1, open question 4).

/// A predicate resolved against one schema.
pub struct Bound { pub matcher: Box<dyn BoundPredicate>, pub warnings: Vec<String> }
pub trait BoundPredicate: MaybeSend + MaybeSync {
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>;
    /// Per search field, the rows in which a positive term matched. Feeds the `matched` column.
    fn evidence(&self, view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> { Ok(vec![]) }
}

pub enum RankTerm { Term(String), Phrase(String), Regex(String) }

#[derive(Debug, Clone)]
pub struct SearchOptions { pub case_sensitive: bool, pub fields: Vec<String>, pub limit: Option<usize> }
```

### The built-in language — `liquers-records/src/search/syntax.rs`

The AST is an enum, so its matches are exhaustive. Every node carries the
`liquers_core::query::Position` it was parsed from, so warnings and errors point at the text.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchExpr { pub node: Node, pub position: Position }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Always,                                   // an empty expression
    All(Vec<SearchExpr>),
    Any(Vec<SearchExpr>),
    Not(Box<SearchExpr>),
    Term(String),
    Phrase(String),
    Regex(String),                            // `/re/`
    Field { path: Vec<String>, test: FieldTest },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Glob(String),                // `f:v`, exact unless `*` or `?`; `f:*` means present
    Compare(CompareOp, String),  // `f:>v` and the like; the value is parsed at bind time
    Regex(String),               // `f:/re/`
    Nested(Box<SearchExpr>),     // `f:( … )`
}

impl Predicate for SearchExpr { /* … */ }
impl TryFrom<&str> for SearchExpr { type Error = Error; /* strict nom parse */ }
impl PredicateSyntax for SearchExpr { const LANGUAGE: &'static str = "liquers-search"; }
impl SearchExpr {
    /// For the command. Never fails: when the strict parse fails, the input is searched as
    /// whitespace-separated words, and the error (with its position) comes back for a warning.
    pub fn parse_lenient(text: &str) -> (SearchExpr, Option<Error>);
}
```

**Grammar (EBNF).** `ws` is optional whitespace and `ws1` is required whitespace.

```ebnf
expr       = ws , or_expr , ws ;
or_expr    = and_expr , { ws , "|" , ws , and_expr } ;
and_expr   = unary , { ws1 , unary } ;
unary      = [ "-" ] , atom ;
atom       = group | field_test | phrase | regex | term ;
group      = "(" , expr , ")" ;
field_test = field_path , ":" , ( group | regex | [ cmp_op ] , ( phrase | value ) ) ;
field_path = name , { "." , name } ;
name       = ( letter | "_" ) , { letter | digit | "_" } ;
cmp_op     = ">=" | "<=" | ">" | "<" ;
phrase     = '"' , { ( char - '"' - "\" ) | '\"' | "\\" } , '"' ;
regex      = "/" , re_body , "/" , & delimiter ;   (* the closing "/" must be followed by a delimiter *)
re_body    = re_char , { re_char } ;
re_char    = ( char - "/" - "\" ) | "\" , char | "/" ;   (* see the regex rule below *)
delimiter  = ws | ")" | "|" | end ;
term       = term_char , { term_char | "-" | "/" } ;
value      = { term_char | "-" | ":" | "/" } ;   (* `key:specs/issues/*`, `area:core/store` *)
term_char  = char - ws - "(" - ")" - "|" - '"' - "-" - "/" ;
```

**When `/` starts a regular expression.** Only an atom or a field value that *begins* with `/`
can be a regex. It is one only if a later unescaped `/` is immediately followed by a delimiter
(whitespace, `)`, `|` or the end); the body runs to the **last** such `/`. Otherwise the text is an
ordinary term or value. So:

| Input | Parsed as |
|---|---|
| `area:core/store` | a field test with the value `core/store`; the value does not begin with `/` |
| `key:specs/issues/*` | a glob value |
| `a/b` | the term `a/b` |
| `/fn \w+_info/` | a regex term, `fn \w+_info` |
| `title:/^Phase [12]/` | a field regex |
| `path:/usr/bin` | the value `/usr/bin`: its only inner `/` is followed by `b`, and nothing closes it |
| `/a/b/` | the regex `a/b`; an inner `/` needs no escape |

These cases are Phase 3 parser tests.

`field_test` is tried```

`field_test` is tried before `term`, so `a:b` is a field test. Inside a `term`, `:` is an ordinary
character. Precedence from tightest to loosest: `-`, then juxtaposition (AND), then `|`.

**The parser.** It uses `nom` 8 over `nom_locate` 5's `LocatedSpan<&str>`, the same pairing as
`liquers-core/src/parse.rs`. Both crates are new dependencies of `liquers-records`, at
`liquers-core`'s versions. A failure is `Error::parse_error(..).with_position(&pos)`.

### Semantics

**Search fields** are what a `Term` or `Phrase` at the top level matches against. They are chosen
in this order:
1. the explicit list;
2. otherwise the schema's text-role fields (`RecordSchema::text_fields`);
3. otherwise whichever of `filename`, `title`, `description`, `doc` and `content` exist;
4. otherwise every Text column.

`catalog` and `commands` declare their roles, so step 2 is what applies to them. An inferred CSV
schema has no roles and falls through to step 3 or 4.

**Matching** produces one mask per leaf, combined with `and`, `or` and `not`. A null never matches.
- **Terms and phrases:** a substring test per search field, ORed. It is lowercased on both sides
  unless `case_sensitive` is set.
- **Regex:** the `regex` crate, whose matching time is linear, so a pattern cannot make a search
  hang. It is wasm-clean. `case_sensitive=false` compiles the pattern with `(?i)`. As a term it is
  tested per search field; as a field test, against that field. An invalid pattern is a parse
  error with its position.
- **Glob on a Text column:** the same substring kernel with wildcards, anchored at both ends.
- **Glob or compare on any other column:** the value is parsed to the column's `FieldType`, then
  tested with `Column::compare`.
- **`Nested` on a Text column:**
  - Without field tests (`content:(a -b)`), the inner predicate is bound with that column as its
    only search field and masked like any search. Nothing is parsed.
  - With field tests (`attr:(status:draft)`), each cell is parsed as JSON: the trimmed text must
    start with `{` or `[` and parse whole. The inner predicate runs through `matches_json`, where
    a `Field` path walks object keys, a list matches when any element does, and a term matches any
    string leaf.
  - A cell that is not JSON does not match, and is counted in one warning. Other formats are
    **not** parsed here: front-matter, YAML and TOML are `field_extract`'s job.
- **A Glob or Compare on a Text cell holding a JSON array** (`[` first, and it parses) matches when
  any element matches. This is how list columns written by `field_extract` work.
- **`Nested` on a non-Text column:** a bind error.

**Field paths.** For `a.b`, the longest prefix that names a column is the column, and the rest is a
JSON path inside it: `attr.status:draft` ≡ `attr:(status:draft)`. An exact column name always
wins.

**Ranking** is BM25 (k1 = 1.2, b = 0.75) over the rows that survive the mask.
- The statistics come from those rows. Tokens are split on non-alphanumerics, and term frequency is
  the substring occurrence count, or the match count for a regex.
- Field weights: `title` 3, `filename` 2, `name` 2, `description` 2, all others 1. A phrase counts
  double.
- The sort is stable and descending.
- An expression with no `rank_terms` keeps the input order (AC-15).

**Columns appended when `rank` is true:**
- `score` (Float);
- `matched` (Text): the comma-separated search fields that matched, from `evidence`;
- `excerpt` (Text, nullable): about 160 characters around the first match in the first matching
  field, skipping a leading front-matter block.

### Structure parsing for `field_extract` — `liquers-records/src/search/structure.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructFormat { Json, Yaml, FrontMatter }
/// The parsed structure, or `None` when the text has none in that format.
pub fn parse_structure(text: &str, format: StructFormat) -> Option<serde_json::Value>;
```

| Format | Rule |
|---|---|
| `Json` | the whole text |
| `Yaml` | the whole text; the result must be a map |
| `FrontMatter` | the first line is `---`, then a line `---` or `...`, and the block is YAML; or the first line is `+++`, then a line `+++`, and the block is TOML. The result must be a map |

The format is never guessed: `field_extract` takes it from the file extension. YAML and TOML are
converted to `serde_json::Value`, and the extracted columns hold scalars or JSON text, so the
search side only ever needs to recognise JSON, which is reliable. TOML needs `toml = "0.8"`,
already used optionally by `liquers-core`, as a plain dependency of `liquers-records`.

### Record schemas### Record schemas

**`catalog`:**

| Column | Type | Roles |
|---|---|---|
| `key` | Text | Id; keyword |
| `filename`, `title`, `description` | Text | text + stored |
| `content` | Text, nullable | text; the column exists only with `content = true` |
| `extension`, `parent`, `status`, `type_identifier`, `media_type` | Text | keyword |
| `is_dir` | Bool | keyword |
| `size` | Int, nullable | numeric |
| `updated` | Timestamp, nullable | numeric |

**`commands`:**

| Column | Type | Roles | Source |
|---|---|---|---|
| `id` | Text | Id | `CommandKey` display |
| `query` | Text | keyword | the call fragment `ns-<namespace>/<name>` |
| `realm`, `namespace`, `name`, `module` | Text | text + keyword | — |
| `title` | Text | text | `label` |
| `description` | Text | text | the generated signature with argument names, types and defaults |
| `doc` | Text | text | `doc` |
| `volatile` | Bool | keyword | — |
| `output_filename` | Text | keyword | the metadata's `filename` |

**`field_extract` output:**
- The input's columns, minus the rows it drops.
- Extracted columns, named `prefix + name`. Nested maps become `parent_child` columns with
  `flatten`, and one JSON-text column otherwise. Lists are JSON text.
- Each column's type is unified over its rows: Int if every value is an integer, Float if every
  value is a number, Bool if every value is a boolean, otherwise Text (values written as strings;
  ISO dates compare correctly as text).
- A column that replaces an input column keeps that column's role. Every other extracted column is
  keyword, so it filters but is not a default free-text field.
- Characters outside `[A-Za-z0-9_]` in a key become `_`.

`FilteredSource` (`liquers-records/src/search/source.rs`) holds
`{ inner: Arc<dyn RecordSource>, predicate: Arc<dyn Predicate>, options: SearchOptions }`.

## Trait Implementations

- **`RecordSource for FilteredSource`.**
  - `stream` passes each inner view through `filter(&mask)`, which is zero-copy. It skips empty
    views, stops at `limit`, and re-binds the predicate when a view's schema changes.
  - `chunks`, `describe_chunk` and `schema` delegate to the inner source.
  - `truncated` reports the inner source's value.
  - `manifest` is `None`.
- **`Predicate`, `PredicateSyntax` and `TryFrom<&str>` for `SearchExpr`.**
- **`PredicateLanguage` for `Language<P>`.**
- Nothing on `AsyncStore`, `AssetManager` or `Value`.

## Function Signatures

`liquers-records/src/search/`, all sync and pure:

```rust
pub fn search_fields(schema: &RecordSchema, requested: &[String]) -> Result<Vec<usize>, Error>;
pub struct SearchReport { pub matched: usize, pub shown: usize, pub warnings: Vec<String> }
/// Mask, then (when `rank`) score, sort and append `score`, `matched` and `excerpt`; then the limit.
pub fn search_view(view: &Arc<dyn RecordView>, predicate: &dyn Predicate, options: &SearchOptions,
    rank: bool) -> Result<(Arc<dyn RecordView>, SearchReport), Error>;
impl FilteredSource {
    pub fn new(inner: Arc<dyn RecordSource>, predicate: Arc<dyn Predicate>, options: SearchOptions) -> Self;
}
```

`liquers-lib/src/search/` (feature `records`). Every command is generic over
`E: Environment<Value = Value>`:

```rust
pub async fn catalog<E>(state: State<Value>, recursive: bool, content: bool, max_bytes: i64,
    max_entries: i64, context: Context<E>) -> Result<Value, Error>;
pub async fn commands<E>(namespace: String, context: Context<E>) -> Result<Value, Error>;
pub async fn search<E>(state: State<Value>, query: String, rank: bool, case_sensitive: bool,
    limit: i64, fields: Vec<String>, context: Context<E>) -> Result<Value, Error>;
pub async fn field_extract<E>(state: State<Value>, extension: String, prefix: String, flatten: bool,
    fields: Vec<String>, context: Context<E>) -> Result<Value, Error>;
pub async fn split<E>(state: State<Value>, unit: String, context: Context<E>) -> Result<Value, Error>;
pub async fn folder_of<E>(state: &State<Value>, context: &Context<E>) -> Result<Option<Key>, Error>;
pub enum SearchInput { View(Arc<dyn RecordView>), Source(Arc<dyn RecordSource>) }
pub async fn search_input<E>(state: State<Value>, context: &Context<E>) -> Result<SearchInput, Error>;
```

**`folder_of`** maps the input state to a folder:

| Input state | Folder |
|---|---|
| No value and no key | The root key |
| A `dir` state, or an `AssetInfo` listing (from `-R-dir` or `-R-sdir`) | The key in its metadata |
| A `Key`, or a `Query` whose `key()` is `Some`, that is a directory | That key |
| Anything else | `None` |

**`search_input`** turns a state into records:
- a folder becomes its catalog, recursive and without content;
- a `Key` or `Query` is evaluated, and the result is converted again;
- anything else goes through `records::convert::to_record_source`.

**What `search` returns:**

| `rank` | `View` input | `Source` input |
|---|---|---|
| true | `search_view`, giving a `RecordView` | Materialized (`DEFAULT_MATERIALIZE_MAX_ROWS`), then `search_view` |
| false | `search_view` without scoring | `FilteredSource` |

After a search, `SearchReport` sets the result's description through `context.set_description`, to
`N of M matches shown`, or `first N matches; more may exist` for a stream. It also goes into an
Info log entry, and warnings go to `context.warning`.

**`catalog`** steps:
1. Resolve the folder with `folder_of`.
2. List the entries: `listdir_keys_deep` plus `get_asset_info` (16 calls in flight), or
   `listdir_asset_info` when not recursive.
3. Stop at `max_entries`, saying so in the description and the log.
4. Sort the rows by key.
5. With `content = true`, read the stored bytes, under the rules from AC-5. Stored bytes come only
   from `Ready`, `Source` or `Override` entries with a text media type, and are cut at a UTF-8
   boundary after `max_bytes`.
6. With `content = true`, a `.md` entry whose metadata title is empty takes its first `# ` heading
   (after any front-matter) as `title` (AC-30).

Reading content is one helper, `read_text(key, info, max_bytes)`, shared by `catalog`,
`field_extract` and `split`.

**`field_extract`** steps. It needs no `catalog` in front of it:
1. **A folder** (`folder_of` succeeds, as for `-R-key/specs/issues`) is listed recursively, like
   `catalog` without content. **Records** are taken to be a catalog: rows with `key`, and
   optionally `filename` and `content`. Anything else is an error. For one folder level, put
   `catalog-f` first.
2. Keep the rows whose `filename` (or, failing that, `key`) ends in `.<extension>`.
3. Take each row's text from `content` when the input has it; otherwise read it with `read_text`.
   Only the selected files are read.
4. Parse with the extension's format: `md` as `FrontMatter`, `json` as `Json`, `yaml` and `yml`
   as `Yaml`.
5. Drop the rows with no fields, and count them in an Info log entry.
6. With `fields` given, extract only those, as dotted paths. Otherwise extract every top-level key
   seen; the schema is the union, and a missing key is null.
7. Apply `prefix` and `flatten`, unify the types, and apply the replacement rule (AC-26).

**`split` — a proof of concept** (AC-29). It is deliberately minimal, and stands in for a fuller
Markdown tool (`NO-MARKDOWN-NAVIGATION-COMMANDS`).
- Input: as for `field_extract`, a folder or catalog records. Text comes from `content`, or from
  `read_text` for text media types.
- `unit = "line"`: one row per line.
- `unit = "section"`: one row per Markdown heading and its body, up to the next heading of the
  same or a higher level. Text before the first heading is section 0. Lines inside fenced code
  blocks are never headings.
- Columns:
  - `id` (the Id): `key#n`;
  - `key`: keyword;
  - `n`, `line`, `end_line`: Int; lines are 1-based;
  - `heading` (sections only): text, the path `H1 > H2 > H3`;
  - `level`: Int;
  - `text`: text.

The simple `ns-rec` filters that reuse these kernels are designed in
[`record-filter-commands`](../record-filter-commands/).

**Sync or async:** the kernels are sync; the commands are async.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-records` | `Cargo.toml` | `nom = "8"`, `nom_locate = "5"`, `toml = "0.8"` and `regex = "1"` |
| `liquers-records` | `src/search/{mod,predicate,syntax,structure,kernels,rank,source}.rs` (new), `src/lib.rs` | Re-exports |
| `liquers-lib` | `src/search/{mod,catalog,commands,extract,split}.rs` (new), `src/lib.rs` | Commands, `folder_of`, `search_input`, `register_search_commands!` |
| `liquers-lib` | `src/commands.rs`, `src/bin/export_command_registry.rs` | Register the `search` group beside `records`, with a no-op when `records` is off |
| `specs` | `command_registry.yaml` | Regenerated |

## Error Handling

| Situation | Outcome |
|---|---|
| `TryFrom<&str>` on malformed input | `Error::parse_error(..).with_position(..)` |
| The same input in the `search` command | Searched as plain words; the error becomes a `context.warning` |
| `catalog` input is not a folder | `Error::conversion_error_with_message(identifier, "folder", …)` |
| `search` input cannot become records | `Error::conversion_error` |
| An explicit field the schema lacks; an ambiguous name | `Error::general_error`, or `resolve_field`'s error |
| A `Field` path whose column is missing | A warning; the clause matches nothing |
| `Nested` on a non-Text column; a `Compare` value that does not parse | `Error::general_error` naming the field and its position |
| A nested field test on a cell that is not JSON | A non-match, counted in one warning |
| An invalid regex | `Error::parse_error(..).with_position(..)`; in the command, a warning and a literal search |
| `field_extract` or `split` given an input that is neither a folder nor records | `Error::conversion_error_with_message` |
| A negative `limit`, `max_bytes` or `max_entries` | `Error::general_error` |
| `field_extract` with an extension outside the enum | Rejected by the enum argument |
| `field_extract` with an extracted field named like the Id column | Skipped, with a warning |
| A source too large to rank | The `materialize` error, which suggests `rank=false` |

## Sync vs Async

Kernels and parsing are sync. The commands are async. No lock is held across an `.await`. All
trait objects are `MaybeSend + MaybeSync`.

## Relevant Commands

Namespace **`search`**:

```
async fn catalog(state, recursive: bool = true, content: bool = false, max_bytes: i64 = 262144,
                 max_entries: i64 = 100000, context) -> result
async fn commands(namespace: String = "", context) -> result
async fn search(state, query: String = "", rank: bool = true, case_sensitive: bool = false,
                limit: i64 = 50, fields: Vec<String> multiple, context) -> result
async fn field_extract(state, extension: String = "md" (enum: ["md", "json", "yaml", "yml"]),
                       prefix: String = "", flatten: bool = false, fields: Vec<String> multiple,
                       context) -> result
async fn split(state, unit: String = "section" (enum: ["section", "line"]), context) -> result
```

The results feed `rec` (`to_json`, `head`, `records_schema` for field discovery) and `pl`.

**When `Value::Predicate` would be justified** (kept from revision 7):
- a predicate built by one query and consumed by another;
- a predicate produced by a command;
- partial predicates reused across searches.

The trait makes that variant cheap to add later: it would hold an `Arc<dyn Predicate>`.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/SEARCH.md` | reference (new) | The grammar, including when `/` starts a regex, its semantics and escaping in a query; the trait and how to add a language; default fields; JSON paths; `field_extract` and front-matter; `split`; ranking and evidence; budget messages; schemas; inputs; non-evaluation; field discovery with `ns-rec/records_schema` |
| `specs/reference/RECORD_STREAMS.md` | reference | Roles and `resolve_field` now have a consumer; `FilteredSource` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | A record-producing command for search must not evaluate |
| `specs/README.md` | map | The capability changes from designing to built |

`affects_docs`: `reference/SEARCH.md`, `reference/RECORD_STREAMS.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | The new modules above; `liquers-lib/src/commands.rs`; `export_command_registry.rs`; `command_registry.yaml`; `liquers-records/Cargo.toml` |
| Crates and workflows | `liquers-records` and `liquers-lib`; the build matrix, including the wasm32 row (AC-22) |
| Existing tests likely to change | `registry_export` |
| New validation | Parser tests with positions; a property that `to_syntax` round-trips through `try_from`; kernel tests; command tests through `evaluate` for each AC |
| Performance | `catalog` is O(corpus) per search. Nested JSON tests parse each cell once per mask. `split-line` over a large tree makes many rows: about 100 000 for this repository's `specs/`, which is fine in columnar form. Indexes are `external-index-sync`'s job |
| Compatibility | Additive. The predicate trait is new public API in `liquers-records` |
| Certainty | High for the commands and kernels. The trait's method set is the part most likely to change, at the Phase 3 tests |
