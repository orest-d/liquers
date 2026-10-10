# Phase 2: Solution & Architecture — Store and asset search

> **Revision 8, 2026-10-10. Draft, written alongside the revised Phase 1.** It is reviewed at the
> Phase 2 gate, after Phase 1 is re-approved. It replaces revision 7, which is archived at
> `specs/archive/2026-10-10-store-and-asset-search-rev7-phase2-architecture.md`. What was kept,
> dropped or moved is in `DESIGN.md` §"Revision 8".

## Overview

Search is a **predicate over records**, written as a short syntax and applied by one command,
`search`. Records come from three places:

- `catalog` makes them from a store folder;
- `commands` makes them from the command registry;
- any other input is converted (a table, a stored CSV, an evaluated key or query).

The predicate, the parser, the matching kernels, BM25 ranking and a lazily filtering
`RecordSource` all live in `liquers-records`. The commands and the state-to-records conversion live
in `liquers-lib`. There is no store-trait change and no new `Value` variant.

Rejected alternatives:
- **`AsyncStore::select`.** Producing the records is what reads the corpus, so a store method would
  only push the filter down, and no backend can do that today. `STORE-NO-CONTENT-OR-METADATA-SEARCH`
  is closed as superseded.
- **A filter pipeline** (`text-a/not_text-b/…`). It cannot express OR or grouping, and the
  predicate never exists as a whole (revision 5).
- **`Value::Predicate`.** A syntax string in one parameter carries the same information. Revision
  7's conditions for adding the variant are kept below.

## Known-Issue Preflight

| Issue | Status | Pri | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | closed | — | The former blocker, resolved by `record-streams` | — | no | none |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | closed | — | `get_asset_info` no longer starts an asset, which is what AC-4 relies on | — | no | AC-4's test guards it |
| `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` | draft | P3 | The filtering source it asks for is `FilteredSource` here | no | no | **Taken into this design**; closed when built |
| `DIRECTORY-KEY-CANNOT-BE-EVALUATED-AS-A-RESOURCE` | draft | P3 | `-R/<folder>/-/ns-search/catalog` fails with "No recipe found" until it is fixed | no | no | Design around it: documentation and examples use `-R-key/<folder>`, and `catalog` also accepts the `dir` state, so the plain form works once the issue is fixed |
| `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH` | draft | P2 | Over HTTP, `search` arguments can only go into the path, escaped | no | no | Filed 2026-10-10; independent |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live | no | no | Later columns under an `attr.` prefix |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Non-evaluation is a contract, not enforced | no | no | Tests assert it (AC-4) |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | draft | P2 | Superseded | — | no | Set to `closed_not_planned` with this revision |

## Data Structures

`liquers-records/src/search/` (new; no feature gate, so wasm builds get it too):

```rust
/// The parsed search syntax. No default match arm anywhere (CLAUDE.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Predicate {
    Always,                                  // an empty expression
    All(Vec<Predicate>),                     // juxtaposition
    Any(Vec<Predicate>),                     // `|`
    Not(Box<Predicate>),                     // leading `-`
    Term(String),                            // a word, matched as a substring
    Phrase(String),                          // "a phrase", whitespace-normalized substring
    Field { name: String, test: FieldTest }, // `name:value`, `name:>v` …
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Glob(String),               // `:` — exact unless the value has `*` or `?`
    Compare(CompareOp, String), // `:>` `:>=` `:<` `:<=`, parsed into the column's FieldType at bind
}

/// A predicate resolved against one schema. Owns its column indices, so it has no lifetime.
pub struct BoundPredicate { /* tree of resolved nodes */ }

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub fields: Vec<String>,    // empty: the defaults (below)
    pub limit: Option<usize>,   // None: all rows
}

/// A `RecordSource` whose stream yields each inner view filtered (a `RowIndexView`), skipping
/// empty ones and stopping after `limit` rows. Binds the predicate once per distinct schema.
#[derive(Debug)]
pub struct FilteredSource { inner: Arc<dyn RecordSource>, predicate: Predicate, options: SearchOptions }

pub const DEFAULT_SEARCH_FIELDS: [&str; 5] = ["filename", "title", "description", "doc", "content"];
```

