# Phase 3: Examples and Tests

1. A pull request changing `liquers-lib/src/value/mod.rs` schedules the build-matrix job.
2. A change to `.cargo/config.toml`, `Cargo.lock`, the script, or workflow also schedules it.
3. A docs-only change outside configured paths does not consume matrix time.
4. Missing wasm target setup or a failing matrix row makes the job fail visibly.

Validation consists of YAML parsing/action review, path-filter inspection, and
`bash scripts/check-build-matrix.sh`. No Rust test source is added by this CI-only change.
