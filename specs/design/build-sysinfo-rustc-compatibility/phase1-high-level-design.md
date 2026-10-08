# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — build-settings fix: a `rust-version` declaration in the workspace
  and member `Cargo.toml` files and one documentation line; no code, dependency or feature change.
  (The member manifests are package metadata, not code in several crates.)
- **Leading issue:** None
- **Explanation:** Decided (maintainer, 2026-10-08, backlog compaction D3): declare the minimum
  supported Rust version instead of pinning. The originally proposed pin was impossible:
  `.gitignore` excludes `Cargo.lock`, so there is no committed lockfile to pin, and the repository
  declared no 1.94 window. The current dependency set's own floor is **1.95** (`egui` 0.36, `sysinfo`
  0.39, measured from `cargo metadata` on 2026-10-08). The cloud toolchain is rustc 1.97, so the
  declaration constrains nothing there; it only makes an older toolchain fail with a clear
  "requires rustc 1.95" message from Cargo itself.
- **Open questions:** None. Recorded for the reviewer: `rust-version` is a minimum, never a pin, so
  newer toolchains (cloud, CI's `stable`) are unaffected.

## Problem Example

On rustc 1.94.1, `cargo check -p liquers-lib --tests` fails with `sysinfo@0.39.6 requires rustc
1.95` (and, since `47757dd`, the library target fails on nine crates). Nothing in the repository
says which Rust is supported, so a contributor cannot tell whether that is their toolchain or a
regression.

## Scope and Acceptance Criteria

- **AC-1** The minimum is declared
  - WHEN a contributor reads the workspace `Cargo.toml`
  - THEN `[workspace.package] rust-version` states the minimum, and every member inherits it
- **AC-2** The declared minimum is the real floor
  - WHEN the dependency set is resolved
  - THEN no dependency declares a `rust-version` above the workspace's, and the workspace's own code
    builds on that version where a toolchain for it can be installed
- **AC-3** Newer toolchains are unaffected
  - WHEN the default test loop runs on the cloud toolchain (1.97)
  - THEN it builds as before

Out of scope: committing `Cargo.lock`; switching to the MSRV-aware resolver (`resolver = "3"`); a CI
job that builds on the declared minimum.

## Design Dependencies

None.
