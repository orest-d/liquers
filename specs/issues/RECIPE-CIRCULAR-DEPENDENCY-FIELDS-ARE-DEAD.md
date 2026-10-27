---
id: RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD
kind: issue
title: The recipe circular-dependency fields are never set or read
status: draft
priority: P3
complexity: S
area: [core/assets]
design: recipe-circular-dependency-fields-removal
created: 2026-10-20
github:
---
## Problem

`Recipe::has_circular_dependencies` and `Recipe::circular_dependency_key`
(`liquers-core/src/recipes.rs`) are public, always serialized (no `skip_serializing_if`), and
documented as a provider's validation result. No code in any crate sets them to anything but
`false`/`None` or reads them. Cycles are detected by `analyze_plan_dependencies` and reported as
`Plan::error` ("Circular dependency detected: …").

## Problem Example

Re-checked at HEAD (`cd6d5da`, 2026-10-26): unchanged; the only writers are `Recipe::new`, the four
`From` impls and `liquers-records/src/provider.rs` `template_chunk_recipe`, all writing
`false`/`None`. `serde_yaml::to_string(&Recipe::new("-R/data/sales.csv/-/to_text/sales.txt".into(), …)?)`
emits `has_circular_dependencies: false` and `circular_dependency_key: null`. The fields were asked
for in a maintainer review of `volatility-system` Phase 4 (`review-comment-to-phase4.md`), so
removing them is a maintainer decision.

## Impact

Every serialized recipe carries two meaningless fields, and a provider author may believe
setting them has an effect.

## Expected behaviour

Remove the fields (keeping deserialization tolerant of old documents), or give them a real
producer and consumer.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
