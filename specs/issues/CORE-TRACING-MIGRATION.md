---
id: CORE-TRACING-MIGRATION
kind: issue
title: Diagnostics use `eprintln!` rather than structured logging
status: draft
priority: P2
complexity: M
area: [core/error, build]
design: 
created: 2026-08-08
github:
---
## Problem

Half of this is already done. The `println!` → `eprintln!` conversion landed with
`query-validation`, and `CLAUDE.md` now forbids `println!` in library code — so stdout is no longer
corrupted. What remains is the other half of WP-6: `eprintln!` is not structured logging.

## Impact

Diagnostics cannot be filtered by level or module, carry no span context, and cannot be routed
anywhere but stderr. For an async asset lifecycle with concurrent evaluations, interleaved
unstructured lines are close to unreadable.

## Expected behaviour

`tracing` throughout the libraries, with the binaries choosing a subscriber. WP-6 pairs this with
panic hygiene — no `unwrap`/`expect` in library code — which `CLAUDE.md` already requires and which
should be verified rather than assumed.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-6. Verified against HEAD: the stdout half is done; the tracing half is not started. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
