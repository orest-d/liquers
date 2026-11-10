# Liquers for Rust: review history

One row per review of a chapter against its `sources:`. A chapter's `reviewed:` date must
have a row here (`specs/guides/USER_DOCS_EDITING_GUIDE.md`, maintenance).

| Date | Chapter | Change |
|---|---|---|
| 2026-11-01 | (all) | skeleton created, every chapter `outline` |
| 2026-11-09 | a-query-syntax.md | includes the new shared fragment `_shared/query-syntax.md`; still `outline` |
| 2026-11-10 | (all) | first complete text: every chapter written from its sources and set to `ready`; the Rust examples are compiled and run by `liquers-lib/tests/user_guide_GUIDE.rs` and `liquers-axum/tests/user_guide_GUIDE.rs`, and `liquers-docs/tests/rust_guide_examples.rs` ties the chapter text to them. Chapters 8 and 12 keep the text written earlier for configuration documents and the server |
