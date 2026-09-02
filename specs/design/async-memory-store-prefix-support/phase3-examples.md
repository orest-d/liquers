# Phase 3: Examples & Use-cases - Memory-Store Prefix Support

## High-Level Introduction

Runnable unit tests call both memory-store predicates directly, avoiding the router's independent
prefix guard. They prove the minimum support boundary while preserving the possibility of narrower
stores such as a single-file overlay.

## Example Type

**User choice:** Runnable inline unit tests in `liquers-core/src/store.rs`.

## Overview Table

| # | Scenario | Expected result |
|---|---|---|
| 1 | `data/report.txt` under prefix `data` | supported |
| 2 | `other/report.txt` under prefix `data` | unsupported |
| 3 | `data/../secret` under prefix `data` | unsupported as relative |
| 4 | `any/report.txt` under empty prefix | supported by memory store |
| 5 | key exactly equal to `data` | supported |
| 6 | `database/report.txt` under prefix `data` | unsupported by segment-aware matching |

## Example 1: Configured Prefix Boundary

```rust
fn memory_store_support(prefix: &Key, key: &Key) -> (bool, bool) {
    let sync_store = MemoryStore::new(prefix);
    let async_store = AsyncMemoryStore::new(prefix);
    (sync_store.is_supported(key), async_store.is_supported(key))
}

#[test]
fn memsupport01_absolute_key_inside_prefix_is_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    assert_eq!(memory_store_support(&prefix, &parse_key("data/report.txt")?), (true, true));
    Ok(())
}

#[test]
fn memsupport02_absolute_key_outside_prefix_is_not_supported() -> Result<(), Error> {
    let prefix = parse_key("data")?;
    assert_eq!(memory_store_support(&prefix, &parse_key("other/report.txt")?), (false, false));
    Ok(())
}
```

## Example 2: Absolute and Structural Boundaries

The remaining four tests cover a matching-prefix relative key, an empty prefix, equality with the
prefix, and the `data`/`database` segment boundary. Plain `#[test]` is correct because both trait
methods are synchronous.

## Example 3: Single-File Overlay

A single-file overlay may report an empty `key_prefix()` and implement `is_supported` as absolute
key validation plus equality with its one intercepted key. Placed before a general store in a
router, it receives that file only; every other key passes to subsequent stores. This example is
conceptual because no such store exists yet, but it defines why prefix checking does not make
store-specific filtering redundant.

## Corner Cases

### 1. Memory

The predicates borrow keys and allocate nothing.

### 2. Concurrency

No store data or lock is accessed.

### 3. Errors

The predicate returns `false`; fallible operations retain separate `Key::as_absolute()` errors.

### 4. Serialization

Not applicable.

### 5. Integration

No new integration test. Existing router tests protect selection, but direct tests are needed to
expose this defect because the router already checks prefixes.

## Documentation and Learning Log

The guide decision remains none. `STORE_SEMANTICS.md` owns the cumulative contract and overlay
rationale; the inline tests are its executable evidence.

## Test Plan

Add `memsupport01`-`memsupport06` as six named plain tests using one paired sync/async helper.

```bash
cargo test -p liquers-core --lib memsupport
cargo test -p liquers-core --lib store::tests
cargo test -p liquers-core
cargo fmt --all -- --check
```

## Commands, Queries, and Namespaces

None.

## Auto-Invoke: liquers-unittest Skill Output

Inline unit tests are appropriate for a synchronous trait predicate on two concrete types. No
environment, runtime, fixture file, mock, or dependency is required.
