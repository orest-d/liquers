# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None.
- **Explanation:** Derive enabled service config fields from Default plus Serialize behind per-service cfg gates, then add offline S3 construction tests.
- **Open questions:** None.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

Use borrowed config metadata where possible; the factory remains !Send-compatible and no network call is permitted in tests. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

- **overlaps:** `store-factories-in-core` — the completed broader design recorded discovery; this one owns the remaining source issue.

