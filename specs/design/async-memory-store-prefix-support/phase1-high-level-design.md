# Phase 1: High-Level Design - Memory-Store Prefix Support

## Feature Name

Memory-store support predicates respect prefixes

## Purpose

Make the asynchronous and synchronous memory stores report support only for absolute keys beneath
their configured prefix. `is_supported` remains independently useful for narrower store policies:
for example, an empty-prefix single-file overlay supports only its intercepted file and lets later
router stores handle everything else.

## Core Interactions

### Query System
No language change. Reuse `Key::is_relative` and segment-wise `Key::has_key_prefix`.

### Store System
Change `AsyncMemoryStore::is_supported` and `MemoryStore::is_supported`. Support is cumulative:
absolute key, configured-prefix membership, then any store-specific exclusions such as folders or
file types. Fallible operations retain their separate `Key::as_absolute()` enforcement.

### Command, Asset, Value, Web/API, and UI Systems
No behavior change. Routers already combine prefix membership with `is_supported`.

## Crate Placement

`liquers-core/src/store.rs`, beside both implementations and their inline tests. No public
signature, dependency, or cross-crate change.

## Documentation Intent

Extend `specs/reference/STORE_SEMANTICS.md`; no new reference or guide is required. Update the
source issue, the accepted store-conformance issue, trait rustdoc, and generated documentation
links. Phase 5 must explain the cumulative contract and the single-file overlay example.

## Open Questions

None. Prefix membership is necessary but not sufficient; `is_supported` may be narrower.
