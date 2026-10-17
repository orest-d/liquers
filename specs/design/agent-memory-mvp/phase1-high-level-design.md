# Phase 1: High-Level Design — Agent Memory Service (MVP)

## Feature Name

Agent memory service (MVP), revision 4 (2026-10-17: reconciled with the merged `store-adapters` and
the approved Phases 1–2 of `liquers-mustache`; earlier revisions in `DESIGN.md`).
Companion analyses: [survey](./memory-systems-survey.md) ·
[specification](./service-specification.md) · [workflow porting](./workflow-porting.md) ·
[tiers and storage](./tiers-and-storage.md) · [interfaces and MCP](./interfaces-and-mcp.md) ·
[brainstorm: workflow improvements](./brainstorm-workflow-improvements.md) ·
[template language evaluation](./template-language-evaluation.md).

## Purpose

An MVP of a **competitive agent memory system**: a memory agents browse in tiers, search, write
safely, check, and draw assembled context packs from — served as Liquers commands behind an MCP
server and a CLI. It is **configurable**: one configuration document (stores, collections, tiers,
views, tools) shapes it like this repository's manual system over the unchanged `specs/`, or like
OpenViking's context database, on local directories or remote stores such as S3. The bookkeeping
becomes commands, derived files become assets, judgement stays with the agent.

## Problem Example

An agent fixing a bug notices that Markdown sections cannot be addressed, and must check for an
existing issue before filing one (`DOCS_STRUCTURE_GUIDE.md` §4.8).

**Today:** it greps `specs/index.csv` for words it guesses, opens candidate issue files whole to
judge them, and, finding none, runs `python3 scripts/docs_index.py new "…" --area lib/commands`,
which repeats a word-overlap duplicate search, writes the template and regenerates the index.
Three tools, two of them hand-written Python, and several full documents read to make a yes/no
decision. An agent without a shell cannot do it at all.

**With the service:** one MCP call, `memory_new(collection="issues", title="Markdown sections cannot
be addressed", …)`, returns ranked candidates — first among them the existing
`NO-MARKDOWN-NAVIGATION-COMMANDS` — each with `title`, `description` and an excerpt, and writes
nothing. The agent reads one L1, sees a duplicate, and is done. The same check as a query:

```
-R-key/specs/issues/-/ns-search/field_extract/search-markdown~.section-t-f-5-title
```

## Scope and Acceptance Criteria

The configuration profiles are sketched in the brainstorm §2. Tool keys are relative to the memory
root. Collections under `specs/` are writable through the
service only, except `archive`, which is mounted read-only; agent notes live in a separate mount.

- **AC-1** Tiered listing
  WHEN an agent calls `memory_list` on `specs/issues`
  THEN every entry carries `key`, `title` and `description` — from front-matter, or from the
  collection's declared fallback where front-matter has none — and no document body is returned
- **AC-2** Ranked, honest search
  WHEN an agent calls `memory_search` with `text: expiration`, `filter: -status:closed*`,
  `path: specs/issues`, `limit: 5`
  THEN at most five hits return in descending `score`, none with a `closed*` status, each with
  `key`, `title`, `description`, `excerpt` and `matched`, plus `shown` ≤ `total`
- **AC-3** Section read
  WHEN an agent calls `memory_read` with a key and `section: Problem`
  THEN only that section's text is returned, with the document's `version`
- **AC-4** Capture with duplicate check
  WHEN an agent calls `memory_new` for a collection with a title close to an existing memory's
  THEN the candidates are returned and nothing is written; with `force` the memory is created with a
  derived id, the collection's template, schema-valid fields and `status: draft`
- **AC-5** Schema refusal
  WHEN a write sets a field to a value outside the collection's vocabulary, or creates a Markdown
  memory without `title` or `description`
  THEN the write is refused with a message naming the field, and nothing changes
- **AC-6** Precondition
  WHEN `memory_write` or `memory_edit` carries an `expected_version` that is not the current one
  THEN the write is refused and the memory is unchanged
- **AC-7** Write log and stamps
  WHEN any write succeeds through the service
  THEN `updated` (and `created`, on creation) is stamped in the front-matter, and one line —
  operation, key, versions, reason — is appended to the month's file under `_log/`
