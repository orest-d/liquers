# Phase 2: Solution & Architecture — Store and asset search

> **Revision 2.** The first draft put the record *source* inside the predicate, added `select` to two
> core traits, and gave every hit an `AssetInfo`. Review found all three wrong. §0 records what
> changed and why, because two of the corrections reverse Phase 1 decisions.

## Overview

A search is a **predicate applied to a stream of records**. Records are produced by **commands**; the
stream's identity is the query that produces it. `liquers-core` gains a record module — records,
batches, chunks, a stream trait, a predicate — and **no new method on `AsyncStore` or
`AssetManager`** beyond repairing `get_asset_info` so that describing an asset stops starting it.
`liquers-lib` gains an `ns-search` namespace whose commands are one clause each.

Scope is milestones **M0–M3** of [`roadmap.md`](./roadmap.md) plus the `get_asset_info` repair.

## 0. What changed in revision 2, and what it reverses

| Change | Why |
|---|---|
| **`root` and `sources` leave the predicate** | The predicate is a filter; *what it filters* is the stream. Putting the source inside it smuggled the set into the filter, contradicting this design's own model ("select records **from a set**"). The source is a **query**, which also gives the two execution paths: evaluate it and filter the stream, or hand the query to an external engine that already processed it. |
| **`SearchSource` deleted** | With the source being a query, command discovery is a command producing records from the registry — not an enum variant. One less concept. |
| **`AsyncStore::select` and `AssetManager::select` dropped from the MVP** | **Reverses Phase 1 axis B2/B3.** If records come from commands, a trait method is a *push-down optimization*, not the mechanism. Dropping it removes every change to two widely-implemented traits, removes the conformance-rule family, and forecloses nothing: the method can be added when a backend can actually exploit it. |
| **Fields are `serde_json::Value`, not `liquers_core::value::Value`** | A bug in the first draft: `Value` is **704 bytes**, so a `BTreeMap<String, Value>` per record is indefensible — and the draft cited `CORE-VALUE-ENUM-OVERSIZED` two sections earlier. JSON values are small, need no type parameter, and make a record set directly serializable as JSON or NDJSON, which is the tabular-interchange goal. |
| **`Hit` replaced** | It embedded an `AssetInfo`, which assumes one record per asset. **A CSV row has no `AssetInfo`; the file does.** Asset description moves to a per-source side table, and retrieval is explicit. |
| **`FieldMatch` → `ClauseMatch`** | Evidence now names *which clause* matched, so "why did this match" answers against the predicate rather than floating free. |
| **A record stream is added** | This is what `select` was standing in for. Batches and chunks are the missing abstraction — and it is `futures::Stream`, already a `liquers-core` dependency, rather than a bespoke trait. |

The net effect is a **smaller** core change than revision 1 and a **larger** record model.

## Known-Issue Preflight

Searched: every non-terminal `issue`/`feature` in `specs/index.csv` whose `area` intersects
`core/store`, `core/assets`, `core/value`, `core/commands`, `core/context`, `core/query`,
`core/plan`, `lib/commands`, `macro`, `store/backends`, `axum`, `web` — 93 records.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | The record-producing commands describe assets; this call must not schedule. | yes | no | **Fixed here** | keep P1 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes. Drove fields to JSON and the new variant behind `Arc`. | no | no | Honoured throughout | keep P2 |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | draft | P2 | Asks for selection *on the store*. Revision 2 does **not** close it — the user-facing gap closes, the trait gap does not. | no | no | Update it in Phase 5 to record that the capability now exists above the store, and that push-down remains open | keep P2 |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live. | no | no | Fields resolve by qualified name, so fixing it is a pure upgrade | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last. | no | no | Honoured | keep P2 |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Record-producing commands are ordinary commands with a `Context`; nothing stops one evaluating. | no | no | Purity is a contract until it lands | keep P2 |
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | Record-producing commands read the store. | no | no | Independent; `ns-search` owns its own commands | keep P3 |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft | P2 | A record-producing command over a directory inherits the divergence. | no | no | Document it in `SEARCH.md`: the record set is exactly what enumeration reported | keep P2 |
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | — | no | no | No endpoint added | keep P0 |
| `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`, `DIRECTORY-LISTING-DEPENDENCY-…`, `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`, `CORE-STORE-OPENBIN-MISSING`, `ASSET-EXPIRATION-EVENTS-…` | draft/accepted | P2–P3 | M4–M7 only | no | no | Monitor | keep |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Excluded by design | no | no | — | keep P2 |
| `STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` | draft | P2 | **No longer relevant** — revision 2 adds no conformance rules | no | no | — | keep P2 |

