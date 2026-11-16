---
title: Glossary
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/PIPELINES.md, reference/RECORD_STREAMS.md, guides/PYTHON_GUIDE.md]
reviewed: 2026-11-15
---
# C. Glossary

PURPOSE: The words of Liquers, defined once.

## C.1 Terms

{{$ -R/specs/reference/PROJECT_OVERVIEW.md/-/ns-docs/include-Glossary}}

## C.2 The terms in Python

Each name is reachable as `lq.Name` and from the module named beside it (`liquers.query.Query`).

| Term | In Python | Module |
|---|---|---|
| Query | text, or `lq.Query` (from `lq.parse`) | `liquers.query` |
| Key | text, or `lq.Key` (from `lq.parse_key`) | `liquers.query` |
| Action | `lq.ActionRequest` | `liquers.query` |
| Pipeline | a list of steps, or `lq.Pipeline` | `liquers.pipeline` |
| Command | a function decorated with `@lq.command` or `@lq.first_command`; described by `lq.CommandMetadata` | `liquers.commands` |
| First command | a function decorated with `@lq.first_command` | `liquers.commands` |
| State | `lq.State`, with `.value` and `.metadata` | `liquers.assets` |
| Metadata | `lq.Metadata` | `liquers.metadata` |
| Asset | `lq.AssetRef`, from `env.get_asset(query)` | `liquers.assets` |
| Status | `lq.Status` | `liquers.metadata` |
| Recipe | `lq.Recipe`; the recipes of an environment are `env.recipes` | `liquers.recipes` |
| Store | `lq.Store`; the store of an environment is `env.store` | `liquers.store` |
| Environment | `lq.Environment`; the shared one is `lq.default_environment()` | `liquers.environment` |
| Context | `lq.Context`, received by a command parameter named `context` | `liquers.environment` |
| Record stream | `lq.RecordView` (a table) and `lq.RecordSource` (a table in chunks) | `liquers.records` |
| Volatile | `volatile=True` on a decorator or an `lq.Recipe` | |

## C.3 Terms this guide adds

| Term | Definition |
|---|---|
| **Argument** | A value given to a command in a query, after the command's name |
| **Chunk** | One piece of a record source: a record view that is the result of one query |
| **Data node** | A read-only view that reads a value field by field (`lq.DataNode`, in `liquers.data_nodes`) |
| **Dependency** | Something a result was made from: a key, another query, a command |
| **Extra** | A group of optional Python packages installed with `liquers-py[name]` |
| **Link** | An argument that is the value of another query |
| **Manifest** | A file `<name>.manifest.yaml` that describes a record source: its chunks and their schema |
| **Mount** | One entry of the store configuration: a type of storage and the prefix of the keys it holds |
| **Opaque object** | A Python object Liquers has no type for; it is passed along unchanged |
| **Query editor** | The object that builds or changes a query step by step, placing and escaping the arguments |
| **Record source** | A table that is read in chunks, any number of times; it may be larger than memory |
| **Record stream** | One pass over a record source, chunk by chunk |
| **Record view** | A table in memory, with named, typed columns |
| **Record** | One row of a table: a set of named, typed values |
| **Schema** | The list of the fields of a table: their names, types and roles |
| **Template command** | A command whose result is a rendered template |
| **Template** | Text with tags that are replaced when it is rendered; a query tag holds a query |