- **AC-8** Supersession
  WHEN a memory is written with `supersedes: <old key>`
  THEN the old memory gains `superseded_by` and is omitted from `memory_list` and `memory_search`
  unless `include_superseded` is set
- **AC-9** Check matches the oracle
  WHEN `liquers-agent check specs` runs on this repository and on a fixture with seeded violations
  THEN it reports the same error set (rule, file) as `scripts/docs_index.py --check` for every rule
  the schema expresses, and lists the rules it does not
- **AC-10** Index as an asset
  WHEN the `specs/index.csv` recipe is evaluated twice with one issue's front-matter changed in
  between, and twice with nothing changed
  THEN each result's bytes equal `docs_index.py`'s output, and the asset's `version` changes in the
  first case and not in the second
- **AC-11** Front-matter projection
  WHEN `key/info` is asked for a Markdown memory, the file is edited outside the service, and
  `key/info` is asked again — with the asset loaded in memory and without
  THEN `title` and `description` equal the file's front-matter every time
- **AC-12** Search never evaluates
  WHEN a listed or searched folder contains a key only a recipe declares
  THEN it appears with status `Recipe` and its recipe is not run
- **AC-13** Read-only mount
  WHEN an agent lists and reads `specs/archive`, then tries to write into it
  THEN the reads create no sidecar files and the write is refused
- **AC-14** MCP discovery and use
  WHEN an MCP client starts `liquers-agent mcp` over stdio and lists tools
  THEN the `memory_` tools of `interfaces-and-mcp.md` §4.1 are offered with argument schemas
  as tabulated there, and a `memory_search` call returns structured hits
- **AC-15** Orientation
  WHEN an agent calls `memory_overview`, then `memory_describe` for `issues`
  THEN it receives the memory's L0/L1, its collections with counts and the latest log lines; then
  the collection's fields, vocabularies and template
- **AC-16** Edits
  WHEN an agent replaces a unique string, appends a `## History` row, or sets `status` along an
  allowed transition — and then sets it along a forbidden one
  THEN the first three succeed (AC-7 applies) and the last is refused, naming the transition
- **AC-17** Fallback tiers
  WHEN a Markdown memory has no front-matter, and a folder has a `README.md` with front-matter
  THEN the memory's `title` is its first heading and `description` its first paragraph, and the
  folder's tiers are the README's

- **AC-18** The "liquers-specs" profile
  WHEN the service starts with a configuration mounting this repository's `specs/` and skills
  unchanged
  THEN AC-1 to AC-17 hold without moving or rewriting any file
- **AC-19** An OpenViking-shaped profile
  WHEN the service starts with a configuration declaring category collections (profile,
  preferences, entities, events) and folder overviews, and no code change
  THEN `memory_overview`, `memory_list`, `memory_new` and `memory_search` work over that layout
- **AC-20** A remote mount
  WHEN a collection is mounted from an S3-compatible store (OpenDAL; an in-process or local
  S3-compatible endpoint in tests)
  THEN writes and reads through the tools behave as on a local mount, and the memory survives a
  restart of the service
- **AC-21** Context pack
  WHEN an agent calls `memory_context` for an issue with a token budget
  THEN it receives one Markdown document with the issue's L1, the reference sections, open issues
  and designs matching its area and the symbols it names, within the budget, ending with what was
  omitted; evaluated again after one embedded document changes, the pack is recomputed
- **AC-22** `index.md` as a recipe
  WHEN the `specs/index.md` recipe — active issues filtered, sorted, counted by priority and rendered
  through a template — is evaluated
  THEN its bytes equal `docs_index.py`'s output, and each step is a separate command usable on its own

Non-goals: identity, sessions and ACL (`CORE-SESSION-AND-KEY-ACL`); a language model inside the
service; semantic search and vector indexes (brainstorm §5.3); `memory_forget` and the archive convention (NEXT); the general Liquers
MCP (only kept combinable, `interfaces-and-mcp.md` §6); git commits by the service; retiring
`docs_index.py` before AC-9 and AC-10 pass.