The grammar is revision 7's, unchanged. `or = and {"|" and}`; `and = unary {unary}`;
`unary = ["-"] atom`; `atom = "(" or ")" | field ":" [op] value | '"' phrase '"' | term`. Its rules
are also unchanged:
- input that is not syntax is taken as a literal term, so the parser never fails;
- an empty expression is `Always`;
- an ambiguous unqualified field name is an error from `RecordSchema::resolve_field`.

Revision 7's 64-node cap existed only for the evidence bitmask, so it is gone.

**Search fields.** These are the columns that `Term` and `Phrase` match against:
1. the explicit list, where an unknown name is an error;
2. otherwise whichever of `DEFAULT_SEARCH_FIELDS` exist;
3. otherwise `schema.text_fields()`;
4. otherwise every `FieldType::Text` column.

`Field` tests may name any column. An unknown name matches nothing and is returned as a warning.

**Matching** is mask-based. Each leaf yields a `Bitmap` over the view, and `All`, `Any` and `Not`
combine the masks with `and`, `or` and `not`.
- A term or phrase is a substring test on each search field, ORed across the fields. It is
  lowercased on both sides unless `case_sensitive` is set.
- A glob on a Text column is the same kernel with `*`/`?` wildcards, anchored at both ends.
- On any other column, the value is parsed to the column's `FieldType` and tested with
  `Column::compare`.
- A null never matches.

**Ranking** is BM25 (k1 = 1.2, b = 0.75) over the rows that survive the mask. The statistics come
from those rows: tokens are split on non-alphanumerics, in the spirit of `Analyzer::Simple`.
- A row's score sums, over the search fields, the field weight times the BM25 sum over the positive
  terms and phrases (those not under `Not`).
- Term frequency is the substring occurrence count. Document length is the token count.
- Field weights: `title` 3, `filename` 2, `description` 2, all others 1. A phrase counts double.
- Sorting is by descending score. The sort is stable, so ties keep input order.
- `excerpt` is about 160 characters around the first positive match, taken from the first search
  field that contains one.
- An expression with no positive terms yields `score = 0` everywhere and leaves the order alone
  (AC-15).

**`catalog` schema:**

| Column | Type | Roles |
|---|---|---|
| `key` | Text | Id; keyword + stored |
| `filename` | Text | text + stored |
| `title` and `description` | Text | text + stored |
| `extension`, `parent`, `status`, `type_identifier` and `media_type` | Text | keyword |
| `is_dir` | Bool | keyword |
| `size` | Int, nullable | numeric |
| `updated` | Timestamp, nullable | numeric |
| `content` | Text, nullable | text; the column exists only with `content = true` |

**`commands` schema:**

| Column | Type | Roles | Source |
|---|---|---|---|
| `id` | Text | Id | `CommandKey` display, `realm-namespace-name` |
| `realm`, `namespace`, `name`, `module` | Text | keyword | — |
| `title` | Text | text | `label` |
| `description` | Text | text | the generated signature, `ns-search/search(query: String = "", rank: Boolean = true, …)` |
| `doc` | Text | text | `doc` |
| `volatile` | Bool | keyword | — |
| `output_filename` | Text | keyword | the metadata's `filename`, renamed so it is not a default search field |

## Trait Implementations

- **`RecordSource for FilteredSource`.**
  - `stream` maps the inner stream: each view is passed through `view.filter(&mask)` (zero-copy),
    empty views are skipped, and the stream stops at `limit`.
  - `chunks`, `describe_chunk` and `schema` delegate to the inner source. A filtered chunk keeps
    the inner chunk's provenance.
  - `truncated` reports the inner source's value only. Stopping at `limit` is a caller's choice,
    not a truncation of the source.
  - `manifest` is `None`, so the source is re-derived from its recipe.
- Nothing else. No `AsyncStore`, `AssetManager` or `Value` change.

