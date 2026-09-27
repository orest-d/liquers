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
| **M4** | Per-document filter index | The tinysearch data structure as a derived asset; filter-then-verify; **the first indexation policy** — inclusion patterns, content policy, volatile handling | Content search stops being O(corpus) reads, and what is indexed is stated rather than implied | optional, this design |
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

### A3 in detail: which filters are actually worth anything

An earlier draft of this roadmap called filtering by `kind`, `status` and `area` "the most valuable
thing search does for an agent". That was two claims and both were wrong. Text search is the primary
act — an agent starts by asking what the corpus says about a topic — and filtering is a *precision
aid* applied to its result. And the three fields are not equally valuable, because two of them are
largely obtainable for free.

**What the fields are.** They are `specs/` front-matter (`DOCS_STRUCTURE_GUIDE.md` §4.6), not
Liquers metadata:

| Field | Meaning | Derivable from the key? |
|---|---|---|
| `kind` | document genre: issue, feature, design, reference, guide | **Mostly.** `specs/issues/`, `specs/design/<slug>/`, `specs/reference/`, `specs/guides/` — the only non-derivable distinction is issue versus feature, which share `specs/issues/` (149 and 36 documents) |
| `status` | the document's lifecycle: draft, accepted, closed, complete… | **No, by design.** §1.2 of the guide: "Location encodes genre, front-matter encodes state… It never encodes status — status changes, paths should not" |
| `area` | closed vocabulary — `core/store`, `lib/commands`… multi-valued | **No.** Orthogonal to location |
| `priority` | P0–P3 | **No**, but it is a triage field, not a retrieval one |

**Measured on a realistic retrieval**, "what does this project know about expiration", over the 321
tracked documents at HEAD:

| Step | Predicate | Hits | Needs metadata? |
|---|---|---|---|
| text | documents mentioning "expir" | **72** | no |
| + key prefix | `specs/issues/` | **42** | **no — free** |
| + status | not closed/rejected/duplicate | **23** | **yes** |
| (+ area) | `core/assets` instead of status | ~31 | yes, and weakly |

The ranking that falls out:

1. **Key prefix does the biggest single cut and costs nothing.** 72 → 42. It needs no metadata, no
   attributes and no projection — `use-cases.md` U3 already has it as essential, and it deserves
   more weight than the earlier draft gave it.
2. **`status` is the one filter that genuinely needs metadata.** 42 → 23, and it is neither
   path-derivable (deliberately) nor well-approximated by text, because a document *mentioning*
   another's status matches. This is the real content of the `CORE-METADATA-NO-APPLICATION-ATTRIBUTES`
   dependency.
3. **`area` is weaker than it looks** — it overlaps heavily with what the text search already found,
   since a document about `core/assets` tends to say "asset".
4. **`kind` is nearly free** via the path, except issue-versus-feature.

So the corrected caveat is narrower: `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) buys **one**
high-value filter, not three, and the MVP is usable without it. Still build M1 so that field
predicates **resolve by name against the record** rather than against `MetadataRecord`'s struct
fields — that is what makes fixing the issue a pure upgrade — but schedule the fix with agent memory
on its own merits, not as a search prerequisite.

### Front-matter extraction: still Level 0, but it needs a field source

Extracting `status` and `area` from a Markdown document is easy to implement as a command — and it
appears to break the Level 0 boundary, because Level 0 was described as "fields from metadata". It
does not, and the appearance is the fault of that description: it fused two independent things.

**Cardinality is the level. Field provenance is a separate axis** (`record-model.md` §7).
Front-matter extraction produces exactly **one record per asset**, so it is Level 0 by cardinality —
no ids, no chunks, no batches, no schema. What it needs is a field *source* other than metadata.

**The search may run the extraction inline, and the non-evaluation invariant does not forbid it.**
An earlier draft of this section said otherwise. The distinction it missed is structural:
`CommandExecutor::execute` applies a command to a state and returns a value, creating no asset,
persisting nothing and running no recipe, whereas *evaluating a query* does all three. The invariant
is about the second. And the cost argument agrees — a text clause already reads every candidate's
bytes, so parsing front-matter out of bytes just read is cheaper than the substring match performed
on them (`record-model.md` §7).

So a field resolves in this order: metadata if it carries the field; a materialized projection if one
exists; otherwise a pure projection computed in memory from the candidate's bytes, cached per content
version and never persisted; otherwise the field is *unavailable* and the predicate does not match.

That makes materialization an **optimization rather than an enabler**:

| Route | Mechanism | Cost per search |
|---|---|---|
| **Inline projection** | the search applies the extraction command to each candidate's bytes | a content read and a parse per candidate — the same order as a text search |
| **Into metadata** | a recipe runs the extraction and writes fields into the asset's metadata; needs `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | **none** — a field-only search reads no content at all |

