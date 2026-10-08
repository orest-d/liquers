# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes documented semantics rather than restoring them (rule
  2); also needs-decision (rule 5)
- **Leading issue:** **Open design question — does a predecessor's command-set title/description
  reach the final asset?** The issue asks for exactly this decision: inherit, or keep the per-asset
  semantics and document it (the current state, already documented in
  `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`).
- **Explanation:** Inheritance can be implemented at the one step that crosses the boundary
  (`Step::Evaluate`), reusing the existing recipe-wins rule. The design specifies it.
- **Open questions:**
  1. **Proposed resolution — inherit command-set fields.** When the final asset receives its
     predecessor's value, the fields the predecessor's **commands** set are applied to the final
     asset as if its own command had set them. Later steps then overwrite as usual, and the
     recipe-wins rule still applies. Then `titled/retitled` yields `T2`/`D` whether or not the plan
     was cut. Cutting is an optimisation, so it should not change what a user sees. Fields a
     predecessor got from its own recipe, or a stored resource's title, are **not** inherited: they
     describe the predecessor, not the result.
  2. **Alternative — keep per-asset semantics.** No code. Close the issue as decided, since the
     caveat is already documented.

## Problem

`Context::set_title` / `set_description` write to the asset the command runs in. Plain evaluation
cuts the outermost cacheable prefix into a predecessor asset (`Plan::cut_predecessor`,
`Step::Evaluate`). In `titled/retitled`, `titled` sets `T1`/`D` on the predecessor, and the final
asset sees only what `retitled` sets (`T2`/empty). Applied to an input state, the plan stays
expanded and both steps share one context, giving `T2`/`D`. Whether a command's description
survives depends on whether it is the last action and on whether the caller passed an input state,
neither of which the command author can see.

## Expected behaviour and acceptance (inherit)

1. `titled/retitled` evaluated as a query (cut) gives `T2`/`D`, the same as `apply` with an input
   state (the existing test `a_later_step_in_the_same_query_wins` is joined by a cut-path twin).
2. A predecessor whose title came from **its recipe** does not pass it on.
3. A recipe-declared title on the final key still wins over an inherited one (recipe-wins, per field).
4. A resource-only predecessor (`fetched_key` case, the asset at a key) passes nothing on: its
   fields are not command-set.

## Scope

Title and description only. Other metadata is unchanged.

## Design Dependencies

- `context-title-description` (implemented 2026-10-06) — **overlaps**. It provides
  `set_description_fields_from_command` and the recipe flags this design builds on.

## Documentation assessment

- Reference: `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: replace the predecessor
  caveat with the inheritance rule (inherit answer), or leave it as is (keep answer).

## Consolidated Findings

- The predecessor runs in its own `AssetData`, which knows whether its recipe set a field
  (`recipe_sets_title` / `recipe_sets_description`) but not whether a command did. One flag per field
  (`command_set_title`, `command_set_description`), set inside `set_description_fields_from_command`
  when a value is applied, is enough.
- `Step::Evaluate` currently calls `context.get_dependency_state`, which returns only the `State`.
  To read the predecessor's flags it must use `submit` + `wait_for_dependency`, which return the
  `AssetRef`. `get_dependency_state` is exactly those two calls (`context.rs` ≈757).
