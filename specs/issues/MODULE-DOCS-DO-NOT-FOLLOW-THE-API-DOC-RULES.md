---
id: MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES
kind: issue
title: Existing module docs lack purpose, usage, related concepts and examples
status: draft
priority: P3
complexity: L
area: [docs, core/commands, core/value, lib/commands, axum, py]
design:
created: 2026-11-01
github:
---
## Problem

Once `design/api-documentation-rules/` defines the module documentation rules, the existing modules
do not meet them. At `15e06a4`, 85 of the 276 source files under `liquers-*/src/` have no `//!`
doc: `liquers-core` 11 of 64 (`commands.rs`, `command_metadata.rs`, `error.rs`, `metadata.rs`,
`state.rs`, `value.rs`, …), `liquers-lib` 46 of 87, `liquers-axum` 14 of 24, `liquers-py` 10 of 19,
`liquers-macro` 3 of 3, `liquers-store` 1 of 3. Most of the documented ones state a purpose but not
where the module is used, its related concepts or its examples.

## Impact

The rules apply only to new work until the existing docs catch up; the most-read modules
(`commands`, `value`, `state`, `metadata`) are among the undocumented ones.

## Expected behaviour

A rewrite, module by module, in the order the guide's rollout section gives (public entry modules
of `liquers-core` first), each PR small enough to review. Requires the guide
(`API-DOCS-HAVE-NO-CONTENT-RULES`); likely split into one issue per crate when started.

## Discovery

Counted on 2026-11-01 while starting `design/api-documentation-rules/` (GitHub #232, item 5 of the
owner's request). Filed separately because the rewrite is follow-up implementation, not design.
