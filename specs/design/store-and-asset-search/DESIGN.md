---
id: STORE-AND-ASSET-SEARCH
kind: design
title: Search over stores and assets
workflow: liquers-project
status: in_review
phase: high-level
area: [records, core/assets, lib/commands, docs]
gh_pr: []
issues: [STORE-NO-CONTENT-OR-METADATA-SEARCH, RECORD-SOURCE-WRAPPERS-UNSPECIFIED]
affects_docs: [reference/SEARCH.md, reference/RECORD_STREAMS.md, guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-09-17
superseded_by:
---
# Search over stores and assets — design tracking

**Created:** 2026-09-17 · **Revision 8:** 2026-10-10

## Phase Status

- [ ] Phase 1: High-Level Design — revision 8 **in review**. Revision 7 was approved on 2026-09-18
  and is superseded.
- [ ] Phase 2: Solution & Architecture — revision 8 drafted alongside Phase 1; reviewed at its own
  gate
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## What this design builds

Three commands in a new `search` namespace, over records (`liquers-records`):

| Command | What it does |
|---|---|
| `catalog` | A folder, taken from the input state, becomes records: one row per entry, with key, filename, extension, status, title, description and optionally content. Recursive or folder-only. The root key is used when there is no input |
| `commands` | The command registry, whole or one namespace, becomes records whose columns match search's default fields |
| `search` | Filters any records with a short syntax: terms, phrases, `-`, `\|`, parentheses and `field:value`. Optionally ranks them with BM25. It is case-sensitive on request, and matches the field list given at the end of its arguments, or a default list |

`search` accepts whatever can be made into records:
- a view or a source;
- a folder, through `catalog`;
- a key or query, which is evaluated;
- a stored CSV or other table format.

With `rank=false` over a source, it filters lazily and never materializes.

## Revision 8 — refocused on records, 2026-10-10

Revisions 1–7 ran from 2026-09-17 to 2026-09-25 and built an analysis. Its record half became
[`record-streams`](../record-streams/), which is now complete and built as `liquers-records`. With
records in place, the user brief of 2026-10-10 reduced search to the three commands above.
Revision 8 merges that brief with the earlier analysis.

**Kept from revisions 1–7:**
- the non-evaluation invariant (conclusions 1 and 14 below);
- the key-prefix observation (conclusion 11);
- the `status` collision (conclusion 12);
- the syntax grammar, its literal-term and empty-expression rules, and the escaping notes;
- evidence as appended columns (`score` and `excerpt`), not a wrapper type;
- the ordering promise;
- the predicate as a syntax string, and the conditions under which `Value::Predicate` would earn
  its place;
- commands as a record source.

**Dropped as obsolete:**
- `AsyncStore::select`, `AssetManager::select`, the capability flag, conformance rules, router
  fan-out and push-down. Search is a command over records, so `STORE-NO-CONTENT-OR-METADATA-SEARCH`
  is closed as superseded. A store that later keeps metadata in a queryable backend could evaluate
  a predicate itself; that is an engine in `external-index-sync`'s sense.
- `Hit`, `ClauseMatch`, `RecordSet`, `Diagnostics`, the evidence bitmask and its 64-node cap.
- The roadmap's foundational decisions F1–F7, the Level 0/Level 1 discussion, and milestones M0,
  M5 and M6. `record-streams` delivered all of these.
- "Ranked full text forces an external engine". `search` ranks in-tree.
- The `get_asset_info` repair, which is already done (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`
  is closed).

**Moved to [`external-index-sync`](../external-index-sync/):**
- `interoperability-layer.md` and `indexation-policy.md`, whole;
- milestones M4 (per-document filters) and M7 (sinks);
- conclusions 5, 6, 7, 16, 17, 18 and 20;
- the revision-7 Phase 1 open questions 8, 9 and 18–21.

That design keeps anything held between queries: indexes, engines, vector stores, RAG and SQL
mirrors.

**Archived, so nothing is lost:** the revision-7 `DESIGN.md`, Phase 1, Phase 2 and `roadmap.md`
are at `specs/archive/2026-10-10-store-and-asset-search-rev7-*.md`.

## Amendment of 2026-10-11

Four changes came from user feedback:
1. **Nested field predicates.** `field:(…)` applies a predicate to one field's text. If the inner
   predicate has field tests, the text is parsed first: front-matter, then JSON, then YAML.
   `field.path:v` is shorthand for `field:(path:v)`.
2. **`Predicate` is a trait.** Parsing goes through `TryFrom<&str>`, via a `PredicateSyntax`
   sub-trait, so another query language can be plugged in. The built-in language keeps an AST
   enum with positions.
3. **A formal grammar and a `nom` parser** that reports a `Position`.
4. **Searchable command names.** In the `commands` schema, `realm`, `namespace`, `name` and
   `module` now have the text role.

### Second amendment, 2026-10-11: documents with front-matter

**Structure recognition is layered.** Plain YAML cannot be recognised from text: a YAML parser
reads `Note: remember the milk` as a map, `# Title` as a comment, and a bullet list as a sequence.
So:
- automatic recognition covers only JSON (verified by parsing) and front-matter (closed `---` YAML
  or `+++` TOML at the start);
- every other format is named explicitly (`yaml`, `json`, `frontmatter`, `fenced_yaml`,
  `fenced_json`).

*Simplified by the third amendment, below:* the syntax now recognises JSON only, and front-matter
is `field_extract`'s alone.

**`field_extract`** is the main route for document collections.
- Its `extension` argument is an enum (`md`, `json`, `yaml`, `yml`; default `md`). It both selects
  the files and fixes their format, so nothing is guessed.
- It turns each file's fields into columns. `prefix` defaults to none, and `flatten` turns nested
  maps into `parent_child` columns.
- Files with no fields are dropped.
- An extracted `status` or `title` replaces the catalog's own, which is what a search over issues
  wants.

Its result is an ordinary, cached asset: the query
`-R-key/specs/issues/-/ns-search/field_extract` is `specs/index.csv` as a table.

**Simple `ns-rec` filters** were designed here and then moved to
[`record-filter-commands`](../record-filter-commands/) on 2026-10-11, at the user's request.

**Design documents and phases.** Only `DESIGN.md` carries front-matter, with `phase:` and
`status:`. The phase documents have none. So:
- selecting designs by phase is a filter on `DESIGN.md`:
  `-R-key/specs/design/-/ns-search-rec/catalog/eq-filename-DESIGN.md/field_extract/eq-phase-architecture`;
- selecting one phase's documents is a filename filter:
  `…/catalog/startswith-filename-phase1`.

### Third amendment, 2026-10-11: what an agent asks

[`agent-queries.md`](./agent-queries.md) tests the design against three sets of questions:
those asked of this repository's `specs/`, those asked of an OpenViking-style agent memory, and the
tools a coding agent uses here. Every query in it was validated. The user accepted its proposals
and simplifications:

- **Regular expressions** (AC-28): `/re/` and `field:/re/`, using the `regex` crate, whose
  matching time is linear. `/` starts a regex only at the beginning of an atom or a value, and
  only when a closing `/` is followed by a delimiter, so `area:core/store` stays a plain value.
- **`md_split`** (AC-29): sections or lines as records. It is a proof of concept for a fuller
  Markdown tool, `NO-MARKDOWN-NAVIGATION-COMMANDS`.
- **Title fallback** (AC-30): the first `# ` heading becomes the title of plain Markdown.
- **`field_extract` reads a folder itself**; records given to it are taken as a catalog.
- **Simplified:**
  - nested predicates recognise JSON only;
  - the explicit formats are withdrawn (AC-23), since front-matter belongs to `field_extract`;
  - the trait drops `PredicateLanguage` until a dialect argument exists.
- **Filed as future extensions:**
  - `NO-MARKDOWN-NAVIGATION-COMMANDS`;
  - `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION`.

  Semantic search remains `external-index-sync`'s.

## Coverage check, 2026-10-11

Revision 8 was checked against every requirement, use case and invariant of revisions 1–7: the
archived Phase 1, Phase 2 and roadmap, plus `use-cases.md`, `research-questions.md` and
`options-analysis.md`. It was also checked against what `agent-memory-mvp` needs.

**Gaps found and closed by the amendment:**

| Gap | Old source | Closed by |
|---|---|---|
| No "N of M shown"; the limit cut results silently | A6, RQ §6.5, OA invariant 2 | AC-18 (`set_description` and an Info log entry) |
| No "why it matched" | A2, RQ §6.3, the revision-7 evidence | AC-19 (`matched` column) |
| Commands had no callable address, and names were not text | A4, A7, U6 | AC-13 (`query` column; name columns given the text role) |
| A document's `status` was unreachable (front-matter was a non-goal) | A3, roadmap §3 (72 → 42 → 23 hits) | AC-24 (`field_extract`, then `status:draft`) |
| An unbounded whole-store catalog | OA invariant 2 | AC-20 (`max_entries`) |
| No wasm commitment | S3, OA invariant 5 | AC-22 |
| Ranges, existence, prefix, one-of | U9, the revision-7 `FieldTest` | AC-7 (`f:>v`, `f:*`, `x*`, `\|`) |
| Order not promised without ranking | OA invariant 4 | AC-15 |
| Field qualification (`meta.`, `attr.`, `key.`) | revision-7 Phase 1 OQ 22 | Dotted paths (Phase 2 §Semantics); `attr.*` columns wait for `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` |
| The structured-predicate route over MCP | RQ §7 | Dropped deliberately: the syntax string is the transport form (`to_syntax`), and the MCP surface is `agent-memory-mvp`'s |
| The purity note in the registration guide | revision-7 Phase 2 docs | Added to `affects_docs` |

**Still not covered, deliberately:**
- "Why is this key not in my results?" (old Phase 1 OQ 21), beyond the unknown-field warning.
- Paging with `offset` (Phase 1 OQ 5).
- Recipe text as records (O3, O4, which were optional).
- An icon column (U1; the UI maps it from `type_identifier`).
- Everything listed as moved to `external-index-sync`.

**What agent memory needs.** `agent-memory-mvp` has only Phase 1 written, so it specifies no
recall interface of its own. Its needs are use cases A1–A8 and the archived roadmap §3, and every
one of them maps onto this design:

| Priority | Need | Served by |
|---|---|---|
| Must | Catalog a folder with recipe-declared keys, without evaluation | AC-1, AC-4 |
| Must | Key-prefix and glob filtering | AC-7 |
| Must | Free text with ranking | AC-6 |
| Must | Honest limits | AC-18 |
| Must | `title`, `description`, `excerpt` and `key` per hit | AC-6, AC-19 |
| Must | Command discovery | AC-13 |
| Must | Document status from front-matter | AC-17 |
| Should | Negation and OR, for "not closed" | AC-7 |
| Should | "Why matched" | AC-19 |
| Should | Excerpts without front-matter | AC-19 |
| Could | Date comparisons, such as `content.created:>2026-09` | AC-7 together with AC-17 |
| Could | A semantic clause | `external-index-sync` |

Its `ns-mem` namespace therefore needs no selection or search commands of its own: it builds
`ns-search` queries.

## Conclusions that still hold

Numbers are those of revision 7.

1. **A search never evaluates.**
   - `catalog` describes entries without starting them, and reads content only from stored bytes.
   - The only evaluation is of a key or query the caller hands to `search` explicitly.
2. **The store is not the whole corpus.** `catalog` lists through the asset manager, so keys that
   only a recipe declares are found too.
4. **wasm decides the engine question.** The search kernels live in `liquers-records`, which builds
   for wasm, so the browser build can search.
11. **Key-prefix filtering carries more weight than metadata filtering.** This was measured on this
    repository: 72 text hits, 42 within `specs/issues/`, 23 still open. Hence `key` with globs, and
    a folder-scoped `catalog`.
12. **`status` means two things.** The catalog's `status` column is the asset lifecycle.
    Front-matter fields will get an `attr.` prefix.
14. **The danger is starting an asset, not projecting one.** `get_asset_info` is the safe describe.

## Relationship to other work

- **`agent-memory-mvp`.** This design answers its open question 3 (how far an MVP goes on search):
  search is a set of commands, not something private to `ns-mem`.
- **`QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`**, filed 2026-10-10. Over HTTP a search expression has
  to be escaped into the path. Passing arguments by name is that issue, not this design.
- **`ns-rec/file_records`** overlaps with `catalog` (Phase 1, open question 2).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
- [Agent queries](./agent-queries.md): the questions an agent asks, how each is expressed, the
  gaps, and the proposed simplifications (2026-10-11)
- Background: [use cases](./use-cases.md), [research questions](./research-questions.md),
  [options analysis](./options-analysis.md)
- [`external-index-sync`](../external-index-sync/) and [`record-streams`](../record-streams/)
