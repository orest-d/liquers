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

## Problem Example

A release renames the `issues` collection's status value `in_progress` to `active` and makes
`area` required. Today the maintainer edits `_schema/memory.yaml` by hand, runs

```bash
cargo run -p liquers-agent -- --config memory/liquers-specs.yaml check specs/issues
```

reads one `enum` violation per affected issue and one `required` violation per issue without
`area`, and fixes each with `memory_edit` (`field: status, value: active`) or a script that
bypasses the write path's stamping and log. Instead, `liquers-agent migrate --dry-run` should list
every planned edit, and `liquers-agent migrate` should apply them through `commit` (one log line
each), bump `version`, and end with a clean `check`.

Re-checked at commit 18732dd (2026-10-21): `liquers-agent/src/main.rs` `enum Command` has `Mcp`,
`Check`, `Search`, `Read`, `Query` only; no `migrate` in `liquers-lib/src/mem/`; the migration
guide still lists it as future work (`guides/AGENT_MEMORY_MIGRATION_GUIDE.md`, line 91). The
`design: agent-memory-mvp` link points at a completed design; the next design replaces it.

## Expected behaviour

`liquers-agent migrate` applies a declared set of steps (rename a field, map a status value, fill a
default) to every document, through the normal write path so each change is stamped and logged,
then bumps `memory.yaml`'s `version`, and ends with a clean `check`. A dry run lists the edits.

## Discovery

Recorded 2026-10-17 while drafting the agent memory migration guide; NEXT for the MVP.
