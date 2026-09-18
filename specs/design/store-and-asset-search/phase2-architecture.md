# Phase 2: Solution & Architecture — Store and asset search

## Overview

Search is a **predicate over records**. `liquers-core` gains three small types — a predicate, a
record, a result — plus one defaulted `AsyncStore` method that scans, and one `AssetManager` method
that unions the store with live assets, recipe-declared keys and the command registry without
evaluating anything. `liquers-lib` gains an `ns-search` namespace and a small syntax front end.

Scope is milestones **M0–M3** of [`roadmap.md`](./roadmap.md) plus the `get_asset_info` repair,
which the user placed in scope. Level 1 (record ids, chunks, batches, schema) is specified in
[`record-model.md`](./record-model.md) as direction and is **not implemented here**; only the shapes
that would otherwise be expensive to retrofit are fixed now.

## Known-Issue Preflight

Searched: every non-terminal `issue`/`feature` in `specs/index.csv` whose `area` intersects
`core/store`, `core/assets`, `core/value`, `core/commands`, `core/context`, `core/query`,
`core/plan`, `lib/commands`, `macro`, `store/backends`, `axum`, `web` — 93 records — then read those
touching enumeration, metadata, value size, conformance IDs, command registration or the asset
lifecycle.

| Issue | Status | Pri | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | `get_asset_info` routes a live key through `get`, which submits to the job queue. The non-evaluation invariant rests on this call. | yes | no — hand-rolled resolution is a workaround | **Fixed in this design** (§Trait Implementations) | keep P1 |
| `STORE-NO-CONTENT-OR-METADATA-SEARCH` | draft | P2 | The gap this design closes. | — | no | Close in Phase 5 | keep P2 |
| `CORE-VALUE-ENUM-OVERSIZED` | draft | P2 | `Value` is 704 bytes; a new variant carrying a struct inline would make it worse. | no | no | New variant is `Arc`-wrapped (8 bytes) — §Value Extension | keep P2 |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft | P2 | The default scan inherits `listdir_keys_deep`, so a metadata-only key is searchable in OpenDAL and invisible in `AsyncFileStore`. **A store divergence becomes a search divergence.** | no | no | Conformance rule `select04` states that selection sees exactly what enumeration reports, which pins the divergence to its real cause | keep P2 |
| `STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` | draft | P2 | We add conformance rules; reusing an existing family would deepen the collision. | no | no | New `select*` family, verified against `liquers-store` unit-test names | keep P2 |
| `STORE-COMMAND-NAMESPACE-MISSING` | accepted | P3 | Proposes a `store` namespace for listdir/get/set. | no | no | Independent: search gets its own `search` namespace (user decision) | keep P3 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | `context` must be the last parameter. | no | no | Honoured in every signature | keep P2 |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | draft | P2 | Front-matter fields have nowhere to live in metadata. | no | no | Field lookup resolves **by name against the record**, so fixing it is a pure upgrade | keep P2 |
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | draft | P0 | Six assets endpoints are 501. | no | no | This design adds **no endpoint**; the command is reachable through `/q` | keep P0 |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | draft | P2 | A projection command could evaluate through its `Context`. | no | no | M1/M2 use no projection command; purity is a contract until this lands | keep P2 |
| `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`, `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`, `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER`, `CORE-STORE-OPENBIN-MISSING`, `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` | draft/accepted | P2–P3 | All bear on M4–M7 only. | no | no | Monitor; referenced from the roadmap | keep |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Identity and ACL. | no | no | Excluded by design; results are what the environment sees | keep P2 |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | Hygiene. | no | no | New code adds none | keep P2 |

**No blocker.** The one P1, `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`, is folded into this
design's implementation rather than left as a prerequisite, so nothing is waiting on external work.

## Data Structures

All in a new module `liquers-core/src/search.rs`, re-exported from `lib.rs`.

