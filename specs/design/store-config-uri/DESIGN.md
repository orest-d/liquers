---
id: STORE-CONFIG-URI
kind: design
title: Configuring a store from a URI
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [store/config, store/backends, core/store]
gh_pr: []
issues: [STORE-CONFIG-FROM-URI]
affects_docs: [specs/reference/STORE_CONFIG_FSD.md, specs/guides/STORE_CONFIG_GUIDE.md, specs/guides/STORE_IMPLEMENTATION_GUIDE.md]
created: 2026-08-29
superseded_by:
---
# Configuring a store from a URI

Design for [`issues/STORE-CONFIG-FROM-URI.md`](../../issues/STORE-CONFIG-FROM-URI.md). Phases 1-2
were written on 2026-08-29 as a compatibility audit of `store-factories-in-core`; the design was
resumed on 2026-10-20 under the bulk-design procedure (`specs/guides/autonomous_bulk_design.md`),
re-checked against HEAD, and completed through Phase 4. Not approved; nothing implemented.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## History of the audit

The 2026-08-29 audit's purpose was to find what URI support would require of the factory design
before that design shipped. Its one non-additive finding — `StoreFactory::claims(&str) -> bool` had
to become `resolve(&StoreConfig) -> Option<String>`, because a store type may be inferred from the
entry — was adopted: `resolve` is at HEAD (`liquers-core/src/store_factory.rs`), with
`#[serde(default)]` on `StoreConfig::store_type`. `store-factories-in-core` is complete. Everything
this design still adds is additive.

Changed on resumption, against the 2026-08-29 text:
- `StoreConfig` lives in `liquers-core/src/store_config.rs` and gained `read_only` and a typed
  `metadata` section since; both are orthogonal to `uri`.
- The OpenDAL factory is `OpendalStoreFactory` in `liquers-store/src/store_factory.rs`
  (`resolve_enabled_scheme`, `Operator::via_iter`); `create_opendal_store` is gone.
- Variable expansion "per value after parsing" is not implementable — OpenDAL parses and consumes
  the URI in one call — so the question is re-opened with percent-encoded expansion recommended.
- `uri_schemes` feeds the default `resolve` instead of being documentation only: still one
  dispatch mechanism, and a factory may override it.

## Consolidated Findings

- The required trait change already landed; the remainder is two optional fields, one resolution
  rule, one error and the OpenDAL `from_uri` arms.
- OpenDAL's `OperatorUri::new(uri, extra)` percent-decodes path and query and lets extra options
  override query pairs, so `config:` beside `uri` and percent-encoded variables both fall out.
- A `uri:` key today is silently dropped (`StoreConfig` is not `deny_unknown_fields`) and the entry
  fails as `Unknown store type ''` — a confusing message the design replaces.
- `file://` for core's `filesystem`, never `fs://`, so OpenDAL's `fs` is not silently shadowed.
- Error messages must never echo a URI's query string.
