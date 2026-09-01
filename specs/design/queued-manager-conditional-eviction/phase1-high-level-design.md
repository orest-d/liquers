# Phase 1: High-Level Design - Conditional Queued-Manager Cache Eviction

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** `scc::HashMap::remove_if_async` supplies the required atomic primitive and the
  stale compare/drop/remove sequences remain present at HEAD.
- **Open questions:** None

## Problem and Evidence

`DefaultAssetManager` compares an asset id under an `scc` entry guard, drops the guard, and then
removes by key in several stale-terminal eviction paths. A replacement inserted between compare
and remove can be deleted even though its id differs.

## Expected Behaviour and Acceptance Criteria

Each stale eviction removes the map entry only if the entry still has the stale asset id at removal
time. A replacement asset under the same query or key survives the stale asset's cleanup.

## Affected Systems

The queued/default asset manager's query and key cache maps are affected. Immediate manager,
command execution semantics, value serialization, store contents and query syntax should not
change.

## Scope and Non-Goals

Scope is replacing compare-then-remove sequences with atomic conditional removal helpers. This
does not redesign expiration scheduling, asset ids or cache policy.

## Compatibility, Assumptions and Questions

Behaviour remains compatible except for preserving replacements that were previously vulnerable.
Assumption: `scc::HashMap::remove_if_async` is available and is the intended primitive.

## Documentation Assessment

No new reference or guide is expected. If the asset reference documents discuss eviction
concurrency, add a short note; otherwise Phase 5 can record no docs update needed.

## Design Dependencies

- `overlaps` `asset-manager-insert-key-asset-semantics`: that completed design established
  identity-safe key mutation and its `remove_key_asset_if` pattern should be reused.

## Consolidated Findings

Key and query maps need one atomic identity predicate each. The key path must preserve the existing
`key_mutation_lock` ordering used for durable key mutation, while the query path can use
`remove_if_async` directly. Sequential identity tests prove decisions; a deterministic concurrent
test must coordinate replacement between observation and attempted stale cleanup without sleeps.

## Review

The scope is cohesive, local to `core/assets`, matches an existing fixed helper pattern, and has a
concurrency acceptance test.
