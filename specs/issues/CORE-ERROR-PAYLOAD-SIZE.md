---
id: CORE-ERROR-PAYLOAD-SIZE
kind: issue
title: `Error` is large enough to bloat every `Result`
status: draft
priority: P2
complexity: S
area: [core/error]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers_core::error::Error` carries its fields inline — message, position, query, key, command
key — so every `Result<T, Error>` in the workspace is at least that wide. The archived review
counted 421 clippy warnings on this.

## Impact

A pervasive, cheap-to-fix cost: every fallible call moves more bytes than it needs to, in a
codebase where almost every function is fallible.

## Expected behaviour

Box the payload — `Error(Box<ErrorInner>)` — keeping the public API unchanged. Re-run clippy to
confirm the count before and after; the number is the acceptance criterion.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-9. Verified against HEAD: not re-measured during triage — confirm the clippy count before scheduling. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
