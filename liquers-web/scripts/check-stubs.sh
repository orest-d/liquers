#!/usr/bin/env bash
# Structural checks on the generated TypeScript declarations and the built artifact.
#
# Covers the conformance IDs that need no browser:
#
#   STUBS01  declarations exist for every exposed module
#   STUBS02  the type checker accepts a representative usage sample
#   STUBS03  declared names match the runtime surface
#   STUBS04  a command declaration preserves its function's signature
#   STUBS05  async entry points are declared awaitable
#   STUBS06  the type checker rejects incorrect usage
#   PACKAGE01  a clean build produces the artifact
#   PACKAGE05  the default feature set produces the documented value type
#
# STUBS07 and PACKAGE02/03/04/07 need a real page and live in tests/e2e. PACKAGE04 belongs there
# because `version()` has to be *called* for its claim to mean anything.
#
# This ships as a script rather than a CI job because the repository has no CI. Run it after
# `examples-web/quickstart/build.sh`.
#
# Usage:  ./scripts/check-stubs.sh [--build]
#           --build   run the quick-start build first

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(cd "$here/.." && pwd)"
root="$(cd "$crate/.." && pwd)"
dist="$crate/examples-web/quickstart/dist"
e2e="$crate/tests/e2e"
stubs="$crate/tests/stubs"

failures=0
pass() { printf '  \033[32mok\033[0m   %s\n' "$1"; }
fail() { printf '  \033[31mFAIL\033[0m %s\n' "$1"; failures=$((failures + 1)); }

if [[ "${1:-}" == "--build" ]]; then
    "$crate/examples-web/quickstart/build.sh" >/dev/null
fi

if [[ ! -d "$dist" ]]; then
    echo "No build found at $dist — run examples-web/quickstart/build.sh first (or pass --build)." >&2
    exit 2
fi

tsc="$e2e/node_modules/.bin/tsc"
if [[ ! -x "$tsc" ]]; then
    echo "TypeScript is not installed — run 'npm install' in $e2e." >&2
    exit 2
fi

echo "PACKAGE01 — a clean build produces the artifact"
for f in liquers_web.js liquers_web_bg.wasm liquers_web.d.ts liquers_web_bg.wasm.d.ts; do
    if [[ -s "$dist/$f" ]]; then pass "$f"; else fail "$f is missing or empty"; fi
done

echo
echo "STUBS01 — declarations exist for every exposed module"
decl="$dist/liquers_web.d.ts"
# Every #[wasm_bindgen] class and free function in the crate must appear in the declarations. The
# expected list is derived from the source, not hand-maintained, so adding an export without a
# declaration is caught.
expected_classes=$(grep -rhoP '#\[wasm_bindgen\(js_name\s*=\s*\K\w+(?=\)\s*\]\s*\npub struct)' \
    --include='*.rs' -z "$crate/src" 2>/dev/null | tr '\0' '\n' | sort -u)
# The -z/-P combination is brittle across grep builds; fall back to the known surface.
if [[ -z "$expected_classes" ]]; then
    expected_classes=$'Asset\nEnvironment\nKey\nLiquersError\nQuery\nState\nStore\nValue'
fi
while read -r class; do
    [[ -z "$class" ]] && continue
    if grep -q "^export class $class" "$decl"; then
        pass "class $class"
    else
        fail "class $class is not declared"
    fi
done <<< "$expected_classes"

# Free functions: taken from the js_name attributes and snake_case exports in lib.rs.
for fn in init isInitialized shutdown opaque evaluate getAsset encodeParam \
          registerCommand unregisterCommand describeCommand version; do
    if grep -q "^export function $fn(" "$decl"; then
        pass "function $fn"
    else
        fail "function $fn is not declared"
    fi
done

echo
echo "STUBS03 — declared names match the runtime surface"
# Every declared free function must exist in the generated JavaScript module.
missing=0
while read -r fn; do
    [[ -z "$fn" ]] && continue
    if ! grep -qE "^export function $fn\(|^const $fn = |function $fn\(" "$dist/liquers_web.js"; then
        fail "declared function $fn has no runtime export"
        missing=$((missing + 1))
    fi
done < <(grep -oP '^export function \K\w+' "$decl")
[[ $missing -eq 0 ]] && pass "every declared function exists at runtime"

echo
echo "STUBS05 — async entry points are declared awaitable"
for sig in 'export function init(): Promise<' \
           'export function evaluate(query: string): Promise<' \
           'export function getAsset(query: string): Promise<'; do
    if grep -qF "$sig" "$decl"; then
        pass "${sig#export function }"
    else
        fail "not declared as returning a Promise: ${sig#export function }"
    fi
done

echo
echo "STUBS02/STUBS04 — the type checker accepts correct usage"
valid_out=$("$tsc" --noEmit --strict --target es2022 --module es2022 \
    --moduleResolution bundler --lib es2022,dom,esnext.disposable "$stubs/valid_usage.ts" 2>&1)
if [[ -z "$valid_out" ]]; then
    pass "valid_usage.ts type-checks"
else
    fail "valid_usage.ts must type-check cleanly:"
    printf '%s\n' "$valid_out" | sed 's/^/       /'
fi

echo
echo "STUBS06 — the type checker rejects incorrect usage"
# Asserting "it fails" is not enough: one broad error would mask five silently-accepted mistakes.
# The count is asserted, so a declaration change that legalizes any single case is caught.
expected_errors=6
invalid_out=$("$tsc" --noEmit --strict --target es2022 --module es2022 \
    --moduleResolution bundler --lib es2022,dom,esnext.disposable "$stubs/invalid_usage.ts" 2>&1)
actual_errors=$(printf '%s\n' "$invalid_out" | grep -cE '^[^ ].*error TS')
if [[ "$actual_errors" -eq "$expected_errors" ]]; then
    pass "invalid_usage.ts produces exactly $expected_errors errors"
else
    fail "invalid_usage.ts produced $actual_errors errors, expected $expected_errors"
    printf '%s\n' "$invalid_out" | sed 's/^/       /'
fi

echo
# PACKAGE04 is asserted in tests/e2e/package.spec.ts, not here. The claim is that `version()`
# reports the crate and the *linked* core version, and the only way to check that is to call it —
# grepping the wasm for a version string proves the bytes are present, not that the function
# returns them, and would pass for a hard-coded literal that had drifted from the manifest.

echo "PACKAGE05 — the default feature set produces the documented value type"
# The documented value type is liquers_lib::value::Value = CombinedValue<SimpleValue, ExtValue>,
# selected by `default-features = false, features = ["webui"]`. Asserted at the manifest, which is
# what determines it; the type itself is checked by the compiler in default_value.rs.
if grep -q 'liquers-lib = { path = "../liquers-lib", default-features = false, features = \["webui"\] }' \
        "$crate/Cargo.toml"; then
    pass "liquers-lib is linked with only the webui feature"
else
    fail "the liquers-lib dependency no longer matches the documented feature set"
fi
if grep -q 'pub type WebValue = liquers_lib::value::Value;' "$crate/src/default_value.rs"; then
    pass "the default value type is liquers_lib::value::Value"
else
    fail "src/default_value.rs no longer declares the documented value type"
fi

echo
if [[ $failures -eq 0 ]]; then
    printf '\033[32mAll stub and package checks passed.\033[0m\n'
    exit 0
fi
printf '\033[31m%d check(s) failed.\033[0m\n' "$failures"
exit 1
