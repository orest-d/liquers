---
id: CORE-EVALUATE-PATH-CONSOLIDATION
kind: issue
title: Several evaluation paths duplicate each other
status: accepted
priority: P1
complexity: L
area: [core/assets, core/plan]
design: 
created: 2026-08-08
github:
---
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
