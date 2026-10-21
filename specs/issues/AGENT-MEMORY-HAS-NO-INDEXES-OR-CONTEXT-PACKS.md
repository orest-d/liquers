---
id: AGENT-MEMORY-HAS-NO-INDEXES-OR-CONTEXT-PACKS
kind: feature
title: Agent memory derives nothing from its documents — no rendered templates, index assets or context packs
status: draft
priority: P3
complexity: L
area: [lib/commands, docs]
design: agent-memory-derived
created: 2026-10-21
github:
---

## Problem

`agent-memory-mvp` is complete and delivered the ten `ns-mem` commands and the `memory_` tool set,
but the features split from it into `design/agent-memory-derived/` have no source issue: the design
has `issues: []`, and its former source `AGENT-MEMORY-SERVICE` is closed. At commit 18732dd:

- `-mem-new` copies a collection template's body as it is and replaces only its first `# ` heading
  (`liquers-lib/src/mem/commands.rs` around line 284); placeholders are not rendered, although
  `liquers-template` exists (feature `templates`; `mem` does not depend on it).
- `specs/index.csv` and `specs/index.md` are files written by `scripts/docs_index.py`; no recipe
  produces them, and `check` names them in its `unsupported` note (`AGENT_MEMORY_SERVICE.md` §7).
- There is no `-mem-context` command and no `memory_context` tool (`liquers-agent/tools/memory.yaml`).

## Problem Example

An agent starting on `NO-MARKDOWN-NAVIGATION-COMMANDS` reads the issue whole, greps
`specs/index.csv` for related issues and opens reference documents one by one, tens of kilobytes
for a few kilobytes of context. Wanted: one `memory_context(key=…, budget=8000)` call returning a
budgeted Markdown pack (the issue's L1, matching reference sections, open issues and designs of its
area, and a tail of what was omitted), recomputed when an embedded document changes.

## Impact

Context assembly stays manual and expensive for every agent session; the generated indexes stay a
hand-run script with CI drift checks. Workaround: read whole files.

## Expected behaviour

The four scenarios of `design/agent-memory-derived/` Phase 1: templates rendered in `memory_new`,
`index.csv` and `index.md` as recipes whose bytes equal `docs_index.py`'s output, and context packs
as a template command served as `memory_context`.

## Discovery

Bulk design pass of 2026-10-20/21 (P-agent project plan): the sourceless draft
`agent-memory-derived` was assessed and found still wanted, so this issue is filed as its source.
The design's own `issues:` list is to be set to this ID when the design is next edited.
