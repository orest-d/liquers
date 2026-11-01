---
id: RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED
kind: issue
title: Intra-doc links in the store submodules and value.rs do not resolve
status: draft
priority: P3
complexity: S
area: [docs, core/store, core/value]
design:
created: 2026-11-01
github:
---
## Problem

`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc -p liquers-core --no-deps` reports
26 unresolved links at `d2f72fe` (after `store-config-handlers-usage-docs`). Eighteen are in the
inner (`//!`) docs of `store/realm.rs`, `store/storage.rs`, `store/writes.rs` and
`store/handlers.rs` — `CommandMetadata`, `CommandMetadataRegistry`, `StepBuilder`,
`ComponentBuilder`, `MetadataStorage`, `NativeStorage`, `WriteSet`, … — names that are in scope in
the submodule. The likely cause: each `pub mod x;` in `store.rs` also carries an outer `///` doc
comment, and rustdoc then resolves the combined docs in the parent (`store`) scope. The other
eight are in `value.rs` (`NAN`, `INFINITY`, `NEG_INFINITY`, …).

## Impact

The links render as plain text, so the module docs a reader of the store-config API starts from do
not link to the types they describe. No CI job runs rustdoc with that lint, so new ones go
unnoticed.

## Expected behaviour

The command above reports nothing: drop the outer `///` on the `pub mod` lines (or write the links
as `crate::` paths), qualify the `f64` constants in `value.rs`, and consider adding the lint to a CI
docs job.

## Discovery

Found while writing the pipeline docs for GitHub #230 (design `store-config-handlers-usage-docs`),
whose new links had to be written as `crate::store::…` paths to resolve.
