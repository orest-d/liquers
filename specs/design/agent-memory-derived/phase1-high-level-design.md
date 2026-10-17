# Phase 1: High-Level Design — Agent memory: templates, indexes and context packs

## Purpose

What an agent memory derives from its documents: new memories rendered from a collection template,
indexes kept as assets rather than generated files, and **context packs** — one budgeted Markdown
document that assembles what an agent needs for a task. Split from
[`agent-memory-mvp`](../agent-memory-mvp/) on 2026-10-17 (its decision 12); it builds on that
design's collections, configuration and MCP server, and adds the features that wait on the template
language and on search results that recompute.

## Problem Example

An agent is about to work on `NO-MARKDOWN-NAVIGATION-COMMANDS`.

**Today:** it reads the issue whole, greps `specs/index.csv` for related issues by guessed words,
opens `specs/README.md` to find the reference documents, and opens each one to find the relevant
sections. Tens of kilobytes are read to collect a few kilobytes of context, and nothing records what
was assembled. `specs/index.md` and `specs/index.csv` are files that `scripts/docs_index.py`
regenerates by hand and CI checks for drift.

**With this design:** one MCP call, `memory_context(key="specs/issues/NO-MARKDOWN-NAVIGATION-COMMANDS.md",
budget=8000)`, returns one Markdown document: the issue's L1, the matching reference sections, the
open issues and designs of its area, the Rust items it names, and a tail listing what did not fit.
The pack is an asset rendered from a template, so editing any document it embeds recomputes it.
`index.md` and `index.csv` are recipes over small record commands and a page template, evaluated
when asked for and cached until a source changes.

## Scope and Acceptance Criteria

Former ids in `agent-memory-mvp` are given in brackets; those ids are retired there, not reused.

- **AC-1** Templates in `memory_new` [mvp AC-4, template step]
  WHEN an agent calls `memory_new` with `force` for a collection whose template has placeholders
  for the title, description, id and fields
  THEN the created memory is the template rendered with those values through `liquers-template`,
  and a template error names its position and writes nothing
- **AC-2** Index as an asset [mvp AC-10]
  WHEN the `specs/index.csv` recipe is evaluated twice with one issue's front-matter changed in
  between, and twice with nothing changed
  THEN each result's bytes equal `docs_index.py`'s output, and the asset's `version` changes in the
  first case and not in the second
- **AC-3** Context pack [mvp AC-21]
  WHEN an agent calls `memory_context` for an issue with a token budget
  THEN it receives one Markdown document with the issue's L1, the reference sections, open issues
  and designs matching its area and the symbols it names, within the budget, ending with what was
  omitted; evaluated again after one embedded document changes, the pack is recomputed
- **AC-4** `index.md` as a recipe [mvp AC-22]
  WHEN the `specs/index.md` recipe — active issues filtered, sorted, counted by priority and rendered
  through a template — is evaluated
  THEN its bytes equal `docs_index.py`'s output, and each step is a separate command usable on its own

Non-goals: everything `agent-memory-mvp` delivers (collections, tiers, writes, checks, the MCP
server); semantic search; a template editor UI (`NO-TEMPLATE-EDITOR-UI`); retiring `docs_index.py`
before AC-2 and AC-4 pass.

## Core Interactions

- **Commands:** templates through `liquers-template`'s `-template-render` and `-template-expand`.
  The context pack (AC-3) and the `index.md` page (AC-4) are **template commands**: aliases of
  `-template-render` whose head is the template link (`liquers-mustache` AC-19). Small record
  commands for indexes (`count_by`, `to_md_table`, `to_md_list`) beside `ns-rec`'s filters and
  `sort`; `ns-mem` gains `context`. Commands are named by `CommandKey` Display and take `cwd` as an
  explicit default-link argument (`COMMAND_REGISTRATION_GUIDE.md`).
- **Assets:** indexes and packs are recipes; recomputation relies on recorded dependencies.
- **MCP:** a tenth tool, `memory_context`, in the memory tool set of `agent-memory-mvp`.

## Crate Placement

`liquers-lib` (`src/mem/`, beside the `ns-mem` core), with the `templates` feature (crate
`liquers-template`) and `records`. Rust item records (`syn`), if not delivered by their own design,
go in `liquers-lib` behind a feature of their own.

## Documentation Intent

- Reference: extend `specs/reference/AGENT_MEMORY_SERVICE.md` (created by `agent-memory-mvp`) with
  templates, indexes and packs.
- Guide: extend the `liquers-memory` skill with context packs.
- Other documents: none.
- Documents to update: `DOCS_STRUCTURE_GUIDE.md` (indexes become assets, once AC-2 and AC-4 pass).

## Design Dependencies

- **Requires** [`agent-memory-mvp`](../agent-memory-mvp/) (core, configuration, MCP),
  [`liquers-mustache`](../liquers-mustache/) (Phases 1–2 approved 2026-10-17; AC-1, AC-3, AC-4),
  `record-filter-commands` (complete; ordering for AC-2, AC-4),
  `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` (P2 M, filed 2026-10-17: the recompute and
  version clauses of AC-2, AC-3 and AC-4 fail until it is fixed), Rust item records
  (`NO-SOURCE-CODE-AND-API-DOC-NAVIGATION`, first half; AC-3), Markdown sections
  (`NO-MARKDOWN-NAVIGATION-COMMANDS` minimum; AC-3).
- **Overlaps** `SIMPLE-TEMPLATE-HAS-NO-COMMAND` (moved into `liquers-mustache`),
  `RECIPE-OVERRIDE-ONLY-LAST-ACTION` (a structured placeholder in a non-final action of a pack's
  queries).

## Open Questions

1. **Interim template engine.** Recommended: dropped. `agent-memory-mvp` decision 7 planned an
   existing Mustache engine plus an `expand` pass in case the template track lagged; with
   `liquers-template` approved and this design now after it, templates use it directly.
2. **Rust item records.** Recommended: their own compact design under
   `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` (execution plan step 3.6), required here, rather than a
   part of this design.
3. **The search-dependency fix.** Recommended: fixed as its own issue before this design's Phase 4,
   since it affects every cached search result, not only memory.

## References

`agent-memory-mvp` Phase 1 (decisions 6, 7, 10, 12), `template-language-evaluation.md`,
`brainstorm-workflow-improvements.md` (context packs), `execution-plan.md` (memory D, E, F).
