#!/usr/bin/env bash
# Builds the quick-start page without trunk.
#
# `trunk build` is the documented path (Phase 1 decision 7) and does the same three things: build
# the cdylib for wasm32, run wasm-bindgen over it, and copy the page next to the output. This
# script exists because trunk is a heavy install for what amounts to those three commands, and
# because it is what CI-less environments can run. Both produce a `dist/` a static server can host.
#
# Usage:
#   ./build.sh                 # debug build into ./dist
#   ./build.sh --release       # optimized
#
# Then serve it — any static server works, the page needs no backend:
#   python3 -m http.server 8090 --directory dist

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
profile_dir="debug"
cargo_args=()

if [[ "${1:-}" == "--release" ]]; then
    profile_dir="release"
    cargo_args+=(--release)
fi

echo "Building liquers-web for wasm32…"
cargo build --manifest-path "$root/Cargo.toml" -p liquers-web \
    --target wasm32-unknown-unknown "${cargo_args[@]}"

wasm="$root/target/wasm32-unknown-unknown/$profile_dir/liquers_web.wasm"
[[ -f "$wasm" ]] || { echo "missing $wasm" >&2; exit 1; }

echo "Generating JavaScript bindings…"
rm -rf "$here/dist"
mkdir -p "$here/dist"
# `--target web` produces an ES module with a default-exported `init`, which is what the page
# imports. No bundler is involved, which is the point of the example.
wasm-bindgen --target web --out-dir "$here/dist" --out-name liquers_web "$wasm"

cp "$here/index.html" "$here/dist/index.html"

# Fixture files the `STORE` e2e tests fetch over HTTP. They live here rather than under tests/
# because the fetch store needs them served by the *same* static server as the page, and this is
# the directory that server hosts.
cp -r "$here/fixtures" "$here/dist/fixtures"

echo
echo "Built $here/dist"
echo "Serve with:  python3 -m http.server 8090 --directory '$here/dist'"
