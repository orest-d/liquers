# Phase 1: High-Level Design — Agent Memory Service (MVP)

## Feature Name

Agent memory service (MVP), revision 2 (2026-10-11; revision 1 summarized in `DESIGN.md`).
Companion analyses:
[survey](./memory-systems-survey.md) · [specification](./service-specification.md) ·
[workflow porting](./workflow-porting.md) · [tiers and storage](./tiers-and-storage.md) ·
[interfaces and MCP](./interfaces-and-mcp.md).

## Purpose

Give coding agents a memory they can browse in tiers, search, write safely and check — first
`specs/` and the skills, then any corpus — as Liquers commands behind an MCP server and a CLI. This
repository already runs such a memory by hand; the design turns its bookkeeping into commands and its
derived files into assets, and leaves judgement with the agent.

## Problem Example

An agent fixing a bug notices that Markdown sections cannot be addressed, and must check for an
existing issue before filing one (`DOCS_STRUCTURE_GUIDE.md` §4.8).

**Today:** it greps `specs/index.csv` for words it guesses, opens candidate issue files whole to
judge them, and, finding none, runs `python3 scripts/docs_index.py new "…" --area lib/commands`,
which repeats a word-overlap duplicate search, writes the template and regenerates the index.
Three tools, two of them hand-written Python, and several full documents read to make a yes/no
decision. An agent without a shell cannot do it at all.

**With the service:** one MCP call, `memory_new(collection="issues", title="Markdown sections cannot
be addressed", …)`, returns the candidates ranked by title — `NO-MARKDOWN-NAVIGATION-COMMANDS`
first, with its `title`, `description` and an excerpt — and writes nothing. The agent reads that one
issue's L1, decides it is a duplicate, and is done. The same check, as a query:

```
-R-key/specs/issues/-/ns-search/field_extract/search-markdown~.section-t-f-5-title
```

## Scope and Acceptance Criteria

- **AC-1** Tiered listing
  WHEN an agent calls `memory_list` on `specs/issues`
  THEN every entry carries `key`, `title` and `description` taken from its front-matter, and no
  document body is returned
- **AC-2** Ranked, honest search
  WHEN an agent calls `memory_search` with `text: expiration`, `filter: -status:closed*`,
  `path: specs/issues`, `limit: 5`
  THEN at most five hits return, each with `key`, `title`, `description`, `excerpt` and `matched`,
  plus `shown` and `total`
- **AC-3** Section read
  WHEN an agent calls `memory_read` with a key and `section: Problem`
  THEN only that section's text is returned, with the document's `version`
- **AC-4** Capture with duplicate check
  WHEN an agent calls `memory_new` for a collection with a title close to an existing memory's
  THEN the candidates are returned and nothing is written; with `force` the memory is created with a
  derived id, the collection's template, schema-valid fields and `status: draft`
- **AC-5** Schema refusal
  WHEN a write sets a field to a value outside the collection's vocabulary, or omits `title` or
  `description` on a Markdown memory
  THEN the write is refused with a message naming the field, and nothing changes
- **AC-6** Precondition
  WHEN `memory_write` or `memory_edit` carries an `expected_version` that is not the current one
  THEN the write is refused and the memory is unchanged
- **AC-7** Write log and stamps
  WHEN any write succeeds through the service
  THEN `updated` is stamped in the front-matter and one line — operation, key, versions, reason — is
  appended to the month's file under `_log/`
- **AC-8** Supersession
  WHEN a memory is written with `supersedes: <old key>`
  THEN the old memory gains `superseded_by` and is omitted from `memory_list` and `memory_search`
  unless `include_superseded` is set
- **AC-9** Check matches the oracle
  WHEN `liquers-mem check specs` runs on this repository and on a fixture with seeded violations
  THEN it reports the same error set (rule, file) as `scripts/docs_index.py --check` for every rule
  the schema expresses, and lists the rules it does not
- **AC-10** Index as an asset
  WHEN the `specs/index.csv` recipe is evaluated
  THEN its bytes equal `docs_index.py`'s output, and after one issue's front-matter changes the
  next evaluation recomputes it while an unchanged corpus does not
