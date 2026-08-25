#!/usr/bin/env bash
# Build-configuration matrix for liquers-lib.
#
# `ExtValue::Foreign` is deliberately ungated, so a missing match arm fails to compile in every
# configuration and the compiler is the primary guard. This script exists for what the compiler
# cannot see:
#
#   * feature *interactions* — an arm that is right for `egui` alone but wrong for `egui+polars`;
#   * the wasm32 target, which the native loop never builds;
#   * `DefaultValueSerializer::as_bytes`, whose historical `_ =>` arm meant new variants were
#     absorbed silently. That arm is gone, but the site is worth keeping under a matrix.
#
# The native liquers-lib rows use `--tests`, so the integration test targets are checked in every
# feature configuration too, not only the library. Test targets are the easier place to forget a
# `#[cfg]`: an ungated `use polars::…` compiles under the default features and fails only where
# nobody looks. The wasm32 row stays library-only — liquers-lib's dev-dependencies (liquers-store
# with OpenDAL's `services-fs`) are native, and there is no wasm test runner in this loop.
# See specs/issues/LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED.md.
#
# liquers-store is here for a different reason: its `opendal` feature is optional so that a
# wasm32 consumer can take the configuration and builder without OpenDAL. The compiler catches a
# missed `#[cfg]` only in the configuration that omits the feature, and the native default build
# never omits it. The wasm32 row additionally proves the dependency edge liquers-web relies on.
# See specs/design/liquers-web-store/phase4-implementation.md, Step 4.
#
# Usage: bash scripts/check-build-matrix.sh
# See specs/design/liquers-web/phase4-implementation.md, Step 7.
set -uo pipefail

LIB_CONFIGS=(
  "--no-default-features --tests"
  "--no-default-features --features egui --tests"
  "--no-default-features --features polars --tests"
  "--no-default-features --features webui --tests"
  "--no-default-features --features image-support --tests"
  "--tests"
  "--target wasm32-unknown-unknown --no-default-features --features webui"
)

STORE_CONFIGS=(
  ""
  "--no-default-features --features async_store"
  "--target wasm32-unknown-unknown --no-default-features --features async_store"
)

failed=()
total=0

check() {
  local crate="$1"
  local args="$2"
  local label="cargo check -p $crate ${args:-(default)}"
  total=$((total + 1))
  echo "==> $label"
  # shellcheck disable=SC2086
  if ! cargo check -p "$crate" $args; then
    failed+=("$label")
  fi
}

for args in "${LIB_CONFIGS[@]}"; do
  check liquers-lib "$args"
done

for args in "${STORE_CONFIGS[@]}"; do
  check liquers-store "$args"
done

# The default consumer of liquers-store must be undisturbed by the feature split.
check liquers-axum ""

if [ ${#failed[@]} -ne 0 ]; then
  echo
  echo "FAILED configurations:"
  printf '  %s\n' "${failed[@]}"
  exit 1
fi
echo
echo "All ${total} configurations OK."
