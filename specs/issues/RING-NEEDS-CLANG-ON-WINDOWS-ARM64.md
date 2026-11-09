---
id: RING-NEEDS-CLANG-ON-WINDOWS-ARM64
kind: issue
title: Default builds and cargo doc fail on Windows ARM64 without clang, because Polars and OpenDAL pull in ring
status: draft
priority: P2
complexity: L
area: [build, store/backends, lib/polars]
design:
created: 2026-11-04
github:
---

## Problem

On an `aarch64-pc-windows-msvc` host with only Visual Studio Build Tools (MSVC `cl.exe`), any
build of `liquers-lib`, `liquers-store`, `liquers-axum`, `liquers-py`, `liquers-agent` or
`liquers-docs` in its default configuration fails, and so does `cargo doc` on them (rustdoc still
runs every dependency's build script):

```
error occurred in cc-rs: failed to find tool "clang": program not found
```

The failing crate is `ring` 0.17. Its `build.rs` (`ring-0.17.14/build.rs:563`) switches to
`clang` unconditionally when the target is Windows on AArch64, because MSVC cannot build its
assembly; on x86_64 Windows it builds with `cl.exe` and a bundled NASM, so the failure is
ARM64-specific. `aws-lc-sys` (rustls' default provider, reached through OpenDAL) has the same
kind of native-toolchain requirement.

`ring` reaches the workspace by **two independent routes**, so fencing OpenDAL alone does not
fix `liquers-lib`:

| Route | Chain | Crates affected |
|---|---|---|
| Polars | `liquers-lib` feature `polars` → `polars/parquet` → `polars-stream/parquet` → `polars-stream/cloud` → `polars-io/cloud` → `polars-io/http` → `object_store/http` → `object_store/cloud` → `ring` (also `reqwest` → `rustls` → `ring`) | `liquers-lib` (default), `liquers-py` |
| OpenDAL | `liquers-store` default `services-default` → `services-s3`/`gcs`/`azblob`/… → `reqsign` → `jsonwebtoken` → `ring`; and `opendal` → `futures-rustls` → `rustls` → `aws-lc-rs` → `aws-lc-sys` | `liquers-store` (default), and through it `liquers-axum`, `liquers-py`, `liquers-agent`, `liquers-docs` |

Verified with `cargo tree -e normal -i ring` per crate: `liquers-core`, `liquers-records` (all
features, including its own `parquet`), `liquers-template`, `liquers-markdown` and `liquers-mcp`
are ring-free; `liquers-store --no-default-features` and `liquers-lib` without `polars` are
ring-free. `liquers-lib` depends on `liquers-store`/`opendal` only as dev-dependencies, so the
OpenDAL route does not affect `cargo doc -p liquers-lib`.

### Existing feature fences

- **OpenDAL is already fenced** in `liquers-store`: feature `opendal` (the dependency) and one
  `services-<scheme>` feature per backend, all in `default` via `services-default`. What is
  missing is a fence *by native requirement*: the cloud services (`s3`, `gcs`, `azblob`,
  `dropbox`, `onedrive`, `gdrive`, `github`, `webhdfs`, `ipfs`, `http`, `webdav`) are what bring
  in `reqsign`/`rustls`, while `services-fs` alone does not need ring. The consumers
  (`liquers-axum`, `liquers-agent`, `liquers-docs`, `liquers-py`) take `liquers-store` with its
  defaults and expose no feature to turn the cloud services off.
- **Polars is fenced** by `liquers-lib`'s `polars` feature, but there is no way to keep Polars
  and drop the cloud/HTTP layer: in polars 0.55 the `parquet` feature implies `cloud`
  (via `polars-stream`), and `liquers-lib` enables `parquet`.

## Impact

Contributors on Windows ARM64 (Snapdragon laptops, Surface Pro X, Windows-on-ARM VMs) cannot
build or document the default workspace until they install LLVM. Workaround: install clang
(below), or build docs in a ring-free configuration (below). Linux, macOS and x86_64 Windows
are unaffected, as are docs.rs and CI (ubuntu runners).

**Workaround A — install the toolchain** (documented in `CLAUDE.md`, "Building on Windows"):

```powershell
winget install --id LLVM.LLVM          # native ARM64 installer; puts clang in C:\Program Files\LLVM\bin
# open a new shell, check:  clang --version
```

or add the Visual Studio Installer component *C++ Clang Compiler for Windows*
(`Microsoft.VisualStudio.Component.VC.Llvm.Clang`) to the Build Tools and put
`...\VC\Tools\Llvm\ARM64\bin` on `PATH`. `aws-lc-sys` additionally wants CMake on some targets
(`winget install Kitware.CMake`).

**Workaround B — document without ring:**

```bash
cargo doc --no-deps -p liquers-core -p liquers-records -p liquers-template -p liquers-markdown -p liquers-mcp
cargo doc --no-deps -p liquers-store --no-default-features --features async_store
cargo doc --no-deps -p liquers-lib --no-default-features \
  --features egui,image-support,records,records-ipc,records-parquet,templates,mem,markdown
```

## Expected behaviour

A contributor without clang can build and document everything except the cloud features, and
the cargo error names the feature to disable rather than a C compiler. Options, not chosen here
(a design is needed, since `complexity: L`):

1. **`liquers-store`: split `services-default` by native requirement.** Add
   `services-local = ["services-fs"]` (ring-free) and `services-cloud = [s3, gcs, azblob, …]`;
   `services-default = ["services-local", "services-cloud"]` keeps today's behaviour. Check
   whether `services-http`/`webdav`/`ftp` really need TLS (`ftp` pulls `suppaftp` →
   `futures-rustls`), and place each accordingly. `availability02_…` keeps the table honest.
2. **Forward the fence through the consumers.** `liquers-axum`, `liquers-agent`, `liquers-docs`
   and `liquers-py` depend on `liquers-store` with `default-features = false,
   features = ["async_store", "opendal", "services-local"]` and add their own
   `cloud-stores = ["liquers-store/services-cloud"]` feature, default on.
3. **`liquers-lib`: separate Polars Parquet from Polars core.** Make `polars` = core DataFrame
   commands (csv, ipc, lazy, …) and a new `polars-parquet = ["polars", "polars/parquet"]`,
   default on. Requires gating the `pl` parquet commands, regenerating
   `specs/command_registry.yaml`, and adding the configuration to
   `scripts/check-build-matrix.sh`. Alternative: replace Polars' Parquet I/O with the
   `liquers-records` Parquet path (already ring-free) and drop `polars/parquet` entirely.
4. **Or choose the TLS provider once.** Pin `rustls` to `ring` only (dropping `aws-lc-sys`)
   across the graph; this removes one native dependency but still needs clang on ARM64.

Whichever is chosen, add a `cargo doc` row to `scripts/check-build-matrix.sh` (or a
`scripts/doc-no-native.sh`) for the ring-free configuration, so it stays ring-free.

## Discovery

`cargo doc` failed on a Windows 11 ARM64 machine (rustc 1.98.1, `aarch64-pc-windows-msvc`,
VS 2022 Build Tools, no LLVM). Reproduced with `cargo build -p ring`; traced with
`cargo tree -e normal,features -i ring` and the polars-stream / polars-io / object_store
manifests in the cargo registry.
