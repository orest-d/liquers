# Options analysis — searchable stores and assets

Companion to [Phase 1](./phase1-high-level-design.md). Phase 1 delimits the task; this document
holds the analysis that the delimitation rests on, because it is too long to live inside a 30-line
Phase 1.

It is **not** an architecture. It does not name types, signatures or modules — that is Phase 2. It
names the decisions that have to be made, the options for each, what each option costs, and which
combination is recommended.

---

## 0. Ground truth at HEAD

Verified against the code on 2026-09-17, because an options analysis built on a wrong picture of
what exists is worse than none.

| Fact | Where |
|---|---|
| `AsyncStore` can enumerate (`keys`, `listdir`, `listdir_keys`, `listdir_keys_deep`, `listdir_asset_info`) and fetch (`get`, `get_metadata`, `get_asset_info`). It has **no** selection method. | `liquers-core/src/store.rs` |
| `AssetManager` resolves a key as live asset → store → recipe provider. It exposes `get`, `get_asset`, `apply`, `recipe_opt`, `is_volatile`, `listdir_asset_info`. No enumeration of *live* assets, no selection. | `liquers-core/src/assets.rs:3795` |
| `AssetInfo` already carries `title`, `description`, `type_identifier`, `media_type`, `data_format`, `file_size`, `status`, `updated`, `unicode_icon`, `is_dir`, `is_error`, `is_volatile`. | `liquers-core/src/metadata.rs:678` |
| `Value::AssetInfo(Vec<AssetInfo>)` is already a core value variant, with serialization and an HTTP representation. | `liquers-core/src/value.rs:33` |
| `MetadataRecord` is a closed struct — no application-defined attributes. | `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` |
| `StoreCapabilities` has eight flags (`write`, `remove`, `directories`, `derived_directories`, `explicit_directories`, `remove_directories`, `stored_metadata`, `enumerate_keys`) and a conformance suite keyed to them. | `liquers-core/src/store_conformance/mod.rs:97` |
| Query parameters can carry **any** string; `ActionRequest::encode` escapes. Hand-written URLs get noisy, programmatic ones do not. | `specs/guides/QUERY_ESCAPING_GUIDE.md` |
| A dependency on **one directory's listing** is expressible and is recorded: `Step::GetAssetDirectory` inserts `DependencyKey::from_dir_key`. It is one directory level, not a subtree prefix. `Key::try_from(&DependencyKey)` rejects the `-R-dir/` form, so the fast-track staleness check treats such a dependency as inconclusive rather than as a change signal. | `liquers-core/src/plan.rs:2647`, `liquers-core/src/metadata.rs:256`, `liquers-core/src/assets.rs:1064` |
| Six of ten assets API endpoints are 501 stubs, `listdir` among them. | `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0) |
| No code in `liquers-core`, `liquers-lib` or `liquers-store` implements anything search-shaped. The name space is clear. | grep, 2026-09-17 |

Two consumers already want this and have written down what they need:
`STORE-NO-CONTENT-OR-METADATA-SEARCH` (P2, complexity L) states the capability gap and lists five
questions; `agent-memory-mvp` Phase 1 open question 3 asks how far an MVP goes on search, noting
that "a command over a subtree is honest at ~300 documents and wrong at 100×".

---

## 1. The decision axes

Eight decisions, each independently choosable, which is why they are separated. A design that
answers only some of them will be reopened by the first one it skipped.

| Axis | Question | Recommended |
|---|---|---|
| A | What is searched? | A3 — store entries plus known assets, without evaluating |
| B | Where does the capability live? | B2 + B3 + B5 — predicate and default scan in core, store hook, asset-level union, command surface in lib |
| C | How is a search expressed? | C2 — a serializable predicate built from command arguments, with C4 composition after it |
| D | What comes back? | D2+ — `Vec<AssetInfo>` plus an optional per-hit match record |
| E | How is it executed? | E1 as the contract, with the trait shaped so E3/E4/E5 can override |
| F | What counts as a match? | F1 — boolean, substring and field predicates; scoring field reserved |
| G | How is it reached? | G1 — a query command; G3 maps onto it; G2 deferred |
| H | How wide is one search? | H2 — rooted at a key, router fans out across mounts below it |

---

## Axis A — What is searched

**A1. Store entries only.** Keys, stored metadata, stored bytes. The smallest honest thing; it is
also what `STORE-NO-CONTENT-OR-METADATA-SEARCH` literally asks for. Weakness: it cannot see a
derived asset that exists only because a recipe declares it, which is half of what makes Liquers
interesting. A user asking "what do I have about expiration" does not distinguish stored from
derived.

**A2. Assets only.** Search the asset layer. Matches the agent-memory design's position that the
assets API — not the store API — is the client interface. Weakness: `AssetManager` has no
enumeration primitive at all today, so A2 is strictly more work than A1 *and* depends on it,
because asset enumeration below a key bottoms out in store enumeration.

**A3. The union, without evaluating.** Search returns: every store entry under the root; every
live asset the manager holds under it; every key a recipe provider declares under it. Metadata
for an unevaluated recipe key comes from the recipe, exactly as `get_asset_info` already resolves
it. **Content matching applies only to entries whose bytes already exist.** A recipe-declared key
with no stored data matches on metadata or not at all.

Recommended: **A3**, with the non-evaluation rule stated as a hard invariant (§2). A3 is the
contract users and agents expect, and the "do not evaluate" clause is what keeps it from being a
self-inflicted denial of service — a content search over a corpus of unevaluated recipes would
otherwise recompute the corpus.

---

## Axis B — Where the capability lives

**B1. A command in `liquers-lib`, nothing else.** A `ns-search` namespace whose implementation
walks `listdir_keys_deep` and filters. Zero core change, days of work, unblocks agent-memory-mvp
immediately. Weakness: exactly the failure `STORE-NO-CONTENT-OR-METADATA-SEARCH` describes — the
retrieval logic lands in a consumer, a backend that could filter server-side is not allowed to,
and the next consumer writes a third copy. It is the right *first commit* and the wrong *design*.

**B2. A method on `AsyncStore` with a default implementation.** The issue's own proposal. Every
existing store keeps compiling; a backend that can do better overrides. It needs a
`StoreCapabilities` flag and conformance rules that check the default and an override agree —
which the suite is already built to express. Weakness: it widens a trait with many implementors,
and it says nothing about assets (Axis A).

**B3. An operation at the `AssetManager` level.** Required by A3: only the asset layer knows about
live assets and recipes. It composes *over* B2 rather than replacing it.

**B4. A separate `SearchProvider` service on `Environment`,** registered like the recipe provider
and the store factory. Attraction: the store trait stays narrow, and the scan engine, an indexed
engine and an external engine are swappable by configuration. Weakness: a fourth service to
construct, configure, document and test, and it re-raises "which store does it search" — the
router already answers that for B2.

**B5. A command surface in `liquers-lib`,** over whichever of the above exists. Not an
alternative — every option needs it, because a capability with no query surface is not reachable
from HTTP, the UI, Python or WASM.

Recommended: **B2 + B3 + B5**, with the predicate type itself in `liquers-core` so that core, lib
and any backend speak the same one. B4 is a live alternative *if* indexing (E3/E5) turns out to
need its own lifecycle — Phase 2 should say why it is not needed rather than ignore it.

---

## Axis C — How a search is expressed

**C1. Fixed command arguments.** `search-<text>-<field>-<value>`. Trivial to implement and to
validate. Weakness: every new matching dimension is a new positional argument or a new command.

**C2. A serializable predicate value.** A small typed structure — root key, key-glob, metadata
field/value tests, content substring, limit — that is `Serialize`/`Deserialize`, and that a
command builds from its arguments while HTTP and MCP callers can send directly as JSON. Because
it is data rather than code, a backend can inspect it and decide what it can push down and what
it must scan. This is the form that makes B2's override worth having.

**C3. An expression mini-language in a string.** `title~"store" and area="core/store"`. The most
expressive per character, and the most expensive: a second grammar beside the query language, its
own parser, its own error reporting, its own escaping interaction, and a much harder push-down
story. Not for a first version; C2 does not prevent it later, since a parser can emit C2.

**C4. Composition in the query language.** The search command returns a list; further commands
filter, sort, cut and project it. This is the Liquers-native answer and it costs nothing new — but
alone it cannot be pushed down, because the predicate is hidden behind command boundaries that the
store never sees.

**C5. Regex.** Powerful, and the single hardest thing to push down to any backend. Worth a flag
on C2 later; not worth being the primary interface.

Recommended: **C2 as the predicate, C4 for everything beyond it.** The cheap, pushable, common
cases live in the predicate; anything exotic is a follow-on command over the result list. C1 is
how the command's arguments *look*; it is a spelling of C2, not a rival to it.

A note on escaping: search terms contain spaces, colons and slashes, and `ActionRequest::encode`
handles all of them, so no term is unexpressible. Hand-written search URLs will be ugly
(`search-expiration~.safety`), which is an argument for a UI/MCP surface that builds the
query, not an argument against C2.

---

## Axis D — What comes back

**D1. Keys.** Minimal, honest, and useless on its own: the caller immediately fetches metadata for
every hit, which is the read amplification the whole exercise is meant to remove.

**D2. `Vec<AssetInfo>`.** Already a `Value` variant, already serialized by the HTTP layer, already
rendered by the UI, and already carries the title/description tiers that make a hit judgeable
without reading the document. It is the obvious answer, and it is missing exactly two things: why
the entry matched, and how well.

**D2+. `Vec<AssetInfo>` plus an optional per-hit match record** (matched field, a short excerpt, an
optional score). Agents need the excerpt — an agent that gets only keys spends its context opening
documents to find out which one it meant. Keeping the match record *beside* `AssetInfo` rather
than inside it avoids pushing search concerns into a structure used by everything else.

**D3. A new rich value type in `liquers-lib`.** Needed only if the result grows beyond what D2+
covers; costs an `ExtValue` variant, conversions, serialization and a `TypeInfo` entry. Deferrable.

**D4. A DataFrame.** Excellent for analysing a corpus, wrong as the primary result: it is behind an
optional feature and it puts `liquers-lib`'s polars dependency on the critical path of a core
capability. Better as a conversion command from D2+.

Recommended: **D2+**. Reserve the score field from the first version even though F1 leaves it
unset, so that adding ranking later is not a breaking change to a serialized shape.

---

## Axis E — How it is executed

**E1. Scan on every search.** Walk `listdir_keys_deep` from the root, fetch metadata, fetch bytes
only when the predicate has a content test, filter, stop at the limit. O(corpus) per search, at
the caller. Correct everywhere, needs nothing new, and is the default implementation that lets
every existing store claim the capability.

**E2. The index as a derived asset.** The most Liquers-native idea available: an index is a value
derived from a corpus, the asset layer recomputes a derived value when its sources' content hashes
change, so the index cannot go quietly stale. The substrate is better than it first appears — a
dependency on a *directory listing* already exists (`Step::GetAssetDirectory` records
`DependencyKey::from_dir_key`), so "a document was added to this directory" is in principle a
change the dependency machinery can see, not only "a document I already knew about changed".

Three things still have to be established before an index is built on it:

- **The listing dependency is one level, not a prefix.** An index over a nested corpus needs a
  dependency per subdirectory, and a *newly created* subdirectory is seen only by its parent's
  listing.
- **The listing dependency is recorded and then dropped.** Nothing ever registers a version under
  an `-R-dir/` key, so `register_plan_dependencies` skips the edge silently, and both places that
  would later check it reject the form. Filed as
  `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`. Until that is fixed, a directory
  dependency cannot expire a dependent at all.
- **Fan-out is unmeasured.** An index over ~300 documents is one asset with a few hundred
  dependency entries, rebuilt in full when any one of them changes. That is a measurement, and it
  should be taken before it is designed around.

E2 is therefore a well-founded experiment rather than a speculative one — and still a poor basis
for a first version, because all three questions above are open.

**E3. A store-maintained index.** The store updates an index on `set`/`remove`, in memory or in
sidecar keys. Fast and always fresh for stores that own their writes; wrong for a store whose
backend is written by someone else, and sidecar keys collide with `SIDECAR-COLLIDING-KEYS`
territory.

**E4. Push-down to the backend.** The point of B2's override: an OpenDAL backend over a service
with server-side filtering, a SQL-backed store, a store that already keeps a directory index. Free
when available, absent otherwise — which is precisely why the default implementation must exist.

**E5. An external index engine** (a Tantivy-backed store or service). The right answer at 100×
scale and a large dependency at 1×. It fits behind B2's override or B4's provider, so committing to
either leaves the door open.

Recommended: **E1 as the specified contract**, shaped so E3, E4 and E5 are overrides rather than
rewrites. E2 gets an explicit measurement task, not a commitment.

---

## Axis F — What counts as a match

**F1. Boolean matching.** Case-folded substring on chosen metadata fields and on content;
glob or prefix on the key; equality on typed fields (`type_identifier`, `status`, `is_dir`). No
tokenizer, no stemmer, no ranking. Everything here is implementable identically in a scan and in
most backends, which is what makes push-down realistic.

**F2. Ranked full-text.** Tokenization, stemming, stop words, BM25. A real search engine's job.
It changes the result contract (order becomes meaningful), it is language-dependent, and it
effectively forces E5.

**F3. Semantic / embedding search.** What agents ultimately want, and a different system: an
embedding model, a vector index, chunking, and a runtime dependency Liquers does not have. It is
a *consumer* of this design rather than a variant of it — a store or provider whose selection
happens to be vector-based can implement the same predicate contract's "match" with a `nearest`
clause later.

Recommended: **F1**, with F2 and F3 named as out of scope in Phase 1 rather than left ambiguous,
and with the result shape (D2+) chosen so neither requires a breaking change.

---

## Axis G — How it is reached

**G1. A query command.** `-R/specs/issues/-/ns-search/search-expiration`. Reachable through `/q`, the UI query
console, `liquers-web` and `liquers-py` with no per-surface work; cacheable as an asset;
declarable in a recipe, which makes a saved search a first-class object. This is the Liquers answer.

**G2. A dedicated HTTP endpoint.** A `GET /search` on the assets API. More conventional for
non-Liquers clients, and it needs a specification change to `WEB_API_SPECIFICATION.md` — a document
that six existing endpoints already contradict (`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`,
`WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION`). Adding a tenth specified-but-unimplemented
endpoint to that surface is not the move.

**G3. An MCP tool.** The agent-facing form, owned by `agent-memory-mvp`'s adapter. It maps onto G1
by constructing a query; it should not grow its own search engine.

Recommended: **G1**, with G3 mapping onto it and G2 deferred until the assets API is whole.

---

## Axis H — How wide one search is

**H1. Whole store.** Simple, unbounded, and wrong the moment a store is large.

**H2. Rooted at a key.** Every search names a root; the subtree below it is the corpus. This is
also the natural unit for the store router: a root at or below a mount point dispatches to one
store, and a root above one requires the router to fan out across the mounts beneath it and merge.
That fan-out is a design item, not an accident — if the router does not implement it, a search
above a mount boundary silently sees only one store.

**H3. Explicit multi-root.** A list of roots in the predicate. Cheap once H2 exists; a superset.

Recommended: **H2**, with the router fan-out specified rather than assumed, and H3 as a later
predicate field.

---

## 2. Invariants that constrain every option

These hold regardless of which branch each axis takes. They belong in the reference document that
this design produces.

1. **A search never evaluates.** It reads what exists — stored bytes, stored metadata, live asset
   metadata, recipe declarations. It never triggers a computation, never changes an asset's status,
   and never populates a cache. A search that can recompute a corpus is an outage waiting for a
   careless client.
2. **A search is bounded.** Every search has a root and a result limit, and it can be truncated.
   The `/q` handler's 30-second timeout is reachable, so "it returned everything" must never be the
   only outcome.
3. **A search is addressable.** The result is a value produced by a query, so it composes with
   other commands, can be cached, and can be declared in a recipe. A saved search is a recipe, not
   a new kind of object.
4. **Ordering is not promised** unless a matching mode that defines one (F2) is chosen. Until then,
   results are deterministic for a fixed store state — sorted by key — but that order carries no
   relevance meaning.
5. **No identity, no access control.** Consistent with `agent-memory-mvp`'s exclusion and
   `CORE-SESSION-AND-KEY-ACL`: a search sees what the environment sees. Adding per-user visibility
   later is a filter over the same contract, not a change to it.
6. **A store that cannot select says so.** The capability is declared, and the conformance suite
   checks that a declared override agrees with the default implementation on the same corpus.

---

## 3. Recommended combination

> **A3 · B2+B3+B5 · C2 with C4 · D2+ · E1 (overridable) · F1 · G1 · H2**

In one paragraph: a small serializable predicate type in `liquers-core`; an `AsyncStore` selection
method with a default scan implementation, a capability flag and conformance rules; an asset-level
search that unions store hits with live assets and recipe-declared keys without evaluating
anything; a `liquers-lib` command namespace that builds the predicate from query arguments and
returns asset infos with match records; further filtering by ordinary commands over that list.

Why this and not something smaller: B1 alone would ship faster and would have to be undone, because
it puts retrieval logic in a consumer and forecloses push-down — the exact complaint on file.

Why this and not something larger: E2, E5, F2 and F3 each add a dependency or a lifecycle that the
known consumers do not need at ~300 documents, and every one of them remains reachable afterwards
because the predicate and the result shape were chosen to accommodate them.

What it unblocks:

- `STORE-NO-CONTENT-OR-METADATA-SEARCH` — closed by the store capability.
- `agent-memory-mvp` open question 3 — answered: search is a core capability with a command
  surface, not a bespoke command inside the memory namespace.
- `scripts/docs_index.py` — its selection half becomes expressible as a query, which is the
  dogfooding oracle the memory design was already reaching for.

---

## 4. What is deliberately deferred, and why it stays cheap

| Deferred | Why now is wrong | What keeps it cheap later |
|---|---|---|
| Ranked full-text (F2) | Language-dependent, forces an engine | Score field reserved in the result; ordering promised as unspecified |
| Semantic search (F3) | New runtime dependency, chunking, model choice | It is a selection implementation behind the same contract |
| Index as a derived asset (E2) | Directory-listing dependencies exist but are one level deep, unverified for staleness, and unmeasured at fan-out | Measurement and verification tasks recorded; nothing in the contract prevents it |
| External engine (E5) | Large dependency at current scale | Lands as a store override or a provider |
| A `/search` endpoint (G2) | The assets API has six unimplemented endpoints already | The command is reachable through `/q` meanwhile |
| Application-defined metadata attributes | Separate gap, separately filed | Predicate field tests are defined over metadata generally |

---

## 5. Decisions Phase 2 must make

These are the questions this analysis frames but deliberately does not answer.

1. The exact predicate structure, and which of its clauses a backend is allowed to partially push
   down (a partial push-down that silently drops a clause is a correctness bug, so the contract
   must be "push down and re-check" or "push down all or nothing").
2. Whether the store-level and asset-level searches are one trait method or two, and how the union
   deduplicates a key that exists both as a live asset and in a store.
3. Router fan-out semantics for a root above a mount boundary, including what happens when one
   mounted store cannot select.
4. Whether the match record carries an excerpt for binary values (it should not) and how excerpt
   extraction decides that a value is text.
5. Whether the result limit is a hard cut with a truncation flag, or pagination with a cursor —
   and if a cursor, what it is stable against.
6. Whether `listdir_asset_info` and the new selection share an implementation, since a search with
   an empty predicate and depth 1 is a listing.
