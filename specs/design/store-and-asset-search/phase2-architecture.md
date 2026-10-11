# Phase 2: Solution & Architecture — Store and asset search

> **Revision 8. In review; Phase 1 was approved on 2026-10-11.** This replaces revision 7, archived
> at `specs/archive/2026-10-10-store-and-asset-search-rev7-phase2-architecture.md`. The amendments
> of 2026-10-11 are recorded in `DESIGN.md`.
>
> Rewritten on 2026-10-11 after two independent Phase 2 reviews (conformity and records; codebase
> and soundness). Tables, schemas, step lists and query encodings are in
> [`phase2-details.md`](./phase2-details.md).

## Overview

Search is a **predicate over records**: a trait object made by a parser from a syntax string, and
applied by `search`. Records come from five producers:
- `catalog`: a store folder;
- `commands`: the command registry;
- `field_extract`: document fields;
- `md_split`: Markdown sections or lines;
- any other input, converted by `search` itself (a table, a stored CSV, an evaluated key or query).

`liquers-records` holds everything that does not need `Value`: the predicate trait, the built-in
language (AST, grammar, `nom` parser), the kernels, BM25, structure parsing, and a lazily filtering
`RecordSource`. `liquers-lib` holds the commands and the state-to-records conversion.

Rejected alternatives:
- **`AsyncStore::select`.** It would only push the filter down, and no backend can do that.
- **A filter pipeline.** It has no OR, and the predicate never exists as a whole.
- **`Value::Predicate`.** The syntax string is the transport form. The conditions under which the
  variant would be worth adding are in `DESIGN.md`; the trait would make it cheap.
- **A closed `enum Predicate` as the public type.** It cannot be replaced by another language. The
  enum survives as the built-in language's AST.
- **Guessing structure from cell text.** A JSON column is *declared* (see the `FieldSchema` change
  below), so nothing sniffs cells.

**Namespaces:** `search` is new. `rec` and `pl` consume its results.

## Known-Issue Preflight

| Issue | Status | Pri | Impact | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | closed | — | The former blocker | — | no | none |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | closed | — | AC-4 relies on the fix | — | no | AC-4's test guards it |
| `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` | draft | P3 | Its filtering wrapper is `FilteredSource` | no | no | Taken in; closed when built |
| `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` | draft | P3 | `-R/<folder>/-/ns-search/catalog` fails until fixed | no | no | Designed around: `-R-key/<folder>`, plus the `dir` state |
| `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH` | draft | P2 | Over HTTP the expression is escaped into the path | no | no | Independent |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | No `attr.*` metadata | no | no | Front-matter becomes columns through `field_extract` (AC-24) |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Non-evaluation is a contract | no | no | Tests assert it (AC-4) |
| `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE` | draft | P2 | Results feed `rec` and `pl`; `pl` does not yet accept records | no | no | Carried by `record-filter-commands` |
| `NO-MARKDOWN-NAVIGATION-COMMANDS` | draft | P3 | `md_split` is its proof of concept | no | no | Future |
| `ERROR-MESSAGE-POSITIONS-MIXED-BASE` | draft | P3 | Parser positions must use one convention | no | no | Use `Position` from `nom_locate` (1-based line and column; 0-based offset), as `parse.rs` does |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | closed_not_planned | — | Superseded | — | no | none |

## Data Structures

### Predicate trait — `liquers-records/src/search/predicate.rs`

