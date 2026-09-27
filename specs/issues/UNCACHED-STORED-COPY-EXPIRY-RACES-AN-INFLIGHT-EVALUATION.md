---
id: UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION
kind: issue
title: Expiring an uncached key's stored copy can be overwritten by an evaluation already in flight
status: draft
priority: P3
complexity: M
area: [core/assets]
design: record-streams
created: 2026-09-27
github:
---
# Expiring an uncached key's stored copy can be overwritten by an evaluation already in flight

## Problem

A `cached: false` keyed asset is not registered for reuse. Since the implementation review,
`expire_dependencies_result` (`liquers-core/src/assets.rs`, `expire_stored_copy`) therefore marks
the key's **stored copy** `Expired` directly when an upstream change reaches it. An uncached
evaluation of the same key already in flight can finish afterwards and write `Ready` over that mark.

In-process, the fast-track version check still notices the stale dependency. After a restart it
does not: the stored copy reads as a valid `Ready` value computed from the old input.

## Expected behaviour

The write-back of an uncached evaluation checks that its dependencies' versions are still the ones
it read, or expiry and write-back are ordered through the key's mutation lock, so a copy expired
mid-evaluation is not revived.

## Discovery

Found 2026-09-27 while fixing the implementation review's finding C1 (a `cached: false` asset kept
out of the dependency graph).
