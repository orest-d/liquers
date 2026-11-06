---
id: ASSET-MANAGER-GET-PRINTS-DEBUG-OUTPUT
kind: issue
title: Every AssetManager::get call writes three or four debug lines to stderr
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-11-06
github:
---

## Problem

`AssetManager::get` (`liquers-core/src/assets.rs`, around line 9636) prints "Getting asset for
key …", "Trying fast track …", "Fast track successful …" or "Submitting asset … to job queue" with
`eprintln!` on every call.

## Impact

A server answering a web page with fifty assets writes about two hundred lines to stderr per page
load, burying real diagnostics; the same holds for every binding. `CORE-TRACING-MIGRATION` covers
the general move to structured logging; this instance is debug output that should simply go, as
`RESOURCE-DIRECTORY-STEP-PRINTS-DEBUG-OUTPUT` did for the directory step.

## Expected behaviour

No output on the normal path; anything worth keeping goes through the logging that
`CORE-TRACING-MIGRATION` introduces.

## Discovery

`axum-server-assembly` Phase 1 review, 2026-11-06.
