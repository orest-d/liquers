---
id: RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED
kind: issue
title: Intra-doc links in the store submodules and value.rs do not resolve
status: closed
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

## Resolution

Closed by the PR from branch `claude/rustdoc-intra-doc-links`. Both commands exit 0 with
`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"`; doc comments only, no code or public item changed.

The hypothesis held, and it explains `value.rs` too: rustdoc decides the scope of a module's links
from its *first* doc fragment. An outer `///` first makes the whole combined block resolve in the
parent. The `store` submodules had an outer `///` on each `pub mod` line in `store.rs` plus their
own `//!`; `json_f64` in `value.rs` is an inline module documented only by an outer `///`, and its
eight links (`to_number`, `to_json`, `from_json`, `NAN`, `INFINITY`, `NEG_INFINITY`, `serialize`,
`deserialize`) name its own items — they are string tokens, not `f64` constants.

- `liquers-core/src/store.rs`: the outer `///` on the seven `mod` lines removed; every submodule
  already carries equivalent `//!` docs, so nothing is lost from the rendered pages.
- `liquers-core/src/value.rs`: the `json_f64` docs moved inside the module as `//!`.
- Two links the scope change exposed: `MemoryCentralStorage` in `storage.rs` exists only with the
  `store-conformance` feature (or in tests), so the default doc build has no item to link; it is
  now a code span naming the feature. `StepBuilder` in `handlers.rs` is not imported there and now
  carries a `super::realm::` path. In `realm.rs`, the `crate::store::realm::` targets that the
  parent-scope resolution had forced are dropped, since they became redundant.
- `liquers-records/src/buffer.rs`: the `[COLUMNAR]` citation (the Arrow Columnar Format, as in
  `specs/design/record-streams/phase2-architecture.md`) given a reference-style link target in both
  doc blocks that use it.
- `liquers-records/src/views.rs`: `select_columns` is an inherent method of `dyn RecordView`, which
  an intra-doc path cannot name; the doc now links the trait and keeps the method as code.

The CI docs job itself is the parent design's (`api-documentation-rules`, GitHub #232). The
`private_intra_doc_links` warnings seen in the same run are filed as
`RUSTDOC-PRIVATE-INTRA-DOC-LINKS-RECUR`.
