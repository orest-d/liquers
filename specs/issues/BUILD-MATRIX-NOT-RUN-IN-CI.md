---
id: BUILD-MATRIX-NOT-RUN-IN-CI
kind: issue
title: The feature/target build matrix is never run automatically
status: closed
priority: P2
complexity: S
area: [build, docs]
design: build-matrix-ci
created: 2026-08-25
github:
---
## Problem

`scripts/check-build-matrix.sh` checks all eleven feature and target configurations of
`liquers-lib`, `liquers-store` and `liquers-axum`, but nothing runs it. The only workflow in
`.github/workflows/` is `docs-check.yml`, which validates `specs/index.csv`. There is no workflow
running `cargo test`, `cargo check`, or the matrix.

The script therefore depends entirely on a contributor remembering it. `CLAUDE.md` now documents
it under *Building and testing → Feature matrix*, which is the cheapest half of the fix, but a
documented command is not an enforced one.

## Impact

The configurations the matrix exists to protect are exactly the ones no routine loop builds: a
`--no-default-features` consumer, a wasm32 consumer of `liquers-store` without OpenDAL, and the
`webui`-only build behind `liquers-web`. A regression in any of them is invisible until someone
builds it by hand, which is how `LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED` survived from at least
`dc762b3` to 2026-08-25.

Severity is bounded by the fact that the failure mode is a compile error rather than a wrong
result, and that it is caught the moment anyone does run the script.

## Expected behaviour

The matrix runs on push and pull request. Options, not ranked:

- a GitHub Actions workflow calling `bash scripts/check-build-matrix.sh` — needs
  `rustup target add wasm32-unknown-unknown`, `libssl-dev` for the non-vendored `openssl`, and a
  cargo registry/target cache to keep the wall time reasonable;
- the same, but matrix-per-configuration so the failing row is named in the job list rather than
  in a log;
- a cheaper subset on every push (the `--no-default-features` and wasm32 rows, which are where
  regressions actually land) and the full script on a schedule.

Whichever is chosen, the run cost should be measured first: the eleven configurations are
`cargo check`, but they still build polars and the egui family from scratch on a cold runner.

## Discovery

Found on 2026-08-25 while fixing `LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED`. That issue asked for
"a CI job, or a documented command list in `CLAUDE.md`"; the documented list was delivered, and
this records the half that was not.

## Resolution

Added `.github/workflows/build-matrix.yml`: a single job on `pull_request`/`push`-to-`main`
(path-filtered to the crates, `Cargo.{toml,lock}`, the script, and the workflow file itself) that
installs the `wasm32-unknown-unknown` target and `libssl-dev`, caches with `Swatinem/rust-cache`,
and runs `bash scripts/check-build-matrix.sh` unmodified — the first of the "Expected behaviour"
options, chosen over the per-configuration matrix or scheduled-subset variants for the lowest
implementation risk on an S/P2 fix. Not measured against actual runner cost; if the wall time
proves too high in practice, splitting into a `strategy: matrix` job (the second listed option) is
the natural follow-up and does not require touching the script.
