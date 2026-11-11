---
title: Query syntax
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/RECIPES_AND_PLANS.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# A. Query syntax

PURPOSE: A reference for the query notation, the same in every guide.

This appendix collects the notation of queries in one place. Chapter 3 introduces it step by
step.

{{> ../_shared/query-syntax.md}}

## A.6 The syntax in Rust

Each part of the notation has a type in `liquers_core::query`:

| Notation | Type | Made by |
|---|---|---|
| a whole query | `Query` | `parse_query(text)?` |
| `-R/…` up to `/-/` | `ResourceQuerySegment`, with a `Key` | `Query::from(key)` |
| a chain of actions | `TransformQuerySegment` | |
| the header `-R`, `-`, with a name and parameters | `SegmentHeader` | |
| one action, `name-arg-arg` | `ActionRequest` | `ActionRequest::new(name).with_parameters(…)` |
| one argument | `ActionParameter`: text, or a link | `ActionParameter::new_string(text)` |
| a key | `Key`, a list of `ResourceName` | `parse_key(text)?` |
| the file name at the end | `ResourceName` | |

`encode()` on any of them gives the text back, with every escape applied. All of them implement
`serde::Serialize` and `Deserialize`, `Clone`, `PartialEq` and `Hash`.

A pipeline is a query written as data (section 3.10). It is the type
`liquers_core::pipeline::Pipeline`, and each of its steps is one of:

| Step, in JSON or YAML | Meaning |
|---|---|
| a string | query text, one or more actions or a resource segment |
| `{"action": name, "ns": namespace, "arguments": {name: value}}` | an action with arguments by name; a value can be any JSON-like data |
| `{"action": name, "parameters": [value, …]}` | an action with arguments by position |
| `{"placeholder": name}` in place of a value | an argument of a pipeline command (section 5.9) |

EDITOR: still to be written: realms. The name in a transform segment's header (`-<realm>/…`) selects the realm in which the commands are looked up; `realm:` in `register_command!` places a command in one.