**No blocker.**

## Data Structures

New module `liquers-core/src/records.rs`.

### Record and its identity

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Index into the owning set's `sources`. Constant within a chunk in the common case,
    /// so the asset reference is stored once rather than per record.
    pub source: u32,
    /// Position within that source. `RecordId::Whole` when the source *is* the record.
    pub record_id: RecordId,
    /// A JSON **object**. Named, typed fields — what clauses test and what a SQL column is.
    pub fields: serde_json::Value,
    /// What a text clause matches. `None` when no text was projected.
    pub text: Option<Arc<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordId {
    Whole,
    Index(u64),
    Name(Arc<str>),
}
```

**`fields` is `serde_json::Value`**, which `liquers-core` already depends on directly
(`Cargo.toml:69`) and already uses in `commands.rs`. It is small, needs no type parameter, and makes
a record set serializable as JSON or NDJSON without a conversion step. The invariant that it is an
*object* is a constructor's job, not the type's — a newtype guaranteeing it is an option Phase 3 can
weigh.

### SourceInfo — identity, description and **retrieval**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// The asset these records were projected from, as a query.
    #[serde(with = "query_format")]
    pub asset: Query,
    /// The query that re-produces this source's records. **The retrieval path**: evaluating it
    /// yields the batch again, and `record_id` indexes into it.
    #[serde(with = "query_format")]
    pub chunk: Query,
    /// Description of the asset itself, when it has one. `None` for a source that is not an asset.
    pub info: Option<AssetInfo>,
    /// How to turn a `record_id` into a directly evaluable query, when the projection can.
    pub locator: Option<LocatorRule>,
}

/// A command to apply to `asset`, with the record id supplied as its final parameter.
/// Rendered through `ActionRequest`, never by string templating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatorRule {
    pub namespace: String,
    pub command: String,
    pub leading_parameters: Vec<String>,
}
```

**This is the review's central correction.** A hit must be *retrievable*, not merely identified.
`chunk` is the guaranteed path — re-evaluate and index — and `locator` is the direct one when a
projection can offer it (`-R/f.csv/-/ns-csv/row-42`). `info` is optional because **a CSV row has no
`AssetInfo`; the file does**, and one `AssetInfo` per source rather than per record also keeps 656
bytes from being repeated for every row.

