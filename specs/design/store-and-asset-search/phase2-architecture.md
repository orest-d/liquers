# Phase 2: Solution & Architecture — Store and asset search

> **Revision 7 — the split.** Revisions 1–6 developed a search design and a record model together,
> and the record half outgrew the search half. Revision 7 moves records, columns, schema, buffers and
> the stream to [`record-streams`](../record-streams/), leaving this design with what is actually
> about searching: the predicate, its syntax, its parser, the two execution paths and the
> `get_asset_info` repair. §0 records the whole revision history.
>
> **This design now depends on `record-streams`,** which is being stabilized first. Nothing here can
> be implemented before that design's Phase 1 is approved.

## Overview

A search is a **predicate applied to a stream of records**. The records, their columnar batches, the
schema and the stream come from [`record-streams`](../record-streams/); this design adds the
predicate that filters them, the syntax a person or an agent writes it in, and the single command
that applies it. `liquers-core` gains a predicate type; `liquers-lib` gains an `ns-search`
namespace. **No new method on `AsyncStore` or `AssetManager`** beyond repairing `get_asset_info` so
that describing an asset stops starting it.

Scope is milestones **M0–M3** of [`roadmap.md`](./roadmap.md) plus the `get_asset_info` repair, all
of it sitting on the record mechanism.

## 0. What changed, and what it reverses

| Rev | Change | Why |
|---|---|---|
| 2 | **`root` and `sources` leave the predicate** | The predicate is a filter; *what it filters* is the stream. Putting the source inside it smuggled the set into the filter, contradicting this design's own model ("select records **from a set**"). The source is a **query**, which also gives the two execution paths |
| 2 | **`SearchSource` deleted** | With the source being a query, command discovery is a command producing records from the registry — not an enum variant |
| 2 | **`AsyncStore::select` and `AssetManager::select` dropped** | **Reverses Phase 1 axis B2/B3.** If records come from commands, a trait method is a *push-down optimization*, not the mechanism. Removes every change to two widely-implemented traits and the whole conformance-rule family, and forecloses nothing |
| 2 | **`Hit` replaced** | It embedded an `AssetInfo`, which assumes one record per asset. **A CSV row has no `AssetInfo`; the file does.** Asset description moved to a per-source side table, and retrieval became explicit |
| 5 | **The predicate is an expression, not a filter pipeline** | A clause chain cannot express `OR` or grouping, never produces the predicate as a whole — so an external engine has a sequence of steps to reverse-engineer — and is eager, which forecloses filter-then-verify and push-down |
| 6 | **The expression is syntax parsed by the command — no `Value::Predicate`** | Revision 5 reached for link parameters, which would require adding a predicate variant to the core value enum for a capability nothing yet needs. A syntax string in one parameter answers all three objections *better* |
| **7** | **Records leave this design entirely** | Six revisions established that the interesting half was not the search. Records serve four consumers of which search is one, and carry requirements search never raises (multi-gigabyte lazy processing, per-chunk provenance). They are stabilized first, in their own design, and search is rebuilt on top |
| **7** | **`ClauseMatch` and the parallel match vector replaced by evidence columns** | A record set that carries a field only search fills is a record set that knows what a clause is. Evidence becomes three `Stored`-role columns on the result batch — **which resolves revision 6's open question 2**, and better than either option it offered. See §"Evidence" |

Revisions 3 and 4 (the columnar, Arrow-laid-out batch, and the schema that owns names, types and
roles) are not listed as reversed — they were *correct*, and they are exactly what moved to
`record-streams`. Their reasoning lives there now, together with
[`record-model.md`](../record-streams/record-model.md), which was written here and moved with them.

## Dependency on `record-streams`

Everything tabular is that design's. This one **consumes** it and adds nothing to it:

| From `record-streams` | Used here for |
|---|---|
| `RecordBatch`, `Column`, `Bitmap` | The predicate evaluates to a `Bitmap` per clause over a `Column`; the batch's `filter` gathers the survivors |
| `RecordSchema`, `FieldSchema`, `FieldRole` | `bind` resolves a field name to a column index once per batch; a `Text` clause targets **every** `Text`-role column |
| `FieldValue` | What a `FieldTest` compares against |
| `RecordSource`, `RecordBatchStream` | A source opens the stream the predicate is applied to, and `RecordSource::chunks()` is the partition an external engine reconciles against. A source is re-openable, so a search can be re-run without re-deriving it |
| `ChunkOrigin`, `LocatorRule` | How a surviving row is retrieved — the `chunk` query always, the `locator` when the projection offers one |
| `RecordBatch`, `RecordSource` | The result value. `RecordSet` and `Diagnostics` were **removed** from that design during its Phase 2 review — a result is a batch or a stream, and evaluation facts go to `Metadata`'s log |
| Field qualification (`meta.`, `attr.`, `key.`) | The names a predicate references |
| `ExtValue::RecordChunk`, `ExtValue::RecordSource` | The result is an ordinary value, so a search composes with any record consumer. Note these are `ExtValue` in `liquers-lib`, **not** `Value` in core — an opaque stream cannot satisfy `Value`'s `Deserialize` bound |

