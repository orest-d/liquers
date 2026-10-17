---
id: BUILD-MATRIX-DISK-BUDGET
kind: design
title: The build-matrix script keeps its target directory within the free disk
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [build]
issues: [BUILD-MATRIX-EXHAUSTS-SESSION-DISK]
created: 2026-10-17
---
# The build-matrix script keeps its target directory within the free disk

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`scripts/check-build-matrix.sh` runs `cargo check` for 37 rows (18 `liquers-lib`, 4
`liquers-records`, 2 `liquers-template`, 4 + 1 `liquers-core`, 6 + 1 `liquers-store`, 1
`liquers-axum`) into one `target/`, with incremental compilation on (the dev profile default) and
never cleans. Every feature set and target leaves its own artifacts, so in a Claude Code cloud
session (30 GB) the disk fills partway through `liquers-lib` and the remaining rows fail with
"No space left on device" — failures that say nothing about the configuration. `CLAUDE.md` tells
agents to run this script, so the false failures land exactly where it is used most.

### Problem Example

```bash
bash scripts/check-build-matrix.sh      # cloud session, fresh target/
```

Today (design/liquers-mustache Step 16, 2026-10-17): the disk is full after liquers-lib's first rows,
and every later row is reported under "FAILED configurations" with an I/O error. It should finish,
cleaning `target/` when free space runs low, and list only configurations that really fail to
compile; the workaround (rows one at a time, deleting `target/` by hand) passed every row.

### Scope and Acceptance Criteria

- **AC-1** Incremental off by default
  - WHEN the script runs without `CARGO_INCREMENTAL` set
  - THEN every `cargo check` it starts runs with `CARGO_INCREMENTAL=0`; a value set by the caller is
    kept
- **AC-2** Clean before a row when space is low
  - WHEN free space on the target directory's filesystem is below `MATRIX_MIN_FREE_GB` (default 8)
    before a row
  - THEN the script runs `cargo clean`, says so on stdout, and then runs the row
- **AC-3** A disk failure is retried, not reported
  - WHEN a row fails and free space is then below the threshold
  - THEN the script cleans and runs that row once more, and only the second result counts
