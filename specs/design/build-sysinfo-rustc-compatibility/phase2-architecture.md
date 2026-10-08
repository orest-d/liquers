# Phase 2: Solution and Architecture

## Chosen Solution

Add to the workspace `Cargo.toml`:

```toml
[workspace.package]
rust-version = "1.95"
```

and `rust-version.workspace = true` to the `[package]` of every member (`liquers-core`,
`liquers-macro`, `liquers-store`, `liquers-records`, `liquers-lib`, `liquers-axum`, `liquers-web`,
`liquers-py`). Add one line to `CLAUDE.md` "Building and testing" naming the minimum and how it was
derived.

The value is the maximum `rust_version` over non-workspace packages in `cargo metadata` at
implementation time; re-measure rather than copying 1.95 if dependencies changed.

## Alternatives

- Pin `sysinfo` in a lockfile: impossible while `Cargo.lock` is git-ignored; committing it is a
  separate decision.
- `rust-toolchain.toml`: pins an exact toolchain for everyone, which is what the maintainer was
  concerned about for the cloud environment. Rejected.
- `resolver = "3"`: would make Cargo prefer dependency versions compatible with the declared
  minimum, changing resolution for everyone. Out of scope.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Workspace code uses a feature newer than the declared minimum | Build with that toolchain if `rustup toolchain install <v> --profile minimal` works; if not, record it as unverified in the issue resolution. Raise the value if it fails. |
| The value goes stale after a dependency upgrade | Cargo reports it on any older toolchain; the derivation is documented in `CLAUDE.md` so it can be re-measured. |
| liquers-web / liquers-py manifests | Metadata only; `cargo metadata` and the wasm check in `scripts/check-build-matrix.sh` confirm they still parse. |
