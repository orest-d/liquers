---
id: AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND
kind: feature
title: Agent memory schema changes are applied by hand, with no migrate command
status: draft
priority: P3
complexity: M
area: [lib/commands]
design: agent-memory-mvp
created: 2026-10-17
github:
---
# Agent memory schema changes are applied by hand, with no migrate command

## Problem

`agent-memory-mvp` keeps migration small: documents are `Source` assets with front-matter and no
stored metadata, `ns-mem` results are never stored, and recipe results recompute. What remains is a
change to the memory's own rules — a new `memory.yaml` `version`, a renamed field or status, a new
required field. The migration guide (`design/agent-memory-mvp/migration-guide-draft.md` §3.3–3.4)
has the maintainer edit `memory.yaml` from the release notes, then fix each violation that
`liquers-agent check` reports, one `memory_edit` at a time or with a script.

## Expected behaviour

`liquers-agent migrate` applies a declared set of steps (rename a field, map a status value, fill a
default) to every document, through the normal write path so each change is stamped and logged,
then bumps `memory.yaml`'s `version`, and ends with a clean `check`. A dry run lists the edits.

## Discovery

Recorded 2026-10-17 while drafting the agent memory migration guide; NEXT for the MVP.
