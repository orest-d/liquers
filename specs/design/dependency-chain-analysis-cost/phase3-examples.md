# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| B1 | benchmark (`#[ignore]`), `liquers-core/tests/dependency_chain_scaling.rs` | Evaluates chains of 10/20/40 links one link at a time (as the issue's experiment), printing time per size with `eprintln!` |
| T1 | test (not ignored) | 20-link chain evaluates within 10 s in debug (a smoke bound that fails today at ~6 s on a fast machine only if the bound is set to 5 s. Set the bound after measuring, at roughly 3× the post-fix time). |
| T2 | test | For a 6-link chain, the `DependencyRecord` key set of every link is identical with and without the memo (compute with a fresh environment twice; the memo cannot be disabled, so compare against a recorded fixture of today's keys, captured before the change) |
| T3 | regression | Existing recipe-cycle tests (`Circular dependency detected`) |
| T4 | regression | `dependency_audit_integration::cascade_over_100_link_chain` |

Chain setup: one `recipes.yaml` in `data/` with `l0.txt` = `make_text/l0.txt` and
`l{i}.txt` = `-R/data/l{i-1}.txt/-/upper/l{i}.txt`. Validate one of these with `liquers-validate
--command make_text --command upper`. Note the in-recipe form: a recipe query targeting `data/`
uses a plain filename (see the comment in `asset_manager_remove_expire_describe.rs`'s `env_with_at`).