```rust
/// Object-safe: `search` holds an `Arc<dyn Predicate>` and never names its language.
pub trait Predicate: Debug + MaybeSend + MaybeSync + 'static {
    /// Resolve names against one schema; unknown field names become warnings.
    fn bind(&self, schema: &RecordSchema, options: &SearchOptions) -> Result<Bound, Error>;
    /// Test one JSON value: the inner predicate of `field:(…)` on a JSON column.
    fn matches_json(&self, value: &serde_json::Value, options: &SearchOptions) -> Result<bool, Error>;
    /// Whether any clause is a field test. If none is, a nested predicate is a field-scoped search.
    fn has_field_tests(&self) -> bool;
    /// Positive terms, phrases and regexes, for BM25 and excerpts.
    fn rank_terms(&self) -> Vec<RankTerm>;
    /// The transport form: logged, stored in a recipe, or handed to an engine.
    fn to_syntax(&self) -> String;
}
/// A parseable language. `Sized` keeps it off the trait object.
pub trait PredicateSyntax: Predicate + Sized + for<'a> TryFrom<&'a str, Error = Error> {
    const LANGUAGE: &'static str;
}
// A run-time registry of languages waits for the dialect argument (Phase 1, open question 4).

pub struct Bound { pub matcher: Box<dyn BoundPredicate>, pub warnings: Vec<String> }
pub trait BoundPredicate: MaybeSend + MaybeSync {
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>;
    /// Per search field, the rows in which a positive term matched (the `matched` column).
    fn evidence(&self, _view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> { Ok(vec![]) }
}
#[derive(Debug, Clone)]
pub enum RankTerm { Term(String), Phrase(String), Regex(String) }
#[derive(Debug, Clone)]
pub struct SearchOptions { pub case_sensitive: bool, pub fields: Vec<String>, pub limit: Option<usize> }
```

