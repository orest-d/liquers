# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** Whether directory metadata guarantees eagerly populated children despite its recursive cost.
- **Explanation:** Keep children populated, matching the trait default and seven implementations; document the cost and align AsyncOpenDALStore or explicitly scope its exception.
- **Open questions:** **Proposed resolution:** Keep children populated, matching the trait default and seven implementations; document the cost and align AsyncOpenDALStore or explicitly scope its exception.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

The proposed contract can require remote subtree walks; choosing lazy children would instead break Python/UI consumers. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

- **overlaps:** `store-conformance-suite` — the completed broader design recorded discovery; this one owns the remaining source issue.

