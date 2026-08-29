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
# liquers-store is here for a different reason: its `opendal` feature is optional, and the compiler
# catches a missed `#[cfg]` only in the configuration that omits the feature, which the native
# default build never does. That row also runs `factory04`, which is `#[cfg(not(feature =
# "opendal"))]` and is the only coverage of the message a gated-off store type must produce.
#
# The `async_store,opendal` row is the third state, and the one that used to be the *only* state:
# OpenDAL linked with no `services-*` feature, so every advertised store type but none of the
# services is present. It is what `STORE-OPENDAL-SERVICES-NOT-ENABLED` was about, and it is where
# a `#[cfg(feature = "opendal")]` that should have been `#[cfg(feature = "services-fs")]` shows
# up. Note it needs `async_store` too: `opendal` alone has never compiled, because
# `store_factory.rs` imports `AsyncOpenDALStore`, which `async_store` gates — see
# STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN.
#
# Its wasm32 row used to prove the dependency edge liquers-web relied on. That edge is gone —
# configuration, factories and the builder moved to liquers-core and liquers-web no longer depends
# on this crate at all — but the row still earns its place as the wasm32 half of the feature split.
# See specs/design/store-factories-in-core/.
#
# liquers-core is here because it now carries an optional feature (`toml`) and target-conditional
# store availability (`filesystem` is declared but unavailable on wasm32), neither of which the
# native default build exercises. Note the absence of a `--no-default-features` row: that
# configuration has never compiled, because `async_store` gates `futures`/`async-trait` while
# context.rs, interpreter.rs and store.rs import them unconditionally. Add the row when
# CORE-NO-DEFAULT-FEATURES-BROKEN is fixed; adding it now would make this script red on arrival.
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

CORE_CONFIGS=(
  ""
  "--features toml"
  "--target wasm32-unknown-unknown"
)

STORE_CONFIGS=(
  ""
  "--no-default-features --features async_store"
  "--no-default-features --features async_store,opendal"
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

for args in "${CORE_CONFIGS[@]}"; do
  check liquers-core "$args"
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