## Function Signatures

`liquers-records/src/search/`, all sync and pure:

```rust
pub fn parse_search(input: &str) -> Predicate;
pub fn search_fields(schema: &RecordSchema, requested: &[String]) -> Result<Vec<usize>, Error>;
impl Predicate {
    /// Resolve names. Unknown `Field` names come back as warnings, and match nothing.
    pub fn bind(&self, schema: &RecordSchema, options: &SearchOptions)
        -> Result<(BoundPredicate, Vec<String>), Error>;
    pub fn positive_terms(&self) -> Vec<&str>;
}
impl BoundPredicate { pub fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error>; }
/// Filter, score, sort, apply the limit, and append `score` (Float) and `excerpt` (Text, nullable).
pub fn search_view(view: &Arc<dyn RecordView>, predicate: &Predicate, options: &SearchOptions, rank: bool)
    -> Result<(Arc<dyn RecordView>, Vec<String>), Error>;
impl FilteredSource {
    pub fn new(inner: Arc<dyn RecordSource>, predicate: Predicate, options: SearchOptions) -> Self;
}
```

`liquers-lib/src/search/` (feature `records`). Every command is generic over
`E: Environment<Value = Value>`:

```rust
pub async fn catalog<E>(state: State<Value>, recursive: bool, content: bool, max_bytes: i64,
    context: Context<E>) -> Result<Value, Error>;
pub async fn commands<E>(namespace: String, context: Context<E>) -> Result<Value, Error>;
pub async fn search<E>(state: State<Value>, query: String, rank: bool, case_sensitive: bool,
    limit: i64, fields: Vec<String>, context: Context<E>) -> Result<Value, Error>;

/// The folder a state denotes, if any. The answer depends on the input:
/// - no value and no key → the root key;
/// - a `dir` state, or an `AssetInfo` listing from `-R-dir` or `-R-sdir` → the key in its metadata;
/// - a `Key` value, or a `Query` value whose `key()` is `Some` → that key, when the asset manager
///   reports it `is_dir`;
/// - anything else → None.
pub async fn folder_of<E>(state: &State<Value>, context: &Context<E>) -> Result<Option<Key>, Error>;

pub enum SearchInput { View(Arc<dyn RecordView>), Source(Arc<dyn RecordSource>) }
/// The records a state denotes:
/// - a folder (`folder_of`) → its catalog, recursive and without content;
/// - a `Key` or `Query` value → evaluated, then converted again;
/// - anything else → `records::convert::to_record_source`, which already handles views, sources,
///   JSON, and bytes or text in the format their metadata declares (CSV, NDJSON, Parquet, …).
///
/// A source that is an `InMemorySource` over one view comes back as `View`.
pub async fn search_input<E>(state: State<Value>, context: &Context<E>) -> Result<SearchInput, Error>;
```

**How `search` handles each input:**

| `rank` | Input | What happens | Result |
|---|---|---|---|
| true | `View` | `search_view` | `RecordView` |
| true | `Source` | materialize (`DEFAULT_MATERIALIZE_MAX_ROWS`), then `search_view` | `RecordView` |
| false | `View` | `search_view` without scoring | `RecordView` |
| false | `Source` | `FilteredSource` | `RecordSource` |

**`catalog` steps:**
1. Resolve the folder with `folder_of`. If it is `None`, fail.
2. List the entries:
   - with `recursive`: `listdir_keys_deep`, then `get_asset_info` per key, with up to 16 calls in
     flight (`buffer_unordered`);
   - otherwise: `listdir_asset_info`.
3. Sort the rows by key.
4. With `content = true`, read an entry's bytes from `store.get(key)` only when:
   - its status is `Ready`, `Source` or `Override`;
   - it is not a directory;
   - its media type is `text/*`, JSON, YAML, TOML, XML or CSV, or its extension is in the matching
     list;
   - the bytes are valid UTF-8 after truncation to `max_bytes`, at a character boundary.
   Entries skipped or truncated are counted in one `Info` log entry.

