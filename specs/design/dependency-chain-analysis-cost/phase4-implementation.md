# Phase 4: Implementation Plan

Precondition: question 1 accepted (keep transitive records). Question 2 sets T1's bound.

1. **Measure.** Add B1, run it, and profile the 40-link case (`cargo flamegraph` if available,
   otherwise count calls: add temporary counters for `find_dependencies`, `recipe_opt`,
   `get_recipes` and `add_dependency`, and remove them afterwards). Record the numbers in the PR.
   Capture T2's fixture now.
2. **Lever A.** `DependencyMemo` and the extra parameter. Update the callers (search
   `find_dependencies(`). Proof: T2, T3, `cargo test -p liquers-core --lib --tests`. Re-run B1.
   Agent: sonnet tier; rust-best-practices.
3. **If B1 is still super-linear and the profile shows recipe parsing:** lever B in `recipes.rs`.
   Proof: the recipe provider tests, plus a test that a written `recipes.yaml` is re-read after
   `directory_changed`. Re-run B1.
4. **If neither suffices:** stop, record the profile, and file a follow-up issue on the dominant cost
   (likely the dependency manager), keeping what was gained.
5. Add T1 with the measured bound. Write the transitive-records paragraph in `DEPENDENCIES_STATUS.md`
   (History, `reviewed:`), the issue resolution with numbers, and the index. Diff review: no change to
   recorded dependency sets.
