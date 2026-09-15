# Phase 1 — Agent memory service: analysis and MVP

## 1. The reference model

[OpenViking](https://github.com/volcengine/OpenViking) is the shape this proposal is measured
against. Four of its ideas matter here:

1. **A filesystem paradigm.** All context lives under one URI scheme (`viking://`) and is browsed
   with `ls`, `tree`, `read`, `write`, `find`, `search`, `grep` rather than through an opaque
   vector index. An agent can see the shape of what it knows.
2. **Three loading tiers.** L0 an abstract (one sentence, enough to judge relevance), L1 an
   overview (enough to plan), L2 the full document (loaded only when needed). Processed directories
   carry `.abstract.md` and `.overview.md` beside their content.
3. **Three kinds of context, one namespace.** Resources (documents, repositories, pages), memories
   (preferences, history, extracted experience), and skills (reusable procedures).
4. **Consolidation.** Committing a session triggers background extraction; candidate memories are
   compared against existing ones and created, merged or skipped.

The claim of this document is not that Liquers should copy it. It is that Liquers already
implements the *expensive* parts of (1) and (2), that (3) is a directory layout, and that (4) is
the part which should stay agent-in-the-loop for an MVP.

## 2. What Liquers already is

| Memory-system need | Liquers part | State |
|---|---|---|
| A namespace to browse | `Key`, `AsyncStore::listdir` / `listdir_keys_deep` / `is_dir` | Built, with a written contract (`reference/STORE_SEMANTICS.md`) and a conformance suite |
| Several backends under one namespace | `AsyncStoreRouter`, mounted by key prefix | Built |
| An address that also *derives* | `Query` — `-R/key/-/action` | Built |
| Declared derived entries | `Recipe`, `recipes.yaml`, `DefaultRecipeProvider` | Built |
| Lazy computation with caching | `Asset`, `AssetManager` | Built |
| Invalidation when a source changes | `MetadataRecord::version` (content hash) + `dependencies: Vec<DependencyRecord>` | Built |
| Freshness policy | `expires`, `expiration_time`, volatility | Built |
| Typed, documented operations | `register_command!` → `CommandMetadata` (label, doc, typed arguments, defaults) | Built |
| A machine-readable operation catalogue | `export-command-registry` → `specs/command_registry.yaml` | Built |
| HTTP read, write, list, evaluate | `liquers-axum`: `/api/store/*`, `/api/assets/*`, `/api/recipes/*`, `/q/*` | Built |
| Push notification when a derived entry changes | assets WebSocket | Built |
| Per-entry title, description, type, size, timestamp, icon | `MetadataRecord` | Built |

The single most important line in that table is the invalidation one. A memory system's recurring
problem is that summaries go stale silently. Here, an abstract derived from a document is an asset
whose `dependencies` record the document's content-hash version; when the document changes, the
version no longer matches and the abstract is recomputed on next read. Nobody has to remember to
regenerate anything, and nothing has to be watched.

**The second most important observation is about tools.** The command registry is already a tool
declaration format: name, namespace, label, documentation, typed arguments with defaults and GUI
hints, exportable to YAML. An MCP adapter does not have to invent schemas. But it must *not* export
all ~300 commands as MCP tools — that is a context budget catastrophe at the agent end. See §6.

## 3. What is missing

Four gaps, in descending order of how much they constrain the MVP.

**3.1 No content or metadata search.** `AsyncStore` enumerates and fetches; it does not select.
Finding the issues that mention "expiration" means listing the subtree and reading every file.
Filed as `STORE-NO-CONTENT-OR-METADATA-SEARCH`. The MVP works around it by implementing search as
a *command* over a subtree, which is honest at this corpus size (~300 documents) and wrong at
100×.

**3.2 No application-defined metadata.** `MetadataRecord` is a closed struct with no extension
field, and `Metadata::LegacyMetadata` is an all-or-nothing alternative rather than an extension.
Tags, provenance and retention class have nowhere to live. Filed as
`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`. The MVP works around it by keeping attributes in YAML
front-matter — which is what `specs/` already does — and parsing them with a command.

**3.3 Writes cannot go through a command.** Commands are pure `state → value`; a command that
writes to a store by key was considered and rejected (`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`). So a
memory *write* is an HTTP store `POST`, not a query. This is not a defect — read/derive and write
being different verbs is defensible — but it means the write path skips command metadata,
argument validation and the command registry's documentation. Flagged as an open question in §9.

**3.4 No identity, no authorization, no write preconditions.** `Context` carries no session, so
nothing can be authorized per key (`CORE-SESSION-AND-KEY-ACL`), and `set` has no precondition, so
two writers clobber each other silently (`STORE-WRITE-HAS-NO-PRECONDITION`). Both are out of scope
for an MVP that binds to localhost and serves one agent at a time, and both are blocking for
anything else.

Two smaller ones worth knowing about: resource names are ASCII-only
(`RESOURCE-NAME-ASCII-ONLY`), so memory entries need slugs as keys and keep their real titles in
metadata; and `GET /q/{*query}` polls with a hard 30-second timeout, so anything slower must be
driven through the assets API and its WebSocket instead.

## 4. The MVP

**Scope: serve this repository's `specs/` and `.claude/skills/` as agent memory, over HTTP and
MCP, with derived tiers and search — and beat `scripts/docs_index.py` at its own job.**

Choosing `specs/` as the first corpus is the whole point. It is ~300 documents with a real schema
(front-matter with closed vocabularies), a real consumer (agents working in this repository), an
existing generator to measure against, and a maintainer who will notice immediately if retrieval
is wrong. It also directly answers the brief: issues, the design lifecycle, skills and
documentation are exactly what `specs/` contains.

### 4.1 Key layout

```
mem/
  docs/          AsyncFileStore over specs/           — read-mostly, the corpus
    issues/<ID>.md
    design/<slug>/…
    reference/…  guides/…  archive/…
  skills/        AsyncFileStore over .claude/skills/  — read-only
  notes/         writable store                       — agent-written memory
  sessions/      writable store                       — raw session digests, pre-consolidation
```

One `AsyncStoreRouter`, four mounts, each a store type that already exists. `mem/docs` and
`mem/skills` are the corpus; `mem/notes` and `mem/sessions` are where an agent writes. The split
matters: the first two are version-controlled and reviewed, the second two are not, and conflating
them is how a memory system starts contradicting its own repository.

### 4.2 The `ns-mem` command namespace

Eight commands, all pure, all deterministic, **all LLM-free**. This is a deliberate constraint: an
MVP whose summaries come from a model cannot be unit-tested, cannot run offline, and cannot be
compared against `docs_index.py`. Extractive summaries from a structured corpus are good enough,
and an agent that disagrees can write a better abstract by hand — see §4.3.

| Command | Signature sketch | Produces |
|---|---|---|
| `frontmatter` | `fn frontmatter(text) -> result` | YAML front-matter as JSON |
| `abstract` | `fn abstract(text, max_chars: Int = 280) -> result` | **L0** — `title` + first sentence of the first body section |
| `overview` | `fn overview(text, max_chars: Int = 2000) -> result` | **L1** — front-matter plus the first section (`## Problem`, in this corpus) |
| `index` | `fn index(state, context) -> result` | A directory's children with their L0, status and area |
| `search` | `fn search(state, pattern: String, limit: Int = 20, context) -> result` | Matching keys with score and L0 |
| `select` | `fn select(state, field: String, value: String, context) -> result` | Keys whose front-matter field matches |
| `related` | `fn related(state, id: String, context) -> result` | Follows `design:`, `issues:`, `duplicate_of`, `superseded_by` |
| `validate` | `fn validate(state, context) -> result` | The `docs_index.py --check` rules, as a command |

Every one of these is a function over text or a directory listing. None needs a new value type,
none needs network access, and all of them are testable with `AsyncMemoryStore` fixtures.

All query forms below were checked with `liquers-validate` (`--command` for the not-yet-existing
commands) and parse to the plan they read as:

```
-R/mem/docs/issues/ASSETS-FIX1.md                                  L2 — the document
-R/mem/docs/issues/ASSETS-FIX1.md/-/ns-mem/abstract                L0
-R/mem/docs/issues/ASSETS-FIX1.md/-/ns-mem/overview/overview.md    L1, named
-R/mem/docs/issues/-/ns-mem/index/index.json                       the issue index
-R/mem/docs/-/ns-mem/search-add~_a~_command-10/results.json        search, limit 10
-R/mem/docs/issues/-/ns-mem/select-status-accepted/accepted.json   the accepted backlog
-R/mem/docs/issues/ASSETS-FIX1.md/-/ns-mem/related/related.json    the link neighbourhood
```

Note `~_` for a space — action-parameter escaping is `guides/QUERY_ESCAPING_GUIDE.md`, and any
tool that builds these queries from user text must use `encode_token` rather than string
concatenation.

### 4.3 Recipes make the tiers addressable, and the asset layer keeps them fresh

A `recipes.yaml` under `mem/docs/` declares the derived entries, which gives each tier a plain key
an agent can fetch without knowing the query language, and gives the asset layer something to
cache and invalidate:

```yaml
- query: -R/mem/docs/issues/-/ns-mem/index/index.json
  title: Issue index
- query: -R/mem/docs/-/ns-mem/select-status-accepted/backlog.json
  title: Accepted backlog
```

When an issue file changes, its content hash changes, the dependency record on the index no longer
matches, and the index recomputes on next read. **This is the OpenViking self-evolution story for
derived content, already implemented.** What is *not* automatic is consolidation of new memories —
that is §9.

An agent that wants a better abstract than the extractive one writes `.abstract.md` beside the
document; the recipe for that key is a fallback, not an override, so a hand-written abstract wins.
That is the cheap version of agent-in-the-loop summarization, and it costs one convention.

### 4.4 The HTTP surface already exists

No new endpoints. `liquers-axum` as it stands gives:

| Need | Endpoint |
|---|---|
| Read a document (L2) | `GET /liquer/api/store/data/mem/docs/issues/ASSETS-FIX1.md` |
| Read a tier (L0/L1) | `GET /liquer/q/-R/mem/docs/issues/ASSETS-FIX1.md/-/ns-mem/abstract` |
| Browse | `GET /liquer/api/store/listdir/mem/docs/issues` |
| Entry + metadata in one call | `GET /liquer/api/store/entry/mem/docs/issues/ASSETS-FIX1.md` |
| Write a memory | `POST /liquer/api/store/entry/mem/notes/<slug>.md` |
| Freshness / version of a derived entry | `GET /liquer/api/assets/metadata/…` |
| Notification when an index recomputes | `WS /liquer/ws/assets/…` |

The MVP binary is therefore a thin one: an `EnvironmentConfig` naming the four store mounts, the
`ns-mem` registration, and `FullApiBuilder`.

### 4.5 What replaces `docs_index.py`

Not immediately, and possibly not ever — but it is the measuring stick. `docs_index.py` does five
things: parse front-matter, validate against closed vocabularies, generate `index.csv`, generate
`index.md` and the `README.md` blocks, and scaffold new issues. The first three are `frontmatter`,
`validate` and `index` as queries. The fourth is a rendering command. The fifth is a write, which
stays where it is — scaffolding must work with no server running, and §4 of
`DOCS_STRUCTURE_GUIDE.md` is explicit that filing must need no network.

## 5. Milestones

| # | Deliverable | Size | Depends on |
|---|---|---|---|
| M1 | `ns-mem`: `frontmatter`, `abstract`, `overview`, `index`, with tests | M | — |
| M2 | `search`, `select`, `related`, `validate`; `mem/` router config; `recipes.yaml` for the derived indexes | M | M1 |
| M3 | `liquers-memory` binary: environment config + `liquers-axum`, serving `specs/` | S | M2 |
| M4 | `liquers-mcp` crate: six tools over stdio | L | M3 |
| M5 | `.claude/skills/liquers-memory/` and a `reference/` document for the key layout and tier contract | S | M4 |

**M1–M3 is the MVP proper**: the corpus is served, tiered and searchable over HTTP. M4 is what
makes it usable by an agent without `curl`. M5 is what makes it usable when MCP is not available,
and is required by the project's own documentation rules regardless.

### Acceptance test

Regenerate `specs/index.csv` from a query — `-R/mem/docs/-/ns-mem/index/index.csv` — and diff it
against the file `scripts/docs_index.py` produces. Byte-identical is the target; a documented,
justified difference is a pass; a difference nobody can explain is a bug. This is a real test with
a real oracle, it exercises front-matter parsing, directory traversal, vocabulary validation and
CSV rendering at once, and it can run in CI beside `cargo test`.

## 6. The MCP surface, and why it is six tools and not three hundred

The obvious move — generate `tools/list` from `command_registry.yaml` — is wrong. Three hundred
tool schemas is tens of thousands of tokens in every agent's context window, before the agent has
done anything, and most of them (`ns-img/resize`, `ns-pl/…`) have nothing to do with memory.

The better move exploits what makes Liquers different from a tool list: **a query is a composable
tool call**. One tool that accepts a query has the expressive power of the whole registry, and the
registry export becomes *documentation* the agent fetches when it needs it, rather than a preamble
it always pays for.

```
memory_ls(path, depth=1)                 → listdir
memory_read(path, tier=abstract|overview|full)
memory_search(text, scope="mem/docs", limit=20)
memory_write(path, content, title, tags)
memory_query(query)                      → the escape hatch: any Liquers query
memory_commands(namespace="mem")         → the registry export, on demand
```

Six tools, of which four are conveniences the agent will use 95% of the time and one is the full
power of the system. `memory_query` is also where the query language's composability shows up —
`…/-/ns-mem/select-status-accepted/-/ns-mem/index` is a filter and a render in one address, and no
fixed tool set would have offered it.

**Transport:** stdio first (that is what a local coding agent speaks, and it needs no auth, which
§3.4 says we do not have). Streamable HTTP later, behind the same adapter, once there is an
identity model to authorize it.

**Where it lives:** a new `liquers-mcp` crate depending on `liquers-core` and an MCP server crate,
not inside `liquers-axum` — the axum crate is an HTTP interface library and MCP over stdio is not
HTTP. It should be able to drive either an in-process `Environment` or a remote `liquers-axum`
instance over HTTP; the second is what makes it a *service* rather than a library.

## 7. Non-goals for the MVP

Stated explicitly, because each is a plausible thing to build and each would sink the timeline:

- **No embeddings, no vector search.** Substring and front-matter filters over 300 structured
  documents are strong. Measure before adding a dependency, an index to maintain and a model to
  call.
- **No LLM-generated summaries.** Deterministic extraction, with a hand-written `.abstract.md`
  override. Keeps the MVP offline, testable and diffable.
- **No automatic consolidation.** The agent writes `mem/sessions/<id>.md`; merging sessions into
  durable notes is a later phase and needs a policy nobody has written yet.
- **No auth, no multi-tenancy.** Localhost, one writer. §3.4 says why.
- **No `specs/` write path through the service.** Documents are edited in the checkout and
  reviewed in pull requests. A memory service that can silently rewrite the repository's own
  specifications is a worse tool, not a better one.

## 8. Risks

1. **Search does not scale**, and the fix — an inverted index as a derived asset — puts several
   hundred dependencies on one asset. That is architecturally elegant and completely unmeasured;
   `BENCHMARK-SUITE` is open and would need to come first.
2. **The `specs/` corpus is small enough to flatter the design.** Anything that works at 300
   documents and is O(corpus) per query will look fine and fail later. The acceptance test should
   record timings so the cliff is visible before it is hit.
3. **Two sources of truth.** If notes in `mem/notes/` start contradicting `specs/`, the service
   makes agents *less* reliable. The layout in §4.1 separates them; the skill in M5 has to say
   which wins (`specs/` does).
4. **MCP churn.** The protocol is moving. Keeping the adapter thin — six tools over a stable query
   language — is the hedge.

## 9. Open questions

1. **Should there be a write command after all?** §3.3 routes writes through the store API, which
   means no argument validation, no documented signature and no place to run a policy. A
   side-effecting command model was rejected once for good reasons; a *store-namespace* command
   (`STORE-COMMAND-NAMESPACE-MISSING` is already open) might be the shape that works.
2. **Where do tags live before `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` is fixed?** Front-matter
   works for Markdown and not for anything else. Is the MVP Markdown-only, and is that a problem?
3. **Is `mem/docs` a store over `specs/`, or is `specs/` itself a Liquers store?** The first is a
   read-only view and is what the MVP assumes. The second is more interesting and forces the
   metadata question immediately.
4. **Consolidation policy.** Create / merge / skip needs a rule. This is the one place where an
   LLM is genuinely the right implementation, and the one place where getting it wrong corrupts
   memory rather than just making it unhelpful.
5. **Does `index.csv` stay generated by Python?** The acceptance test in §5 makes the Rust path
   credible; switching over is a separate decision with a migration cost and no urgency.
