---
id: WORKSPACE-RUSTFMT-CHECK
kind: design
title: The workspace is rustfmt-clean and CI keeps it so
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [build, docs]
issues: [WORKSPACE-NOT-RUSTFMT-CLEAN]
created: 2026-10-20
---
# The workspace is rustfmt-clean and CI keeps it so

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`cargo fmt --all -- --check` should pass at `HEAD` and keep passing, so that a contributor's
`cargo fmt` changes only what they wrote.

### Problem Example

At HEAD (18732dd), `cargo fmt --all -- --check` reports **3,034 hunks in 213 files**. At filing
(2026-08-29) it was 39 files. Every crate is affected: `liquers-core` 64 files, `liquers-lib` 58,
`liquers-records` 30, `liquers-axum` 22, `liquers-template` 16, `liquers-web` 8, `liquers-py` 7,
`liquers-agent` 5, `liquers-mcp` 2, `liquers-store` 1. The largest are
`liquers-core/tests/common/manager_scenarios.rs` (288) and `liquers-core/src/assets.rs` (151). No CI
job and no script runs the check (`.github/workflows/` holds `build-matrix.yml` and
`docs-check.yml` only), which is why the count grew sevenfold in seven weeks.

Today: running `cargo fmt` on a file you touched adds unrelated hunks to the diff. Should be: none.

The issue's route 3 is already done. The two `///` comments in statement position in
`assets.rs` are now `//` comments (`:8893`, `:8999`, `DefaultAssetManager::set_binary` and
`set_state`), so they no longer cause an `unused doc comment` warning.

### Scope and Acceptance Criteria

- **AC-1** Clean at HEAD
  WHEN `cargo fmt --all -- --check` runs after the change
  THEN it exits 0
- **AC-2** CI refuses drift
  WHEN a pull request changes a `.rs` file so that rustfmt would reformat it
  THEN the `fmt` workflow fails and names the file
- **AC-3** Blame skips the reformat
  WHEN `git blame` runs with `blame.ignoreRevsFile=.git-blame-ignore-revs`
  THEN lines touched only by the reformat show their previous author
- **AC-4** Behaviour unchanged
  WHEN `cargo test -p liquers-lib --lib --tests` and `bash scripts/check-build-matrix.sh` run on the
  reformat commit
  THEN they pass as on its parent

Non-goals: a `rustfmt.toml` style change; `clippy`; formatting non-Rust files.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — touches every crate (rule 6) and needs a decision on
  timing that affects every open branch (rule 5).
- **Leading issue:** **Open design question — timing of the bulk reformat.** 213 files change at
  once, and every open branch that touches them gets conflicts. *Recommended:* land it as its own
  PR, right after the current batch of implementation PRs merges. Merge it with a merge commit, not
  a squash, so the SHA in `.git-blame-ignore-revs` stays valid. Before rebasing, an open branch runs
  `cargo fmt --all` and commits, so most conflicting hunks become identical.
- **Explanation:** The mechanics are fully determined. Only when to land the change, and whether CI
  hard-fails, is for the maintainer.
- **Open questions:**
  1. **Open design question — enforcement.** Should the CI job fail the PR, or only report?
     *Recommended:* fail. A report-only job drifts the same way the matrix did before CI.
  2. **Proposed resolution — no wrapper script.** The issue's route 2 suggested a script beside
     `check-build-matrix.sh`. The check is one command, so the plan documents it in `CLAUDE.md` §
     "Building and testing" instead.

### Design Dependencies

- `lib-build-warning-cleanup`, `core-wasm-warning-free`, `serde-derive-feature-declared` — overlaps:
  their Rust diffs rebase over the reformat. Landing them **before** the reformat avoids
  re-resolving; this design should go last in this cluster.
- Every open implementation design (e.g. `core-tracing-migration`, which edits many crates) —
  overlaps: same conflict cost; this is what the timing decision is about.

### Consolidated Findings

The cost is coordination, not code. The reformat grew from 39 to 213 files in seven weeks, so a
later fix costs more. CI enforcement must land in the same PR as the reformat, or the drift returns. Use
merge commits so the ignore-revs SHA stays valid.

## Phase 2: Architecture

### Solution

One PR, three commits:

1. `cargo fmt --all` alone, with no other edit (message `Apply rustfmt to the workspace`).
2. `.git-blame-ignore-revs` with commit 1's full SHA and a comment. A `CLAUDE.md` line in
   "Building and testing": `cargo fmt --all -- --check` must pass, and `git config
   blame.ignoreRevsFile .git-blame-ignore-revs` once per clone. GitHub's blame view reads the file
   automatically.
3. `.github/workflows/fmt.yml`: on `pull_request` and `push` to `main`, paths `**/*.rs` and the
   workflow file; one job: `actions/checkout@v4`, `dtolnay/rust-toolchain@stable` with
   `components: rustfmt`, `cargo fmt --all -- --check`. It needs no build, cache or system packages.

`cargo fmt --all` includes `liquers-web`, which is a workspace member that is not in
`default-members`. rustfmt leaves `register_command!` bodies alone, because they do not parse as
Rust expressions.

Rejected: a row in `check-build-matrix.sh`, which is slow and path-filtered differently; a
pre-commit hook, which is not versioned for every host; formatting per crate over several PRs,
which means more conflict windows and no clean point for CI.

### Changes

Every `.rs` file reported (commit 1); new `.git-blame-ignore-revs`; new
`.github/workflows/fmt.yml`; `CLAUDE.md`. No API, command or behaviour change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Open branches conflict | Timing decision; fmt-then-rebase recipe in the PR description | high |
| rustfmt output differs between stable versions | Edition 2021 style is stable; if CI and a local toolchain disagree, CI's stable is the reference | medium |
| Squash merge invalidates the SHA | Merge commit; if squashed, update the SHA after merge | high |
| Reformat changes behaviour | It cannot; AC-4 proves it | high |

## Phase 3: Examples and Tests

### Examples

On a branch that adds `fn f( ){}` to `liquers-core/src/query.rs`, the `fmt` job prints the diff
and fails. After `cargo fmt`, it passes.

### Tests

No unit test: these are repository properties.

- `cargo fmt --all -- --check` exits 0 after commit 1 — AC-1
- the workflow fails on a scratch PR with one mis-formatted line, then passes after `cargo fmt` —
  AC-2 (verified once, by hand, when the PR is opened)
- `git blame --ignore-revs-file .git-blame-ignore-revs liquers-core/src/plan.rs` shows no line
  attributed to commit 1 — AC-3
- `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` — AC-4

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Confirm the timing decision; merge the other designs in this cluster first — maintainer go-ahead
- [ ] 2. `cargo fmt --all`; commit alone — `cargo fmt --all -- --check` exits 0 (AC-1)
- [ ] 3. `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` — AC-4
- [ ] 4. `.git-blame-ignore-revs` (SHA of step 2) and `CLAUDE.md` lines — `git blame --ignore-revs-file …` (AC-3)
- [ ] 5. `.github/workflows/fmt.yml` — scratch-PR check (AC-2)
- [ ] 6. Close `WORKSPACE-NOT-RUSTFMT-CLEAN` with a resolution note (route 3 already done, cite `assets.rs:8893`) — `python3 scripts/docs_index.py --check`

### Validation

The PR's own `fmt` and `build-matrix` runs. Diff review: commit 2 contains whitespace and line
breaks only (`git diff -w --stat` against its parent is near-empty except re-wrapped expressions).
Merge with a merge commit.
