# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None.
- **Explanation:** Make an absent member prefix enumerate as an empty namespace at the AsyncFileStore/listdir boundary, retaining other errors.
- **Open questions:** None.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

Only KeyNotFound for the member root is normalized; permission and I/O failures still surface. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

- **overlaps:** `store-conformance-suite` — the completed broader design recorded discovery; this one owns the remaining source issue.

