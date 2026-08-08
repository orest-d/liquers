---
id: REPO-DEAD-CODE-HYGIENE
kind: issue
title: Dead modules and untracked files in the repository
status: draft
priority: P3
complexity: S
area: [build]
design: 
created: 2026-08-08
github:
---
## Problem

WP-13 identified `entities.rs` and `cache.rs` as candidates for removal, and untracked spec files
that should have been committed. The spec-file half is resolved by this migration; the dead-module
half was not re-verified.

## Impact

Small. Dead code misleads readers about what the system does and is carried by every build.

## Expected behaviour

Each module is either used, or deleted, or carries a comment saying what it is for.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-13. Verified against HEAD: **not re-verified** — check whether `entities.rs` and `cache.rs` have acquired callers before acting. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