### SearchPredicate

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchPredicate {
    /// Subtree the search is rooted at. `Key::new()` is the store root.
    #[serde(with = "key_format")]
    pub root: Key,
    /// Which record sources to union. Empty is an error, not "all".
    pub sources: Vec<SearchSource>,
    /// Implicit AND. An empty list matches every record under `root`.
    pub clauses: Vec<Clause>,
    /// Hard cap on returned hits. Always finite.
    pub limit: usize,
    /// Depth below `root`; `None` is unlimited.
    pub depth: Option<u32>,
}
```

**Ownership:** entirely owned and cheap to clone. It crosses the command → store → override
boundary and is serialized by HTTP and MCP callers, so borrowing would buy nothing.

**Serialization:** `Serialize + Deserialize`. `Key` uses the existing `key_format` helper so the
wire form is the encoded key, as elsewhere in `metadata.rs`.

**`limit` is `usize`, not `Option<usize>`.** Unbounded is not an option a caller may choose;
Phase 1's boundedness invariant is enforced by the type. The command layer supplies a default.

### Clause and FieldTest

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Clause {
    /// Substring match over the record's text.
    Text { needle: String, case_sensitive: bool },
    /// A named field test. Field resolution is by name against the record (§Field resolution).
    Field { name: String, test: FieldTest },
    /// Glob over the record's key, when it has one. A record with no key never matches.
    Key { pattern: String },
    /// Negation — `-term` in the search syntax.
    Not(Box<Clause>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldTest {
    Equals(Value),
    Contains(String),
    Prefix(String),
    OneOf(Vec<Value>),
    Exists,
}
```

**Variant semantics:** `Text` is the free-text clause and is the only one that may require reading
content. `Field` covers everything metadata answers. `Key` is the cheapest and, measured in
`roadmap.md` §3, the highest-yield narrowing. `Not` wraps exactly one clause; there is no `Or`,
matching the syntax's implicit AND.

**Neither enum is `#[non_exhaustive]`, and this deliberately refines Phase 1's F7.** F7 asked for
non-exhaustive enums so new clauses would not be breaking. Within one workspace that is the wrong
trade: **a store override that silently ignores a clause it does not understand is a correctness
bug** — the exact push-down hazard Phase 1 named. An exhaustive match makes a new clause a compile
error at every override, which is the signal `CLAUDE.md` exists to preserve. What must stay
non-breaking is the *serialized* form and *construction*, which builder constructors and serde
defaults provide. No match on either enum uses a default arm.

**`FieldTest::Equals(Value)` uses `liquers_core::value::Value`, not `E::Value`.** Field values are
metadata scalars — strings, numbers, booleans. Making the predicate generic over the environment's
value type would infect every signature that touches it, including the serialized form, for no
capability: no field test is meaningful against a DataFrame.

### SearchSource

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchSource {
    /// Entries the store holds.
    Store,
    /// Live assets and keys a recipe provider declares, described without evaluating.
    Assets,
    /// The command registry, projected to records (M3).
    Commands,
}
```

`Commands` is how command discovery arrives without a second search implementation: the registry is
a record source, and nothing in the matching path knows what a command is. A read-only virtual store
over the registry (`use-cases.md` A4's tidier alternative) remains open as a later refactor — it
would remove this variant in favour of a mounted key space, which is why the variant is deliberately
small.

### Record

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Identity: the asset this came from, as a query. Never optional.
    #[serde(with = "query_format")]
    pub asset: Query,
    /// Asset-dependent position within it. `RecordId::Whole` at Level 0.
    pub record_id: RecordId,
    /// Named, typed fields — the `Value::Object` payload, held as its map.
    pub fields: BTreeMap<String, Value>,
    /// What a text clause matches. `None` when no text was available.
    pub text: Option<Arc<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordId {
    /// The asset itself is the record. The only Level 0 value.
    Whole,
    /// Zero-based position — a CSV row, a line.
    Index(u64),
    /// A named position — a JSON pointer, a named entry.
    Name(Arc<str>),
}
```

**`record_id` is present although Level 0 never sets anything but `Whole`.** This is Phase 1's F1:
adding the field later would change every construction site and every serialized record.

