# Roadmap: what is in scope, and what the agent memory MVP needs

Companion to [Phase 1](./phase1-high-level-design.md). It answers a fair objection: if Level 1 is
"just specialized commands", then Level 0 has to specify the whole contract those commands conform
to — record streams with schema, common fields, chunks with key derivation, batches, streaming — and
that is a great deal of specification to carry in order to ship a search box.

The objection is right about the risk and wrong about the size, and §1 is the argument. §2 sets the
milestones, §3 gives the agent-memory MVP cut, §4 lists what each milestone depends on.

---

## 1. Only seven decisions are foundational

The test for every element of the model is one question: **can this be added later without changing
anything Level 0 shipped?** If yes, it defers safely, however elaborate it eventually becomes. If
no, it must be decided now — and "decided" is much cheaper than "implemented".

### Foundational — decide now, cheap now, expensive later

| # | Decision | Why it cannot wait |
|---|---|---|
| F1 | **The record's shape**: identity, fields, text — with the record-id field *present* even though Level 0 always leaves it absent | Adding a field later changes every construction site and every serialized form |
| F2 | **Identity is (asset, record id)**, not a stored query | It determines what is stored per record, which is the difference between a row costing bytes and costing a query string |
| F3 | **Fields are a `Value::Object`, addressed by name** | Tags, front-matter attributes, and SQL columns must all land in one place, or they land in three |
| F4 | **A search returns an opaque bounded result, not a bare `Vec`** | The subtle one. A `Vec` is a promise of materialization; streaming later would need a second API |
| F5 | **The ordering promise**: unordered unless a scoring clause was used | A promise cannot be retracted once consumers rely on it |
| F6 | **Text and fields are distinct**, even while Level 0 hardcodes which is which | An engine must know what to tokenize; retrofitting the distinction means reindexing |
| F7 | **The record-id and predicate-clause enums are non-exhaustive** | New id kinds and new clauses must not be breaking changes |

Seven decisions, all of them shape rather than machinery. None requires chunks, batches, schemas or
streaming to exist.

### Additive — defer safely

Each of these can arrive later without altering anything Level 0 shipped, *because* F1–F7 left room:

| Deferred | Rides on |
|---|---|
| Schema and field roles | Optional structure beside the stream; Level 0 has no schema and hardcodes title/description as text (F6) |
| Chunks and the partition | Level 0 search produces no chunks at all |
| Batches | A delivery concern with no presence in the record type (F4) |
| Locator derivation rule | Meaningless while record ids are absent (F1) |
| Streaming | Enabled by F4; forbidden by nothing |
| Projection identity | Only meaningful once an external view exists |
| The reconciliation contract | Only meaningful once an external sink exists |
| Further predicate clauses — numeric, regex, similarity | F7 |
| Push notifications | An optimization by construction (`interoperability-layer.md` §1) |

> **The burden on Level 0 is F1–F7 and nothing else.** The rest of `record-model.md` is a
> specification of where the design is *going*, which is what keeps Level 1 from being a redesign —
> not a specification of what the first version must contain.

---

## 2. Milestones

"This design" means `store-and-asset-search`. Later milestones are named so the shape is visible,
not so they are committed to here.

| # | Milestone | Contains | Exit criterion | In this design? |
|---|---|---|---|---|
| **M0** | Foundations | F1–F7; the record and predicate types; `Value` integration | Types exist and round-trip; no user-visible capability | yes |
| **M1** | Metadata search | Scan over metadata only, field predicates, capability flag, conformance rules | A store subtree can be selected by field with no content reads | yes |
| **M2** | Text search and the search box | Content matching, the small syntax, snippets, bounded results with truncation | A user types words in one box and gets a judgeable list | yes |
| **M3** | Asset union and command discovery | Live assets, recipe-declared keys, the command registry as a record source | "Search the system" is one question, not three | yes |
| **M4** | Per-document filter index | The tinysearch data structure as a derived asset; filter-then-verify | Content search stops being O(corpus) reads | optional, this design |
| **M5** | Record streams (Level 1) | Record ids, chunks, key derivation, schema and roles | A CSV's rows are searchable and projectable as a table | no — its own design |
| **M6** | Batching and streaming | Batches, addressable or iterated; incremental read and write | A parquet file larger than memory is processable | no — its own design |
| **M7** | Interoperability and a first sink | The reconciliation contract, a test double, then one real engine | An external engine stays fresh without a hook | no — its own design |

