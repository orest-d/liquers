# The record model: records, streams, chunks and schema

Companion to [Phase 1](./phase1-high-level-design.md) and to
[`interoperability-layer.md`](./interoperability-layer.md), which established that a record is the
common denominator between full-text search, an external engine, a vector store and SQL. This
document answers what a record actually **is**, and what it takes to refresh part of a stream rather
than all of it.

Every query shown here was checked with `liquers-validate`.

---

## 0. The shape of the problem

The motivating usage is:

1. an external engine is configured with a Liquers query that produces a stream of records;
2. it consumes the whole stream once, at initialization;
3. a change — surfaced as an expiration event — triggers a **full or partial** update.

Step 3 is what forces the design. If the only unit is "the stream", every change means reprocessing
everything, and the configuration query's dependency set is the union of every source it touched. A
partial update needs a smaller unit that owns a smaller dependency set **and knows how to rebuild
itself**.

The worked example, and it is a good one because the two query forms in it already mean exactly the
right things at HEAD:

| Role | Query | What it resolves to |
|---|---|---|
| Stream | `-R-key/some/folder/-/csv_records` | `Step::UseKeyValue` — the *key* is the value, so `csv_records` receives a key and walks the folder itself. Its dependency is the **directory**, not the file contents |
| Chunk | `-R-bin/some/folder/specific_file.csv/-/csv_file_records` | `Step::GetAssetBinary` — the command receives one file's bytes, so its dependency is **that one file** |

That difference is not incidental; it is the mechanism. `-R-key` hands over an address and keeps the
data out of the dependency set, while `-R-bin` pulls exactly one file in. The chunk is narrower
because its *query* is narrower, and the planner already computes that.

---

## 1. What is a record?

**A record is a projection of something addressable into named fields, carrying its own address.**

Three parts, and the split matters more than the names:

| Part | What it is | Why it cannot be folded into the others |
|---|---|---|
| **Identity** | the asset this came from, as a **query**; optionally a finer locator | A hit that is not addressable is a dead end (`use-cases.md` A7). Identity must be structurally guaranteed, not a field name every consumer has to agree on |
| **Fields** | named, typed values — `Value::Object` | This is what predicates test and what a SQL column *is*. Named and typed is the one requirement the SQL task imposes (`research-questions.md` §4) |
| **Text** | what full-text matching runs over | Distinct from fields because tokenizing a status code is wrong and exact-matching a paragraph is useless. A search engine must be told which is which |

### Is `Value::Object` enough?

**As the payload, yes. As the whole record, no** — and the three things it cannot carry are exactly
the three that every consumer needs.

`Value::Object(BTreeMap<String, Value>)` already exists, already serializes, is schemaless by
nature, and maps directly onto a GlueSQL row, so there is no case for inventing a map. What it
cannot express:

1. **Guaranteed identity.** If the address is just an entry under some agreed name, nothing enforces
   its presence or its type, and every consumer re-derives the convention. The proposed
   `{asset_key: …, specific_key: …, content: …}` works precisely because everyone agrees — which is
   the definition of a convention that will drift.
2. **The field/text distinction.** An engine configuring itself from a stream needs to know which
   fields to tokenize. A flat map cannot say, so the knowledge moves into the engine's hand-written
   configuration — and now the stream and the engine can disagree silently.
3. **Provenance versus payload.** Fold the address into the map and a SQL projection of the record
   grows provenance columns, and `SELECT *` returns query strings next to data.

> **Recommended:** a thin struct — identity, fields, optional text — whose fields are a
> `Value::Object`. New type, no new *representation*; nothing about serialization or the value
> system changes.

### Identity is a query, not a string

The proposal already gets this right and it is worth making normative:

```
asset     -R/some/folder/specific_file.csv
locator   -R/some/folder/specific_file.csv/-/ns-csv/row-42
```

Both validate today. The second is not a label — it is **evaluable**, so a consumer that wants the
row fetches it, and a citation stays meaningful when passed to a different process. That is the
property that makes a search hit composable rather than a dead reference.

Two honest caveats:

- A locator is only evaluable if the projection can produce one. A line number inside a Markdown
  file has no command behind it. So the locator is **optional**, and coordinates that are merely
  descriptive (`line: 42`, `offset: 1180`) belong in fields. Phase 2 should decide whether a
  *descriptive* locator is a third case or just fields.
- An evaluable locator is a promise that the command exists and keeps working. It is a contract with
  the projection, not a free-form string.

---

## 2. What is a record stream?

**A stream is a query that yields records, plus the partition that says what it is made of.**

Nothing more is needed, because a Liquers query is already an address *plus* a derivation, and the
planner already computes its dependency set. A stream needs no identity of its own: it *is* its
query.

But a stream that can only be consumed whole is the thing step 3 rules out. So a stream has a second
face:

```
stream query  ──▶  partition:  [ chunk descriptor, … ]
chunk query   ──▶  records:    [ record, … ]
```

A flat stream is a stream with exactly one chunk, so there is one shape, not two.

**Materialization matters here.** Liquers values are in-memory; there is no streaming value type,
and a folder of large CSVs cannot become one value. Chunking is what bounds this: a consumer never
holds more than one chunk at a time. **That is the third problem chunks solve**, and it is a reason
to make them first-class even for engines that never do a partial refresh.

---

## 3. What is a chunk?

> **A chunk is the unit of refresh. A record is the unit of retrieval.**

That one sentence resolves most of the follow-up questions. A chunk descriptor carries:

| Field | Purpose |
|---|---|
| **id** | stable across refreshes, so a sink can replace a chunk's records wholesale |
| **query** | how to rebuild exactly this chunk — the refresh query |
| **version** | whether it needs rebuilding, answerable **without** rebuilding it |
| **dependencies** | optional; what it is derived from, for diagnosis and for scoping event subscriptions |

Refresh is then: for each chunk whose version differs from what the sink recorded, evaluate its
query and **replace all of that chunk's records**. Replacement rather than merge is deliberate —
record-level diffing inside a chunk would require record-level versions, which means hashing every
record, which means reading everything, which is the cost the chunk existed to avoid.

### Why not version individual records?

Because a record has no independent existence. It is derived, so its "version" is a function of its
source; computing it per record costs a full read of the source. A chunk is precisely **the smallest
unit whose staleness can be decided from metadata alone** — which is what makes it the right
granularity and not an arbitrary batching convenience.

### Chunk versions come nearly free

Liquers already records what an asset observed: `MetadataRecord.dependencies` is a
`Vec<DependencyRecord { key, version }>`, and `Version` is a content hash. So a chunk's version is a
hash over its dependencies' *current* versions — a metadata read per dependency, no data reads. For
the worked example that is **one metadata read per CSV file** to decide whether that file's chunk
needs reprocessing.

This is the same `(id, version)` diff the interoperability layer already specified, now with the
granularity settled: **reconcile at chunk granularity, not record granularity.**

### Is the partition itself a record stream?

It can be — a stream of records describing chunks — which would give a consumer one mechanism rather
than two. It is tidy and slightly clever; Phase 2 should decide whether the uniformity is worth
making the base case recursive.

---

## 4. Do we need a schema? Is it basically a table?

**A stream is a table**, and saying so is useful: it is why SQL needs no adapter beyond the field
mapping, and why a DataFrame conversion is trivial. It differs from a table in three ways, each
load-bearing:

1. **Identity is a query**, not a primary key drawn from the data.
2. **Partitioning is explicit** and carries refresh semantics; a table's partitions do not.
3. **Rows may be heterogeneous.** A corpus of mixed types has no single column set — which is why a
   schema must be *optional*, and why GlueSQL's schemaless support is the relevant feature.

**The schema should be optional, advisory, and attached to the stream** — and its most valuable
content is not types. It is **field roles**:

| Role | Meaning to a search engine | Meaning to SQL |
|---|---|---|
| `id` | not indexed, returned | a key column |
| `text` | tokenized, matched by the text clause | a text column |
| `keyword` / facet | exact match, facetable, not tokenized | a column |
| `stored` | returned but not searched | a column |
| `numeric` | range clauses | a numeric column |
| `vector` | similarity clause | opaque |

These are the field options every engine already has — Lucene, Tantivy and Elasticsearch mappings
all say the same thing in their own words — and they are the minimum an external engine needs to
**configure its index from a query**. That closes the loop with the motivating usage: the
configuration query yields not only records but the mapping to configure the engine with, so the
stream and the engine cannot drift into disagreement.

A consumer that does not need the schema ignores it. The built-in scan needs only field *names*.

---

## 5. The worked example, end to end

```
configure:   -R-key/some/folder/-/csv_records
partition:   [ { id: "specific_file.csv",
                 query: "-R-bin/some/folder/specific_file.csv/-/csv_file_records",
                 version: hash(version of -R/some/folder/specific_file.csv) },
               … one per file … ]
records:     { asset:   -R/some/folder/specific_file.csv,
               locator: -R/some/folder/specific_file.csv/-/ns-csv/row-42,
               fields:  { price: 12.5, city: "Wien", line: 42 },
               text:    "…" }
```

- Initialization evaluates the partition, then each chunk query in turn — never holding more than
  one file's records.
- One CSV changes. Its content hash changes, so exactly one chunk version changes, so exactly one
  chunk query is re-evaluated and one chunk's records are replaced.
- A file is added. The *partition* changes — which is a dependency on the directory listing, and
  therefore runs into `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` on the push
  path. Reconciliation catches it regardless, which is the third time that issue argues for pull
  being the guarantee.

---

## 6. What this asks of the rest of the design

1. Records are **structurally addressable**: identity is part of the type, not a convention.
2. Fields are **named and typed** — the one thing the SQL task requires
   (`NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`).
3. The reconciliation `(id, version)` pairs of `interoperability-layer.md` §3 are **chunk** ids and
   versions.
4. **Projection identity** (`interoperability-layer.md` §7) belongs in the chunk version: change the
   projection rule and every chunk version must change, or a re-tokenized index silently looks fresh.
5. Field **roles** are what let one record serve a scan, an engine and a table.

## 7. Open decisions for Phase 2

1. Whether a descriptive locator (line, offset) is a distinct case or simply fields.
2. Whether the partition is itself a record stream, or a separate lighter type.
3. Whether a chunk id must be stable across a partition change, and what a sink does when one
   disappears.
4. Where a stream declares its schema — on the partition, on each chunk, or from the command's
   metadata — and whether it may vary between chunks.
5. Whether `text` is a field with the `text` role rather than a separate part of the record. Fewer
   parts is simpler; a separate part makes "this is the body" unambiguous for the common case.
6. Whether chunk versions are computed by the stream command or derived generically from the chunk
   query's dependency set. Generic derivation is far better if it is possible, because it cannot be
   got wrong per command.