**Sync or async.** The kernels are sync. The commands are async because they touch the store, the
asset manager or evaluation. The macro requires an owned `State` with `context` last, and `commands`
omits the state.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-records` | `src/search/{mod,syntax,predicate,rank,source}.rs` (new), `src/lib.rs` | Module and re-exports |
| `liquers-lib` | `src/search/{mod,catalog,commands}.rs` (new), `src/lib.rs` | Commands, `folder_of`, `search_input`, `register_search_commands!` |
| `liquers-lib` | `src/commands.rs` | Invoke `register_search_commands!` beside `register_records_commands!` (`records` on), with a no-op stand-in when it is off |
| `liquers-lib` | `src/bin/export_command_registry.rs` | Register the `search` group |
| `specs` | `command_registry.yaml` | Regenerated, with a CHANGELOG line |

## Error Handling

All errors go through the typed constructors. There is no `Error::new` and no `unwrap`.

| Situation | Outcome |
|---|---|
| `catalog` input is not a folder | `Error::conversion_error_with_message(identifier, "folder", …)` |
| `search` input cannot become records | `Error::conversion_error` (from `to_record_source`) |
| An explicit field the schema lacks | `Error::general_error` naming the field and the available ones |
| An ambiguous field name | The error `resolve_field` returns |
| A `Field` name the schema lacks | Not an error: the clause matches nothing, and `context.warning` names it |
| A `Compare` value that does not parse as the column's type | `Error::general_error` |
| A negative `limit` or `max_bytes` | `Error::general_error`, as `non_negative_usize` does in `ns-rec` |
| A source too large to materialize for ranking | The `materialize` error, which names `rank=false` as the alternative |
| An unreadable entry during content reading | Skipped; counted in the `Info` log entry |

## Sync vs Async

As above: kernels sync, commands async. No lock is held across an `.await`. `FilteredSource`
satisfies `MaybeSend + MaybeSync`, because it holds only `Arc`s and owned data.

## Relevant Commands

Namespace **`search`**, new:

```
async fn catalog(state, recursive: bool = true, content: bool = false, max_bytes: i64 = 262144, context) -> result
async fn commands(namespace: String = "", context) -> result
async fn search(state, query: String = "", rank: bool = true, case_sensitive: bool = false,
                limit: i64 = 50, fields: Vec<String> multiple, context) -> result
```

`limit = 0` means all rows. The namespace interacts with `rec` (`to_json`, `head`, `materialize`,
`to_record` all accept the results) and `pl` (via the polars bridge).

**Conditions for `Value::Predicate` (kept from revision 7).** Add it only when:
- a predicate must be built by one query and consumed by another;
- a predicate is produced by a command;
- partial predicates need reuse across searches.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/SEARCH.md` | reference (new) | The syntax and its escaping in a query; matching and ranking; default fields; the input table; the `catalog` and `commands` schemas; the non-evaluation rule; the `Value::Predicate` conditions |
| `specs/reference/RECORD_STREAMS.md` | reference | Roles and `resolve_field` now have a consumer; `FilteredSource` |
| `specs/README.md` | map | The search capability changes from designing to built |

`affects_docs`: `reference/SEARCH.md`, `reference/RECORD_STREAMS.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | The new modules above, `liquers-lib/src/commands.rs`, `export_command_registry.rs`, `command_registry.yaml` |
| Crates and workflows | `liquers-records` and `liquers-lib`; the build matrix (a new `records`-gated module) |
| Existing tests likely to change | `registry_export` (new commands) |
| New validation | Unit tests per kernel and for the parser; command tests through `evaluate` for each AC |
| Performance | `catalog` makes one `get_asset_info` call per entry, plus one read per entry when content is on: O(corpus) on every search. Fine at thousands of entries. An index is `external-index-sync`'s job |
| Security | Content reading is bounded by `max_bytes` and reads stored bytes only. There is no access control (`CORE-SESSION-AND-KEY-ACL`) |
| Certainty | High for the kernels and commands. The BM25 constants and field weights are tunable without any contract change |
