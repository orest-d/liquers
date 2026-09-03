# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** Whether the serialized field keeps its established wire name while the public Rust field is renamed.
- **Explanation:** Rename the Rust field to had_leading_slash and retain serde rename/alias for absolute, avoiding a stored-query migration.
- **Open questions:** **Proposed resolution:** Rename the Rust field to had_leading_slash and retain serde rename/alias for absolute, avoiding a stored-query migration.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

Public literal construction and bindings must migrate in lockstep; wire compatibility is preserved by serde. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

