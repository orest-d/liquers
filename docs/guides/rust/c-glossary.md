---
title: Glossary
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# C. Glossary

PURPOSE: The words of Liquers, defined once.

## C.1 Terms

{{$ -R/specs/reference/PROJECT_OVERVIEW.md/-/ns-docs/include-Glossary}}

## C.2 The terms in Rust

| Term | In Rust |
|---|---|
| Query | `liquers_core::query::Query`, from `parse_query` |
| Key | `liquers_core::query::Key`, from `parse_key` |
| Action | `liquers_core::query::ActionRequest` |
| Pipeline | `liquers_core::pipeline::Pipeline` |
| Plan | `liquers_core::plan::Plan`, from `PlanBuilder` |
| Command | a function registered with `register_command!`; described by `CommandMetadata` |
| Command registry | `CommandRegistry<E>`, the field `command_registry` of the builder |
| Value | `liquers_lib::value::Value`; the traits `ValueInterface` and `ExtValueInterface` |
| State | `liquers_core::state::State<Value>` |
| Metadata | `liquers_core::metadata::Metadata`; its structured form is `MetadataRecord` |
| Asset | `AssetRef<E>`, from `env.evaluate(query)` |
| Asset manager | `env.get_asset_manager()`; the trait `AssetManager` |
| Status | `liquers_core::metadata::Status` |
| Recipe | `liquers_core::recipes::Recipe`; providers implement `AsyncRecipeProvider` |
| Store | `Arc<dyn AsyncStore>`; the store of an environment is `env.get_async_store()` |
| Environment | `EnvRef<DefaultEnvironment<Value>>`, built by an `EnvironmentBuilder` |
| Context | `liquers_core::context::Context<E>`, the `context` parameter of a command |
| Record stream | `Arc<dyn RecordView>` (a table) and `Arc<dyn RecordSource>` (a table in chunks), in `liquers-records` |
| Error | `liquers_core::error::Error` with an `ErrorType` |
| Volatile | `volatile: true` in `register_command!`, or `volatile` in a recipe |

## C.3 Terms this guide adds

| Term | Definition |
|---|---|
| **Argument** | A value given to a command in a query, after the command's name |
| **Builder** | The object on which an environment is configured before `build()` |
| **Command metadata** | The description of a command: its name, arguments, defaults and documentation, without the function |
| **Data format** | The short name of the byte form of a value, such as `csv` or `png` |
| **Data node** | A read-only view that reads a value field by field |
| **Dependency** | Something a result was made from: a key, another query, a command |
| **Feature** | An optional part of a crate, chosen when the program is compiled |
| **Kind** | The execution model of the asset manager: `Queued` or `Inline` |
| **Link** | An argument that is the value of another query |
| **Mount** | One entry of the store configuration: a type of storage and the prefix of the keys it holds |
| **Payload** | A value that travels with one evaluation, used by user interfaces |
| **Pipeline command** | A command defined as a pipeline, with placeholders as its arguments |
| **Placeholder** | A named gap in a pipeline, filled by an argument |
| **Record view** | A table with named, typed columns |
| **Store factory** | The object that builds stores from a configuration |
| **Template** | Text with tags that are replaced when it is rendered; a query tag holds a query |
| **Template command** | A command whose result is a rendered template |
| **Type identifier** | The name of a value type in metadata and in the store, such as `Text` or `polars.DataFrame` |
