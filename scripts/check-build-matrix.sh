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
# The `records` rows exist because `ExtValue::RecordView`/`RecordSource` are `#[cfg(feature =
# "records")]` — a build with the feature off never sees them, so the compiler alone cannot catch
# a match arm that forgot the gate. `records,polars` and `webui,records` are the interactions
# that matter: `records` alone proves the crate builds without `polars` pulling it in by accident,
# and `webui,records` is the browser-facing combination `ui/web/html.rs::ext_to_html` gains an arm
# for. See specs/design/record-streams/phase2-architecture.md §"Feature-gating discipline".
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
# liquers-core is here because it carries optional features (`toml`, `cli`) and target-conditional
# store availability (`filesystem` is declared but unavailable on wasm32), neither of which the
# native default build exercises. The `--no-default-features` row proves that the core async store
# surface is always available even when no optional features are selected.
#
# Disk budget. Every feature set and target leaves its own artifacts in one `target/`, and the
# whole matrix does not fit in a 30 GB Claude Code cloud session (BUILD-MATRIX-EXHAUSTS-SESSION-DISK).
# So the script:
#
#   * runs cargo with `CARGO_INCREMENTAL=0` unless the caller set `CARGO_INCREMENTAL` — the
#     incremental caches buy nothing in a one-shot run and are the largest part of `target/`;
#   * runs `cargo clean` before a row when free space on the target directory's filesystem is
#     below `MATRIX_MIN_FREE_GB` (default 8; the default was not re-measured against the heaviest
#     row, `cargo check -p liquers-lib --tests`, when it was introduced — raise it if a row still
#     runs out). Cleaning only when low keeps the shared dependency builds as long as space allows;
#   * retries a failed row once, after a clean, when free space is below the threshold after the
#     failure, so a "No space left on device" is not reported as a broken configuration. Only the
#     second result counts.
#
# `MATRIX_MIN_FREE_GB=0` disables the guard (never cleans). `MATRIX_DRY_RUN=1` prints every row and
# every clean decision without running cargo, and exits 0. The target directory is the one Cargo
# itself resolves (`cargo metadata`: `CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR` or a
# `[build] target-dir` config), and it is exported as `CARGO_TARGET_DIR` so that every row, every
# clean and the free-space check use the same directory.
#
# Usage: bash scripts/check-build-matrix.sh
#        MATRIX_MIN_FREE_GB=12 bash scripts/check-build-matrix.sh   # more headroom
#        MATRIX_DRY_RUN=1 bash scripts/check-build-matrix.sh        # show the plan only
# See specs/design/liquers-web/phase4-implementation.md, Step 7, and
# specs/design/build-matrix-disk-budget/.
set -uo pipefail

export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
MATRIX_MIN_FREE_GB="${MATRIX_MIN_FREE_GB:-8}"
MATRIX_DRY_RUN="${MATRIX_DRY_RUN:-0}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Cargo's effective target directory; falls back to the environment or `target/` when cargo
# metadata is unavailable (for example under a stubbed cargo).
TARGET_DIR="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps 2>/dev/null \
  | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
if [ -z "$TARGET_DIR" ]; then
  TARGET_DIR="${CARGO_TARGET_DIR:-${CARGO_BUILD_TARGET_DIR:-$ROOT/target}}"
fi
export CARGO_TARGET_DIR="$TARGET_DIR"

# Free space, in whole GB, on the filesystem holding the target directory (the repository root
# until the target directory exists). In a cloud session `df` reports the session allowance.
free_gb() {
  local dir="$ROOT"
  if [ -d "$TARGET_DIR" ]; then
    dir="$TARGET_DIR"
  fi
  df -Pk "$dir" | awk 'NR == 2 { print int($4 / 1048576) }'
}

# Below the threshold, clean the target directory (announced; skipped under dry run).
ensure_space() {
  if [ "$MATRIX_MIN_FREE_GB" -eq 0 ]; then
    return 0
  fi
  local free
  free="$(free_gb)"
  if [ "$free" -lt "$MATRIX_MIN_FREE_GB" ]; then
    echo "==> low disk (${free} GB free < ${MATRIX_MIN_FREE_GB} GB): cargo clean"
    if [ "$MATRIX_DRY_RUN" != "1" ]; then
      cargo clean
    fi
  fi
}

# True when free space is below a non-zero threshold.
space_low() {
  [ "$MATRIX_MIN_FREE_GB" -ne 0 ] && [ "$(free_gb)" -lt "$MATRIX_MIN_FREE_GB" ]
}