**`text` is `Option<Arc<str>>`.** `Arc` because a record's text is frequently the whole document and
may be shared with the excerpt extractor without copying; `str` rather than `String` because it is
never mutated after projection.

**`fields` is a `BTreeMap<String, Value>` rather than a `Value::Object`.** Same representation, but
the map type is what the field resolver indexes and what a future schema describes; wrapping it in
`Value` would mean unwrapping it on every access. `Value::Object(map)` is a free conversion when a
record is handed to a generic consumer.

### Hit, FieldMatch and SearchResult

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    /// What the record is, in the shape every client already renders.
    pub info: AssetInfo,
    pub record_id: RecordId,
    /// Why it matched. Empty only for a predicate with no clauses.
    pub matched: Vec<FieldMatch>,
    /// Reserved. Always `None` until a scoring clause exists (Phase 1 ordering promise).
    pub score: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldMatch {
    /// Field name, or the reserved name `text` for the body.
    pub field: String,
    /// A short excerpt around the match. `None` for a field whose value is its own evidence.
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    /// True when `limit` stopped the search before the corpus was exhausted.
    pub truncated: bool,
    /// Records examined. Diagnostic, not a promise.
    pub scanned: usize,
    /// Fields a clause tested that no record could supply — the answer to
    /// "why is X not in my results?" for the commonest cause.
    pub unavailable_fields: Vec<String>,
}
```

**`unavailable_fields` implements two Phase 1 rules at once:** an unresolvable field makes the
predicate *not match* rather than silently report false, and the result says so. Without it, a
search for `status:draft` over a store with no attribute support returns nothing and looks like an
empty corpus.

**`Hit` embeds `AssetInfo`, which is 656 bytes** (`CORE-VALUE-ENUM-OVERSIZED`). With a bounded
`limit` that is at most tens of kilobytes, which is acceptable; it is recorded here so that a future
larger limit is understood as a memory decision, not a free parameter.

### Value extension

```rust
// liquers-core/src/value.rs
pub enum Value {
    // … existing variants …
    SearchResult(Arc<SearchResult>),
}
```

**`Arc`-wrapped**, so the variant costs one pointer and `Value` does not grow past its current 704
bytes. Type identifier `SearchResult` — bare CamelCase, because `liquers-core` owns the concept
(`VALUE_TYPE_SYSTEM.md`). Default extension `json`, default media type `application/json`.

Every existing `match` on `Value` — `identifier`, `type_name`, `default_extension`,
`default_filename`, `as_bytes`, the deserializer — gains an explicit arm. No default arm is
introduced. Measured cost: an existing variant of comparable standing (`Value::Recipe`) is named at
**17 sites** across `liquers-core`, `liquers-lib` and `liquers-axum`, 8 of them inside `value.rs`.

**A `TypeInfo` entry is required, and is the step that is easy to miss.** `Value::type_descriptions()`
(`liquers-core/src/value.rs:407`) returns the registry seed for every variant, and `CLAUDE.md` states
the rule plainly — *four steps, not three; a type with no `TypeInfo` cannot be stored*, because the
write path refuses an identifier the registry does not contain. So:

```rust
TypeInfo::new("SearchResult")
    .with_type_name("SearchResult")
    .with_defaults("json", "json", "application/json")
```

The existing test `type_descriptions_match_identifier` catches a mismatch between the declared
identifier and the one `identifier()` reports, so this is checked rather than trusted.

## Trait Implementations

### `AsyncStore::select` — new, defaulted

```rust
// liquers-core/src/store.rs
#[async_trait]
pub trait AsyncStore: Send + Sync {
    // … existing methods …