**The MVP uses the inline route and is complete without any new dependency.** The metadata route is
the upgrade, it belongs to agent memory — which already plans recipes to populate `title` and
`description`, so front-matter fields are a small extension of scoped work — and it needs no change
to search, because field predicates resolve **by name against the record** either way.

Three honest consequences:

1. **`CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) is a performance upgrade, not a prerequisite.**
   It turns a field-only search from O(corpus bytes) into O(corpus metadata). At ~300 small documents
   the inline route is fine; it is the first thing that stops being fine as the corpus grows.
2. **The dangerous operation is starting an asset, not projecting.** A search must be able to
   describe an asset without running its recipe, and today `get_asset_info` schedules one for a live
   key in `Status::Recipe` (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`, P1). That is the fix
   the invariant actually depends on. A secondary hazard remains: a projection command's body could
   reach through its `Context` and evaluate, arriving at the same place by a different door —
   `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` (P2). Until that lands, purity is a contract
   enforced by review, which is acceptable for commands this project writes itself and is the thing
   to fix before third-party projections are accepted.
3. **The repository already relies on the same trade.** `specs/index.csv` is a materialized
   projection of every document's front-matter, regenerated and committed because computing it per
   query would be wasteful. Searching *that file* as a record source would work but is Level 1 — rows
   from a CSV — which is a good reason not to reach for it in the MVP.

### A name collision to resolve in Phase 2

`status` means two unrelated things. `MetadataRecord.status` is the **asset** lifecycle — `Ready`,
`Expired`, `Source`, `Override` — while `specs/` front-matter `status` is the **document's**
lifecycle — `draft`, `accepted`, `closed`. A bare `status:draft` predicate is ambiguous between
them, and the ambiguity is silent: both resolve, to different things.

Field resolution therefore needs either a namespace (`meta.status` versus an attribute namespace) or
an explicit, documented precedence. This is cheap to decide now and a source of confusing results
forever if it is not. `kind` has no such collision — Liquers' nearest field is `type_identifier`,
which is the value type, not a genre.

---

## 4. Dependencies, and what actually blocks what

| Milestone | Depends on | Blocking? |
|---|---|---|
| M0–M2 | `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` (P1) | **no, but fix it first.** Without it, enumeration must hand-roll the live→store→recipe resolution to avoid scheduling jobs, and the non-evaluation invariant becomes a policy each caller re-implements |
| M3 | `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0) only for the HTTP surface; the command surface is unaffected | no |
| M1 (A3 quality) | `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) | no — degrades to text matching |
| M4 | nothing; each filter is a derived asset of one document | no |
| M4/M7 (indexation policy) | `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) only for a *per-asset* opt-out; scope and pattern policy need nothing | no |
| M5 | `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` (P2) for chunk-set changes | partially |
| M6 | `CORE-STORE-OPENBIN-MISSING` (P3), `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` (P2) | yes, for real streaming |
| M7 | `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (P2) for latency only | no — reconciliation is the guarantee |

Indexation policy (`indexation-policy.md`) is not in the MVP's dependency list because the MVP has
no index: a scan that may not evaluate arrives at the *when-ready* content policy for free, and
inclusion is the root the user searched. Policy becomes a real decision at M4 and M7, where something
is written down ahead of a query and can therefore be wrong.

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
