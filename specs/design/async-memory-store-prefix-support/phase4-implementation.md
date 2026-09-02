# Phase 4: Implementation Plan - Memory-Store Prefix Support

## Overview

Change both memory-store predicates to require absolute keys beneath their configured prefix,
document the cumulative contract, and add six direct tests. Estimated complexity is low.

## Implementation Steps

### Step 1: Correct predicates and trait documentation

**File:** `liquers-core/src/store.rs`

Replace both bodies with:

```rust
!key.is_relative() && key.has_key_prefix(&self.prefix)
```

Update `Store` and `AsyncStore` rustdoc: support requires absoluteness, prefix membership, and any
narrower backend exclusions. Include the empty-prefix single-file overlay example and preserve
fallible-method enforcement.

**Validation:** `cargo check -p liquers-core`.

**Agent:** Sonnet-equivalent with rust-best-practices; needs Phase 2 and store/router source.

### Step 2: Add direct tests

**File:** `liquers-core/src/store.rs`, existing `tests` module.

Add the shared helper and `memsupport01`-`memsupport06` from Phase 3. Do not replace direct tests
with router tests, because routers already mask the missing memory-store prefix term.

**Validation:** `cargo test -p liquers-core --lib memsupport` and
`cargo test -p liquers-core --lib store::tests`.

**Agent:** Haiku-equivalent with liquers-unittest and rust-best-practices.

### Step 3: Validate and document

Run the full core suite and diff checks. In Phase 5 update `STORE_SEMANTICS.md`, close the source
issue with test evidence, update the conformance issue, remove the temporary capability entry, and
regenerate documentation indexes.

**Validation:** `cargo test -p liquers-core`, `git diff --check`, and documentation validators.

**Agent:** Sonnet-equivalent with the verified diff and documentation structure.

## Testing Plan

### Unit Tests

Six direct sync/async parity tests cover inside, outside, relative, empty, equal, and segment-
lookalike keys.

### Integration Tests

None added. Existing router tests remain regression coverage for composition.

### Manual Validation

Inspect both predicate bodies and trait rustdoc; no service or interactive workflow is involved.

## Agent Assignment Summary

| Step | Model | Skills |
|---|---|---|
| 1 | Sonnet-equivalent | rust-best-practices |
| 2 | Haiku-equivalent | liquers-unittest, rust-best-practices |
| 3 | Sonnet-equivalent | rust-best-practices, liquers-unittest |

## Rollback Plan

Revert only the predicate, rustdoc, test, and documentation hunks. No migration, dependency, or
public signature is introduced.

## Documentation Updates

Update the existing reference, source issue, and conformance issue. No guide, `CLAUDE.md`, or
project-overview change.

## Phase 5 Entry Criteria

- [x] Both predicates and rustdoc implemented.
- [x] Six focused tests and full core suite pass.
- [x] Documentation matches verified behavior.

## Execution Options

Executed now, followed by mandatory Phase 5 documentation.