    /// Select keys under `predicate.root` whose record matches.
    ///
    /// The default scans: `listdir_keys_deep`, then `get_metadata` per key, then `get` only for a
    /// key whose surviving clauses include a text test. An override must return the same set.
    async fn select(&self, predicate: &SearchPredicate) -> Result<SearchResult, Error> {
        // Phase 4
    }
}
```

**Object safety is preserved:** no generic parameters, no `Self` by value, one `&self` method with
concrete argument and return types. `AsyncStore` is used as `Arc<dyn AsyncStore>` throughout, so
this is a hard constraint, not a preference.

**Default implementation, so no existing store breaks** — the rule from
`rust-best-practices`: extend, do not mutate. `liquers-py` and `liquers-web` implementors compile
unchanged.

**No `Capability::Select` is added.** Phase 1 sketched one; it would always be true, because the
default makes selection universal, and a capability that cannot be false is not a capability. A
store that cannot enumerate cannot select either, and `Capability::EnumerateKeys` already says so —
so the conformance rules require that instead.

**Clause evaluation order is part of the contract**, because it is what keeps the scan affordable:
`Key` clauses first (no read), then `Field` (metadata read), then `Text` (content read) — so a
content read happens only for a key that survived everything cheaper.

### `AssetManager` — one repair and one addition

```rust
// liquers-core/src/assets.rs
#[async_trait]
pub trait AssetManager<E: Environment>: … {
    /// Describe an asset. **Never schedules evaluation.**
    ///
    /// Behaviour change: the previous default routed a live key through `get`, which submits to the
    /// job queue for an asset that is neither finished nor fast-trackable. It now reports from the
    /// handle `lookup_key_asset` returns, so an asset in `Status::Recipe` is described as `Recipe`
    /// rather than being run. A caller that wants resolution calls `get` itself.
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> { /* Phase 4 */ }

    /// Union selection over the sources named in the predicate. Never evaluates.
    async fn select(&self, predicate: &SearchPredicate) -> Result<SearchResult, Error> {
        // Phase 4
    }
}
```

`listdir_asset_info` calls `get_asset_info` per entry and therefore inherits the repair, which is
most of its value: listing a directory stops starting the assets in it.

**Union precedence** is the one already used by `get_asset_info`: live asset, else store, else
recipe provider. A key present as both a live asset and a store entry yields one hit, described from
the live asset, because that is the fresher of the two and the precedence a client already sees.

**Bounds:** none added. The method is on the existing `AssetManager<E: Environment>` trait and uses
no capability beyond what `get_asset_info` and `listdir_keys_deep` already require.

### Conformance rules — new `select` family

New file `liquers-core/src/store_conformance/rules/select.rs`. IDs use a fresh family to avoid
`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS`; `select` collides with no unit-test family in
`liquers-store`.

| ID | Claim | `requires` | `min_level` |
|---|---|---|---|
| `select01` | An empty predicate returns every key `listdir_keys_deep` reports under the root, up to `limit`. | `EnumerateKeys` | `ReadOnly` |
| `select02` | A `Key` glob returns exactly the keys a manual glob over the same listing selects. | `EnumerateKeys` | `ReadOnly` |
| `select03` | A `Field` test returns exactly the keys a manual `get_metadata` scan selects. | `EnumerateKeys`, `StoredMetadata` | `ReadOnly` |
| `select04` | Selection sees exactly what enumeration reports — no key that `listdir_keys_deep` omits appears in a result, and none it reports is missed. | `EnumerateKeys` | `ReadOnly` |
| `select05` | `limit` is honoured and `truncated` is true exactly when the corpus exceeded it. | `EnumerateKeys` | `ReadOnly` |
| `select06` | A `Text` clause returns exactly the keys a manual `get` scan selects. | `EnumerateKeys` | `ReadOnly` |

`select04` is the rule that makes `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` visible as a
store defect rather than a search one: both `AsyncFileStore` and `AsyncOpenDALStore` will pass it,
and they will pass it while disagreeing about what exists — which is that issue, not this design's.

## Generic Parameters & Bounds

No new generic types. `SearchPredicate`, `Record`, `Hit` and `SearchResult` are concrete, which is
what lets them cross the `dyn AsyncStore` boundary and serialize.

`AssetManager<E: Environment>` keeps its existing parameter; the new method adds no bound.

## Sync vs Async Decisions

| Function | Async | Rationale |
|---|---|---|
| `AsyncStore::select` | yes | Reads metadata and content through the store |
| `AssetManager::select`, `get_asset_info` | yes | Store and recipe-provider access |
| `Clause::matches(&Record)` | no | Pure comparison on data already in memory |
| `parse_search_syntax(&str)` | no | Pure parsing |
| `search` command | **async** | Reaches the store through `Context` |

## Function Signatures

### `liquers-core/src/search.rs`

### Field resolution: names are qualified, and ambiguity is a parse-time error

`status` means two unrelated things — the **asset** lifecycle on `MetadataRecord`
(`Ready`, `Expired`, `Source`) and a **document's** lifecycle in front-matter (`draft`, `closed`).
Phase 1 question 21 and `roadmap.md` §3 both asked Phase 2 to settle it. The resolution:

**A record's field names are qualified at projection time**, by the source that supplied them:

| Prefix | Source |
|---|---|
| `meta.` | `AssetInfo` / `MetadataRecord` — `meta.status`, `meta.type_identifier`, `meta.media_type`, `meta.file_size`, `meta.updated`, `meta.title`, `meta.description` |
| `attr.` | application attributes, when `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` lands — `attr.status`, `attr.kind`, `attr.area` |
| `key.` | derived from the key — `key.name`, `key.extension`, `key.dir` |

`Clause::Field { name }` matches a **qualified name exactly**. The matcher is deliberately exact and
dumb, so a predicate arriving from HTTP or MCP is unambiguous by construction and cannot change
meaning as the system grows.

**Unqualified names are a front-end convenience, expanded by the syntax parser**, not by the matcher.
`parse_search_syntax` expands `status:draft` to the one qualified name available; when more than one
source could supply it, the parse **fails with an error naming both candidates** rather than picking
one. That is the property that matters: a query written today against `meta.status` keeps its meaning
when `attr.status` appears, and a bare `status:` that was unambiguous becomes a reported error rather
than a silently different question.

```rust
impl SearchPredicate {
    pub fn new(root: Key) -> Self;                       // sources = [Store, Assets], limit = 50
    pub fn with_clause(self, clause: Clause) -> Self;
    pub fn with_limit(self, limit: usize) -> Self;
    pub fn with_sources(self, sources: Vec<SearchSource>) -> Self;
    /// True when no clause needs the record's text — the scan then reads no content.
    pub fn needs_text(&self) -> bool;
}

