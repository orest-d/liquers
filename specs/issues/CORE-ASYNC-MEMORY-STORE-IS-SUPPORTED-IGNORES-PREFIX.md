---
id: CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX
kind: issue
title: AsyncMemoryStore::is_supported claims keys outside its own prefix
status: closed
priority: P1
complexity: S
area: [core/store]
design: async-memory-store-prefix-support
created: 2026-09-02
github:
---
## Problem

`AsyncMemoryStore` and the synchronous `MemoryStore` reported every absolute key as supported,
including keys outside their configured prefix. Routers happened to mask the defect with their own
prefix prefilter, but direct callers and composition mechanisms must be able to trust each store's
answer independently.

## Impact

A support predicate is allowed to be narrower than its prefix. For example, an empty-prefix
single-file overlay can return true only for one intercepted file and allow later router stores to
handle every other key. A predicate that omits its own prefix boundary does not truthfully describe
the store's supported namespace.

## Expected Behavior

Support is cumulative: the key is absolute, begins with `key_prefix()`, and passes any additional
store-specific exclusions such as folders, file types, path collisions, or a single-file allowlist.
Fallible operations continue enforcing absolute keys independently.

## Resolution

Both memory-store predicates now use
`!key.is_relative() && key.has_key_prefix(&self.prefix)`. `memsupport01`-`memsupport06` cover prefix
descendants, outside keys, relative keys, the empty prefix, equality, and segment boundaries for
both implementations. Trait and reference documentation now explain the cumulative contract and
the single-file overlay use case.

## Discovery

Found while enumerating `AsyncStore` contract divergences. Resolved by
`design/async-memory-store-prefix-support/`.
