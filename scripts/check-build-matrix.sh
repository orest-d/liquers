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
# Usage: bash scripts/check-build-matrix.sh
# See specs/liquers-web/phase4-implementation.md, Step 7.
set -uo pipefail

CONFIGS=(
  "--no-default-features"
  "--no-default-features --features egui"
  "--no-default-features --features polars"
  "--no-default-features --features webui"
  ""
  "--target wasm32-unknown-unknown --no-default-features --features webui"
)

failed=()
for args in "${CONFIGS[@]}"; do
  label="cargo check -p liquers-lib ${args:-(default)}"
  echo "==> $label"
  # shellcheck disable=SC2086
  if ! cargo check -p liquers-lib $args; then
    failed+=("$label")
  fi
done

if [ ${#failed[@]} -ne 0 ]; then
  echo
  echo "FAILED configurations:"
  printf '  %s\n' "${failed[@]}"
  exit 1
fi
echo
echo "All ${#CONFIGS[@]} configurations OK."
