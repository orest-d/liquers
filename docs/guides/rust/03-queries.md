---
title: Queries
status: outline
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md]
reviewed:
---
# 3. Queries

PURPOSE: The reader can read and write a query, and run one from Rust.

{{> ../_shared/queries.md}}

TBD: 3.1 Running a query: `evaluate` and `EnvRef::evaluate`; the returned state.
TBD: 3.2 Building a query in code: `parse_query`, `Query` and `Key`; encoding values with `encode_token`.
SOURCE: guides/QUERY_ESCAPING_GUIDE.md
TBD: Pipelines: the structured (JSON/YAML) form of a query; `QueryEditor` to build queries safely.
