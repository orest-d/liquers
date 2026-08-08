---
id: MACRO-QUERY-VALIDATION-AND-HINTS
kind: issue
title: `register_command!` does not validate queries or implement hints
status: draft
priority: P3
complexity: M
area: [macro]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-macro/src/registration.rs:36` — `// TODO: Validate query` on the default-value literal
parser; `:1002` — `Hint(String, String), // TODO: Implement hints`; `:1610` — the hint statement is
matched and discarded.

## Impact

A malformed `query "…"` default reaches runtime instead of failing the build, which is exactly the
class of error `liquers_core::validate` now exists to catch — the macro could call it. Hints are
accepted syntax that does nothing, which is worse than not accepting them.

## Expected behaviour

The macro validates query literals at expansion time and either implements hints or rejects them.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #16, work package WP-15. Verified against HEAD: markers present — but **moved** from `lib.rs` to `registration.rs`, so the audit path is stale while the issue is live. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
