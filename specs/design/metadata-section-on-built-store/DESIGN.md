---
id: METADATA-SECTION-ON-BUILT-STORE
kind: design
title: Wrapping an already-built store in the pipeline its metadata section describes
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/config]
issues: [STORE-METADATA-SECTION-CANNOT-WRAP-A-BUILT-STORE]
affects_docs: [guides/STORE_CONFIG_GUIDE.md]
created: 2026-11-01
---
# Wrapping an already-built store in the pipeline its metadata section describes

Compact design, 2026-11-01. Spun off `store-config-handlers-usage-docs` (GitHub #230). Phases 1-4
written, not approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`StoreRouterBuilder` applies a mount's `metadata` section automatically; code that already holds a
store cannot. Give it the same one call, so handlers are automatic wherever a store comes from.

### Problem Example

```rust
let backend: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
let section = MetadataConfig { on_write: Some("defaults/set-title-Hello".into()), ..Default::default() };
// Today: no public call turns (backend, section) into a store. The caller copies
// store_factory::pipeline: PipelineParts::new, compile_component ×2, compile ×4,
// version_verification — as tests/external_change_integration.rs does.
```

Should: `let store = MetadataPipelineStore::from_section(backend, &section, false, &cmr, &functions)?;`

### Scope and Acceptance Criteria

- **AC-1** Same behaviour as a configured mount
  WHEN a backend with data-only access and a non-empty section are passed to the new call
  THEN the returned store reads and writes exactly as the same section on a `StoreRouterBuilder`
  mount (same records, same warnings)
- **AC-2** A backend without data access is refused
  WHEN the backend's `data_capabilities().data_access` is false
  THEN the call fails with the builder's configuration error
- **AC-3** One path
  WHEN `StoreRouterBuilder` builds a mount with a section
  THEN it goes through the new call, and the two integration tests that build a pipeline by hand
  use it instead

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` item to `liquers-core` (rule 4).
- **Leading issue:** question 1.
- **Explanation:** Small and mechanical once the shape is chosen.
- **Open questions:**
  1. **Shape (normal).** (a) `MetadataPipelineStore::from_section(backend, &MetadataConfig,
     read_only, &registry, &functions)`, recommended: it sits on the type it returns. (b) make
     `store_factory::pipeline` public as `metadata_pipeline(...)`. (c) also an
     `EnvironmentBuilder::with_async_store_and_metadata(store, section)` that uses the
     environment's own registry, which (a) alone cannot.
  2. **Backups (minor).** Central-storage backups stay builder-only; the call takes none.

### Design Dependencies

None.

## Phase 2: Solution and Architecture

Move the body of `store_factory::pipeline` (minus `backups`) into `from_section` in
`store/pipeline.rs`; `pipeline` becomes a call plus `parts.backups`. Notes the component
compilers return are returned or printed as the builder does today (decision with question 1).

## Phase 3: Examples and Tests

| Case | Proof |
|---|---|
| AC-1 | Unit test in `store/pipeline.rs`: the same section through the call and through a builder mount, comparing the records of a write and a read |
| AC-2 | Unit test: `NoAsyncStore` as backend, asserting `ErrorType` |
| AC-3 | The existing `tests/store_adapters.rs` suite and conformance `A1`–`A7` pass unchanged; the two integration tests compile against the call |

The doc example of `store::pipeline` gains the direct form.

## Phase 4: Implementation Plan

### Steps

- [ ] Step 1: add the call; route `store_factory::pipeline` through it
- [ ] Step 2: replace the hand-built pipelines in the two integration tests
- [ ] Step 3: tests; `STORE_CONFIG_GUIDE.md` §1 names the call, with a History row
