---
title: Architecture Reference
kind: reference
audience: internal
area: [core/query, core/plan, core/assets]
reviewed: 2026-03-02
---
# DOC-01: Architecture and API Reference Entry Point

Status: Complete  
Priority: P0  
Last reviewed: 2026-07-26

## Objective

Provide the top-level structure needed to navigate the Liquers API as a reference.
DOC-01 is not a user guide. It establishes:

1. The workspace crates and their public responsibilities.
2. The direct workspace dependency relationships.
3. The major concepts and their canonical Rust modules and types.
4. The relationships among the core runtime abstractions.
5. Feature and target-dependent API availability.
6. Links from the repository entry point to detailed reference material.
7. The authority and stability rules for existing documentation.

A separate user guide can later derive task-oriented workflows from the completed
API reference.

## Baseline findings

Before DOC-01, the root `README.md` contained only:

```text
# liquers
Next-gen Liquer Framework
```

The substantive overview was available only in `specs/reference/PROJECT_OVERVIEW.md` and
development instructions. There was no canonical index mapping concepts to public
modules and types. The available documents also did not clearly distinguish:

- Cargo dependency relationships from runtime relationships
- Core traits from their concrete implementations
- Public API from implementation internals
- Native APIs from feature-gated or WASM-specific APIs
- Current behavior from proposals and historical specifications

For coding agents, this made source discovery dependent on broad repository search
and encouraged stale specifications to be treated as authoritative.

## Scope

### Included

- Root API-reference navigation
- Workspace crate responsibility table
- Direct workspace dependency table
- Concept-to-module/type index
- Core type relationship diagrams
- Environment implementation comparison
- Feature and target availability summary
- Integration-surface inventory
- Documentation authority and maintenance rules

### Excluded

- Installation walkthroughs
- Application tutorials
- Step-by-step command authoring
- Task recipes
- Extended end-to-end examples
- Concept-specific method contracts

Those items belong in a future user guide or in later concept-specific API-reference
work.

## Decisions

### The root README is a reference entry point

The root README now provides a concise map of the public API. It does not attempt to
teach a workflow. Its main tables answer:

- Which crate defines a given API?
- Which workspace crates does that crate depend on?
- Which module contains a concept?
- Which public types are the primary entry points?
- Which feature or target controls availability?

### Rustdoc is the canonical method-level reference

The README points to generated rustdoc and provides commands for generating it.
Concept-specific work should improve crate and module rustdoc rather than building
a separate manually maintained method catalogue.

Markdown reference documents remain appropriate for contracts that span several
types or crates, such as:

- Query grammar
- Asset lifecycle
- Serialization invariants
- Store semantics
- HTTP representations

### Dependency and runtime relationships are separate

The reference lists direct workspace dependencies explicitly. It separately
describes the runtime path:

```text
Query
  -> Recipe
  -> Plan
  -> interpreter
  -> CommandExecutor
  -> State<Environment::Value>
```

This avoids implying a Cargo dependency from a conceptual runtime relationship.

### Public concepts are mapped to exact API paths

The concept index identifies primary types and modules, for example:

| Concept | Primary API |
|---|---|
| Query parsing | `liquers_core::parse::{parse_query, parse_key}` |
| Assets | `liquers_core::assets::{AssetRef, AssetManager}` |
| Commands | `liquers_core::commands::{CommandExecutor, CommandRegistry}` |
| Registration | `liquers_macro::register_command!` |
| Store configuration | `liquers_store::{StoreConfig, StoreRouterBuilder}` |

This exact mapping is particularly important for coding agents because it narrows
source search and reduces invented imports.

### Examples verify the reference but do not organize it

`liquers-core/examples/hello_world.rs` remains as a small integration check for the
relationship among:

- `SimpleEnvironment`
- `register_command!`
- `EnvRef::evaluate`
- `AssetRef::get`
- `State<Value>`

It is linked as supporting verification. It is not presented as the primary
documentation structure, and further tutorial development is deferred to the user
guide.

### Documentation authority is explicit

Until specification status metadata is introduced:

1. The compiled Rust public API and tests are authoritative.
2. Rustdoc should describe current public contracts.
3. Specifications without a status may contain proposals or historical material.
4. Conflicts must be resolved against the implementation and recorded in the
   relevant documentation analysis.

Factual claims in DOC-01 were checked against the workspace manifests, public source
modules, the repository `LICENSE`, generated rustdoc, and focused tests. The license
is the GNU Affero General Public License, version 3; it must not be abbreviated as
the GPL.

## Agent-performance considerations

| Failure mode | DOC-01 mitigation |
|---|---|
| Searching every crate for a concept | Concept-to-module/type index |
| Inventing an import path | Exact primary public API names |
| Assuming a conceptual relationship is a Cargo dependency | Direct dependency table separated from runtime flow |
| Using a type unavailable under the selected feature set | Feature and target availability table |
| Treating Python as a complete mirror of Rust | Python surface explicitly described as selected |
| Treating all specifications as current behavior | Documentation authority and stability notice |
| Selecting internal interpreter functions when an asset API is intended | Both abstraction levels are identified in the execution reference |

## Human-developer considerations

The reference entry point is organized for lookup rather than sequential reading:

1. Generate/open rustdoc.
2. Select a crate.
3. Locate a concept and its primary types.
4. Inspect relationships and feature availability.
5. Follow a link to a cross-cutting contract where necessary.

This structure is deliberately concise. Explanations of how to build a particular
application should not be added to DOC-01.

## Files changed

- `README.md`
- `liquers-core/examples/hello_world.rs`
- `liquers-core/src/lib.rs`
- `specs/reference/api/doc-01-architecture-reference.md`
- `specs/archive/2026-03-02-api-docs-gap-analysis.md`

## Verification

- [x] The root README maps all workspace crates to their API responsibilities.
- [x] The direct workspace dependency table agrees with crate manifests.
- [x] Major concepts map to concrete public modules and types.
- [x] Feature names and defaults agree with crate manifests.
- [x] The license name agrees with the repository `LICENSE` text.
- [x] `cargo run -p liquers-core --example hello_world` produces
  `Hello, world!`.
- [x] `cargo test -p liquers-core --lib` passes all 326 tests.
- [x] `cargo doc -p liquers-core --no-deps` completes without rustdoc warnings.
- [x] All affected relative Markdown links resolve.

## Remaining work outside DOC-01

- Method and type contracts are handled by the concept-specific DOC items.
- A complete feature/platform matrix belongs to DOC-13.
- Published rustdoc links can be added when hosted documentation exists.
- The future user guide should use the reference as its source of truth and reuse
  tested examples where appropriate.

## History

| Date | Change | Source |
|---|---|---|
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
