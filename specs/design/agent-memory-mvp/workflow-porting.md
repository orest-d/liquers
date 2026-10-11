# Porting this repository's knowledge workflows into the memory service

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-11. It inventories how this
repository already manages knowledge for agents — documents, front-matter, skills, Python scripts,
templates — and asks, for each mechanism, what memory service it is, and which Liquers command
(tool) could replace or serve it.

Counts and behaviour are taken at `main` `df2d5ee`. The search commands cited here
(`ns-search/*`) are implemented on the `store-and-asset-search` branch
(`claude/relaxed-brahmagupta-1tadmg`), not yet merged; their contract is
`specs/reference/SEARCH.md` and `specs/guides/SEARCH_GUIDE.md` on that branch.

## 1. `specs/` is already an agent memory

The repository runs a memory system by hand. Mapped onto the vocabulary of the systems surveyed in
[`memory-systems-survey.md`](./memory-systems-survey.md):

| `specs/` genre | Count | Memory type | Temporal rule today |
|---|---|---|---|
| `reference/` | 25 | **Semantic** — what is true now | Must be true at HEAD; `reviewed:` older than 92 days is *overdue* |
| `guides/` | 15 | **Procedural** — how to do things | Same `reviewed:` rule |
| `design/<slug>/` | 169 folders | **Episodic / rationale** — why a change was made | Frozen once `complete`; `superseded_by` links forward |
| `issues/` | 290 | **Prospective** — what is wrong or wanted | `status` lifecycle (§4.3 of `DOCS_STRUCTURE_GUIDE.md`) |
| `archive/` | 41 | **Episodic snapshot** — what was true on a date | Never edited |
| `.claude/skills/*/SKILL.md` | 4 (+1 adapter) | **Procedural**, tiered: `description` always loaded, body on demand | None |
| `CLAUDE.md` | 1 | **Core memory** — always in context (Letta's "core block") | None |
| `specs/index.csv`, `index.md`, README generated blocks | 3 | **Index / catalog** (LLM-wiki `index.md`) | Regenerated; `--check` fails when stale |

Three properties of this corpus matter for the design, because a general memory system has to
provide them and most do not:

1. **Genre is location, state is front-matter** (`DOCS_STRUCTURE_GUIDE.md` §1). This is exactly
   OpenViking's split (directories carry kind) and Basic Memory's (front-matter carries fields).
2. **Temporal validity is explicit and per genre.** A reference document must be true now, an
   archive document was true on a date, a design is rationale that is never "updated". That is a
   lightweight version of what Zep/Graphiti does with bi-temporal edges, and it is the part
   naive vector memories lack: they cannot tell a current fact from a historical one.
3. **Every rule is mechanically checkable or explicitly judgement.** `docs_index.py --check` checks
   ~25 rules over 499 documents (0 errors, 32 warnings at HEAD); everything else is written as a
   procedure for an agent. That line between bookkeeping and judgement is the line between
   *commands* and *the agent* in the service.

## 2. The mechanisms, and the service each one is

### 2.1 Scripts

| Script | Size | What it does | Memory service | Port to |
|---|---|---|---|---|
| `scripts/docs_index.py` (default mode) | 1344 lines | Parses front-matter of every document; writes `index.csv`, `index.md`, `index.html`, README generated blocks | **Catalog / index generation** | A recipe: `specs/index.csv` = `ns-search/field_extract` over the collections, projected to the CSV columns. The file becomes a derived asset, recomputed when a source's version changes |
| `docs_index.py --check` | — | ~25 rules: vocabularies, required fields, id = filename, reference resolution, phase/status coupling, readiness reciprocity, AC coverage, History rows, dead links, stale index | **Lint / integrity** (LLM-wiki *lint*) | `ns-mem/check`: schema-driven rules over front-matter records, plus a link graph; output is a record table of violations |
| `docs_index.py new` | — | Near-duplicate search on title words, derives the id, validates `area`, writes the template, regenerates | **Capture with deduplication** (mem0's ADD-vs-NOOP decision, made by the agent) | `ns-mem/similar` (BM25 over titles, `ns-search/search-…-title`) + `ns-mem/new` (id rule + template + schema validation) |
| `docs_index.py --sort` | — | Queue order: priority, complexity, readiness, autofix | **Ranking by policy** | A saved query using `sort` from `record-filter-commands` |
| `liquers-project/scripts/init_feature.py` | 169 lines | Creates a design folder from templates | **Capture from a multi-file template** | `ns-mem/new` with a folder template |
| `liquers-project/scripts/validate_phase.py` | 311 lines | Required sections per phase, leftover placeholders, size limits, AC coverage | **Lint, section-level** | `ns-mem/check` rules of kind `required_sections`, `no_placeholder`, `max_size` |
| `liquers-validate/scripts/lqv.py` | 282 lines | Wraps `liquers-validate`, renders the plan compactly | Not memory — **query validation** | A tool of the *general* Liquers MCP (`validate_query`); the memory MCP needs it only to explain a malformed `memory_query` |

The Python scripts are not to be deleted by this work. They are the **oracle**: a port is done when
its output equals the script's on the real corpus (Phase 1 AC-9 and AC-10).

### 2.2 Front-matter vocabularies and procedures

| Mechanism | Memory service | Port to |
|---|---|---|
| Closed vocabularies (`area` 24 values, `status`, `priority`, `complexity`, `phase`, `readiness`, `autofix`) | **Schema** — what fields a memory of a kind has | A collection schema, stored as data in the corpus (`_schema/<collection>.yaml`), read by `new`, `check`, `set_field` and `describe` |
| Status transition diagram (§4.3) | **Lifecycle / supersession** | `ns-mem/set_field` refuses a transition the schema does not allow |
| `## History` table, top row = `reviewed:` (§9.2) | **Change log per document** (mem0's `history`) | `ns-mem/append_row` into a named section, bumping the coupled field |
| `reviewed:` older than 92 days → overdue | **Staleness / decay** | A `check` rule with a `max_age` per collection |
| Readiness ownership, reciprocal `design:` links | **Relations with integrity** (graph memory) | Link fields declared in the schema with `reciprocal:`; `check` verifies them |
| Issue filing §4.8 (search, id, template, fill, regenerate) | **Capture procedure** | `memory_new` MCP tool = `similar` + `new`, one call |
| Overlap tests T1–T4 (`references/overlap.md`) | **Consolidation** — merge or link related memories | Judgement. The service supplies candidates (`similar`, `links`), the agent decides |
| `autonomous_issue_fixing.md`, `autonomous_bulk_design.md`, compaction | **Procedural memory** for multi-step work | Stay as skills. Their *mechanical* steps become calls: enumerate (saved query), scaffold (`new`), validate (`check`), regenerate (recipe), transition (`set_field`) |

### 2.3 Skills and templates

`SKILL.md` front-matter has `name` and `description`; the host loads every description and reads the
body only when the skill applies. That **is** two-tier retrieval — L0/L1 in front-matter, L2 the
body — the same mechanism OpenViking applies to its `skills/` directory. So skills fit the memory
model without change: a `skills` collection whose L1 is the front-matter `description`.

The phase templates (`references/phase{1..4}-template.md`, `compact-design-template.md`) are
extracted by `init_feature.py` from a fenced block. In the service a template is just a memory under
`_templates/`, addressed by key, and `new` instantiates it with the supplied fields. One mechanism
then serves issue templates, design folders, and an agent's note templates.

## 3. Commands instead of scripts

Because Liquers commands are already the tool layer — typed arguments, documentation, a registry,
caching, and composition through the query language — the port is a command set, not a script set.
Every row below is **deterministic bookkeeping**; nothing in it calls a language model.

| Command (`ns-mem`) | Replaces | Input → output |
|---|---|---|
| `describe` | reading `DOCS_STRUCTURE_GUIDE.md` to learn the fields | collection name → its schema, folders, templates and id rule, as a record |
| `similar` | the duplicate search in `docs_index.py new`; the "search `index.csv` first" step | title text → ranked records (`ns-search/search` over `title`, `description`) |
| `new` | `docs_index.py new`, `init_feature.py` | collection, title, fields → created key(s); refuses an unknown vocabulary value; optional `force` after `similar` |
| `set_field` | hand edits of `status:`, `phase:` | key, field, value → updated document; validates vocabulary and transition |
| `append_row` | hand edits of `## History` | key, section, cells → updated document |
| `check` | `docs_index.py --check`, `validate_phase.py` | folder or collection → violation records (`rule`, `key`, `severity`, `message`) |
| `links` | dead-link check; "incoming references" by grep | folder → link records (`from`, `to`, `kind`, `resolved`) |
| `render_index` | `index.csv`, `index.md`, README blocks | records → CSV / Markdown text; used by recipes, not by agents directly |
| `outline`, `section` | `Read` with offsets; `sed -n` | key → heading tree / one section (overlaps `NO-MARKDOWN-NAVIGATION-COMMANDS`) |

Generated files become **recipes**, not command output written by a script. `specs/index.csv` is
declared once in a `recipes.yaml`, and the asset layer recomputes it when any source's content-hash
version changes — the property `docs_index.py --check`'s "index is stale" rule approximates by
regenerating and diffing.

Proposed signatures (not yet registered; validated with `liquers-validate --command`):

```
-R-key/specs/issues/-/ns-mem/check
-R-key/specs/-/ns-mem/similar-store~.metadata~.search-5
ns-mem/describe-issues
```

## 4. What stays judgement

The service must not try to automate these; it supplies the inputs and records the outputs:

- choosing priority, complexity, area (the agent guesses; review corrects — §4.8);
- overlap triage and merge decisions;
- writing Phase 1–5 content, tiering open questions, choosing readiness;
- deciding that a memory is stale in substance (as opposed to by date).

This is also the line Claude's memory tool and the LLM-wiki pattern draw: the *agent* writes and
consolidates, the *store* keeps files, indexes and rules. mem0 and OpenViking draw it differently —
they call an LLM inside the service. [`service-specification.md`](./service-specification.md) §4
records why the MVP takes the first side.

## 5. What does not port

- `specs/index.html` (untracked, a human convenience) — the assets API and a UI serve it better.
- `--sync` (a stub for GitHub status) — GitHub state is not memory.
- Host adapters (`.agents/skills/…`) — these are a host concern; the MCP server replaces the need
  for per-host adapters only for *reading*, not for skill registration.
