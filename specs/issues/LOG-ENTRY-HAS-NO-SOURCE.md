---
id: LOG-ENTRY-HAS-NO-SOURCE
kind: issue
title: A state's log does not record whether an entry came from the command or from the engine
status: draft
priority: P3
complexity: S
area: [core/value, core/plan]
design: agent-memory-mvp
created: 2026-10-19
github:
---
# A state's log does not record whether an entry came from the command or from the engine

## Problem

`Context::info` and the planner's own diagnostics (`Step::Info`: "Dependency detected: …",
"Recipe set CWD to …", "Predecessor boundary not cut …") land in the same `MetadataRecord::log`
as `LogEntryKind::Info` entries with nothing to tell them apart. `LogEntry` has no field naming
its source.

Found while building `liquers-mcp`, which returns a command's notes to the agent as a separate
content block: with every Info entry included, an agent reading one document got four lines of
dependency diagnostics before the command's own note.

## Impact

- `liquers-mcp` recognises engine lines by their text (`ENGINE_NOTE_PREFIXES` in
  `liquers-mcp/src/server.rs`). A new planner message, or a reworded one, leaks into tool
  results until the list is updated.
- Any other consumer wanting "what the command said" (a UI, the CLI) has the same problem.

## Expected behaviour

A `LogEntry` carries its origin (command, plan, interpreter, store), or engine diagnostics use
`LogEntryKind::Debug`, so a consumer can select the command's own notes without matching text.
