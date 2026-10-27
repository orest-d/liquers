---
id: BUILD-MATRIX-CI-SKIPS-NEWER-CRATES
kind: issue
title: The build-matrix workflow does not run on a change confined to liquers-template, liquers-mcp or liquers-agent
status: closed
priority: P3
complexity: S
area: [build]
design: build-matrix-path-filters
created: 2026-10-20
github:
---

## Problem

The `pull_request` and `push` path filters in `.github/workflows/build-matrix.yml` list
`liquers-core`, `-macro`, `-store`, `-lib`, `-axum`, `-web`, `-py` and `-records`, but not
`liquers-template/**`, `liquers-mcp/**` or `liquers-agent/**`. `scripts/check-build-matrix.sh` has
rows for all three (`TEMPLATE_CONFIGS`, including wasm32; `check liquers-mcp "--tests"`;
`check liquers-agent "--tests"`). So a PR that touches only one of these crates (and `specs/`) runs
the docs check and nothing else, and a broken configuration lands unseen.

This is the same gap as the closed `BUILD-MATRIX-CI-SKIPS-LIQUERS-RECORDS-CHANGES`, recurring for
the crates added after it. Each new crate repeats it.

## Problem Example

A PR changes only `liquers-template/src/render.rs` and breaks its wasm32 build
(`TEMPLATE_CONFIGS` checks `--target wasm32-unknown-unknown --tests`). Because the path filters
miss `liquers-template/**`, `build-matrix` does not run, and the PR merges with only the `docs`
check. Re-checked at `cd6d5da`. Of the routes in Expected behaviour, `'**/*.rs'` would miss
`include_str!` inputs such as `liquers-agent/tools/*.md` and `*.yaml`. The design uses
`'liquers-*/**'`.

## Impact

A wasm32 or `--tests` break in `liquers-template`, or a break in the MCP engine or agent binary, is
found only by the next PR that happens to touch a listed crate, and then blamed on that PR.

## Expected behaviour

The three paths are added to both lists. To stop the next crate repeating the gap, either filter on
`'**/*.rs'` plus `'**/Cargo.toml'`, or have `check-build-matrix.sh` (or the docs check) fail when
a workspace member has a matrix row but no path filter.

## Discovery

Bulk design of `workspace-rustfmt-check` and `core-benchmark-suite` (2026-10-20), while reading
the CI configuration at main 18732dd.

## Resolution

Fixed 2026-10-27 by `design/build-matrix-path-filters/`. Both `paths` lists in `.github/workflows/build-matrix.yml` now name one entry, `'liquers-*/**'`, in place of the eight per-crate entries, so a change confined to `liquers-template`, `liquers-mcp`, `liquers-agent` or any future `liquers-*` crate runs the matrix. The other five triggers are unchanged. A comment above each list says a member named otherwise needs its own entry.
