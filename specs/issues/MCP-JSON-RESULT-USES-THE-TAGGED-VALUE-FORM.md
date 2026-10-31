---
id: MCP-JSON-RESULT-USES-THE-TAGGED-VALUE-FORM
kind: issue
title: A JSON tool result from liquers-mcp is the value's tagged serde form, not plain JSON
status: draft
priority: P2
complexity: S
area: [mcp, lib/value]
design:
created: 2026-10-31
github:
---

## Problem

`memory_new`, `memory_write` and `memory_edit` are documented to return JSON such as
`{written: true, key, version}` (`specs/reference/AGENT_MEMORY_SERVICE.md` §4). The commands build
exactly that (`json(...)` in `liquers-lib/src/mem/commands.rs`, via `Value::try_from_json_value`),
but the MCP result an agent receives is:

```json
{"Object":{"value":{"key":{"Text":{"value":"notes/x.md"}},"version":{"Text":{"value":"bf3e…"}},"written":{"Bool":{"value":true}}}}}
```

`SimpleValue::as_bytes("json")` (`liquers-lib/src/value/simple.rs`, the `"json"` arm) writes
scalars plainly but sends `Object` and `Array` to `serde_json::to_vec(&self)`, the externally
tagged enum form; `liquers_core::value::Value` does the same. `liquers-mcp`'s
`ToolOutput::structured` parses that text, so `structuredContent` is tagged as well.

Record views (`memory_list`, `memory_search` with `format: json`) are not affected: they serialize
as plain rows.

## Impact

Every agent that writes through the memory gets a result it has to unwrap two levels deep to find
`key` and `version`, and the `version` it needs for the next `expected_version` is buried in it
(it also arrives as a note, which is the practical workaround). The tests in
`liquers-agent/tests/` only check that the result text *contains* the key, so they pass.

## Expected behaviour

A JSON tool result is the plain JSON of the value (`{"key": …, "version": …, "written": true}`).
Either the `json` format of `Object`/`Array` becomes plain JSON (and the read side keeps
dispatching on the type identifier, which `json_payload` already does), or `liquers-mcp` converts
a value with `try_into_json_value` before serializing. Add an assertion on the parsed shape to
`liquers-agent/tests/mcp_tools.rs`.

## Discovery

Found 2026-10-31 while smoke-testing `liquers-agent mcp` with `memory/liquers-specs.yaml` over
stdio JSON-RPC: `tools/call memory_new` on collection `notes` returned the tagged form above.
