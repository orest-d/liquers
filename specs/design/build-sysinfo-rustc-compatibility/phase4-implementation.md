# Phase 4: Implementation Plan

## Progress

- [x] 1. Measure the floor: `cargo metadata --format-version 1` → max `rust_version` over
  non-workspace packages (expected 1.95) — measured 1.95 (egui 0.36.x family, sysinfo 0.39.6) — `4716dc7`
- [x] 2. `Cargo.toml` `[workspace.package] rust-version`; `rust-version.workspace = true` in each
  member — `msrv-declared` — `4716dc7`
- [x] 3. `msrv-is-floor`; `msrv-builds` if a toolchain can be installed, otherwise note it as
  unverified — both pass; `cargo +1.95 check -p liquers-lib --lib` builds (rustup 1.95.0) — `4716dc7`
- [x] 4. `CLAUDE.md` line; `bash scripts/check-build-matrix.sh` (or at least the native rows and
  `cargo check -p liquers-web --target wasm32-unknown-unknown`) — `default-loop` (536 tests pass on 1.97; liquers-web wasm32 check passes) — `11ccec4`
- [x] 5. Issue resolution and `status: closed`; `python3 scripts/docs_index.py --check` — this commit

Rollback: revert the commit.
