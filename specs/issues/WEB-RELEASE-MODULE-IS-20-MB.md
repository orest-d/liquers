---
id: WEB-RELEASE-MODULE-IS-20-MB
kind: issue
title: The optimized browser module with records is 20 MB, and liquers-web/README.md says 16.5 MB
status: draft
priority: P3
complexity: M
area: [web, build]
design:
created: 2026-11-10
github:
---

## Problem

`./liquers-web/examples-web/quickstart/build.sh --release --features records` at HEAD writes a
`liquers_web_bg.wasm` of 20,330,869 bytes (4.9 MB with `gzip -9`). `liquers-web/README.md`
("Record streams") states 12.4 MB without and 16.5 MB with records, so the module has grown since
that was measured, and nothing tracks its size. The build runs no size optimization after
`wasm-bindgen` (no `wasm-opt`, the release profile is not tuned for size).

## Impact

The module is published on the project site and imported by every example of the Web user guide;
its size is the first thing a visitor pays for. The README's figures are stale.

## Expected behaviour

The README states the measured sizes, a check reports the size on each build so growth is seen, and
the published build is reduced where that is cheap (`wasm-opt -Oz`, `opt-level = "s"` and
`lto` for the wasm target, a look at what the user-interface and template libraries add).

## Discovery

Building the module for the Web user guide (2026-11-10). The guide states the measured 20 MB and
5 MB compressed.