- **AC-4** Guard can be disabled and inspected
  - WHEN `MATRIX_MIN_FREE_GB=0`
  - THEN the script never cleans (today's behaviour); and WHEN `MATRIX_DRY_RUN=1`, it prints each
    row and each clean decision without running cargo and exits 0
- **AC-5** Whole matrix finishes in a cloud session
  - WHEN the script runs in a 30 GB cloud session from an empty `target/`
  - THEN it reaches its summary line, and no row fails with "No space left on device"

Non-goals: fewer or different rows; a per-crate `CARGO_TARGET_DIR` (rejected below); the CI
workflow, which keeps calling the script unchanged.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tooling fix confined to `scripts/check-build-matrix.sh`, no crate
  code, no interface, no new dependency.
- **Leading issue:** None
- **Explanation:** The script's rows and their result reporting stay as they are; only disk
  housekeeping around each `cargo check` is added. The threshold is a default an environment variable
  overrides, so its exact value is an implementation detail measured in step 1.
- **Open questions:** None.

### Design Dependencies

- `build-matrix-ci` (complete) — **overlaps**: wrote the workflow that runs the script; it is not
  edited. No ordering.

### Consolidated Findings

Threshold cleaning keeps the shared dependency builds (core, store, polars) as long as space allows,
which a clean after every crate or a per-crate target directory would throw away. In CI the guard is
inert unless the runner itself runs low, in which case it prevents the same false failures.

## Phase 2: Architecture

### Solution

In `scripts/check-build-matrix.sh`:

1. After `set -uo pipefail`: `export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"`; read
   `MATRIX_MIN_FREE_GB` (default 8) and `MATRIX_DRY_RUN` (default 0); compute the repository root
   from the script's own path and `TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"`.
2. A helper `free_gb` — `df -Pk` on `TARGET_DIR` if it exists, else on `ROOT`, fourth column of the
   second line, divided by 1048576. (`df` in a cloud session reports the session allowance,
   `CLAUDE.md` "Building and testing".)
3. A helper `ensure_space` — when the threshold is non-zero and `free_gb` is below it, print
   `==> low disk (<n> GB free < <t> GB): cargo clean` and run `cargo clean` (skipped under dry run).
4. `check()` calls `ensure_space` before the row; on failure, if `free_gb` is below the threshold,
   it calls `ensure_space` and runs the row once more before recording a failure. Under dry run it
   echoes the command instead of running it.

Rejected: a `CARGO_TARGET_DIR` per crate removed after its rows (the issue's other suggestion) —
`liquers-lib`'s 18 rows alone exceed the disk, so it does not solve the problem, and every crate
would rebuild `liquers-core` from scratch; `cargo clean` after every row (correct but roughly
doubles the run time for no benefit when space is ample); deleting only `target/debug/incremental`
(irrelevant once incremental is off).

### Changes

`scripts/check-build-matrix.sh` only: the variables, `free_gb`, `ensure_space`, the retry in
`check()`, and a header paragraph documenting `MATRIX_MIN_FREE_GB`, `MATRIX_DRY_RUN` and the
incremental default. No crate, workflow or registry change.

### Risks

| Risk | Note |
|---|---|
| Files | `scripts/check-build-matrix.sh` |
| Time | each clean costs one cold rebuild of the next row's dependencies; only paid when space is low |
| Threshold too low | one row's artifacts could still overrun the remaining space; AC-3's retry covers that row, and step 1 sets the default from a measured worst row |
| CI | `actions/rust-cache` saves `target/` at job end; after a mid-run clean it saves a smaller cache. Acceptable; `MATRIX_MIN_FREE_GB=0` restores today's behaviour if needed |
| Portability | `df -P` and `awk` are POSIX; the script already requires bash |
| Certainty | High; recovery is reverting the script |

## Phase 3: Examples and Tests

### Examples

`bash scripts/check-build-matrix.sh` (defaults); `MATRIX_MIN_FREE_GB=12 bash scripts/check-build-matrix.sh`
(more headroom); `MATRIX_MIN_FREE_GB=0 bash scripts/check-build-matrix.sh` (today's behaviour).

### Tests

Tooling, so the proofs are commands rather than Rust tests:

- `bash -n scripts/check-build-matrix.sh` (and `shellcheck` where installed) — syntax of every change
- `matrix_dry_run_cleans_below_threshold`:
  `MATRIX_DRY_RUN=1 MATRIX_MIN_FREE_GB=100000 bash scripts/check-build-matrix.sh` prints a clean
  decision before every row, runs no cargo, exits 0 — AC-2, AC-4
- `matrix_dry_run_never_cleans_when_disabled`: the same with `MATRIX_MIN_FREE_GB=0` prints no clean
  line — AC-4
- `matrix_dry_run_shows_incremental_off`: the dry-run line for a row includes
  `CARGO_INCREMENTAL=0`; with `CARGO_INCREMENTAL=1` exported it shows `1` — AC-1
- `matrix_retry_after_disk_failure`: run with `PATH` pointing at a stub `cargo` (in the scratchpad)
  whose `check` fails the first time and succeeds the second, and `MATRIX_MIN_FREE_GB=100000`; the
  summary reports success with the retry visible — AC-3
- `matrix_full_run_in_cloud_session`: the real script, fresh `target/`, in a cloud session — AC-5

## Phase 4: Implementation Plan

### Steps

- [ ] 1. (Not run in the automatic fix: measuring needs a `cargo clean` of a shared target directory; default kept at 8 and the header says it is unmeasured.) Measure the heaviest row: `cargo clean && CARGO_INCREMENTAL=0 cargo check -p liquers-lib --tests && du -sh target`;
  set the default threshold to that size plus margin (8 GB if it is under 6) — record the figure in the header
- [x] 2. `scripts/check-build-matrix.sh` — `CARGO_INCREMENTAL` default, the two variables, `ROOT`/`TARGET_DIR`,
  `free_gb`, `ensure_space` — `bash -n scripts/check-build-matrix.sh`
- [x] 3. `check()` — `ensure_space` before the row, the single retry, dry-run echo — the three dry-run
  commands of Phase 3 and `matrix_retry_after_disk_failure`
- [x] 4. Header comment — the variables and why — `bash -n`
- [ ] 5. (Not run in the automatic fix: a full matrix would exhaust the disk shared with concurrent agents; left for the PR's CI and a maintainer run.) Full run in a cloud session from a clean `target/` — `bash scripts/check-build-matrix.sh`
  reaches "All 37 configurations OK" (AC-5). Rollback: `MATRIX_MIN_FREE_GB=0` or revert
- [x] 6. Close `BUILD-MATRIX-EXHAUSTS-SESSION-DISK` with a resolution note; regenerate the index —
  `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

The CI `build-matrix` workflow passes on the PR (the script is in its path filter). `CLAUDE.md`
"Feature matrix" may gain one sentence naming `MATRIX_MIN_FREE_GB`; that file is maintainer-owned,
so propose it in the PR rather than editing it unasked. Final diff review: the script only.
