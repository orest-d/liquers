# Phase 1: High-Level Design - Structured Error Context
## Feature Name
Structured Error Context for Keys and Nested Queries
## Purpose
Preserve the distinct roles of asset/recipe keys, evaluated queries, nested resource/link queries,
actions, and positions instead of overwriting one flat error slot during propagation.
## Core Interactions
- **Query:** key-to-query conversion remains lossless; `Query::key()` tests pure-key extraction.
- **Store:** typed keyed failures and persistence warnings retain the accessed resource key.
- **Commands:** no new commands; action identity and position remain associated with their query.
- **Assets:** every keyed asset/recipe boundary adds its owner key without replacing an inner key.
- **Values/UI:** no value-type change; optional clickable text is derived rendering, not storage.
- **Web/API:** serde, metadata, web, Python, and Axum receive an explicit compatibility contract.
## Crate Placement
Core structure and enrichment belong in `liquers-core`; stores attach access context, while
`liquers-web`, `liquers-py`, and `liquers-axum` expose the selected representation.
## Documentation Intent
- **Reference:** create `specs/reference/ERROR_CONTEXT.md` for the authoritative contract.
- **Guide:** extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` for boundary mapping workflows.
- **Other documents:** none; the design and source issue retain rationale and unfinished work.
- **Updates:** `PROJECT_OVERVIEW.md`, `ASSETS.md`, `ASSET_LIFECYCLE.md`,
  `WEB_API_SPECIFICATION.md`, and `specs/README.md`; Phase 2 defines exact changes and audience.
## Open Questions
1. **Blocking:** model, roles/order/dedup/bounds, legacy projection, binding exposure, and rendering;
   Phase 3/4 cannot be valid until these are chosen. This is why readiness is `phase2-blocked`.
2. **Non-blocking:** exact helper names and renderer presentation follow the public contract.
## References
- `specs/issues/ERROR-WITH-KEY-SETS-QUERY-FIELD.md`; coordinate `ASSETS-IMPROVEMENTS` and related
  error-payload designs during Phase 2.
