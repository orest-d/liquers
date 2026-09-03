# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** Whether every command implementation-token change must require regenerating the committed registry.
- **Explanation:** Compare implementation versions as well as signatures and regenerate the checked-in registry, because impl_version is exported semantic data.
- **Open questions:** **Proposed resolution:** Compare implementation versions as well as signatures and regenerate the checked-in registry, because impl_version is exported semantic data.

## Problem, Behaviour, and Scope

The source issue documents the observed failure and acceptance evidence. The desired behaviour is the source's expected behaviour, with compatibility preserved unless Phase 2 explicitly states otherwise. Affected systems are limited to the inspected files below; implementation, migrations, and test changes remain out of scope for this design-only work.

## Constraints and Documentation

This makes comments and formatting in command functions part of generated-file maintenance by design. Current documentation that names the affected contract must be updated with implementation, while historical design records remain frozen.

## Design Dependencies

None.

