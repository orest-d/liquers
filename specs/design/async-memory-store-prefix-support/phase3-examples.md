# Phase 3: Examples & Use-cases - Memory-Store Prefix Support

## High-Level Introduction

These examples turn the Phase 1 support-reporting rule into direct executable evidence for both
memory-store implementations. They progress from an ordinary store mounted at `data`, through the
root-store configuration, to two key shapes that expose incorrect string-prefix or absolute-key
logic. All examples call `is_supported` directly so the router's independent prefix check cannot
hide the defect.

## Example Type

**User choice:** Runnable unit tests, placed inline in the existing
`liquers-core/src/store.rs` `tests` module.

## Overview Table

| # | Type | Name | Purpose |
|---|---|---|---|
| 1 | Scenario | Mounted memory-store boundary | Proves equality and descendants are supported while an unrelated absolute key is rejected. |
| 2 | Scenario | Empty-prefix root store | Proves the empty prefix accepts every absolute key without weakening the absolute-key rule. |
| 3 | Scenario | Structural prefix pitfalls | Proves segment comparison rejects `database` and the absolute-key guard rejects `data/../secret`. |
| 4 | Unit tests | `memsupport01`-`memsupport06` | Gives both implementations identical, directly attributable regression coverage. |
| 5 | Existing regression checks | Router and `keyabs` tests | Confirms unchanged routing and absolute-key behavior without treating them as evidence for this fix. |

## Example 1: A Store Mounted at `data`

### Connection to the High-Level Design

A configured prefix advertises the namespace a store accepts. The representative case therefore
constructs both memory stores with `data`, asks the predicate about the prefix itself, a descendant,
and an unrelated absolute key, and requires the sync and async implementations to agree.

### Sequence of Steps

1. Parse the configured `data` prefix and a candidate key.
2. Construct `MemoryStore` and `AsyncMemoryStore` with that same prefix.
3. Call each trait's synchronous `is_supported` predicate directly.
4. Compare the paired result with the expected support decision.

### Core Example Code

The shared helper belongs inside the existing `#[cfg(test)] mod tests` block, where `super::*` and
`crate::parse::parse_key` are already imported:

```rust
fn memory_store_support(prefix: &Key, key: &Key) -> (bool, bool) {
    let sync_store = MemoryStore::new(prefix);
    let async_store = AsyncMemoryStore::new(prefix);
    (
        sync_store.is_supported(key),
        async_store.is_supported(key),
    )
}

#[test]
fn memsupport01_prefix_key_is_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    let key = parse_key("data")?;
    assert_eq!(memory_store_support(&prefix, &key), (true, true));
    Ok(())
}

#[test]
fn memsupport02_descendant_key_is_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    let key = parse_key("data/reports/summary.txt")?;
    assert_eq!(memory_store_support(&prefix, &key), (true, true));
    Ok(())
}

#[test]
fn memsupport03_key_outside_prefix_is_not_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    let key = parse_key("other/report.txt")?;
    assert_eq!(memory_store_support(&prefix, &key), (false, false));
    Ok(())
}
```

**Expected output:** all three tests pass with no stdout after the implementation is applied.

### Guide and Executable Example

No guide or standalone example is warranted because this is an implementor contract rather than a
user workflow. The complete `memsupport01`-`memsupport06` unit-test group will be the canonical
executable evidence linked by the updated `specs/reference/STORE_SEMANTICS.md`.

## Example 2: Empty Prefix Means Root Store

An empty prefix represents a root store. `Key::has_key_prefix(&Key::new())` deliberately accepts
every key structurally, but `is_supported` must still reject relative keys through its separate
absolute-key term. This scenario exercises the positive half with an ordinary absolute key; the
negative half appears in Scenario 3.

```rust
#[test]
fn memsupport05_empty_prefix_supports_absolute_key() -> Result<(), Error> {
    let key = parse_key("anywhere/report.txt")?;
    assert_eq!(memory_store_support(&Key::new(), &key), (true, true));
    Ok(())
}
```

**Expected output:** the test passes with no stdout. This preserves the current root-store default
while making nonempty prefixes meaningful.

## Example 3: Structural Prefix Pitfalls

Two plausible shortcuts would be wrong. A textual prefix comparison could treat `database` as a
child of `data`, and a prefix-only check could accept `data/../secret` because its first segment
matches. The correct expression uses segment-wise `has_key_prefix` together with `!is_relative()`.

```rust
#[test]
fn memsupport04_similar_segment_is_not_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    let key = parse_key("database/report.txt")?;
    assert_eq!(memory_store_support(&prefix, &key), (false, false));
    Ok(())
}

#[test]
fn memsupport06_relative_key_under_prefix_is_not_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    let key = parse_key("data/../secret")?;
    assert!(key.has_key_prefix(&prefix));
    assert!(key.is_relative());
    assert_eq!(memory_store_support(&prefix, &key), (false, false));
    Ok(())
}
```

**Expected output:** both tests pass with no stdout. The two preparatory assertions in
`memsupport06` prove that its fixture isolates the absolute-key term instead of accidentally
testing an out-of-prefix key.

## Corner Cases

### 1. Memory

The predicates borrow two `Key` values and perform no clone, map access, or payload allocation.
Input size only affects a bounded segment comparison, so large-data, allocation-failure, and leak
tests would not exercise the changed behavior.