LIB_CONFIGS=(
  "--no-default-features --tests"
  "--no-default-features --features egui --tests"
  "--no-default-features --features polars --tests"
  "--no-default-features --features webui --tests"
  "--no-default-features --features image-support --tests"
  "--no-default-features --features records --tests"
  "--no-default-features --features records,polars --tests"
  "--no-default-features --features webui,records --tests"
  "--no-default-features --features records-ipc --tests"
  "--no-default-features --features records-parquet --tests"
  "--no-default-features --features records-parquet,polars --tests"
  "--no-default-features --features templates --tests"
  "--no-default-features --features templates,records --tests"
  # `mem` pulls `records` in; every row above it is the build without it (design/agent-memory-mvp).
  "--no-default-features --features mem --tests"
  "--tests"
  "--target wasm32-unknown-unknown --no-default-features --features webui"
  "--target wasm32-unknown-unknown --no-default-features --features webui,records"
  "--target wasm32-unknown-unknown --no-default-features --features webui,records,records-ipc"
  "--target wasm32-unknown-unknown --no-default-features --features webui,templates"
  "--target wasm32-unknown-unknown --no-default-features --features webui,mem"
)

# liquers-records has no default features: each format behind its own flag, and every one of them
# must build for wasm32 as well, since liquers-web carries the crate into the browser.
RECORDS_CONFIGS=(
  "--tests"
  "--features ipc,parquet --tests"
  "--target wasm32-unknown-unknown"
  "--target wasm32-unknown-unknown --features ipc,parquet"
)

# liquers-template depends on liquers-core only and adds no async runtime, so it must build for
# wasm32 too (design/liquers-mustache, AC-13).
TEMPLATE_CONFIGS=(
  "--tests"
  "--target wasm32-unknown-unknown --tests"
)

# liquers-web is wasm32-only and its `records` feature is opt-in: both sides of that gate must
# compile, library and tests, since a `#[cfg(feature = "records")]` arm missing on either side
# breaks only that configuration.
WEB_CONFIGS=(
  "--target wasm32-unknown-unknown --tests"
  "--target wasm32-unknown-unknown --features records --tests"
)

CORE_CONFIGS=(
  "--no-default-features"
  ""
  "--features toml"
  "--target wasm32-unknown-unknown"
)

STORE_CONFIGS=(
  ""
  "--no-default-features --features async_store"
  "--no-default-features --features async_store,opendal"
  # `opendal` alone, and a service without `async_store`: both compile only since PR #58 gated the
  # `AsyncOpenDALStore` import correctly (`STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`). The second
  # is the sharper row — it has a service compiled in and still no store type to construct, which
  # is where the factory advertised `fs` as available and then blamed the wrong feature.
  "--no-default-features --features opendal"
  "--no-default-features --features services-fs"
  "--target wasm32-unknown-unknown --no-default-features --features async_store"
)

failed=()
retried=()
total=0

check() {
  local crate="$1"
  local args="$2"
  local label="cargo check -p $crate ${args:-(default)}"
  total=$((total + 1))
  ensure_space
  echo "==> $label"
  if [ "$MATRIX_DRY_RUN" = "1" ]; then
    echo "    (dry run) CARGO_INCREMENTAL=$CARGO_INCREMENTAL cargo check -p $crate $args"
    return 0
  fi
  # shellcheck disable=SC2086
  if cargo check -p "$crate" $args; then
    return 0
  fi
  if space_low; then
    ensure_space
    echo "==> retrying after clean: $label"
    # shellcheck disable=SC2086
    if cargo check -p "$crate" $args; then
      retried+=("$label")
      return 0
    fi
  fi
  failed+=("$label")
}

for args in "${LIB_CONFIGS[@]}"; do
  check liquers-lib "$args"
done

for args in "${RECORDS_CONFIGS[@]}"; do
  check liquers-records "$args"
done

for args in "${TEMPLATE_CONFIGS[@]}"; do
  check liquers-template "$args"
done

for args in "${WEB_CONFIGS[@]}"; do
  check liquers-web "$args"
done

for args in "${CORE_CONFIGS[@]}"; do
  check liquers-core "$args"
done

for args in "${STORE_CONFIGS[@]}"; do
  check liquers-store "$args"
done

# The default consumer of liquers-store must be undisturbed by the feature split.
check liquers-axum ""

# The MCP engine (liquers-core only) and the agent binary (liquers-lib with `mem` alone). Native
# only: rmcp's stdio transport and the binary's tokio runtime have no wasm32 form.
check liquers-mcp "--tests"
check liquers-agent "--tests"

# The conformance suite is behind a non-default feature, so the default build never exercises it —
# and a store author enabling it must not be the first to discover it does not compile. Both
# directions matter: with the feature ON (the module and its tests), and with it OFF (the
# `#[cfg]`-gated items must vanish cleanly).
check liquers-core "--features store-conformance --lib --tests"
check liquers-store "--features store-conformance --lib --tests"

if [ ${#retried[@]} -ne 0 ]; then
  echo
  echo "Passed on retry after a low-disk clean:"
  printf '  %s\n' "${retried[@]}"
fi

if [ ${#failed[@]} -ne 0 ]; then
  echo
  echo "FAILED configurations:"
  printf '  %s\n' "${failed[@]}"
  exit 1
fi
echo
echo "All ${total} configurations OK."
