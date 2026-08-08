# KEY-LEVEL-ACL

Status: Draft

## Summary
Introduce authorization checks for write operations (`set()` / `set_state()`) based on key patterns and caller principal.

## Problem
Write operations currently have no key-level access control, which is unsuitable for multi-tenant and production environments.

## Goals
1. Restrict write access by key pattern.
2. Support principal-aware policy checks.
3. Provide auditable allow/deny outcomes.

## Non-Goals
1. Replacing full authentication stack.
2. Designing generic policy engine integration in first phase.

## Proposed Scope
1. Add ACL abstraction in core asset-manager write path.
2. Define principal extraction contract for API layer.
3. Evaluate write permission before `set()` / `set_state()` mutates data.
4. Log authorization decisions with key + principal context.

## Policy Model (Phase 1)
1. Pattern-based allow lists for write operations.
2. Default policy configurable (`allow-all` for local/dev, `deny-all` for hardened setups).

## API/Behavior
1. Unauthorized writes fail with typed authorization error.
2. HTTP integration maps to `403 Forbidden`.

## Acceptance Criteria
1. Writes to restricted patterns are blocked for unauthorized principals.
2. Authorized writes continue unchanged.
3. Decision logs are recorded for both allow and deny paths.
4. Core + API tests validate policy behavior.