### 2. Concurrency

Both methods read immutable `prefix` and input keys without taking either store's data lock. A
threaded or Tokio stress test would test no additional state transition. The async store's method
is intentionally synchronous, so all six regression tests use plain `#[test]` and require no
runtime.

### 3. Errors

`is_supported` is infallible. Relative and out-of-prefix keys return `false`; they do not construct
an `Error`. Existing fallible operations continue using `Key::as_absolute()` and are already
covered directly for both memory-store implementations by
`keyabs07_memory_stores_refuse_relative_keys`; router propagation is covered separately by
`keyabs10_routers_report_key_not_absolute`. Existing
`keyabs11_is_supported_false_on_directly_held_store` is direct relative-key predicate evidence,
not fallible-operation coverage.

The new tests intentionally do not call `get`, `set`, or other operations with out-of-prefix keys.
Those operations currently enforce absoluteness, not prefix membership, and changing that contract
is outside the source issue and approved Phase 2 architecture.

### 4. Serialization

No state, key encoding, metadata, or serialized format changes. The test inputs use `parse_key` so
they exercise the same segment representation as production callers; no round-trip test is needed.

### 5. Integration

No new integration test is planned. `StoreRouter::find_store` and `AsyncStoreRouter::find_store`
already check `has_key_prefix` before calling `is_supported`, so a router-only test passes before
this fix and cannot prove the direct predicate was corrected. Existing
`async_router_listdir_at_store_prefix` and `sync_router_listdir_at_store_prefix` remain regression
checks for unchanged router behavior. There is no command, asset, web endpoint, or cross-crate
behavior to exercise.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

No guide candidate emerged: users do not perform a new workflow, and backend implementors need a
contract rather than a tutorial. Phase 5 should link the canonical inline test group from
`STORE_SEMANTICS.md` instead of duplicating the snippets or creating an `examples/` target.

### Usage and Meaning

The examples make three cumulative facts visible: supported keys are absolute, belong to the
configured segment prefix, and may additionally be excluded by backend-specific policy. The first
two are the memory-store baseline fixed here; file-type or folder exclusions remain reserved for
individual backends.

### Repeatable Development Guidance

Future store implementations should test their predicate directly rather than relying on router
selection. Boundary cases should include equality, descendants, unrelated keys, similar segment
names, the empty prefix, and a relative key whose leading segment matches the prefix.

### Corrections and Unexpected Learning

- The repository currently has no non-router overlay or fallback implementation, despite stale
  trait documentation naming such helpers. Direct calls and future composition remain the reason
  each store must report its own support accurately.
- Router tests are useful regression coverage but are not proof of this change because the router
  independently filters by prefix.
- The Phase 1 decision not to create a guide still holds; the accumulated material belongs in the
  existing store semantics reference and canonical tests.

## Test Plan

### Unit Tests

**File:** `liquers-core/src/store.rs`, existing inline `tests` module.

Use the exact helper and six runnable tests shown in Scenarios 1-3. Six named tests are preferred to
one combined case matrix: each semantic boundary gets an independently searchable failure and can
be linked as evidence, while `memory_store_support` keeps sync/async setup and assertions paired.
The small amount of repeated `parse_key` setup is clearer than a table with opaque row diagnostics.

| Test | Contract boundary |
|---|---|
| `memsupport01_prefix_key_is_supported` | Key equal to nonempty prefix |
| `memsupport02_descendant_key_is_supported` | Descendant of nonempty prefix |
| `memsupport03_key_outside_prefix_is_not_supported` | Unrelated absolute key |
| `memsupport04_similar_segment_is_not_supported` | Segment-wise `data` versus `database` |
| `memsupport05_empty_prefix_supports_absolute_key` | Root-store positive case |
| `memsupport06_relative_key_under_prefix_is_not_supported` | Matching prefix cannot override relativity |

Plain `#[test]` is deliberate for every case. Both trait signatures are synchronous boolean
predicates; using `#[tokio::test]` for `AsyncMemoryStore` would add runtime setup without awaiting
anything and would obscure that part of the API contract.

### Integration Tests

None added. The defect is isolated to two trait method bodies in `liquers-core`, and direct inline
unit tests are the smallest tests that fail before the fix and pass afterward. Preserve the
existing router and `keyabs` tests, but do not add a router-only duplicate or a cross-crate test.

### Validation Commands

```bash
# Focused new regression tests
cargo test -p liquers-core --lib memsupport

# Existing inline store test module
cargo test -p liquers-core --lib store::tests

# Full affected crate
cargo test -p liquers-core

# Formatting after Phase 4 inserts the tests and implementation
cargo fmt --all -- --check
```

Expected result: all commands exit successfully. Before the implementation change, the focused
tests for out-of-prefix keys are expected to fail, demonstrating that they detect the issue.

## Commands, Queries, and Namespaces

None. The scenarios contain no Liquers query, registered command, or command namespace, so query
syntax, resource-store setup, and command-registration validation do not apply.

## Auto-Invoke: liquers-unittest Skill Output

The `liquers-unittest` guidance selects inline unit tests because the subject is a single type
method and requires no environment. It also supports `Result<(), Error>` for fallible fixture
parsing, descriptive test names, direct boolean assertions, and plain `#[test]` for synchronous
methods. No integration environment, payload type, command registry, async runtime, mock, fixture
file, or additional dependency is required.
