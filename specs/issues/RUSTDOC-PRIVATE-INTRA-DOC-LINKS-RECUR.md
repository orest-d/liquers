---
id: RUSTDOC-PRIVATE-INTRA-DOC-LINKS-RECUR
kind: issue
title: Public rustdoc in liquers-core and liquers-records links private items again
status: draft
priority: P3
complexity: S
area: [docs, core/plan, core/assets, records]
design:
created: 2026-11-03
github:
---
## Problem

`RUSTDOC-PUBLIC-DOCS-LINK-PRIVATE-ITEMS` (closed 2026-09-04) removed the three
`rustdoc::private_intra_doc_links` warnings of that time. At `d8bb375` there are fifteen new ones,
plus two `rustdoc::redundant_explicit_links`. `cargo doc --no-deps -p <crate>` reports them as
warnings, so nothing fails; each is a link a reader of the published API documentation cannot
follow, since rustdoc renders it as plain text.

`liquers-core` (3 private links, 2 redundant targets):

| Location | Link | Lint |
|---|---|---|
| `liquers-core/src/interpreter.rs:396` | `apply_plan_state` (from `apply_plan`) | private item |
| `liquers-core/src/interpreter.rs:689` | `do_step_state` (from `do_step`) | private item |
| `liquers-core/src/assets.rs:8091` | `crate::dependencies::DependencyManager::expire` (from `expire_dependencies_result`) | private item |
| `liquers-core/src/lib.rs:109` | — | redundant explicit link target |
| `liquers-core/src/assets.rs:6344` | — | redundant explicit link target |

`liquers-records` (12 private links):

| Location | Link |
|---|---|
| `liquers-records/src/buffer.rs:33` | `AlignedChunk` (from `AlignedBuffer`) |
| `liquers-records/src/formats/infer.rs:14` | `is_float_literal` (module docs of `infer`) |
| `liquers-records/src/formats/ndjson.rs:10–11` | `objects_to_batch`, `rows_to_json_values`, `row_to_json`, `scalar_to_json`, `json_to_scalar` (module docs of `ndjson`) |
| `liquers-records/src/formats/shapes.rs:8` | `super::ndjson::objects_to_batch` |
| `liquers-records/src/mutable.rs:873` | `crate::buffer::AlignedBuffer::into_mut` |
| `liquers-records/src/schema.rs:59`, `:83` | `lenient` (from `Analyzer` and `IndexKind`) |
| `liquers-records/src/sources.rs:294` | `Self::derive_naming` (from `naming`) |

## Impact

Cosmetic: the sentences still read, because rustdoc drops a private link to code text. The cost is
the same one the closed issue named — permanent warning noise in `cargo doc`, which is what let
these fifteen accumulate unnoticed. The parent design `api-documentation-rules` adds a CI job with
`-D rustdoc::broken_intra_doc_links`; that lint does not cover private links, so this set survives
it.

## Expected behaviour

Zero warnings from `cargo doc --no-deps` on both crates, so that
`-D rustdoc::private_intra_doc_links` (and `-D rustdoc::redundant_explicit_links`) can join the CI
docs job. Per link, either make the target public, link a public item that explains the same thing,
or drop the brackets and name the item in prose, as the closed issue did. The `ndjson` and `infer`
module docs may be better served by `--document-private-items` being the intended audience, in
which case the prose should say so rather than link.

## Discovery

Seen in the output of `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps` while
fixing `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` (branch `claude/rustdoc-intra-doc-links`), whose
scope is the broken-link lint only. Eligible for automatic fixing; not fixed in that PR.