impl Clause {
    /// Pure match against a projected record. `None` means a field the record cannot supply,
    /// which the caller records in `unavailable_fields` and treats as "does not match".
    pub fn matches(&self, record: &Record) -> Option<bool>;
}

/// Build a record from what an asset description and (optionally) its bytes supply.
/// Level 0: one record per asset, `RecordId::Whole`.
pub fn record_from_asset_info(info: &AssetInfo, text: Option<Arc<str>>) -> Record;

/// Extract a short excerpt around the first match. Returns `None` for a needle not present.
pub fn excerpt(text: &str, needle: &str, case_sensitive: bool, radius: usize) -> Option<String>;
```

### `liquers-lib/src/search/mod.rs`

```rust
/// Parse the search syntax into a predicate's clauses.
/// Accepts: bare terms (implicit AND), "quoted phrases", -negation, field:value.
/// Anything it does not recognise is treated as a literal term rather than rejected.
pub fn parse_search_syntax(input: &str) -> Result<Vec<Clause>, Error>;

/// Command: search under the key carried by the state, or under the store root.
pub async fn search(
    state: State<Value>,
    query: String,
    limit: i64,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error>;

/// Command: narrow an existing result. Composition, per Phase 1 axis C4.
pub fn filter(state: &State<Value>, field: String, value: String) -> Result<Value, Error>;

/// Command: project a result to the keys only, for piping into other commands.
pub fn hit_keys(state: &State<Value>) -> Result<Value, Error>;
```

**`search` is async and takes an owned `State<Value>`**, per the macro's rule for async command
functions. `context` is last, per `COMMAND-CONTEXT-PARAM-ORDER`. `filter` and `hit_keys` are sync
and borrow, because they are pure transformations of a value already in hand.

**The root comes from the state.** `-R-key/specs/issues/-/ns-search/search-expiration` puts a `Key`
in the state via `Step::UseKeyValue`, so the command searches that subtree; a bare
`ns-search/search-expiration` searches from the root. This reuses the mechanism
`record-model.md` §0 already relies on, and keeps the dependency on the *directory* rather than on
its contents.

### Worked queries

All three checked with `liquers-validate` (`--command search --command filter --command hit_keys`):

| Query | Meaning |
|---|---|
| `ns-search/search-expiration` | search the whole store for "expiration" |
| `-R-key/specs/issues/-/ns-search/search-expiration` | the same, rooted at `specs/issues` |
| `-R-key/specs/-/ns-search/search-expiration/filter-status-draft/hit_keys` | search, narrow by field, project to keys — axis C4 composition end to end |

A term containing a separator is escaped by `ActionRequest::encode`, never by hand:
`expiration safety` becomes `search-expiration~.safety`.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/search.rs` (new) | Predicate, record, result, pure matching, excerpting |
| `liquers-core` | `src/lib.rs` | `pub mod search;` |
| `liquers-core` | `src/store.rs` | `AsyncStore::select` with default scan |
| `liquers-core` | `src/assets.rs` | `get_asset_info` repair; `AssetManager::select` |
| `liquers-core` | `src/value.rs` | `Value::SearchResult(Arc<SearchResult>)` + every match arm |
| `liquers-core` | `src/store_conformance/rules/select.rs` (new), `rules/mod.rs` | Six rules |
| `liquers-lib` | `src/search/mod.rs` (new) | Syntax parser, command functions |
| `liquers-lib` | `src/commands.rs` | `register_command!` for the three commands |
| `specs` | `command_registry.yaml` | Regenerated, not edited |

No change to `liquers-store`, `liquers-axum`, `liquers-web` or `liquers-py`. A backend override of
`select` is possible but not part of this milestone.

**Dependencies:** none added. Globbing is a short hand-rolled matcher over key segments rather than
a `glob` crate, because the pattern language is three constructs (`*`, `**`, literal) and the crate
would have to be wasm-checked.

## Relevant Commands

### New — namespace `search` (user decision)

| Command | Signature | Purpose |
|---|---|---|
| `search` | `async fn search(state, query: String, limit: i64 = 50, context) -> result` | The one essential command |
| `filter` | `fn filter(state, field: String, value: String) -> result` | Narrow a result (C4 composition) |
| `hit_keys` | `fn hit_keys(state) -> result` | Project to keys for piping |

```rust
register_command!(cr,
    async fn search(state, query: String, limit: i64 = 50, context) -> result
    namespace: "search"
    label: "Search"
    doc: "Select assets under the current key whose metadata or content match"
    filename: "search.json"
    version: auto
)?;
```

### Existing namespaces consulted

`dep` — `command_metadata` and `commands_doc` already describe commands; `SearchSource::Commands`
projects the same registry rather than duplicating them. `store` — proposed by
`STORE-COMMAND-NAMESPACE-MISSING` and unimplemented; deliberately not depended on.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/SEARCH.md` | reference (new) | contributor, agent | The predicate, the record, the match and ordering contracts, the invariants, the field-resolution order, what an override must guarantee |
| `specs/reference/STORE_SEMANTICS.md` | reference | store author | `select` contract; the six `select*` rules; clause evaluation order |
| `specs/reference/CONFORMANCE_TERMS.md` | reference | store author | The `select` rule family |
| `specs/reference/ASSETS.md` | reference | contributor | **`get_asset_info` never schedules** — the behaviour change, stated where the asset lifecycle is owned |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | guide | store author | When to override `select`, and the agreement obligation |
| `specs/README.md` | map | all | Capability line moves `designing` → `built` |

`affects_docs`: `reference/SEARCH.md`, `reference/STORE_SEMANTICS.md`, `reference/CONFORMANCE_TERMS.md`,
`reference/ASSETS.md`, `guides/STORE_IMPLEMENTATION_GUIDE.md`.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors — `Error::general_error`,
`Error::key_not_found(&key)`, `Error::not_supported`, `Error::conversion_error`. No `Error::new`, no
new error type, no `unwrap`/`expect` in any of it.

| Situation | Outcome |
|---|---|
| Root key absent | `Error::key_not_found(&root)` |
| Empty `sources` | `Error::general_error` — "all" must be written, not implied |
| Unreadable entry mid-scan | Skipped, counted in `scanned`; a search is not failed by one bad key |
| Unresolvable field | Not an error — the clause does not match, and the name lands in `unavailable_fields` |
| Malformed search syntax | Not an error — treated as a literal term (Phase 1 open question 12, resolved towards tolerance) |
| Ambiguous unqualified field name | `Error::general_error` from the parser, naming every candidate. The one place the syntax is deliberately intolerant, because the alternative is silently answering a different question |

## Serialization Strategy

Every new type derives `Serialize, Deserialize`. `Key` and `Query` use the existing `key_format` /
`query_format` helpers so their wire form matches everything else in `metadata.rs`. `SearchResult`
derives `Default` so an empty result is constructible without ceremony. `Arc<str>` and
`Arc<SearchResult>` serialize transparently as their contents.

Round-trip is asserted for the predicate in particular, because an HTTP or MCP caller constructs one
directly.

## Concurrency Considerations

The scan is sequential per store. Concurrency is deliberately deferred: a bounded `buffer_unordered`
over metadata reads is an obvious later optimization, and adding it now would fix a concurrency
level before any measurement. No shared mutable state is introduced, no lock is held across an
`.await`, and `SearchPredicate`/`SearchResult` are plain data — `Send + Sync` by construction.

## Codebase Alignment — verified at HEAD

Checked against the source rather than assumed, in the Phase 2 review:

| Assumption | Verdict |
|---|---|
| `AsyncStore` is object-safe and used as `Arc<dyn AsyncStore>`; a defaulted `select` with concrete types preserves that | confirmed |
| `RuleMeta { id, title, contract, requires, refutes, min_level }` is the conformance rule shape | confirmed |
| `key_format` / `query_format` serde helpers exist and are usable as assumed | confirmed, `metadata.rs:970`, `:990` |
| `register_command!` accepts `namespace`, `label`, `doc`, `filename`, `version: auto` | confirmed, `registration.rs:861-918` |
| An async command function takes an owned `State` and may take `context` last | confirmed |
| `get_asset_info` routes a live key through `get`, which submits — the defect this design repairs | confirmed at `assets.rs:3967-3970` and `:5334-5336` |
| `listdir_asset_info` calls `get_asset_info` per entry and inherits the repair | confirmed, `assets.rs:4043` |
| A new `Value` variant needs a `TypeInfo` in `type_descriptions()` | confirmed — **this was missing from the first draft** and is now specified above |

## Compilation Validation

- `AsyncStore` stays object-safe: concrete argument and return types, no generics, `&self`.
- Every `match` on `Value` gains an explicit `SearchResult` arm; no default arm anywhere.
- `liquers-core` gains no dependency on `liquers-lib`; the parser and commands live in `liquers-lib`.
- `Value::SearchResult(Arc<_>)` keeps `size_of::<Value>()` unchanged.
- No feature gate is involved, so the build matrix is unaffected — but `scripts/check-build-matrix.sh`
  runs anyway because `Value` changed.

## Open Questions for Phase 3

1. Excerpt radius and whether it is a predicate field or a constant.
2. Whether `filter` is worth shipping in M2, or whether the predicate covers enough that composition
   can wait.
3. Whether `SearchSource::Commands` survives, or is replaced by a read-only virtual store over the
   registry before M3 ships.
4. Whether `key.` fields are worth projecting in M1, or whether a `Key` glob clause covers every
   case they would serve.
