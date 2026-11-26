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

## Measurements, 2026-11-23

`cargo build -p liquers-web --target wasm32-unknown-unknown --release` at `d0790d3`, the raw
cargo output before `wasm-bindgen` (which removes about 0.9 MB of its own sections), default
features (no records) unless stated.

| Build | Raw | gzip -9 |
|---|---|---|
| Current release profile | 19.8 MB | 4.9 MB |
| A: `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"` | 9.5 MB | 2.7 MB |
| B: A, and `image` with only `png`, `jpeg`, `gif`, `bmp`, `ico`, `tiff`, `webp` | 7.6 MB | 2.2 MB |
| C: A, and `image` with only `png`, `jpeg` | 7.0 MB | 2.0 MB |
| D: C with `--features records` | 9.0 MB | 2.6 MB |

Where the current 19.8 MB goes (`twiggy top`, by the crate named in each symbol):

- 4.2 MB is the function name section, which `strip = true` removes.
- About 3.2 MB is image code: `rav1e` 1.35 MB and `av_scenechange` 0.11 MB (the AVIF encoder),
  `exr` 0.34 MB, `zune_jpeg` 0.32 MB, `tiff` 0.21 MB, `image_webp` 0.20 MB, `image` 0.39 MB,
  `png` 0.10 MB. `liquers-lib` depends on `image` with its default features, which include AVIF
  and OpenEXR; no code in `liquers-lib/src` names either format.
- `liquers_core` 3.0 MB and `serde_json` 1.2 MB, mostly generic code instantiated per
  environment type; `core` + `alloc` 2.2 MB; `scc` 0.6 MB; `serde_yaml` with `unsafe_libyaml`
  0.5 MB.
- `resvg`, `usvg` and `tiny-skia` are unconditional dependencies of `liquers-lib` but hardly
  appear in the module.

Not tried: `wasm-opt -Oz`, `opt-level = "z"`, making `image` formats cargo features of
`liquers-lib`.