### Built-in language — `liquers-records/src/search/syntax.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchExpr { pub node: Node, pub position: Position } // liquers_core::query::Position
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node { Always, All(Vec<SearchExpr>), Any(Vec<SearchExpr>), Not(Box<SearchExpr>),
    Term(String), Phrase(String), Regex(String), Field { path: Vec<String>, test: FieldTest } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest { Glob(String), Compare(CompareOp, String), Regex(String), Nested(Box<SearchExpr>) }
impl Predicate for SearchExpr { /* … */ }
impl TryFrom<&str> for SearchExpr { type Error = Error; /* strict */ }
impl PredicateSyntax for SearchExpr { const LANGUAGE: &'static str = "liquers-search"; }
impl SearchExpr { pub fn parse_lenient(text: &str) -> (SearchExpr, Option<Error>); }
```

**Grammar.** `ws` is optional whitespace, `ws1` is required whitespace, and `regex` is tried
before `term`.

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
regex      = "/" , re_char , { re_char } , "/" , & delimiter ;
re_char    = ( char - "/" - "\" ) | "\" , char | "/" , ~ delimiter ;
delimiter  = ws | ")" | "|" | end ;
term       = ( term_char | "/" ) , { term_char | "-" | "/" } ;
value      = ( term_char | "/" ) , { term_char | "-" | ":" | "/" } ;
term_char  = char - ws - "(" - ")" - "|" - '"' - "-" - "/" ;
```

**Rules:**
- A regex body ends at the **first** unescaped `/` followed by a delimiter. A `/` followed by
  anything else is part of the body.
- A `/`-initial atom with no such closing slash is a term or value.
- An empty value (`f:`) is not a field test: the parse falls back to the term `f:`.
- Precedence, tightest first: `-`, then juxtaposition (AND), then `|`.
- The parser uses `nom` 8 over `nom_locate` 5, like `liquers-core/src/parse.rs`, and uses the
  existing `From<LocatedSpan> for Position`.
- Errors are `Error::parse_error(..).with_position(..)`. Positions are offsets into the *decoded*
  expression.

Cases and encodings: [`phase2-details.md`](./phase2-details.md) §A–B.

### Semantics

**Search fields** (AC-10): the explicit list; otherwise the schema's text-role fields; otherwise
whichever of `filename`, `title`, `description`, `doc` and `content` exist; otherwise every Text
column. An explicit name the schema lacks is an error.

**Field paths.** For `a.b`, the column is the longest prefix for which
`RecordSchema::resolve_field` returns `Some` (so its `meta.`/`attr.`/`key.` expansion applies); the
rest is a JSON path. A column whose name is not a `name` (spaces, `-`) cannot be addressed in this
version, a stated limitation. Quoted field names are a possible later addition.

**Matching** produces one mask per leaf, combined with `Bitmap::and`, `or` and `not`. A null never
matches.
- **Terms and phrases:** a substring test per search field, ORed. Both sides are lowercased
  unless `case_sensitive`.
- **Regex:** the `regex` crate, which matches in linear time. `(?i)` is added unless
  `case_sensitive`. As a term it applies to every search field; as a field test, to that field.
- **Glob on a Text column:** a wildcard match, anchored at both ends.
- **On a non-Text column:**
  - `f:*` means "present" (`null_mask().not()`);
  - any other wildcard is a bind error;
  - a plain value or a compare is parsed with `Column::parse_text` and tested with
    `Column::compare`. A date-only value against a Timestamp column is widened to midnight UTC.
- **Columns declared JSON:**
  - A glob, compare or regex on a cell holding an array matches when any element matches.
  - `Nested` with field tests (`attr:(status:draft)`, `attr.status:draft`) parses the cell once per
    column per bind, and evaluates it through `matches_json`: paths walk object keys, an array
    matches any element, and a term matches any string leaf.
  - A cell that is not valid JSON does not match, and is counted in one warning.
- **`Nested` without field tests** (`content:(a -b)`): the inner predicate is bound with that one
  column as its search field. Nothing is parsed.
- **`Nested` with field tests on a column that is not declared JSON**, or on a non-Text column, or
  a dotted path into one: a bind error.

**Ranking** (when `rank` is true) is BM25 over the surviving rows. It appends `score`, `matched`
(the search fields that matched) and `excerpt`, and sorts stably. With no `rank_terms`, input order
is kept (AC-15). Constants: [`phase2-details.md`](./phase2-details.md) §D.

**Limits:** `limit = 0` means no limit (`None`). A negative `limit`, `max_bytes` or `max_entries`
is an error.

### `FieldSchema` gains an optional media type — `liquers-records/src/schema.rs`

```rust
pub struct FieldSchema { /* existing fields */ pub media_type: Option<String> }
impl FieldSchema { pub fn with_media_type(self, media_type: impl Into<String>) -> Self; }
```

This is an additive serde field (`#[serde(default, skip_serializing_if = "Option::is_none")]`
in `FieldSchemaSpec`), so existing schemas read unchanged. `field_extract` declares
`application/json` on its JSON columns, and search applies JSON rules only to such columns. It is a
records-crate change, so it is listed for `RECORD_STREAMS.md`.

### Structure parsing — `liquers-records/src/search/structure.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructFormat { Json, Yaml, FrontMatter }
pub fn parse_structure(text: &str, format: StructFormat) -> Option<serde_json::Value>;
```

Only `field_extract` uses this, with the format taken from the extension. Rules and the TOML/YAML
conversion: [`phase2-details.md`](./phase2-details.md) §E.

Schemas of `catalog`, `commands`, `field_extract` and `md_split`:
[`phase2-details.md`](./phase2-details.md) §C.

## Trait Implementations

**`RecordSource for FilteredSource`**, with fields
`{ inner: Arc<dyn RecordSource>, predicate: Arc<dyn Predicate>, options: SearchOptions,
warnings: Arc<Mutex<Vec<String>>>, stopped: Arc<AtomicBool> }`:
- `stream` filters each view with `filter(&mask)`, which is zero-copy, and skips empty views. It
  stops at `limit` and then sets `stopped`. It binds once per distinct schema, and pushes bind
  warnings into `warnings`.
- `chunks`, `describe_chunk` and `schema` delegate to the inner source. **`chunks` describes the
  unfiltered input** (provenance), not which chunks will yield rows.
- `truncated` is `inner.truncated() || stopped`.
- `manifest` is `None`.

When the inner source declares `schema()`, the `search` command binds once itself and logs the
warnings through `Context`. Warnings from later schemas stay readable from `warnings()`, and the
command drains them only when it materializes the source. This is documented in `SEARCH.md`.

`Predicate`, `TryFrom<&str>` and `PredicateSyntax` are implemented for `SearchExpr`. No
`AsyncStore`, `AssetManager` or `Value` change.

## Function Signatures

`liquers-records/src/search/`, all sync:

```rust
pub fn search_fields(schema: &RecordSchema, requested: &[String]) -> Result<Vec<usize>, Error>;
#[derive(Debug, Clone)]
pub struct SearchReport { pub matched: usize, pub shown: usize, pub warnings: Vec<String> }
pub fn search_view(view: &Arc<dyn RecordView>, predicate: &dyn Predicate, options: &SearchOptions,
    rank: bool) -> Result<(Arc<dyn RecordView>, SearchReport), Error>;
impl FilteredSource {
    pub fn new(inner: Arc<dyn RecordSource>, predicate: Arc<dyn Predicate>, options: SearchOptions) -> Self;
    pub fn warnings(&self) -> Vec<String>;
}
```

`liquers-lib/src/search/` (feature `records`). Every function is generic over
`E: Environment<Value = Value>`:

```rust
pub async fn catalog<E>(state: State<Value>, recursive: bool, content: bool, max_bytes: i64,
    max_entries: i64, context: Context<E>) -> Result<Value, Error>;
pub async fn commands<E>(namespace: String, context: Context<E>) -> Result<Value, Error>;
pub async fn search<E>(state: State<Value>, query: String, rank: bool, case_sensitive: bool,
    limit: i64, fields: Vec<String>, context: Context<E>) -> Result<Value, Error>;
pub async fn field_extract<E>(state: State<Value>, extension: String, prefix: String, flatten: bool,
    fields: Vec<String>, context: Context<E>) -> Result<Value, Error>;
pub async fn md_split<E>(state: State<Value>, unit: String, context: Context<E>) -> Result<Value, Error>;
pub async fn folder_of<E>(state: &State<Value>, context: &Context<E>) -> Result<Option<Key>, Error>;
pub(crate) async fn read_text<E>(key: &Key, info: &AssetInfo, max_bytes: usize, context: &Context<E>)
    -> Result<Option<String>, Error>;
#[derive(Debug)]
pub enum SearchInput { View(Arc<dyn RecordView>), Source(Arc<dyn RecordSource>) }
pub async fn search_input<E>(state: State<Value>, context: &Context<E>) -> Result<SearchInput, Error>;
```

`search_input` checks `folder_of` first, and a folder becomes its catalog (recursive, no content).
It evaluates a `Query` value, which is the only new conversion. Everything else, `Key` included,
goes to `records::convert::to_record_source`, which already evaluates keys.

What `search` returns:

| `rank` | `View` input | `Source` input |
|---|---|---|
| true | `search_view`, giving a `RecordView` | materialized, then `search_view` |
| false | `search_view` without scoring | `FilteredSource` |

When materializing, a "stream exceeds max_rows" error is wrapped (`map_err`) with the advice to
use `rank=false`. `SearchReport` becomes the result description via the async
`context.set_description`: `N of M matches shown`, or `first N matches; more may exist` for a
stream. It is also logged with `context.info`, and warnings go to `context.warning`. `commands`
reads the registry through `context.get_envref()`, bound to a local first.

Step lists for `folder_of`, `catalog` (including a bounded breadth-first walk), `field_extract` and
`md_split`: [`phase2-details.md`](./phase2-details.md) §F.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-records` | `Cargo.toml` | `nom = "8.0.0"`, `nom_locate = "5.0.0"`, `regex = "1.13.1"` (core's versions, pinned so that `From<LocatedSpan> for Position` lines up) and `toml = "0.8"`. All are wasm-clean |
| `liquers-records` | `src/schema.rs` | `FieldSchema::media_type` |
| `liquers-records` | `src/search/{mod,predicate,syntax,structure,kernels,rank,source}.rs` (new); `src/lib.rs` | Re-exports |
| `liquers-lib` | `src/search/{mod,catalog,commands,extract,md_split}.rs` (new); `src/lib.rs` | Commands and `register_search_commands!` |
| `liquers-lib` | `src/commands.rs` | Register the `search` group beside `records`, with a no-op when `records` is off. On wasm32 too (AC-22) |
| `liquers-lib` | `src/bin/export_command_registry.rs` | A `Search` group variant behind `#[cfg(feature = "records")]`. The registry tests' feature gating is updated to match |
| `specs` | `command_registry.yaml` | Regenerated |

## Error Handling

All errors go through typed constructors. Each match on the built-in language's AST is exhaustive,
with no default arm.

| Situation | Outcome |
|---|---|
| Malformed syntax or an invalid regex, through `TryFrom` | `Error::parse_error(..).with_position(..)` |
| The same, in `search` | A warning with the position; the input is searched as whitespace-separated words |
| `catalog`, `field_extract` or `md_split` input that is neither a folder nor records | `Error::conversion_error_with_message` |
| `search` input that cannot become records | `Error::conversion_error` |
| An explicit field the schema lacks; an ambiguous name | `Error::general_error`, or `resolve_field`'s error |
| A field test naming a missing column | A warning; the clause matches nothing |
| A wildcard on a non-Text column; nested field tests on a non-JSON column; a compare value that does not parse | `Error::general_error` naming the field and its position |
| A cell in a JSON column that is not valid JSON | A non-match, counted in one warning |
| A negative `limit`, `max_bytes` or `max_entries` | `Error::general_error` |
| An extracted field named like the Id column | Skipped, with a warning |
| A source too large to rank | The materialize error, wrapped with the advice to use `rank=false` |

## Sync vs Async

Kernels and parsing are sync. The commands are async. No lock is held across an `.await`; the
`Mutex` in `FilteredSource` is held only to push or drain warnings. All trait objects are
`MaybeSend + MaybeSync`.

## Relevant Commands

Namespace **`search`**. `context` comes last, as in `ns-rec`:

```
async fn catalog(state, recursive: bool = true, content: bool = false, max_bytes: i64 = 262144,
                 max_entries: i64 = 100000, context) -> result
async fn commands(namespace: String = "", context) -> result
async fn search(state, query: String = "", rank: bool = true, case_sensitive: bool = false,
                limit: i64 = 50, fields: Vec<String> multiple, context) -> result
async fn field_extract(state, extension: String = "md" (enum: ["md", "json", "yaml", "yml"]),
                       prefix: String = "", flatten: bool = false, fields: Vec<String> multiple,
                       context) -> result
async fn md_split(state, unit: String = "section" (enum: ["section", "line"]), context) -> result
```

Results feed `rec` (`to_json`, `head`, `records_schema` for discovering fields) and `pl`. The
simple `ns-rec` filters are designed in [`record-filter-commands`](../record-filter-commands/).

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/SEARCH.md` | reference (new) | The grammar, the `/` rule and the query encodings; semantics; the trait and how to add a language; default fields; JSON columns and paths; the five commands and their schemas; ranking and evidence; budget messages; `FilteredSource` warnings and `chunks`; non-evaluation; field discovery with `ns-rec/records_schema`. Most of `phase2-details.md` moves here |
| `specs/reference/RECORD_STREAMS.md` | reference | `FieldSchema::media_type`; roles and `resolve_field` gain a consumer; `FilteredSource` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | A record-producing command for search must not evaluate |
| `specs/README.md` | map | The capability changes from designing to built |

`affects_docs`: `reference/SEARCH.md`, `reference/RECORD_STREAMS.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | The new modules; `schema.rs`; `liquers-lib/src/commands.rs`; `export_command_registry.rs`; `command_registry.yaml`; `liquers-records/Cargo.toml` |
| Crates and workflows | `liquers-records` and `liquers-lib`; the build matrix, including the wasm32 row |
| Existing tests likely to change | `registry_export`, and its gating; schema serde round-trips |
| New validation | Parser tests from `phase2-details.md` §B; a round-trip property (`to_syntax` back through `try_from`); kernels; commands through `evaluate` for each AC |
| Performance | `catalog` is O(corpus) per search, bounded by `max_entries`. JSON cells are parsed once per column per bind. `md_split-line` makes about 100 000 rows for this repository's `specs/`, which is fine in columnar form |
| Compatibility | Additive; `media_type` defaults to absent |
| Certainty | High. The trait's method set is the most likely to change in Phase 3 |