**If the record design changes, this one follows.** In particular, open questions 5 and 6 there
(the extension point for derived columns, and where the 64-clause cap is documented) are answered
jointly with §"Evidence" below.

## Known-Issue Preflight

Searched: every non-terminal `issue`/`feature` in `specs/index.csv` whose `area` intersects
`core/store`, `core/assets`, `core/value`, `core/commands`, `core/context`, `core/query`,
`core/plan`, `lib/commands`, `macro`, `store/backends`, `axum`, `web` — 93 records.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `NO-RECORD-STREAM-ABSTRACTION` | draft | P2 | **The prerequisite.** Search filters a record stream; there is none | **yes** | **yes** | Resolved by the `record-streams` design, which is stabilized first | **raise to P1 when this design's Phase 2 is approved** — a blocker must be at least P1 (§4.4), and it is not one until this design is live |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | The record-producing commands describe assets; this call must not schedule | yes | no | **Fixed here** | keep P1 |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | draft | P2 | Asks for selection *on the store*. This design does **not** close it — the user-facing gap closes, the trait gap does not | no | no | Update in Phase 5 to record that the capability exists above the store and push-down remains open | keep P2 |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live, so `attr.status:draft` has nothing to match | no | no | Fields resolve by qualified name, so fixing it is a pure upgrade | keep P2 |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | Record-producing commands are ordinary commands with a `Context`; nothing stops one evaluating | no | no | Purity is a contract until it lands | keep P2 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be last | no | no | Honoured | keep P2 |
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | Record-producing commands read the store | no | no | Independent; `ns-search` owns its own commands | keep P3 |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft | P2 | A record-producing command over a directory inherits the divergence | no | no | Document in `SEARCH.md`: the result is exactly what enumeration reported | keep P2 |
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | — | no | no | No endpoint added | keep P0 |
| `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`, `DIRECTORY-LISTING-DEPENDENCY-…`, `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`, `CORE-STORE-OPENBIN-MISSING`, `ASSET-EXPIRATION-EVENTS-…` | draft/accepted | P2–P3 | M4–M7 only | no | no | Monitor | keep |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Excluded by design | no | no | — | keep P2 |

**One blocker: `NO-RECORD-STREAM-ABSTRACTION`,** and it is blocking by construction — the split made
it so deliberately, rather than leaving it implicit inside a larger design. It is resolved not by a
fix but by the `record-streams` design completing. **This Phase 2 cannot be approved while that
design's Phase 1 is unapproved**, which is the intended sequencing rather than an obstacle.

## Data Structures

New module `liquers-lib/src/search/`, behind the `records` feature.

**Changed by `record-streams` revision 5.** The predicate operates on `RecordBatch`, and records
moved to `liquers-lib` behind a feature — so the predicate follows. `liquers-core` gains nothing from
this design either, which makes the whole search capability additive to one crate. Everything tabular comes from `liquers_core::records`.

### SearchPredicate — a pure filter over a record stream

