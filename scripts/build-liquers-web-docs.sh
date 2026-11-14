#!/usr/bin/env bash
# Generates the HTML API reference of liquers-web's JavaScript interface with TypeDoc, from the
# TypeScript declarations wasm-bindgen writes (liquers_web.d.ts, which carries the Rust doc
# comments as JSDoc).
#
#   bash scripts/build-liquers-web-docs.sh [--out DIR] [--features LIST] [--release]
#
#   --out DIR        where to write the reference (default target/liquers-web-docs)
#   --features LIST  cargo features of the build documented (default none; `records` adds the
#                    record classes)
#   --release        build the module optimized. The declarations are the same either way; the
#                    documentation site asks for it because it also publishes the module this
#                    script leaves in the quick-start page's dist/ (scripts/build-docs.sh)
#
# Needs the wasm32-unknown-unknown target, a wasm-bindgen CLI of the version Cargo.lock resolves,
# and Node (npx). It builds the quick-start page (liquers-web/examples-web/quickstart/build.sh),
# then runs TypeDoc over its declarations. TypeDoc and TypeScript are fetched by npx at the
# versions in TYPEDOC_VERSION and TYPESCRIPT_VERSION. Nothing it writes is committed; the
# documentation site (scripts/build-docs.sh) places it under api/js/.
# Issue: specs/issues/WEB-API-REFERENCE-NOT-GENERATED.md.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# --out is made absolute here, against the directory the script was invoked from, because TypeDoc
# runs from the work directory below (WEB-DOCS-SCRIPT-RELATIVE-OUT-PATH-LANDS-IN-WORK-DIR).
out="$repo_root/target/liquers-web-docs"
features=()
release=()
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out="$(mkdir -p "$2" && cd "$2" && pwd)"; shift ;;
        --features) features=(--features "$2"); shift ;;
        --release) release=(--release) ;;
        -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
        *) echo "build-liquers-web-docs.sh: unknown option $1" >&2; exit 2 ;;
    esac
    shift
done
cd "$repo_root"

TYPEDOC_VERSION="${TYPEDOC_VERSION:-0.28.20}"
TYPESCRIPT_VERSION="${TYPESCRIPT_VERSION:-5.9.3}"
quickstart="$repo_root/liquers-web/examples-web/quickstart"

echo "==> building liquers-web for wasm32" >&2
"$quickstart/build.sh" "${release[@]}" "${features[@]}" >&2
dts="$quickstart/dist/liquers_web.d.ts"
[ -f "$dts" ] || { echo "build-liquers-web-docs.sh: no $dts" >&2; exit 1; }

# TypeDoc reads a project; a minimal tsconfig keeps it from looking for one in the repository.
work="$repo_root/target/liquers-web-docs-work"
rm -rf "$work"
mkdir -p "$work"
cp "$dts" "$work/liquers_web.d.ts"
cat > "$work/tsconfig.json" <<'EOF'
{ "compilerOptions": { "target": "es2022", "module": "es2022", "lib": ["esnext", "dom"],
  "strict": true, "skipLibCheck": false }, "files": ["liquers_web.d.ts"] }
EOF

echo "==> typedoc $TYPEDOC_VERSION -> $out" >&2
rm -rf "$out"
(cd "$work" && npx --yes -p "typedoc@$TYPEDOC_VERSION" -p "typescript@$TYPESCRIPT_VERSION" \
    typedoc --tsconfig tsconfig.json --entryPoints liquers_web.d.ts \
    --name "liquers-web (JavaScript)" --readme none --out "$out" \
    --validation.notDocumented false --treatWarningsAsErrors false >&2)

[ -f "$out/index.html" ] || { echo "build-liquers-web-docs.sh: TypeDoc wrote no index.html" >&2; exit 1; }
echo "==> $out/index.html" >&2
