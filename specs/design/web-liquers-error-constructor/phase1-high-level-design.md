# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** Whether JavaScript construction may set optional key and query provenance or only type and message.
- **Explanation:** Expose a two-argument wasm constructor validated by error_type_from_name; leave key/query Liquers-populated.
- **Open questions:** **Proposed resolution:** Expose a two-argument wasm constructor validated by error_type_from_name; leave key/query Liquers-populated.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

A TypeError for unknown type names prevents silent downgrade; adding optional provenance later remains additive. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

