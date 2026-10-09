---
id: BUILD-MATRIX-CI-SKIPS-LIQUERS-RECORDS-CHANGES
kind: issue
title: The build-matrix workflow does not run on a change confined to liquers-records
status: closed
priority: P3
complexity: S
area: [build, records]
created: 2026-10-08
github:
---
# The build-matrix workflow does not run on a change confined to `liquers-records`

## Problem

The `pull_request` and `push` path filters in `.github/workflows/build-matrix.yml` list every
workspace member except `liquers-records`, which was added to the workspace after the workflow.
`scripts/check-build-matrix.sh` does check `liquers-records` (its `ipc` / `parquet` features and the
`records` rows of `liquers-lib`). But a PR that touches only `liquers-records/**` and `specs/**`
runs the docs check and nothing else.

**Example.** PR #90 changes `liquers-records/src/formats/{ndjson,shapes}.rs` and `specs/`. Its
only check run is `check` from `docs-check.yml`. Expected: `build-matrix.yml` also runs, as it
does for a change to any other crate.

## Impact

A `liquers-records` change that breaks a feature configuration, or a `liquers-lib` build that uses
it, is not caught until a later PR happens to touch a listed path.

## Expected behaviour

`liquers-records/**` is in both path filters.

## Discovery

Found 2026-10-08 while driving PR #90 (`design/ordered-json-orient-column-order/`) to green: only
the docs check reported on it.

## Resolution

Closed 2026-10-08: `'liquers-records/**'` added to the `pull_request` and `push` path lists of
`.github/workflows/build-matrix.yml`. Evidence: this PR, whose own diff touches the workflow file,
runs the matrix, and the next PR that changes only `liquers-records` triggers it.
