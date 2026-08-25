# Phase 1: High-Level Design - Expiration Integration Suite Triage

## Feature Name

Expiration integration suite triage and issue closure

## Purpose

Establish whether `EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD` remains a runtime defect. At
current HEAD, the named suite passes 32/32; the solution is to capture reproducible closing
evidence and related historical context before closing the issue, rather than changing behavior.
This design does not claim which prior change made the suite pass without git evidence.

## Core Interactions

### Query, Store, Command, Asset, Value, Web/API, and UI Systems

No production interface changes are proposed. The verification exercises core asset expiration,
dependency invalidation, keyed-store persistence, and query-triggered recomputation only.

## Crate Placement

`liquers-core/tests/expiration_integration.rs` is the sole runtime evidence; issue/design records
belong under `specs/`. No crate changes are proposed. `expiration-safety` is related historical
context, not an established cause of the currently passing suite.

## Documentation Intent

**Reference:** Neither; no current runtime contract is changing.

**Guide:** Neither; the existing `CLAUDE.md` test guidance is sufficient for this scoped triage.

**Other documents to create:** This design folder, to preserve the evidence and closure rationale.

**Specific documents to update:** If the final rerun passes,
`specs/issues/EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD.md` will record the closing revision
and Cargo output and become `closed`; `specs/README.md` and generated `specs/index.csv` will be
regenerated to retain the design's normal map/index presence. A failing rerun leaves the issue open
and requires a separately scoped runtime-remediation effort.

## Open Questions

1. Does history identify the exact landing change, or is current passing evidence sufficient?
2. Should the core integration suite be added to the routine local test loop?

## References

- `specs/issues/EXPIRATION-INTEGRATION-SUITE-FAILING-AT-HEAD.md`
- `specs/design/expiration-safety/DESIGN.md`
- `liquers-core/tests/expiration_integration.rs`
