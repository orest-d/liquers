# Liquers

Liquers names computations with queries. A query such as `-R/data/sales.csv/-/ns-pl/head-10` reads
a file and applies commands to it; the result is cached as an *asset*, and the same query always
names the same result. Commands are ordinary functions, written in Rust, Python or JavaScript.

## User guides

Three guides with the same chapters, one for each way of using Liquers:

- [Liquers for Rust](guides/rust/index.html): building on the crates, writing commands, value
  types, stores and servers.
- [Liquers for Python](guides/python/index.html): the `liquers` package, for data work and
  Python services.
- [Liquers for the Web](guides/web/index.html): Liquers running in a browser page.

How the three relate: [User guides](guides/index.md). Each guide is also offered as EPUB, PDF and
an offline HTML copy under [downloads](downloads/index.html).

## API reference

Generated from the source code on every release of this site.

- **Python**: [the `liquers` package](api/python/liquers.html).
- **JavaScript**: [the `liquers-web` module](api/js/index.html), the classes and functions a
  browser page imports.
- **Rust**, one reference per crate:
  - [liquers-core](api/rust/liquers_core/index.html): queries, keys, stores, assets, commands.
  - [liquers-macro](api/rust/liquers_macro/index.html): the `register_command!` macro.
  - [liquers-store](api/rust/liquers_store/index.html): storage backends and their configuration.
  - [liquers-records](api/rust/liquers_records/index.html): record streams, tables, views and
    table formats.
  - [liquers-template](api/rust/liquers_template/index.html): query-aware Mustache templates.
  - [liquers-markdown](api/rust/liquers_markdown/index.html): a lossless, query-aware Markdown
    parser.
  - [liquers-lib](api/rust/liquers_lib/index.html): the command library and rich values
    (DataFrames, images, user interfaces).
  - [liquers-mcp](api/rust/liquers_mcp/index.html): commands served as MCP tools.
  - [liquers-agent](api/rust/liquers_agent/index.html): agent memory over MCP and the command
    line.
  - [liquers-axum](api/rust/liquers_axum/index.html): the HTTP server.
  - [liquers-web](api/rust/liquers_web/index.html): the browser integration, compiled to
    WebAssembly.
  - [liquers-py](api/rust/liquers_py/index.html): the Rust side of the Python bindings.
  - [liquers-docs](api/rust/liquers_docs/index.html): the builder of this site.

## Source

The guides are written in the repository's `docs/guides/` folder:
<https://github.com/orest-d/liquers>. The guides are in preparation; a chapter appears here when
it is ready.
