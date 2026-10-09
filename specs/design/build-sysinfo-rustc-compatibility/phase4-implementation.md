# Phase 4: Implementation Plan

## Progress

- [ ] 1. Measure the floor: `cargo metadata --format-version 1` → max `rust_version` over
  non-workspace packages (expected 1.95)
- [ ] 2. `Cargo.toml` `[workspace.package] rust-version`; `rust-version.workspace = true` in each
  member — `msrv-declared`
- [ ] 3. `msrv-is-floor`; `msrv-builds` if a toolchain can be installed, otherwise note it as
  unverified
- [ ] 4. `CLAUDE.md` line; `bash scripts/check-build-matrix.sh` (or at least the native rows and
  `cargo check -p liquers-web --target wasm32-unknown-unknown`) — `default-loop`
- [ ] 5. Issue resolution and `status: closed`; `python3 scripts/docs_index.py --check`

Rollback: revert the commit.
