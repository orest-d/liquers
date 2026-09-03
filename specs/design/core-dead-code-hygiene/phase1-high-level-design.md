# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None.
- **Explanation:** Re-audit the named modules, retain modules with callers, and close the stale issue with evidence rather than deleting live code.
- **Open questions:** None.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

HEAD shows entities and cache are live; the implementation is issue-record maintenance, not a code deletion. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

