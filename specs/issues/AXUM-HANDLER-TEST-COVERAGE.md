---
id: AXUM-HANDLER-TEST-COVERAGE
kind: issue
title: liquers-axum has no handler test scaffolding, so handler behaviour is verified only by review
status: draft
priority: P2
complexity: M
area: [axum]
design: expired-binary-read-safety
created: 2026-08-08
github:
---
## Problem

`cargo test -p liquers-axum` runs **zero** tests. The crate has no test router, no request helper,
and no way to assert what a handler returns for a given asset state. Handler behaviour is therefore
verified by code review alone.

This was surfaced, not caused, by the `expired-binary-read-safety` work. That design changed both
query polling loops in `liquers-axum/src/query/handlers.rs` — replacing catch-all status arms with
exhaustive ones and returning an error response for `Status::Expired` — and none of it is covered by
a test. The `liquers-core` side of the same change carries fourteen tests.

The gap matters more than the line count suggests: the HTTP layer is where the original bug
(`ASSET-EXPIRED-CACHED-BINARY-READ`) actually reached users, because a handler holds an `AssetRef`
across an expiry while polling. It is the one place where a regression is both most likely and least
likely to be noticed.

Making the matches exhaustive immediately revealed that the POST loop had no `Status::Ready` arm at
all — a real gap that the catch-all had hidden for as long as it existed. That is the class of
defect a handler test would catch directly.

## Expected behaviour

`liquers-axum` has enough scaffolding to assert handler outcomes:

1. A test `Router` built from a caller-supplied `Environment`, so a test can stage an asset in a
   chosen state.
2. A request helper returning status code and body, so assertions read as
   "GET this query → 4xx with this error type".
3. Tests for the states the polling loops now distinguish: `Ready` (bytes), `Expired` (error, and
   **promptly** — not after the 30 s timeout), `Error`, `Cancelled`, `Directory`.

The timing assertion is the important one. Several of these states differ from "still processing"
only in that they must not spin, and a test that ignores latency would pass against the bug.

## Verification

1. `cargo test -p liquers-axum` runs a non-zero number of tests.
2. A request for an expired asset returns an error response in well under the 30 s query timeout.
3. A request for a directory key returns an error rather than timing out.
4. The scaffolding is reusable — adding a handler test does not require new infrastructure.

## Notes

Recorded as the explicit outcome of Step 9 of the `expired-binary-read-safety` implementation plan,
which offered a choice between building this scaffolding as part of that work or filing it. Filing
was chosen to keep a P0 read-contract fix from growing into a test-infrastructure project; the
obligation to record it rather than merely mention it is why this file exists.
