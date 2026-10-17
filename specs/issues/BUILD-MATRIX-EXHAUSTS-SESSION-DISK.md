---
id: BUILD-MATRIX-EXHAUSTS-SESSION-DISK
kind: issue
title: The build-matrix script fills a cloud session's disk before it finishes
status: closed
priority: P3
complexity: S
area: [build]
design: build-matrix-disk-budget
created: 2026-10-17
github:
---
## Problem

`scripts/check-build-matrix.sh` runs `cargo check` for every row into one `target/` and never
cleans it. Each feature configuration and target leaves its own artifacts, so with 34 rows the
directory outgrows the 30 GB a Claude Code cloud session has, and the later rows fail with
"No space left on device" rather than with their own result.

## Problem Example

```bash
bash scripts/check-build-matrix.sh      # cloud session (30 GB), fresh target/
```

Today the disk fills during liquers-lib's rows and every later row is listed under "FAILED
configurations" with "No space left on device". It should reach its summary line and list only
configurations that genuinely fail to compile.

## Impact

`CLAUDE.md` tells an agent to run the script after touching a `cfg`, an optional dependency or an
`ExtValue` match; in a cloud session the run reports failures that are only the disk. Workaround:
run the rows one at a time and delete `target/` when free space runs low (that run passed all 34
rows on 2026-10-17). CI is unaffected.

## Expected behaviour

The script finishes within a cloud session's disk, for example by cleaning, or by using a
separate `CARGO_TARGET_DIR` per crate removed after its rows, when free space falls below a
threshold; and `CARGO_INCREMENTAL=0` for its own runs.

## Discovery

design/liquers-mustache Step 16: the first full run filled the disk after liquers-lib's first row.

## Resolution

2026-10-17, design `build-matrix-disk-budget`. `scripts/check-build-matrix.sh` now exports
`CARGO_INCREMENTAL=0` unless the caller set it, runs `cargo clean` before a row when free space on
the target directory's filesystem is below `MATRIX_MIN_FREE_GB` (default 8; `0` disables), and
retries a failed row once after a clean when space is low after the failure, listing such rows
under "Passed on retry after a low-disk clean". `MATRIX_DRY_RUN=1` prints the rows and clean
decisions without running cargo. Proven by the design's command tests
`matrix_dry_run_cleans_below_threshold`, `matrix_dry_run_never_cleans_when_disabled`,
`matrix_dry_run_shows_incremental_off` and `matrix_retry_after_disk_failure` (stub `cargo`);
`matrix_full_run_in_cloud_session` (AC-5) and the threshold measurement were not run in the
automatic fix and remain for the PR's CI and a maintainer run.