## Core Interactions

- **Query:** no grammar change.
- **Store:** no new store code. Mounts use what `store-adapters` shipped: `read_only: true`
  (AC-13), and a `metadata` section whose read and write chains include `-store_config-front_matter`
  and `-store_config-front_matter_sync` (AC-11; `liquers-lib`, feature `records`). Write-back
  happens in `set_metadata` only; in `set` the written bytes are the truth, which suits writes that
  carry a whole document.
- **Commands:** new `ns-mem` (`describe`, `similar`, `new`, `write`, `edit`, `set_field`,
  `append_row`, `check`, `section`, `context`), built on `ns-search`; small record commands for
  indexes (`count_by`, `to_md_table`, `to_md_list`) beside `ns-rec`'s filters and `sort` (on
  `main`); templates through `liquers-template`'s `-template-render` and `-template-expand`. A
  context pack (AC-21) and the `index.md` page (AC-22) are **template commands**: aliases of
  `-template-render` whose head is the template link, as `liquers-mustache` AC-19 specifies.
  Commands are named by `CommandKey` Display; a command that resolves relative keys takes `cwd` as
  an explicit default-link argument, never from `Context` (`COMMAND_REGISTRATION_GUIDE.md`).
- **Configuration:** the existing store-router configuration with its `metadata` sections,
  extended with collections, tiers, views and tool sets.
- **Assets:** writes go through the asset manager; recipes for indexes and folder tiers.
- **Web/API:** the existing assets API; MCP tool arguments map onto argument overrides; MCP over
  HTTP is NEXT (question 5).

## Crate Placement

`liquers-lib` (`src/mem/`, feature `mem` implying `records`): `ns-mem` and the front-matter store,
beside `ns-search`; it uses `liquers-lib`'s `templates` feature (crate `liquers-template`). New `liquers-mcp` (depends on `liquers-core`): the MCP engine. New
`liquers-agent` (named 2026-10-15, replacing the working name `liquers-mem`; named for its
audience so it can later carry the workbench tool sets of `AGENT-WORKBENCH-SURFACE` without a
rename): the binary, over `liquers-lib`, `liquers-axum` and `liquers-mcp`.

## Documentation Intent

- Reference: new `specs/reference/AGENT_MEMORY_SERVICE.md` — layout, document and schema format,
  tiers, tools and CLI. No existing reference owns it.
- Guide: new skill `.claude/skills/liquers-memory/` for agents using the service.
- Other documents: none.
- Documents to update: `CLAUDE.md` project structure and dependency flow (two crates);
  `DOCS_STRUCTURE_GUIDE.md` §3 (an `area` for MCP) and §4.8 (the fast path becomes the tool);
  `specs/README.md` capability map.

## Design Dependencies

