---
id: SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON
kind: issue
title: set_binary and set_state accept a supplied Expired status and store it without an expiry reason
status: closed
priority: P3
complexity: S
area: [core/assets]
design: supplied-expired-status-reason
created: 2026-10-02
github:
---
## Problem

`AssetManager::set_binary` and `set_state` (both managers, `liquers-core/src/assets.rs`) keep a
supplied `Status::Expired` as the final status (`Status::Expired => Status::Expired` in the
final-status match) and write it to the store. That is a route into `Expired` that does not go
through `AssetManager::record_expiry`, so the stored record has `status: Expired` and no
`expiry_reason`, and `Metadata::expiry_reason()` answers `None` for it. Every other route records a
reason since Step 4 of `dependency-audit-and-expiry-provenance`.

## Impact

Low. Only a caller that deliberately writes a value as `Expired` reaches it, and the asset behaves
correctly (it is a cache miss); only the "why" is missing. It does contradict the design's
invariant that every `Expired` asset carries a reason, which a test asserting that invariant
across all routes would trip over.

## Expected behaviour

Either a supplied `Expired` records a reason (`Direct { Explicit }` is the nearest existing cause,
or a new cause naming an externally supplied status), or `set_binary` / `set_state` refuse
`Expired` as an input status. Which one is a design decision.

## Discovery

Found 2026-10-02 while enumerating the routes into `Expired` for Phase 4 Step 4 of
`dependency-audit-and-expiry-provenance`; the step's call-site table does not list these two.

## Resolution (2026-10-07)

Fixed by design `supplied-expired-status-reason`, applying the maintainer decision of 2026-10-06:
a value written already expired logs a warning "Asset expired", then after-the-fact diagnostics.
Both managers' `set_state` and `set_binary` now append the warning and an info entry stating that
the expiry is recorded after the fact, naming the route and either "its original cause is unknown"
or the supplied reason. Interpretation, as stated in the design for review: the structured
`expiry_reason` is kept as supplied (usually `None`), because Liquers did not observe the cause.

Evidence: `liquers-core/tests/supplied_expired_status.rs` (four tests, each over both managers).
