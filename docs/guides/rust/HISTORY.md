# Liquers for Rust: review history

One row per review of a chapter against its `sources:`. A chapter's `reviewed:` date must
have a row here (`specs/guides/USER_DOCS_EDITING_GUIDE.md`, maintenance).

| Date | Chapter | Change |
|---|---|---|
| 2026-11-01 | (all) | skeleton created, every chapter `outline` |
| 2026-11-09 | a-query-syntax.md | includes the new shared fragment `_shared/query-syntax.md`; still `outline` |
| 2026-11-10 | index.md | preface written: audience, what Liquers is for, conventions of the examples; `ready` |
| 2026-11-10 | 01-introduction.md | introduction written from `reference/VISION.md`, parallel to the Python guide; examples run; `ready` |
| 2026-11-10 | 02-installing.md | written: the crates, a project with git dependencies, features, toolchain, `library.rs` and `prelude.rs`; `ready` |
| 2026-11-10 | 03-queries.md | written with the shared fragment `_shared/queries.md`; running, parsing, the query editor, pipelines, planning and `liquers-validate`; examples run; `ready` |
| 2026-11-10 | 04-values.md | written from `reference/VALUE_TYPE_SYSTEM.md` and `guides/TYPE_SYSTEM_GUIDE.md`: value kinds, state, metadata, data formats, adding a type; examples run; `ready` |
| 2026-11-10 | 05-commands.md | written from `guides/COMMAND_REGISTRATION_GUIDE.md` and `guides/COMMAND_DESIGN_GUIDE.md`: the macro, input, arguments, context, options, async, errors, aliases, pipeline and template commands; examples run; `ready` |
| 2026-11-10 | 06-stores.md | written from the store guides: `AsyncStore`, configuration, factories and OpenDAL services, guarantees, writing a store; examples run; `ready` |
| 2026-11-10 | 07-assets.md | written with the shared fragment `_shared/assets.md`; `AssetRef`, freshness, cancelling, recipes, recipe providers, caching options; examples run; `ready` |
| 2026-11-10 | 08-environment.md | 8.1, 8.3 and 8.4 written from `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`; 8.2 kept, its example replaced by a tested one; `ready` |
| 2026-11-10 | 09-data.md | written from `guides/RECORD_STREAM_GUIDE.md` and the command library references: records, Polars, chunks, images, saving; examples run; `ready` |
| 2026-11-10 | 10-templates.md | written with the shared fragment `_shared/templates.md` and `guides/TEMPLATE_GUIDE.md`; examples run; `ready` |
| 2026-11-10 | 11-ui.md | written from `reference/UI_INTERFACE_FSD.md` and the examples of `liquers-lib`: the model, `UISpec`, egui, the web backend; code is sketches; `ready` |
| 2026-11-10 | 12-serving.md | 12.1, 12.2 and 12.5 kept; requests, embedding in an Axum application, live updates and embedding without a server written from `guides/WEB_API_GUIDE.md`; examples run in `liquers-axum`; `ready` |
| 2026-11-10 | 13-search.md | written from `guides/SEARCH_GUIDE.md` and `reference/AGENT_MEMORY_SERVICE.md`; examples run; an `EDITOR:` note for a worked `liquers-mcp` example; `ready` |
| 2026-11-10 | 14-testing.md | written: the three levels, a test module, assertions, planning shipped queries, servers and stores; examples run; `ready` |
| 2026-11-10 | 15-troubleshooting.md | written with the shared fragment `_shared/errors.md`; errors in Rust, frequent mistakes, known limitations; examples run; `ready` |
| 2026-11-10 | a-query-syntax.md | the syntax in Rust and the pipeline step forms added; an `EDITOR:` note for realms; `ready` |
| 2026-11-10 | b-command-library.md | introduction written; `ready` |
| 2026-11-10 | c-glossary.md | the terms in Rust and the terms this guide adds; `ready` |
| 2026-11-10 | d-sister-guides.md | introduction written; `ready` |
| 2026-11-10 | (all) | every `rust` block is a slice of `liquers-lib/tests/user_guide_GUIDE.rs` or `liquers-axum/tests/user_guide_GUIDE.rs`, checked by `liquers-docs/tests/rust_guide_examples.rs` |
