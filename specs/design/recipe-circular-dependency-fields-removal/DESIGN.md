---
id: RECIPE-CIRCULAR-DEPENDENCY-FIELDS-REMOVAL
kind: design
title: Retire the unused Recipe circular-dependency fields
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, records]
issues: [RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD]
created: 2026-10-26
---
# Retire the unused Recipe circular-dependency fields

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`Recipe` carries two public, always-serialized fields that nothing sets or reads. They mislead a
provider author into thinking setting them has an effect, and add noise to every serialized recipe
(HTTP Recipes API, `recipes.yaml` written by tools). Either they get a real producer and consumer, or
they go.

### Problem Example

Verified at HEAD (`cd6d5da`). `Recipe::has_circular_dependencies: bool` and
`Recipe::circular_dependency_key: Option<Key>` (`liquers-core/src/recipes.rs`) have `#[serde(default)]`
but no `skip_serializing_if`. Every constructor (`Recipe::new`, the four `From` impls,
`liquers-records/src/provider.rs` `template_chunk_recipe`) writes `false` / `None`; no code reads
them. Cycles are found by `analyze_plan_dependencies` and reported as `Plan::error`.

```rust
let recipe = Recipe::new("-R/data/sales.csv/-/to_text/sales.txt".into(), "".into(), "".into())?;
serde_yaml::to_string(&recipe)?
// today:  query: -R/data/sales.csv/-/to_text/sales.txt
//         has_circular_dependencies: false
//         circular_dependency_key: null
//         (plus the always-serialized `expires`)
// should: neither circular field appears; a document that still carries them deserializes.
```

