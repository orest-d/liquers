#!/usr/bin/env bash
# Runs the examples of specs/guides/JAVASCRIPT_GUIDE.md under Node (`DOCS03`).
#
# Builds liquers-web for wasm32, generates the `--target web` module a page imports, and runs
# tests/guide/run-guide-examples.mjs over it. Needs the wasm32 target, a `wasm-bindgen` CLI whose
# version matches the `wasm-bindgen` crate, and Node 18 or newer.
#
# Usage:  ./scripts/test-guide-examples.sh [--release] [cargo build arguments…]

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(cd "$here/.." && pwd)"
root="$(cd "$crate/.." && pwd)"
profile_dir="debug"
for arg in "$@"; do
    case "$arg" in
        --release) profile_dir="release" ;;
        --profile|--profile=*|--target-dir|--target-dir=*)
            echo "test-guide-examples.sh: $arg is not supported" >&2
            exit 2 ;;
    esac
done

cargo build --manifest-path "$root/Cargo.toml" -p liquers-web --target wasm32-unknown-unknown "$@"

out="$root/target/liquers-web-guide"
rm -rf "$out"
wasm-bindgen --target web --out-dir "$out" --out-name liquers_web \
    "$root/target/wasm32-unknown-unknown/$profile_dir/liquers_web.wasm"

node "$crate/tests/guide/run-guide-examples.mjs" "$out"
