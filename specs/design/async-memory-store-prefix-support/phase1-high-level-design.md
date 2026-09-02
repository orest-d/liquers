# Phase 1: High-Level Design - Memory-Store Prefix Support

## Feature Name

Memory-store support predicates respect prefixes

## Purpose

Make the asynchronous and synchronous memory stores report support only for absolute keys beneath
their configured prefix. This restores parity with the other store backends and makes direct
`is_supported` calls and layering composition agree with each store's advertised namespace.

## Core Interactions

### Query System
No language change. Reuse `Key::is_relative` and segment-wise `Key::has_key_prefix`.

### Store System
Change only `AsyncMemoryStore::is_supported` and the equivalent `MemoryStore` predicate. Preserve
the existing `Key::as_absolute()` enforcement in every fallible key-taking store operation.

### Command System
None.

### Asset System
No asset lifecycle change; correct support reporting only affects store selection and composition.

### Value Types
None.

### Web/API (if applicable)
None; configured routers already prefilter by prefix, so normal API routing is unchanged.

### UI (if applicable)
None.

## Crate Placement

`liquers-core/src/store.rs`: both memory-store implementations and focused regression tests already
live there. No public signature, dependency, or cross-crate change is required.

## Documentation Intent

**Reference:** Extend `specs/reference/STORE_SEMANTICS.md` in Phase 5 by replacing its warning with
the settled, tested prefix-support behavior; no new reference is needed.

**Guide:** Neither; this is backend conformance, not a repeatable user workflow. Reconsider only if
implementation reveals guidance beyond the existing absolute-key and store semantics references.

**Other documents to create:** None; tests and the existing reference are the durable evidence.

**Specific documents to update:** Link this design from
`specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md`; update `specs/README.md` now,
then close the issue and update `STORE_SEMANTICS.md` after implementation is verified.

Backend maintainers should see that absolute-key validity, prefix membership, and optional
backend-specific exclusions are cumulative parts of support reporting.

## Open Questions

None blocking. Phase 2 will verify whether any non-router composition implementation currently
consults these predicates and whether a shared helper would reduce rather than obscure the rule.

## References

- `specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md`
- `specs/reference/STORE_SEMANTICS.md` sections 6-7
- `specs/design/store-key-guard/`
