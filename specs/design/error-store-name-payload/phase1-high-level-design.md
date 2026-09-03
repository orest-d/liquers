# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None.
- **Explanation:** Add optional store provenance to ErrorPayload and builders; existing constructors populate it without changing their signatures.
- **Open questions:** None.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

An optional serialized field preserves old payload deserialization and boxed Error size. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

