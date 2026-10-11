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
    /// Test one field value, seen as a document. This is what makes a predicate usable *inside*
    /// a field test (`content:(…)`).
    fn matches_value(&self, value: &DocValue<'_>, options: &SearchOptions) -> Result<bool, Error>;
    /// Whether any clause needs a structured value. If none does, a text field is never parsed.
    fn needs_structure(&self) -> bool;
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

/// Object-safe face of a language, for choosing one at run time (a later `language` argument).
pub trait PredicateLanguage: MaybeSend + MaybeSync {
    fn name(&self) -> &str;
    fn parse(&self, text: &str) -> Result<Arc<dyn Predicate>, Error>;
}
pub struct Language<P: PredicateSyntax>(PhantomData<P>); // impl PredicateLanguage for Language<P>

/// A predicate resolved against one schema.
pub struct Bound { pub matcher: Box<dyn BoundPredicate>, pub warnings: Vec<String> }
pub trait BoundPredicate: MaybeSend + MaybeSync {
    fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>;
    /// Per search field, the rows in which a positive term matched. Feeds the `matched` column.
    fn evidence(&self, view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> { Ok(vec![]) }
}

/// One field value, as a nested predicate sees it.
pub enum DocValue<'a> { Text(&'a str), Structured(&'a serde_json::Value) }
pub struct RankTerm { pub text: String, pub phrase: bool }

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
    Field { path: Vec<String>, test: FieldTest },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Glob(String),                // `f:v`, exact unless `*` or `?`; `f:*` means present
    Compare(CompareOp, String),  // `f:>v` and the like; the value is parsed at bind time
    Nested(Option<StructFormat>, Box<SearchExpr>), // `f:( … )`, `f:yaml( … )`, …
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
atom       = group | field_test | phrase | term ;
group      = "(" , expr , ")" ;
field_test = field_path , ":" , ( [ format ] , group | [ cmp_op ] , ( phrase | value ) ) ;
format     = "yaml" | "json" | "frontmatter" | "fenced_yaml" | "fenced_json" ;  (* only directly before "(" *)
field_path = name , { "." , name } ;
name       = ( letter | "_" ) , { letter | digit | "_" } ;
cmp_op     = ">=" | "<=" | ">" | "<" ;
phrase     = '"' , { ( char - '"' - "\" ) | '\"' | "\\" } , '"' ;
term       = term_char , { term_char | "-" } ;
value      = { term_char | "-" | ":" | "/" } ;   (* `key:specs/issues/*` *)
term_char  = char - ws - "(" - ")" - "|" - '"' - "-" ;
```

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

**Field paths.** For `a.b.c`, the longest prefix that names a column is the column, and the rest
is a path inside the parsed value. So `content.status:draft` ≡ `content:(status:draft)`. An exact
column name always wins, which keeps dotted column names working.

**Matching** produces one mask per leaf, combined with `and`, `or` and `not`. A null never matches.
- **Terms and phrases:** a substring test per search field, ORed. It is lowercased on both sides
  unless `case_sensitive` is set.
- **Glob on a Text column:** the same substring kernel with wildcards, anchored at both ends.
- **Glob or compare on any other column:** the value is parsed to the column's `FieldType`, then
  tested with `Column::compare`.
- **`Nested` on a Text column:** applied row by row through `matches_value`.
  - If the inner predicate does not `needs_structure`, it sees `DocValue::Text`.
  - Otherwise the text goes through `parse_structure` once per row and column, and the inner
    predicate sees `DocValue::Structured`.
  - On the parsed value, a `Field` path walks object keys, a list matches when any element does,
    and a `Term` matches any string leaf.
  - Text that yields no structure is a non-match, counted in one warning.
- **A Glob or Compare on a Text cell holding a JSON array** (`[` first, and it parses) matches when
  any element matches. This is how list columns written by `field_extract` work.
- **`Nested` on a non-Text column:** a bind error.

**Ranking** is BM25 (k1 = 1.2, b = 0.75) over the rows that survive the mask.
- The statistics come from those rows. Tokens are split on non-alphanumerics, and term frequency is
  the substring occurrence count.
- Field weights: `title` 3, `filename` 2, `name` 2, `description` 2, all others 1. A phrase counts
  double.
- The sort is stable and descending.
- An expression with no `rank_terms` keeps the input order (AC-15).

**Columns appended when `rank` is true:**
- `score` (Float);
- `matched` (Text): the comma-separated search fields that matched, from `evidence`;
- `excerpt` (Text, nullable): about 160 characters around the first match in the first matching
  field, skipping a leading front-matter block.

### Structure recognition — `liquers-records/src/search/structure.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructFormat { Json, Yaml, FrontMatter, FencedYaml, FencedJson }
/// `None` selects automatic recognition. Returns `None` when the text has no structure.
pub fn parse_structure(text: &str, format: Option<StructFormat>) -> Option<serde_json::Value>;
```

| Format | Rule | Reliable |
|---|---|---|
| automatic | JSON if the trimmed text starts with `{` or `[` and parses whole; otherwise front-matter; otherwise none | yes |
| `Json` | the whole text | — |
| `Yaml` | the whole text; a result that is a scalar counts as no structure | explicit only: YAML accepts most prose |
| `FrontMatter` | the first line is `---`, then a line `---` or `...`, and the block is YAML; or the first line is `+++`, then a line `+++`, and the block is TOML. The result must be a map | yes |
| `FencedYaml`, `FencedJson` | the first fence of that language (```` ```yaml ````/```` ```yml ````, ```` ```json ````), up to its closing fence | explicit only: it catches examples |

YAML and TOML are converted to `serde_json::Value`, so one navigation code path serves all three.
TOML needs `toml = "0.8"`, already used optionally by `liquers-core`, as a plain dependency of
`liquers-records`.

### Record schemas

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

**`field_extract`** steps:
1. Turn the input into a view with `search_input`, rank-free. A folder becomes its catalog without
   content.
2. Keep the rows whose `filename` (or, failing that, `key`) ends in `.<extension>`.
3. Take each row's text from its `content` column when there is one. Otherwise read the stored
   bytes of `key`, under the same rules and `max_bytes` cap as `catalog`. Only the selected files
   are read.
4. Parse with the extension's format: `md` as `FrontMatter`, `json` as `Json`, `yaml` and `yml`
   as `Yaml`, where the result must be a map.
5. Drop the rows with no fields, and count them in an Info log entry.
6. With `fields` given, extract only those, as dotted paths. Otherwise extract every top-level key
   seen; the schema is the union, and a missing key is null.
7. Apply `prefix` and `flatten`, unify the types, and apply the replacement rule (AC-26).

The simple `ns-rec` filters that reuse these kernels are designed in
[`record-filter-commands`](../record-filter-commands/).

**Sync or async:** the kernels are sync; the commands are async.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-records` | `Cargo.toml` | `nom = "8"`, `nom_locate = "5"` and `toml = "0.8"` |
| `liquers-records` | `src/search/{mod,predicate,syntax,structure,kernels,rank,source}.rs` (new), `src/lib.rs` | Re-exports |
| `liquers-lib` | `src/search/{mod,catalog,commands,extract}.rs` (new), `src/lib.rs` | Commands, `folder_of`, `search_input`, `register_search_commands!` |
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
| Text that does not parse as JSON or YAML | A non-match, counted in one warning |
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
| `specs/reference/SEARCH.md` | reference (new) | The grammar, its semantics and escaping in a query; the trait and how to add a language; default fields; paths and front-matter; ranking and evidence; budget messages; schemas; inputs; non-evaluation; field discovery with `ns-rec/records_schema` |
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
| Performance | `catalog` is O(corpus) per search. Nested structured tests parse each row's text, which matters on large `content`; the parse is cached per row and column within one mask. Indexes are `external-index-sync`'s job |
| Compatibility | Additive. The predicate trait is new public API in `liquers-records` |
| Certainty | High for the commands and kernels. The trait's method set is the part most likely to change, at the Phase 3 tests |