- **AC-11** Front-matter projection
  WHEN the assets API is asked for `key/info` of a Markdown memory with front-matter, then the file
  is edited outside the service and asked again
  THEN `title` and `description` equal the front-matter both times
- **AC-12** Search never evaluates
  WHEN a listed or searched folder contains a key only a recipe declares
  THEN it appears with status `Recipe` and its recipe is not run
- **AC-13** Read-only corpus
  WHEN the corpus is mounted read-only and an agent lists, reads and then tries to write it
  THEN the reads create no sidecar files and the write is refused
- **AC-14** MCP discovery and use
  WHEN an MCP client starts `liquers-mem mcp` over stdio and lists tools
  THEN the nine `memory_` tools of `interfaces-and-mcp.md` §4.1 are offered, each argument schema
  matching its command's arguments, and a `memory_search` call returns structured hits

Non-goals: identity, sessions and ACL (`CORE-SESSION-AND-KEY-ACL`); a language model inside the
service; semantic search (`external-index-sync`); the general Liquers MCP (constrained only so the
two combine, `interfaces-and-mcp.md` §6); git commits by the service; replacing `docs_index.py`
before the oracle tests pass.

## Core Interactions

- **Query:** no grammar change; tools build queries with `ActionRequest`.
- **Store:** a front-matter store adapter (`tiers-and-storage.md` §2 C); a read-only mount.
- **Commands:** new `ns-mem` (`describe`, `similar`, `new`, `write`, `edit`, `set_field`,
  `append_row`, `check`, `render_index`, `section`), built on `ns-search`.
- **Assets:** recipes for derived indexes and folder tiers, kept fresh by versions.
- **Web/API:** the existing assets API; MCP over Streamable HTTP beside it (last step).

## Crate Placement

`liquers-lib` (`src/mem/`, feature `mem` implying `records`): the `ns-mem` commands and the
front-matter store, beside `ns-search`. New crate `liquers-mcp` (depends on `liquers-core` only): the
MCP engine — tool sets declared as queries, schemas generated from `CommandMetadata`. New crate
`liquers-mem`: the binary (`mcp`, `serve`, `check`, `index`, `search`, `read`, `new`), depending on
`liquers-lib`, `liquers-axum` and `liquers-mcp`.

## Documentation Intent

- Reference: new `specs/reference/AGENT_MEMORY_SERVICE.md` — layout, document and schema format,
  tier contract, tool and CLI surface. No existing reference owns it.
- Guide: new skill `.claude/skills/liquers-memory/` for agents using the service, and a
  contributor section in `specs/guides/SEARCH_GUIDE.md` or a new guide if Phase 2 finds it large.
- Other documents: none.
- Documents to update: `DOCS_STRUCTURE_GUIDE.md` §4.8 (the fast path becomes the tool), `CLAUDE.md`
  if the service becomes the normal way to read `specs/`, `specs/README.md` capability map.

## Open Questions

1. **Tier storage** — front-matter projected into metadata by a store adapter (C), or front-matter
   reached only through `field_extract` records? *Recommended: C*, with records as the fallback
   (`tiers-and-storage.md` §3).
2. **Sequencing** — build on `store-and-asset-search` after it merges? *Recommended: yes*; Phase 2
   starts when it is on `main`.
3. **`STORE-NO-READ-ONLY-ADAPTER`** — fix it first as its own change, or wrap here? *Recommended:
   first*, it is generic and AC-13 depends on it.
4. **Where agent-written memory lives** — a separate writable mount outside `specs/` (committed or
   not is a deployment choice), with promotion into `specs/` by filing? *Recommended: yes.*
5. **Schema language** — a small own YAML (`service-specification.md` §2.3) or JSON Schema plus
   rule names? *Recommended: own YAML*, exported as JSON Schema for MCP.
6. **MVP oracle scope** — AC-10 regenerates `index.csv` only; `index.md` and README blocks follow.
   Agreed?
7. **Tool count** — nine tools, `memory_forget` deferred to Phase 2. Agreed?

## References

Source `AGENT-MEMORY-SERVICE`; prerequisites and gaps in `service-specification.md` §5; external
systems in the survey.
