# Phase 3: Examples and Tests

In `liquers-core/tests/context_title_description.rs` (existing commands `titled` and `retitled`):

| # | Kind | Checks |
|---|---|---|
| T1 | integration | `evaluate("titled/retitled")` (cut path) → final title `T2`, description `D` |
| T2 | integration | The existing `a_later_step_in_the_same_query_wins` (input-state path) is unchanged: `T2`/`D` |
| T3 | integration | A key whose recipe sets title `R`, evaluated with `titled` inside its prefix → final title `R` (recipe wins), description `D` (inherited) |
| T4 | integration | `-R/data/x.txt/-/retitled`, where `x.txt` has a recipe title `X` → final title `T2`, description empty (nothing inherited from a recipe-set field) |

Validate the queries with `liquers-validate --command titled --command retitled`, and confirm with
the digest that `titled/retitled` is cut into a predecessor (a `Step::Evaluate` in the plan).