`SearchPredicate` is a `liquers-core` type — `Serialize`/`Deserialize`, so an HTTP or MCP caller that
would rather send a structured predicate than a string can — but it is **not** a `Value` variant.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchPredicate {
    pub expr: Predicate,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Predicate {
    /// Matches everything — the identity of `All`, and what an empty expression parses to.
    #[default]
    Always,
    Never,
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    /// Matches against **every** column whose role is `Text`. No privileged target.
    Text { needle: String, case_sensitive: bool },
    Field { name: String, test: FieldTest },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Equals(FieldValue),
    Contains(String),
    Prefix(String),
    Glob(String),
    Range { min: Option<FieldValue>, max: Option<FieldValue> },
    OneOf(Vec<FieldValue>),
    Exists,
}
```

**No `root`, no `sources`, no `depth`.** The stream decides what is in scope; the predicate decides
what survives. The `Key` clause of revision 1 becomes `Field { name: "key.path", test: Glob(..) }` —
a key is a field like any other.

**A `Text` clause has no privileged target.** It matches against every `Text`-role column, which is
the review's point: a title, a body and a comment are all text and none is special.

**Evaluation is mask-based and bottom-up over columns.** Each leaf yields a `Bitmap` over the batch;
`All` ANDs its children, `Any` ORs them, `Not` complements; the surviving rows are gathered once with
`RecordBatch::filter`. That is how a vectorized engine evaluates a filter tree, it is faster than a
row walk, and it makes evidence cheap to attribute because the mask says exactly which node admitted
which row.

Neither `Predicate` nor `FieldTest` is `#[non_exhaustive]`: a consumer that silently ignores a node
it does not understand is a correctness bug, so an exhaustive match making it a compile error is the
signal `CLAUDE.md` exists to preserve. No match uses a default arm.

### Evidence

Revision 6 left open whether a search result should be a record set with a parallel
`matches: Vec<Vec<ClauseMatch>>` or a distinct type wrapping one. **Revision 7 answers: neither.**
Evidence is expressed the way every other per-row fact is — as columns appended to the result batch
with role `Stored`:

| Column | Type | Meaning |
|---|---|---|
| `match.clauses` | `UInt` | Bitmask; bit *i* is set when node *i* of a pre-order walk of `expr` admitted this row |
| `match.excerpt` | `Text`, nullable | The best excerpt, when a text clause produced one |
| `match.score` | `Float`, nullable | `Null` until a scoring clause exists — Phase 1's ordering promise, reserved |

This is strictly better than either option that was on the table: a search result composes with any
record consumer with no unwrapping, evidence serializes as CSV or NDJSON like everything else, and
the record design loses a field it should never have had (and, in its own review, the whole
`RecordSet` type). **Two costs, stated rather than discovered
later:** the bitmask caps a predicate at **64 nodes**, which `parse_search_syntax` reports as an
error rather than truncating silently; and only one excerpt per row is representable, which is the
same trade every search UI makes.

The columns are added through `RecordBatch::with_columns`, whose suitability as the extension point
is `record-streams` open question 5 — this is its first user.

## The predicate is written as syntax, parsed by the command

Revision 5 corrected revision 2's clause-chain form

```
-R-key/specs/issues/-/ns-search/records/text-expiration/not_text-expired/field-meta.status-draft
```

which was described as "the query language already has a syntax for clauses". It does not. That chain
mixes two different things — building a record stream, then **progressively reducing it** — and a
filter pipeline is not an expression that evaluates to a predicate. Three consequences, each real:

1. **It cannot express `OR` or grouping.** Sequential filters are AND-only. `a AND (b OR c)` would
   need `union`/`intersect` commands over materialized record sets.
2. **The predicate never exists as a value.** It is only a side effect of the chain — so it cannot be
   handed to an external engine, stored, or reused. That directly undermines execution path (b),
   where what the engine needs *is the predicate*.
3. **It is eager.** Each step reduces a set, so nothing can know the whole predicate before touching
   data — which is what filter-then-verify and push-down both require.

### A syntax and a parser, not a Value variant

The obvious fix — make the predicate a value and pass it through a **link parameter**
(`~X~<query>~E`, which does exist and would work) — requires `Value::Predicate`. That is a permanent
addition to the core value enum in exchange for a capability nothing yet needs, so it is **not
taken**. Instead the predicate is an ordinary Rust type, and a syntax with a parser produces it
inside the command.

```
-R-key/specs/issues/-/ns-search/records/select-<expression>
```

**All three objections to the pipeline are answered better this way than by links:**

| Objection | Answer |
|---|---|
| Cannot express `OR` or grouping | The *syntax* has `\|` and parentheses. Validated below |
| The predicate is never a value | It **is** one — as **text in a single parameter**. A perfectly good serialized form: storable in a recipe, inspectable in the plan, and handed to an external engine as one string it parses with the same parser. Strictly better than reverse-engineering a chain of steps, and it costs no `Value` variant |
| Eager evaluation | The whole expression is parsed **before** the stream is touched, so filter-then-verify and push-down both remain open |

### The syntax

```ebnf
expr    = or ;
or      = and , { "|" , and } ;          (* lowest precedence *)
and     = unary , { unary } ;            (* implicit AND by juxtaposition *)
unary   = [ "-" ] , atom ;               (* leading "-" negates *)
atom    = "(" , expr , ")"
        | field , ":" , value
        | '"' , phrase , '"'
        | term ;
```

The vocabulary is the intersection of what Google, GitHub and Lucene users already expect:

```
expiration                          a term
"expiration safety"                 a phrase
-expired                            negation
meta.status:draft                   a field test
expiry | expiration                 disjunction
(expiry | expiration) -expired      grouping
```

Unrecognised input is a **literal term**, not an error — except an ambiguous unqualified field name,
which is reported with every candidate, and an expression exceeding 64 nodes, which is reported
rather than truncated. An empty expression is `Predicate::Always`.

### Ergonomics, honestly

Every operator character must be escaped inside a query parameter. Checked: `(`, `)`, `|`, `:` and
`>` all **fail to parse raw**; only alphanumerics, `.`, `_` and escapes survive. So

```
ns-search/select-~nlpar~expiry~.~nverbar~~.expiration~nrpar~~.~_expired~.meta.status~ncolon~draft
```

decodes to exactly `(expiry | expiration) -expired meta.status:draft` — verified with
`liquers-validate`. A hand-written search URL is therefore essentially unwritable, and that is fine:
the user types into a box, and a UI, an MCP tool or `ActionRequest::encode` builds the query. Note
`~_` for the ASCII hyphen, **not** `~nminus~` (U+2212) — the trap the escaping guide warns about, and
one this design fell into once already.

### When the `Value` variant would earn its place

Recorded so the decision can be revisited on evidence rather than taste. Add `Value::Predicate` when
one of these actually arrives:

- a predicate must be **built by one query and consumed by another** — a saved-search asset composed
  into a larger search;
- a predicate must be **produced by a command** rather than written, for instance derived from a
  user's profile or from another record set;
- **partial predicates need reuse** across several searches without repeating their text.

Until then a string in a parameter carries the same information at no cost to the value system.

## Execution: two paths from one query

Because the source is a query, a search has exactly two executions, and the design supports both
without choosing:

| Path | How | When |
|---|---|---|
| **(a) Evaluate and filter** | Evaluate the source query, consume its `RecordBatchStream`, apply the predicate batch by batch, retain up to `limit` | The MVP, and the only path for a corpus with no engine |
| **(b) Push the predicate to an engine** | Hand the source query and the predicate text to an external engine already fed from the same query | [`interoperability-layer.md`](./interoperability-layer.md); what the engine holds and what (a) would compute are the same records, because `partition()` reconciliation keeps them so |

Path (b) needs no new interface here: the engine reads the plan and parses the predicate text with
the same parser.

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

### `liquers-core/src/search.rs`

```rust
/// A predicate bound to one schema: every field name already resolved to a column index.
/// Built once per batch, evaluated column-wise.
pub struct BoundPredicate<'a> { /* … */ }

impl SearchPredicate {
    /// Resolve names against a schema. Names no schema declares are returned for
    /// a `Warning` log entry on the evaluation's `Metadata` rather than failing.
    pub fn bind(&self, schema: &RecordSchema) -> (BoundPredicate<'_>, Vec<String>);
    /// True when no clause needs a `Text`-role field — the producer may skip projecting bodies.
    pub fn needs_text(&self) -> bool;
    /// Node count of a pre-order walk. More than 64 exceeds the evidence bitmask.
    pub fn node_count(&self) -> usize;
}

impl<'a> BoundPredicate<'a> {
    /// One mask per node, over the whole batch. Pure; no I/O.
    pub fn node_masks(&self, batch: &RecordBatch) -> Result<Vec<Bitmap>, Error>;
    /// Combine the masks, gather the surviving rows, and append the evidence columns.
    pub fn apply(&self, batch: &RecordBatch) -> Result<RecordBatch, Error>;
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

/// Parse the search syntax into a predicate. Unrecognised input is a literal term, not an
/// error; an ambiguous unqualified field name IS an error, naming every candidate, and so is
/// an expression of more than 64 nodes.
pub fn parse_search_syntax(input: &str) -> Result<Predicate, Error>;

/// Parse `expr` and apply it to the record stream in the state. The only search command.
pub fn select(state: &State<Value>, expr: String, limit: i64) -> Result<Value, Error>;
```

`select` is **sync and borrows** — a pure transformation of a value in hand. `records` and
`command_records` are **async with owned `State`**, per the macro's rule, with `context` last.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-lib` | `src/search/predicate.rs` (new, `records` feature) | `SearchPredicate`, `Predicate`, `FieldTest`, `BoundPredicate`, `excerpt` |

| `liquers-core` | `src/assets.rs` | `get_asset_info` repair (two sites) |
| `liquers-lib` | `src/search/mod.rs` (new) | Record producers, the syntax parser, the `select` command |
| `liquers-lib` | `src/commands.rs` | `register_command!` registrations |
| `specs` | `command_registry.yaml` | Regenerated |

**No new dependency.** `serde` and `async_trait` are already direct dependencies of `liquers-core`;
everything columnar comes from `liquers_core::records`. **`liquers-core/src/store.rs` and
`src/value.rs` are untouched by this design** — the value variants belong to `record-streams`, and
live on `ExtValue` in `liquers-lib`.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/SEARCH.md` | reference (new) | contributor, agent | Predicate semantics, the syntax and its grammar, the evidence columns, the ordering promise, the two execution paths, and what a result says when a field is unavailable |
| `specs/reference/ASSETS.md` | reference | contributor | **`get_asset_info` never schedules** — the behaviour change, stated where the asset lifecycle is owned |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | contributor | The purity expectation on a record-producing command |
| `specs/README.md` | map | all | Capability line `designing` → `built` |

`specs/reference/RECORD_STREAMS.md` and `specs/guides/RECORD_STREAM_GUIDE.md` are the
`record-streams` design's, and `SEARCH.md` links to them rather than restating them.

**No change to `STORE_SEMANTICS.md`, `CONFORMANCE_TERMS.md` or `STORE_IMPLEMENTATION_GUIDE.md`** —
this design touches no store trait, which is the clearest measure of how much smaller the core change
became across the revisions.

`affects_docs`: `reference/SEARCH.md`, `reference/ASSETS.md`, `guides/COMMAND_REGISTRATION_GUIDE.md`.

## Relevant Commands — namespace `ns-search`

| Command | Signature | Purpose |
|---|---|---|
| `records` | `async fn records(state, context) -> result` | Assets under the state's key become records |
| `command_records` | `async fn command_records(state, context) -> result` | The registry becomes records |
| `select` | `fn select(state, expr: String = "", limit: i64 = 50) -> result` | Parse the expression and apply it to the record stream in the state. **The only search command** |

`ns-records` (serialization, schema inspection, head) belongs to `record-streams`.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors. No `Error::new`, no new error
type, no `unwrap`/`expect`.

| Situation | Outcome |
|---|---|
| State is not a record chunk or stream | `Error::conversion_error` |
| Unreadable entry while producing records | Skipped, counted in an `Info` log entry on `Metadata` |
| Unresolvable field | Not an error — the clause does not match; a `Warning` log entry names it |
| Ambiguous unqualified field name | `Error::general_error` naming every candidate |
| An expression of more than 64 nodes | `Error::general_error` — the evidence bitmask's cap, reported rather than truncated |
| Otherwise malformed search syntax | Treated as a literal term |

## Sync vs Async, Serialization, Concurrency

Record production is **async** (store access); predicate binding, evaluation, syntax parsing and
excerpting are **sync** (pure). `SearchPredicate`, `Predicate` and `FieldTest` derive
`Serialize, Deserialize` so a structured predicate can arrive over HTTP or MCP; `BoundPredicate`
borrows a schema and is not serializable. No shared mutable state, no lock held across an `.await`.

## Compilation Validation

- `SearchPredicate` and `Predicate` are concrete — no type parameter, because `FieldValue` is a
  dynamic enum.
- `BoundPredicate<'a>` borrows the schema it was bound against; the lifetime ties it to one batch's
  `Arc<RecordSchema>`, which outlives the evaluation.
- Every `match` on `Predicate` and `FieldTest` is exhaustive; no default arm anywhere.
- `liquers-core` gains no dependency on `liquers-lib`, and no `unsafe`.
- No feature gate. The build matrix runs anyway because `record-streams` changes `Value`.

## Open Questions for Phase 3

1. ~~Are `matches` parallel to the rows acceptable, or should a search result be a distinct type?~~
   **Resolved by the split**: neither — evidence is columns. See §"Evidence".
2. Should `limit` default to `Some(50)` at the command layer while the type allows `None`, or should
   the type forbid `None`?
3. Does a phrase clause need position information, or is substring matching on the concatenated text
   column honest enough for the MVP?
4. Does `Text` matching every `Text`-role column need a way to *restrict* to one without naming it as
   a `Field` clause — `title:(a | b)` — or is the field test enough?
5. How does an external engine receive the predicate text: as a parameter on the plan step it reads,
   or reconstructed from the `ActionRequest`? The former is simpler; the latter needs nothing new.