- **Requires** `store-and-asset-search` (merged; `ns-search`, `SEARCH_GUIDE.md`),
  `axum-query-named-arguments` (merged; argument overrides), `store-adapters` (**complete**, PR #137:
  read-only mount AC-13, `front_matter` handlers AC-11), `record-filter-commands` (**complete**),
  `liquers-mustache` (Phases 1–2 approved, PR #139; templates for AC-4, AC-21, AC-22), and
  **`SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`** (filed 2026-10-17, P2 M): `ns-search`
  results record no dependency on the files they read, so the "recomputed" and "version changes"
  clauses of AC-10, AC-21 and AC-22 fail until it is fixed. It must be fixed before memory E and F.
- **Overlaps** `STORE-METADATA-HANDLING-NOT-COMPOSABLE` (decision 4; the front-matter adapter is one
  instance), `NO-MARKDOWN-NAVIGATION-COMMANDS` (AC-3, AC-17, AC-21),
  `SIMPLE-TEMPLATE-HAS-NO-COMMAND` (AC-21), `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` (question 6),
  `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (`front_matter` projects only `title` and
  `description`; other fields are read as `field_extract` records),
  `RECIPE-OVERRIDE-ONLY-LAST-ACTION` (an MCP tool's arguments override the last action of its
  declared query only, so each tool's query ends in the command that takes them), `STORE-WRITE-HAS-NO-PRECONDITION` (AC-6 is
  single-process until it is fixed).
- **Feeds** `external-index-sync`, where semantic memory search belongs.

## Open Questions

Answered by the user on 2026-10-12; the answers are decisions of this phase.

1. ~~*Sequencing.*~~ Resolved: `store-and-asset-search` is merged.
2. **`STORE-NO-READ-ONLY-ADAPTER` is fixed first** — done: `ReadOnlyStore` and `read_only: true`
   shipped with [`store-adapters`](../store-adapters/) (PR #137, merged 2026-10-17).
3. **L1 for the existing corpus:** per-collection fallbacks now (AC-1, AC-17); descriptions are
   backfilled by agents over time.
4. **Tiers:** a store adapter projects front-matter into metadata, and setting `title` /
   `description` through metadata rewrites the front-matter. This adapter and the read-only one are
   instances of a general, configurable metadata pipeline (create, enhance, store, policy), filed as
   `STORE-METADATA-HANDLING-NOT-COMPOSABLE` and designed in [`store-adapters`](../store-adapters/)
   (2026-10-13). It shipped: the front-matter adapter is the `front_matter` / `front_matter_sync`
   handler pair in `liquers-lib`, so this design configures it rather than building it.
5. **HTTP MCP (`serve`)** is NEXT; stdio MCP and the CLI first.
6. **Rust item records** (`syn`) are in the MVP; references and call graphs (SCIP) later.
7. **Templates must understand queries** (parse errors at template positions; evaluation through the
   asset layer), so the long-term language will be reimplemented. Recommended in
   [`template-language-evaluation.md`](./template-language-evaluation.md): reimplement **Mustache with
   query tags** (logic-less like Liquers, small specified language with a conformance suite, partials
   as the built-in extension point). Since 2026-10-13 the language is its own design,
   [`liquers-mustache`](../liquers-mustache/) (`NO-QUERY-AWARE-TEMPLATE-LANGUAGE`), whose Phases 1–2
   were approved on 2026-10-17: crate `liquers-template`, namespace `template`, hygienic `{{$ q}}`
   (fills the query AST) and classic `{{$$ q}}` (expands text, then parses), `{{$. q}}` to apply a
   query to the current context. **To confirm at this gate:** memory templates use that language
   directly, and the interim (an existing Mustache engine plus an `expand` pass) is dropped, since
   the template track no longer lags the memory core.
8. **Agent-written memory** is a separate writable mount — committed `notes/` by default, S3 when the
   network policy allows. A **separate repository** works today as a second clone mounted with the
   `filesystem` type (in a cloud session: added to the session and cloned; git push by the agent).
   OpenDAL has no generic git service; its `github` service reads and writes through the GitHub
   Contents API (one commit per write; needs API access, which this container's policy blocks). A git
   store over a local clone (commit on write) would be a new backend — NEXT.
9. **Schemas** are a small YAML format of our own; the tool converts between formats (YAML, JSON,
   TOML, JSON Schema for MCP) itself.
10. **Indexes are recipes over separate, simple commands** — filtering, sorting, counting by a field,
    rendering records as a Markdown table — and a template for the page. Both `index.csv` (AC-10) and
    `index.md` (AC-22) are in scope.
11. **Ten tools**, `memory_forget` later.
12. **Splitting — open, to decide at this gate.** Recommended: two designs, cut by what they wait
    on. This design keeps the memory core, the MCP server, tiers and profiles (execution plan
    memory A, B, C; AC-1 to AC-3, AC-5 to AC-9, AC-11 to AC-20, and AC-4 without the template
    step), all of whose inputs are now on `main`. A new design, `agent-memory-derived`, takes
    templates in `memory_new`, the indexes and context packs (memory D, E, F; AC-4's template step,
    AC-10, AC-21, AC-22), which wait on `liquers-template`, Markdown sections, Rust item records and
    `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`. Memory C no longer needs a design of its
    own, because the front-matter adapter it waited for has shipped.

## References

Source `AGENT-MEMORY-SERVICE`; prerequisites in `service-specification.md` §5; systems in the survey.