M0–M3 are one coherent deliverable. M4 is a performance step that changes no contract. M5–M7 are
separate efforts that the foundations admit.

---

## 3. What the agent memory MVP needs

**M0 through M3. Nothing else.**

Agent memory serves ~300 Markdown documents with YAML front-matter. Mapping its needs
(`use-cases.md` §1) onto the milestones:

| Need | Milestone |
|---|---|
| A1 find documents on a topic | M2 |
| A2 judge a hit without opening it | M2 (snippet) + agent memory's own recipes (title, description) |
| A3 filter by `kind`, `status`, `area`, `priority` | M1 — **see the caveat below** |
| A4 discover which commands exist | M3 |
| A5 discover what data exists under a prefix | M1 + M3 |
| A6 budgeted retrieval, truthful truncation | M2 |
| A7 every hit is an address | M0 (F2) |

It needs **no** record streams, chunks, batches, schema, streaming, external engine or
interoperability layer. Level 1 is not on the agent-memory path at all.

### The one caveat, and it is worth deciding deliberately

A3 — filtering by front-matter facts — is the most valuable thing search does for an agent, and it
needs those facts to be *in metadata*. `MetadataRecord` is a closed struct
(`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`), so `kind`, `status`, `area` and `priority` have nowhere
to live. Three ways through, and they are not equally good:

1. **Text fallback.** `kind:issue` degrades to matching the text "kind: issue" in the document.
   Works today, costs a content read per document, and is imprecise — a document *mentioning*
   another's status matches.
2. **Fix `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` first.** Agent memory already plans recipes to
   populate `title` and `description`; the same recipe would populate an attribute map, and field
   predicates then work over metadata with no content reads at all. This is the good version.
3. **A per-media-type projection command.** Correct, and more machinery than the MVP needs —
   effectively an early slice of M5.

Recommendation: **build M1 so that field predicates resolve by name against whatever the metadata
carries**, so route 1 works now and route 2 upgrades it with no change to search. Route 2 should be
scheduled with agent memory rather than with this design; at ~300 small documents route 1 is
tolerable, and it is the first thing that stops being tolerable as the corpus grows.

---

## 4. Dependencies, and what actually blocks what

| Milestone | Depends on | Blocking? |
|---|---|---|
| M0–M2 | nothing outside this design | — |
| M3 | `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0) only for the HTTP surface; the command surface is unaffected | no |
| M1 (A3 quality) | `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) | no — degrades to text matching |
| M4 | nothing; each filter is a derived asset of one document | no |
| M5 | `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` (P2) for chunk-set changes | partially |
| M6 | `CORE-STORE-OPENBIN-MISSING` (P3), `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` (P2) | yes, for real streaming |
| M7 | `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (P2) for latency only | no — reconciliation is the guarantee |

**Nothing blocks the MVP.** Every dependency above degrades a later milestone's quality or speed
rather than preventing it, which is a consequence of having made reconciliation the guarantee and
the scan the baseline.

---

## 5. What this roadmap asks Phase 2 to do

1. Specify F1–F7 precisely, and no more of the record model than that.
2. Write `record-model.md`'s Level 1 material into the reference as **direction**, explicitly marked
   as not-yet-implemented, so a future implementer inherits the reasoning without inheriting a claim
   that it exists.
3. Design M1's field resolution so it is **by name against the record**, never against
   `MetadataRecord`'s struct fields — that single choice is what makes route 2 above a pure upgrade.
4. Decide whether M4 is in this design or the next. It changes no contract either way, which makes
   it a scheduling question rather than an architectural one.
