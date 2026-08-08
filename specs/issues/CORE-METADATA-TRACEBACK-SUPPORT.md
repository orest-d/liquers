---
id: CORE-METADATA-TRACEBACK-SUPPORT
kind: issue
title: Metadata has no place for an error traceback
status: draft
priority: P2
complexity: S
area: [core/value, core/error]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/metadata.rs:473` — `// TODO: Set/support traceback somehow`. When a command
fails, the error message survives into metadata but structured context does not.

## Impact

A failure is reported without the chain that produced it, so diagnosing a failed asset means
re-running it. This is the same shape as
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`, which loses an exception class and stack for the
same reason: `Error` has nowhere to put them.

## Expected behaviour

A structured, optional traceback field on the error metadata, additive and skipped when empty.
Solve it together with the language-context field the language-exception issue asks for — one field
design serves both.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #14, work package WP-2. Verified against HEAD: marker present at `metadata.rs:473`. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
