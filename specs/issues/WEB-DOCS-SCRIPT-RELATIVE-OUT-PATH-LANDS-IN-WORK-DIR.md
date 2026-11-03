---
id: WEB-DOCS-SCRIPT-RELATIVE-OUT-PATH-LANDS-IN-WORK-DIR
kind: issue
title: build-liquers-web-docs.sh --out with a relative path writes inside its work directory and then reports no index.html
status: closed
priority: P3
complexity: S
area: [web, docs, build]
design:
created: 2026-11-03
github:
---
## Problem

`scripts/build-liquers-web-docs.sh` runs TypeDoc from its work directory
(`cd "$work" && npx … typedoc … --out "$out"`), so a relative `--out` such as
`--out target/liquers-web-docs` resolves to `target/liquers-web-docs-work/target/liquers-web-docs`.
TypeDoc reports `html generated at ./target/liquers-web-docs`, the script then checks
`$out/index.html` relative to the repository root, finds nothing, prints
`TypeDoc wrote no index.html` and exits 1. The default `--out` is absolute, and `build-docs.sh`
passes an absolute path, so CI is not affected.

## Impact

A contributor running the script by hand with a relative `--out` gets a misleading failure and a
stray output tree under `target/liquers-web-docs-work/`.

## Expected behaviour

The script resolves `--out` against the directory it was invoked from before changing directory
(`out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"` after creating the parent, or
`realpath -m`), as `build-docs.sh` does for its own `--out`. Eligible for automatic fixing: one
script, no interface change.

## Discovery

Found on 2026-11-03 while verifying the `@packageDocumentation` module page for
`design/api-documentation-rules/` (Phase 4, Step 5).

## Resolution (2026-11-03)

Fixed in `scripts/build-liquers-web-docs.sh`: `--out` is made absolute as soon as it is parsed
(`out="$(mkdir -p "$2" && cd "$2" && pwd)"`, the form `scripts/build-docs.sh` uses), against the
directory the script was invoked from, and the `cd "$repo_root"` now follows argument parsing. The
later `cd "$work"` for TypeDoc no longer changes what `--out` means.

Verified with a copy of the script whose wasm build was replaced by a stub declaration file (the
wasm32 target and wasm-bindgen were not available where the fix was made): at the previous
revision, `--out target/liquers-web-docs` from the repository root wrote
`target/liquers-web-docs-work/target/liquers-web-docs/index.html` and exited 1 with
`TypeDoc wrote no index.html`; with the fix it exits 0 and leaves `target/liquers-web-docs/index.html`.
`scripts/build-liquers-py-docs.sh` does not have this shape: pdoc and its check run from the same
directory, so it was left alone.