### RecordSet — one value, and searches compose

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecordSet {
    /// Dictionary of sources; `Record::source` indexes it.
    pub sources: Vec<SourceInfo>,
    pub records: Vec<Record>,
    /// Optional and advisory. Field roles are its valuable content.
    pub schema: Option<RecordSchema>,
    /// A limit stopped production before the source was exhausted.
    pub truncated: bool,
    /// Present when this set is a search result; parallel to `records`.
    pub matches: Option<Vec<Vec<ClauseMatch>>>,
    pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClauseMatch {
    /// Which clause of the predicate matched. Answers "why" against the predicate itself.
    pub clause_index: usize,
    /// The field that satisfied it; `None` for a text clause over the body.
    pub field: Option<String>,
    pub excerpt: Option<String>,
    /// Reserved. `None` until a scoring clause exists — Phase 1's ordering promise.
    pub score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Diagnostics {
    pub scanned: usize,
    /// Fields a clause tested that no record could supply — the commonest cause of
    /// "why is X not in my results?", reported rather than silently false.
    pub unavailable_fields: Vec<String>,
}
```

**A search result *is* a record set**, so a search composes with another search and with any command
that consumes records. `matches` is parallel to `records` rather than embedded in `Record`, because a
record is data and evidence is about a *predicate*; the cost is the usual parallel-vector discipline,
which one constructor owns.

### SearchPredicate — a pure filter

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchPredicate {
    /// Implicit AND. Empty matches every record.
    pub clauses: Vec<Clause>,
    /// Hard cap on retained records. `None` is a deliberate, explicit choice by a caller that
    /// knows the stream is bounded.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Clause {
    Text { needle: String, case_sensitive: bool },
    Field { name: String, test: FieldTest },
    Not(Box<Clause>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Equals(serde_json::Value),
    Contains(String),
    Prefix(String),
    Glob(String),
    OneOf(Vec<serde_json::Value>),
    Exists,
}
```

**No `root`, no `sources`, no `depth`.** The stream decides what is in scope; the predicate decides
what survives. The `Key` clause of revision 1 becomes `Field { name: "key.path", test: Glob(..) }` —
a key is a field like any other, projected by whatever produced the record.

Neither enum is `#[non_exhaustive]`: a consumer that silently ignores a clause it does not understand
is a correctness bug, so an exhaustive match making it a compile error is the signal `CLAUDE.md`
exists to preserve. No match uses a default arm.

### Field resolution: qualified names, ambiguity is a parse error

`status` is the asset lifecycle on `MetadataRecord` and a document's lifecycle in front-matter. So a
record's field names are **qualified at projection time**: `meta.` (`meta.status`,
`meta.type_identifier`, `meta.file_size`, `meta.updated`), `attr.` (application attributes, when they
exist), `key.` (`key.path`, `key.name`, `key.extension`). `Clause::Field` matches a qualified name
**exactly**, so a predicate from HTTP or MCP is unambiguous by construction. Unqualified names are a
front-end convenience the syntax parser expands, **failing with an error naming every candidate**
when more than one source could supply it.

## The record stream: batches and chunks

This is what `select` was standing in for, and the abstraction the review identified as missing.

### The stream is `futures::Stream`, not a bespoke trait

`futures = "0.3.34"` is **already a direct dependency of `liquers-core`** (`Cargo.toml:77`, used in
`assets.rs`), so the standard trait costs nothing to adopt and a hand-rolled `next_batch` would be a
worse version of it. The whole combinator vocabulary comes with it — and it is exactly the vocabulary
this design needs: applying a predicate is `filter_map`, the limit is `take`, and the concurrency
deferred in revision 1 is later `buffer_unordered` rather than a rewrite.

`liquers-core/src/maybe_send.rs` already solves the wasm half, and its own documentation states the
trap: a trait-object bound cannot use the `MaybeSend` marker (E0225), so the **whole boxed type is
aliased per target**, and `FutureExt::boxed()` is "always `Send`-boxed and thus wrong on wasm".
`StreamExt::boxed()` has the identical defect, so the sibling alias and boxing helper are added
beside the existing ones:

```rust
// liquers-core/src/maybe_send.rs — mirroring BoxFuture / MaybeBoxed exactly
#[cfg(not(target_arch = "wasm32"))]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + 'a>>;

/// Box a stream with the target-correct Send-ness. Replaces `StreamExt::boxed()`,
/// which is always `Send`-boxed and therefore wrong on wasm.
pub trait MaybeBoxedStream<'a>: Stream + Sized + 'a {
    fn maybe_boxed(self) -> BoxStream<'a, Self::Item>;
}
```

```rust
// liquers-core/src/records.rs

/// A batch of records sharing one source dictionary. The unit of memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    pub sources: Vec<SourceInfo>,
    pub records: Vec<Record>,
}

/// An in-process stream of batches. A **type alias, not a trait** — there is nothing to add to
/// `Stream` that a combinator does not already give. Not a `Value`: neither cloneable nor
/// cacheable, which is why it stops at the query boundary (`record-model.md` §4).
pub type RecordBatchStream<'a> = BoxStream<'a, Result<RecordBatch, Error>>;

/// A source that knows its own partition. The unit of refresh.
#[async_trait]
pub trait ChunkedRecordSource: MaybeSend + MaybeSync {
    /// Chunk descriptors — id, refresh query, version — **without producing any records**.
    async fn partition(&self) -> Result<Vec<ChunkDescriptor>, Error>;
    /// Open one chunk. The returned stream borrows `self`.
    async fn open_chunk(&self, id: &ChunkId) -> Result<RecordBatchStream<'_>, Error>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    #[serde(with = "query_format")]
    pub query: Query,
    pub version: Option<Version>,
    pub source: SourceInfo,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}
```

**Three consequences of using the standard trait**, each a simplification:

1. **`RecordStream` stops being a trait.** One fewer concept, and no object-safety question to
   reason about: `Stream` is object-safe and `BoxStream` is the established boxed form.
2. **`schema()` moves off the stream** onto `ChunkDescriptor`, which is where it belongs — a schema
   describes a *source*, not an iteration, and a consumer needs it *before* opening the stream in
   order to configure an engine (`record-model.md` §5).
3. **`filter_batch` becomes a combinator.** `SearchPredicate` keeps a per-batch method for testing,
   but the pipeline is `stream.map(|b| predicate.filter_batch(b)).take(limit)` rather than a
   hand-written loop.

`ChunkedRecordSource` uses the project's `MaybeSend`/`MaybeSync` supertrait markers rather than bare
`Send`/`Sync`, as `maybe_send.rs` requires for supertrait bounds, and `#[async_trait]` at each site
needs the usual `cfg_attr(…, async_trait(?Send))` pair.

**Defined now, minimally implemented now.** The roadmap's M0 said Level 0 only; this is a deliberate
extension, on the roadmap's own test: a stream interface is F4 generalized ("a bounded opaque result,
not a bare `Vec`"), expensive to retrofit and cheap to define. M0–M3 ship one implementation —
`futures::stream::once` over a `RecordSet`'s single batch — so nothing streams yet and nothing has to
be redesigned when something does.

### Value extension

```rust
// liquers-core/src/value.rs
pub enum Value {
    // … existing …
    Records(Arc<RecordSet>),
}
```

One variant, `Arc`-wrapped, so `Value` does not grow past 704 bytes. Type identifier `Records`
(bare CamelCase — `liquers-core` owns the concept), default extension `json`, media type
`application/json`. A `TypeInfo` entry in `Value::type_descriptions()` (`value.rs:407`) is
**required** — `CLAUDE.md`'s "four steps, not three; a type with no `TypeInfo` cannot be stored" —
and `type_descriptions_match_identifier` checks it. Every existing `match` on `Value` gains an
explicit arm; a comparable variant (`Value::Recipe`) is named at 17 sites, 8 of them in `value.rs`.

## Clause syntax: one action per clause

The question was whether clauses need a syntax, and whether Liquers entities (`~entity`) should
carry it. **The query language already has a syntax for composition, and clauses should use it.**

```
-R-key/specs/issues/-/ns-search/records/text-expiration/not_text-expired/field-meta.status-draft
```

Checked with `liquers-validate`: it plans cleanly and `meta.status` decodes as written — a bare `.`
is not a separator, so a qualified field name needs no escaping.

One action per clause. Each parameter is escaped independently by `ActionRequest::encode`, so no
operator character can collide with `-`, `~` or `/`; every clause is separately validatable by
`liquers-validate`; and the chain is inspectable in the plan, which is what lets an external engine
translate it (execution path (b)).

**The entity alternative, and why not now.** A single parameter carrying `"phrase" -excluded
kind:issue` needs operator characters that the query language reserves, and entities are the
mechanism for that — `~n<name>~` decodes named entities today. Defining search-specific entities
would work, but it extends the query language for an ergonomic gain that composition already
provides, and the entity table decodes to *characters*, so the operators would still need a parser
downstream. **Deferred, and named as the right mechanism if a one-parameter syntax is ever wanted.**

The human-facing syntax (`use-cases.md` U1: one box, no syntax required) is then a **front end that
compiles to a clause chain**, living in `liquers-lib` and never in the query itself.

## Execution: two paths from one query

Because the source is a query, a search has exactly two executions, and the design supports both
without choosing:

| Path | How | When |
|---|---|---|
| **(a) Evaluate and filter** | Evaluate the source query, consume its `RecordStream`, apply the predicate batch by batch, retain up to `limit` | The MVP, and the only path for a corpus with no engine |
| **(b) Push the query to an engine** | Hand the source query and the clause chain to an external engine that already processed it | `interoperability-layer.md`; the engine is fed by the same query, so what it holds and what (a) would compute are the same records |

Path (b) needs no new interface here: the engine reads the plan.

## Trait Implementations

### `AssetManager::get_asset_info` — repaired, and the only core-trait change

```rust
/// Describe an asset. **Never schedules evaluation.**
///
/// Previously a live key was routed through `get`, which submits to the job queue for an asset
/// that is neither finished nor fast-trackable — so describing an asset in `Status::Recipe` ran
/// it. It now reports from the handle `lookup_key_asset` already returns.
async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error>;
```

Confirmed at `assets.rs:3967-3970` (trait default) and `:5334-5336` (`DefaultAssetManager`).
`listdir_asset_info` (`:4043`) calls it per entry and inherits the repair, so listing a directory
stops starting the assets in it.

### No `select`, and no conformance rules

Revision 1 added `AsyncStore::select` and `AssetManager::select` with a default scan and six
`select*` conformance rules. **Both are dropped.** With records produced by commands, a trait method
is a push-down optimization: valuable when a backend can filter without materializing, absent
everywhere today, and addable later without changing a single consumer. Dropping it removes all
changes to two widely-implemented traits, removes the rule family, and makes
`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` irrelevant to this design.

The cost is stated plainly: `STORE-NO-CONTENT-OR-METADATA-SEARCH` asks for selection *on the store*
and is **not** closed by this work. Phase 5 updates it to record that the capability exists above the
store and that push-down remains open.

## Function Signatures

### `liquers-core/src/records.rs`

```rust
impl Clause {
    /// Pure match. `None` means a field the record cannot supply — recorded in
    /// `unavailable_fields` and treated as "does not match".
    pub fn matches(&self, record: &Record, fields: &serde_json::Value) -> Option<bool>;
}

impl SearchPredicate {
    /// Filter one batch, appending survivors and their evidence.
    pub fn filter_batch(&self, batch: &RecordBatch, out: &mut RecordSet) -> Result<(), Error>;
    /// True when no clause needs `Record::text` — the producer may then skip projecting it.
    pub fn needs_text(&self) -> bool;
}

impl SourceInfo {
    /// Build the directly evaluable query for one record, when `locator` allows.
    pub fn locator_query(&self, record_id: &RecordId) -> Option<Query>;
}

pub fn excerpt(text: &str, needle: &str, case_sensitive: bool, radius: usize) -> Option<String>;
```

### `liquers-lib/src/search/mod.rs`

```rust
/// Produce records describing the assets under the key carried by the state.
/// One record per asset (Level 0). Never evaluates: uses the repaired `get_asset_info`.
pub async fn records(state: State<Value>, context: Context<CommandEnvironment>)
    -> Result<Value, Error>;

/// Produce records describing registered commands. Command discovery, no search-specific code.
pub async fn command_records(state: State<Value>, context: Context<CommandEnvironment>)
    -> Result<Value, Error>;

/// Clause commands — each applies one clause to the record set in the state.
pub fn text(state: &State<Value>, needle: String) -> Result<Value, Error>;
pub fn not_text(state: &State<Value>, needle: String) -> Result<Value, Error>;
pub fn field(state: &State<Value>, name: String, value: String) -> Result<Value, Error>;
pub fn limit(state: &State<Value>, n: i64) -> Result<Value, Error>;

/// The one-box convenience: compile a human syntax into clauses and apply them.
pub fn search(state: &State<Value>, query: String) -> Result<Value, Error>;

/// Compile the human syntax. Unrecognised input is a literal term, not an error;
/// an ambiguous unqualified field name IS an error, naming every candidate.
pub fn parse_search_syntax(input: &str) -> Result<Vec<Clause>, Error>;
```

Clause commands are **sync and borrow** — pure transformations of a value in hand. `records` and
`command_records` are **async with owned `State`**, per the macro's rule, with `context` last.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/records.rs` (new) | Record, SourceInfo, RecordSet, predicate, `RecordBatchStream`, `ChunkedRecordSource`, pure matching |
| `liquers-core` | `src/maybe_send.rs` | `BoxStream` + `MaybeBoxedStream`, mirroring `BoxFuture`/`MaybeBoxed` |
| `liquers-core` | `src/lib.rs` | `pub mod records;` |
| `liquers-core` | `src/value.rs` | `Value::Records(Arc<RecordSet>)`, every match arm, the `TypeInfo` entry |
| `liquers-core` | `src/assets.rs` | `get_asset_info` repair (two sites) |
| `liquers-lib` | `src/search/mod.rs` (new) | Record producers, clause commands, syntax parser |
| `liquers-lib` | `src/commands.rs` | `register_command!` registrations |
| `specs` | `command_registry.yaml` | Regenerated |

**`liquers-core/src/store.rs` is untouched.** No dependency added: `serde_json` and `async_trait` are
already direct dependencies.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/RECORDS.md` | reference (new) | contributor, agent | The record, source identity and **retrieval**, the batch/chunk/stream contract, field qualification, the predicate and its match semantics. Revision 2 makes this the primary new reference — records are the capability, search is its first consumer |
| `specs/reference/SEARCH.md` | reference (new) | contributor, agent | Clause semantics, the clause-per-action form, the human syntax, the ordering promise, the invariants, and what a record set says when a field is unavailable |
| `specs/reference/ASSETS.md` | reference | contributor | **`get_asset_info` never schedules** — the behaviour change, stated where the asset lifecycle is owned |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | reference | contributor | The `Records` type identifier and its `TypeInfo` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | contributor | How a record-producing command is written, and the purity expectation on it |
| `specs/README.md` | map | all | Capability line `designing` → `built` |

**No change to `STORE_SEMANTICS.md`, `CONFORMANCE_TERMS.md` or
`STORE_IMPLEMENTATION_GUIDE.md`** — revision 2 touches no store trait, which is the clearest measure
of how much smaller the core change became.

`affects_docs`: `reference/RECORDS.md`, `reference/SEARCH.md`, `reference/ASSETS.md`,
`reference/VALUE_TYPE_SYSTEM.md`, `guides/COMMAND_REGISTRATION_GUIDE.md`.

## Relevant Commands — namespace `search`

| Command | Signature | Purpose |
|---|---|---|
| `records` | `async fn records(state, context) -> result` | Assets under the state's key become records |
| `command_records` | `async fn command_records(state, context) -> result` | The registry becomes records |
| `text` / `not_text` | `fn …(state, needle: String) -> result` | One text clause |
| `field` | `fn field(state, name: String, value: String) -> result` | One field clause |
| `limit` | `fn limit(state, n: i64 = 50) -> result` | Truncate, setting `truncated` |
| `search` | `fn search(state, query: String) -> result` | The one-box front end |

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| State is not a `Value::Records` | `Error::conversion_error` |
| Unreadable entry while producing records | Skipped, counted in `scanned` |
| Unresolvable field | Not an error — the clause does not match; the name lands in `unavailable_fields` |
| Ambiguous unqualified field name | `Error::general_error` naming every candidate — the one place the syntax is intolerant |
| Malformed search syntax | Treated as a literal term |

## Sync vs Async, Serialization, Concurrency

Record production is **async** (store access); clause application, syntax parsing and excerpting are
**sync** (pure). Every data type derives `Serialize, Deserialize`; `Query` uses the existing
`query_format` helper. The stream traits are not serializable and deliberately never cross a query
boundary. No shared mutable state, no lock held across an `.await`; batch production is sequential,
with concurrency deferred until there is something to measure.

## Compilation Validation

- `Stream` is object-safe and already boxed through the project's per-target alias pattern;
  `ChunkedRecordSource` is object-safe (`&self`, no generics, concrete types).
- `BoxStream`/`MaybeBoxedStream` are gated on `target_arch`, **never** on a Cargo feature —
  `maybe_send.rs` documents why: feature unification would silently strip `Send` from the native
  build workspace-wide.
- `Value::Records(Arc<_>)` keeps `size_of::<Value>()` unchanged; the `TypeInfo` entry is present.
- Every `match` on `Value` gains an explicit arm; no default arm anywhere.
- `liquers-core` gains no dependency on `liquers-lib`.
- No feature gate; the build matrix runs anyway because `Value` changed.

## Open Questions for Phase 3

1. Is `Record::fields` a newtype guaranteeing a JSON object, or a bare `serde_json::Value` with the
   invariant owned by constructors?
2. Are `matches` parallel to `records` acceptable, or should a search result be a distinct type that
   *contains* a `RecordSet`? Parallel vectors let a search result compose as a record set; a wrapper
   is safer and costs one unwrap at every consumer.
3. Does `ChunkedRecordSource` earn its place in M0–M3, given nothing implements a non-trivial
   partition until M5? (`RecordBatchStream` is now a type alias and costs nothing, so the question
   narrows to the trait.)
4. `RecordSchema` is referenced but not specified here — field roles are its valuable content
   (`record-model.md` §5). Specify it in Phase 3 or defer the field to M5?
5. Should `limit` default to `Some(50)` at the command layer while the type allows `None`, or should
   the type forbid `None` as revision 1 had it?
