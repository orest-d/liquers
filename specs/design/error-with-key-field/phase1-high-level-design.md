# Phase 1: High-Level Design - Error::with_key Populates the Key Field

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** HEAD confirms the builder writes the wrong existing field; the correction is
  local, signature-compatible, and directly testable.
- **Open questions:** None

## Problem and Evidence

`Error::with_key` in `liquers-core/src/error.rs` writes the encoded key into `ErrorPayload.query`
instead of `ErrorPayload.key`, making it indistinguishable from `with_query` and leaving `key`
empty for callers and serialized metadata.

## Expected Behaviour and Acceptance Criteria

Calling `with_key(&key)` sets `error.key` to `Some(key.encode())` and does not set `error.query`.
Dedicated constructors keep their documented fields, and serde output keeps separate `query` and
`key` meanings.

## Affected Systems

Core error handling, metadata error serialization and web/binding consumers that expose query and
key separately are affected. Query parsing, command dispatch and store implementations do not
change.

## Scope and Non-Goals

Scope is the builder bug plus tests around field separation. Do not redesign dependency-cycle
constructor semantics unless Phase 2 confirms they must change to keep the invariant coherent.

## Compatibility, Assumptions and Questions

This intentionally changes serialized field placement for errors enriched through `with_key`.
Assumption: no workspace caller relies on the incorrect query pollution.

## Documentation Assessment

No new reference or guide is expected. If an error reference exists, add one sentence clarifying
that `query` and `key` are separate serialized contexts.

## Design Dependencies

None.

## Consolidated Findings

Change only `Error::with_key`; dependency mismatch/cycle constructors intentionally populate both
fields and are not builder call sites. Future serialized errors place key context under `key`
instead of `query`; historical diagnostics require no migration. Unit tests must distinguish
`with_key`, `with_query`, serde output, and one real recipe/planner call path.

## Review

The issue is narrow, has direct evidence, and acceptance is unit-testable. The only known question
is whether two dependency constructors should continue mirroring key into query.
