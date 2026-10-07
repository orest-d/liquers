---
id: REPO-DEAD-CODE-HYGIENE
kind: issue
title: Dead modules and untracked files in the repository
status: closed
priority: P3
complexity: S
area: [build]
design: core-dead-code-hygiene
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

Migration triage, 2026-08-08. Source: work package WP-13. Verified against HEAD: **not re-verified** — check whether `entities.rs` and `cache.rs` have acquired callers before acting. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Resolution (2026-10-06)

Resolved by `design/core-dead-code-hygiene/`, under the issue's own rule ("used, deleted, or
carries a comment saying what it is for"). Audit at HEAD:

| Module | Callers | Outcome |
|---|---|---|
| `liquers-core/src/entities.rs` | `escape.rs` (`curated_name`, `lookup`, `compiled_count`), `bin/generate_entities.rs` (`codegen`) | live; unchanged |
| `liquers-core/src/cache.rs` | none inside `liquers-core`; `liquers-py/src/context.rs` (legacy `Environment.cache`, `with_cache`); `liquers-py/src/cache.rs` is an undeclared orphan (`PY-MODULES-NOT-DECLARED-IN-LIB`) | obsolete; now carries a module doc saying so; unused `use chrono::format;` removed |

Deleting `cache.rs` would change `liquers-py`'s public API, so it is assigned to
`CORE-SYNC-STORE-TRAIT-OBSOLETE`, which already removes the synchronous `Store` from the same
`Environment`. `reference/PROJECT_OVERVIEW.md`'s module table now describes `cache.rs` as legacy.

Evidence: `cargo check -p liquers-core`, `cargo check -p liquers-core --target
wasm32-unknown-unknown`, and `cargo check -p liquers-py --lib` (with
`PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1`, see `PY-PYO3-REJECTS-PYTHON-3-13`) all build.
