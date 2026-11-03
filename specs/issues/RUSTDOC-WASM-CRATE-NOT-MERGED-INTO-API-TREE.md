---
id: RUSTDOC-WASM-CRATE-NOT-MERGED-INTO-API-TREE
kind: issue
title: liquers-web's Rust API page does not link the other crates and is missing from the API search
status: draft
priority: P3
complexity: S
area: [web, docs]
design:
created: 2026-11-02
github:
---
## Problem

The documentation site's Rust API (`api/rust/`) is one rustdoc tree for the native crates.
`liquers-web` is wasm32-only, so `scripts/build-docs.sh` documents it in a second
`cargo doc --target wasm32-unknown-unknown -p liquers-web --no-deps` run and copies
`liquers_web/` into the tree. Rustdoc links and indexes only the crates of one run, so on its pages
a `liquers_core::query::Query` is plain text rather than a link, and `liquers_web` is not in the
tree's crate list (`crates.js`) or search index.

## Impact

A reader of `liquers_web`'s Rust API cannot follow a type into `liquers_core`, and searching the
Rust API does not find `liquers_web` items. The JavaScript reference (`api/js/`) is unaffected.

## Expected behaviour

`liquers_web` appears in the tree's crate list and search, and its cross-crate types link. Options:
rustdoc's `--merge`/`--parts-out-dir`/`--include-parts-dir` once stable; `--extern-html-root-url`
(unstable, `-Zunstable-options`) pointing at `../`; or documenting the wasm32 build with the
dependencies it shares (`liquers-core`, `liquers-lib` with `webui`) in its own tree under
`api/rust-wasm/`.

## Discovery

Found while adding every API reference to the documentation site (`design/user-documentation-guides`,
2026-11-02 addendum).
