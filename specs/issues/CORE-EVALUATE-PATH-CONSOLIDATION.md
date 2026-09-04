---
id: CORE-EVALUATE-PATH-CONSOLIDATION
kind: issue
title: Several evaluation paths duplicate each other
status: closed
priority: P1
complexity: L
area: [core/assets, core/plan]
design: evaluate-path-consolidation
created: 2026-08-08
github:
---

## Resolution

Closed 2026-09-04 by `design/evaluate-path-consolidation/`. `AssetRef` now has one private
`evaluate(payload)` reached by two run harnesses (spawning and spawn-free — a platform split, not
duplication) behind four manager entry-point implementations, down from two bodies, four run entry
points and six implementations. Dependency recording is unconditional in that body, so it no longer
depends on which entry point was used.

The work also unified a second duplication it uncovered: the store **write** target and the
**invalidation** target were independent derivations with opposite precedence, disagreeing for
volatile keyed assets, so an asset could be written under a key it could never invalidate. Both now
read the key recorded on the asset at construction.

See `phase5-documentation.md` for what was omitted and the six issues filed.
## Problem

Evaluation happens through more than one route — the asset lifecycle, the immediate path, and
`apply` — and they do not record dependencies identically. `Context::apply` is documented as
bypassing dependency tracking altogether.

## Impact

A behaviour fixed on one path stays broken on another, and which path a query takes is not always
obvious from the query. The `ASSETS-FIX1` brief reaches the same conclusion from the other
direction, cataloguing markers in `assets.rs`.

## Expected behaviour

One evaluation path, with the others as thin wrappers, and dependency recording that does not
depend on which entry point was used.

Wants a design: it is the largest structural change in the backlog.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-8, with the `ASSETS-FIX1` brief. Verified against HEAD: the immediate path and `apply` are still distinct. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