The fields came from a maintainer review comment on `volatility-system` Phase 4 ("Recipe should
have a flag has_circular_dependencies … that should prevent unnecessary re-evaluation of
circularity"); the producer was never built, and cycle detection moved into dependency analysis.

### Scope and Acceptance Criteria

- **AC-1** Serialized recipes no longer carry the fields
  WHEN any `Recipe` is serialized (YAML or JSON)
  THEN the output has no `has_circular_dependencies` and no `circular_dependency_key`
- **AC-2** Old documents still load
  WHEN a `recipes.yaml`, manifest-derived recipe or JSON recipe containing either field is deserialized
  THEN it loads, the fields are ignored, and the recipe behaves as before
- **AC-3** Cycle reporting is unchanged
  WHEN a recipe depends on itself through another key
  THEN its plan still carries the existing "Circular dependency detected" `Plan::error`
- **AC-4** No Rust code refers to the fields
  WHEN the workspace builds
  THEN no crate sets or reads them (they no longer exist on `Recipe`)

Non-goals: caching dependency-analysis results (that is
`DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS`); changing cycle detection.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — removes two `pub` fields of `liquers_core::recipes::Recipe`
  and changes its serialized form (rule 4); touches `liquers-core` and `liquers-records` (rule 6).
- **Leading issue:** question 1.
- **Explanation:** removal is fully specified and serde already ignores unknown fields on `Recipe`
  (no `deny_unknown_fields`, not flattened into a strict struct), so old documents keep loading. The
  fields exist because of a maintainer request, so retiring them is the maintainer's call.
- **Open questions:**
  1. **Open design - remove or implement:** (a) remove both fields (**recommended**: the
     "avoid re-checking circularity" goal now belongs to a derived, invalidatable summary cache —
     `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS` — not to user-authored recipe
     data, which cannot be invalidated when an upstream recipe changes); (b) keep them, add
     `skip_serializing_if` and mark them `#[deprecated]` (hides the noise, keeps the misleading
     API); (c) implement a producer in `DefaultRecipeProvider` (runs dependency analysis per
     listed recipe at load time, duplicating `analyze_plan_dependencies` and going stale on any
     upstream edit).

### Design Dependencies

- `DEPENDENCY-ANALYSIS-SUMMARIES-NOT-REUSED-ACROSS-EVALUATIONS` (issue, no design) — **overlaps**:
  if it chose to store summaries on `Recipe`, option (c) would be its producer. Its own text points
  at per-key summaries outside the recipe; no order.
- `volatility-system` (complete) — origin of the fields; E4, not extended.
- `record-streams` (complete) — `manifest-format.md` lists the fields as provider-supplied; E4.
  The reference documents are what change.

### Consolidated Findings

- Constructor sites: `liquers-core/src/recipes.rs` (`Recipe::new`, `From<&Query>`, `From<Query>`,
  `From<Key>`, `From<&Key>`) and `liquers-records/src/provider.rs` `template_chunk_recipe`.
  `explicit_chunk_recipe` clones a manifest-deserialized `Recipe` and needs no edit; a manifest that
  still carries the fields loads them into nothing (AC-2).
- No Python, JavaScript or HTTP test names the fields (repository grep); the HTTP Recipes API
  response simply loses two keys.
- `specs/reference/RECIPES_AND_PLANS.md` lists the fields as "Reserved"; that row goes.

## Phase 2: Architecture

### Solution

Under the recommended option (a):

- `liquers-core/src/recipes.rs` `Recipe`: delete `has_circular_dependencies` and
  `circular_dependency_key` and their doc comments; delete their initializers in `Recipe::new` and
  the four `From` impls. Serde's default (ignore unknown fields) gives AC-2 with no attribute.
- `liquers-records/src/provider.rs` `template_chunk_recipe`: delete the two initializers.
- Any other struct literal found by `grep -rn "has_circular_dependencies\|circular_dependency_key"`
  at implementation time: delete the initializer.

Under option (b) instead: add `#[serde(skip_serializing_if = "is_false")]` /
`#[serde(skip_serializing_if = "Option::is_none")]` and `#[deprecated(note = "…")]` (with
`#[allow(deprecated)]` at the constructors); same tests minus AC-4.

Rejected: option (c), see Phase 1 question 1.

### Changes

- `liquers-core/src/recipes.rs`: `Recipe` fields, `Recipe::new`, `From<&Query>`, `From<Query>`,
  `From<Key>`, `From<&Key>`.
- `liquers-records/src/provider.rs`: `template_chunk_recipe`.
- Breaking for Rust code that names the fields (none in the workspace); not breaking for documents.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` (remove the "Reserved" row and the Known
  limitations link; History row; `reviewed:`), `specs/reference/PROJECT_OVERVIEW.md` only if it
  lists the fields (check).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An external crate sets the fields | pre-1.0; compile error is explicit; option (b) if the maintainer wants a deprecation cycle | medium |
| A strict deserializer somewhere rejects old documents | `Recipe` has no `deny_unknown_fields` and is not flattened into `MetadataRecord`; AC-2 test | high |
| A golden YAML/JSON fixture contains the fields | repository grep finds none | high |

Recovery: revert; old documents never stopped loading.

## Phase 3: Examples and Tests

### Examples

The Problem Example: `serde_yaml::to_string(&Recipe::new(…)?)` contains neither field;
`serde_yaml::from_str::<Recipe>("query: a/b.txt\nhas_circular_dependencies: true\ncircular_dependency_key: x/y\n")`
is `Ok`.

### Tests

In `liquers-core/src/recipes.rs` `mod tests`:

- `recipe_serialization_has_no_circular_fields` — YAML and JSON of `Recipe::new` and
  `Recipe::from(&key)` contain neither name. Fails at HEAD. Covers AC-1.
- `recipe_with_retired_circular_fields_still_deserializes` — YAML and JSON with both fields load;
  `get_query()` works; a `RecipeList` with such an entry loads. Covers AC-2.
- Existing cycle tests in `liquers-core/src/plan.rs` and `liquers-core/tests/deep_chain_evaluation.rs`
  that assert "Circular dependency detected" stay green (re-run, no edit). Covers AC-3.
- `cargo build -p liquers-records -p liquers-lib` compiling is AC-4's proof (no test).

Run: `cargo test -p liquers-core --lib recipes`, `cargo test -p liquers-records --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/recipes.rs` `mod tests` — add the two tests; the first fails at HEAD — `cargo test -p liquers-core --lib recipes`. Rollback: delete them.
- [ ] 2. `liquers-core/src/recipes.rs` `Recipe` and its five constructors; `liquers-records/src/provider.rs` `template_chunk_recipe`; any other literal found by grep — depends on 1 — `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-records --all-features --lib --tests`. Rollback: revert both files.
- [ ] 3. `specs/reference/RECIPES_AND_PLANS.md` (row, Known limitations, History, `reviewed:`); close `RECIPE-CIRCULAR-DEPENDENCY-FIELDS-ARE-DEAD` with the decision taken — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-records --all-features --lib --tests`;
`cargo test -p liquers-lib --lib --tests`; `bash scripts/test-liquers-py.sh` only if
`liquers-py` names a `Recipe` field (it does not at HEAD); diff review: no edit outside the two
source files, the reference and the issue.
