# Phase 1: High-Level Design - keyed-delegation-hand-off

## Feature Name

keyed-delegation-hand-off

## Purpose

Make the keyed-recipe **delegation** branch of `AssetRef::evaluate_recipe` succeed. Today it
records a dependency edge from the delegating asset onto the key's registered owner; both ends of
that edge are the *same* key, so `would_create_cycle` reports a self-edge and delegation always
returns `Error::dependency_cycle` (`ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`, P0). Delegation is a
**hand-off between two assets that occupy one node of the dependency graph**, not a dependency
relation, so no edge should be recorded and the wait should proceed.

## Core Interactions

### Query System

None. No parsing, plan or key-encoding change; the branch is selected from an already-parsed key.

### Store System

Indirect only. A delegating asset that now receives a value will persist it under the key it was
already going to persist under — the same bytes the owner wrote. No store API changes. The
redundant second write is recorded as a follow-up issue rather than fixed here.

### Command System

None. No new commands, no registry change, no `specs/command_registry.yaml` regeneration.

### Asset System

The whole change. `AssetRef::record_dependency_on_asset` gains a same-node guard, and the
delegation call site in `AssetRef::evaluate_recipe` documents the hand-off semantics. Behaviour
downstream of the guard (`AssetManager::wait_for_dependency`, `enter_dependencies`,
`leave_dependencies_and_resume`) is already correct and is reached for the first time.

### Value Types

None.

### Web/API (if applicable)

None directly. `liquers-web` runs `ImmediateAssetManager`, which reaches the same branch, so the
fix applies there without wasm-specific code.

### UI (if applicable)

None.

## Crate Placement

`liquers-core` only — `liquers-core/src/assets.rs` plus its in-file test module, and
`liquers-core/tests/manager_parametric.rs`. The defect is in core asset lifecycle logic, which is
where dependency-graph semantics live; nothing about it is rich-value or backend specific.

## Documentation Intent

**Reference:** Extend an existing reference — `specs/reference/DEPENDENCIES_STATUS.md`. Its
"Issue F-1 and the implemented fix" section and its function glossary both state that delegation
records the delegated child in parent metadata and in `DependencyManager`. That becomes false, and
it is exactly the sentence that made the defect look intentional. No new reference is warranted:
the change is one rule inside an already-documented mechanism.

**Guide:** Neither. Delegation is not something a user or command author drives; there is no "how
do I …" workflow to write. The audience is a maintainer reading `assets.rs`, served by the
reference update and the code comment.

**Other documents to create:** None. The design folder plus the Phase 5 summary carry the
reasoning.

**Specific documents to update:**

- `specs/reference/DEPENDENCIES_STATUS.md` — correct the delegation description and the
  `record_dependency_on_asset` glossary entry; add a `## History` row and bump `reviewed:`.
- `specs/issues/ASSET-KEYED-DELEGATION-ALWAYS-CYCLES.md` — `status: closed` with a resolution note.
- `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` — its "the remainder is tracked as
  ASSET-KEYED-DELEGATION-ALWAYS-CYCLES" pointer now leads to a closed issue; check it still reads
  correctly.
- `specs/issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md` — it cites the spurious cycle as one of the
  outcomes a bare-key `apply` can produce; that outcome changes.
- `specs/index.csv` and `specs/README.md` — new design folder, changed issue status.

A future maintainer should be able to learn, without opening this folder, that *two assets holding
the same key are one dependency-graph node*, and that waiting on the key's owner is therefore a
hand-off with no edge to record.

## Open Questions

1. Should the delegating asset skip re-persisting the owner's value? It writes the same bytes the
   owner already wrote. Out of scope here (it is a separate defect in `evaluate_and_store`, not in
   the cycle check); to be filed as an issue.
2. Should the guard live in `record_dependency_on_asset` (general, applies to any caller) or at the
   delegation call site (narrow)? Resolved in Phase 2.

## References

- `specs/issues/ASSET-KEYED-DELEGATION-ALWAYS-CYCLES.md` (P0, accepted) — the issue being fixed.
- `specs/design/keyed-recipe-ownership/` — made the branch rare but not correct; filed this issue.
- `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` — the earlier, partly-overlapping report.
- `specs/reference/DEPENDENCIES_STATUS.md` — the reference this change contradicts.
- `liquers-core/src/assets.rs:1868` `evaluate_recipe`, `:1107` `record_dependency_on_asset`.
