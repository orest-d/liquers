# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | integration (`liquers-core/tests/stale_edge_at_birth.rs`) | Dependency `a.txt` (recipe) evaluated to version `V1`; before evaluating `b.txt = -R/a.txt/-/upper`, register `V2` for `a.txt` directly in the dependency manager (simulating a write that landed after `b`'s read) by holding `b`'s evaluation in a command that waits on a `tokio::sync::Notify` the test triggers after `register_version`. `b` finishes `Expired` with `StaleDependency{a.txt}`. A second request recomputes `b`. |
| T2 | unit | Unknown recorded version (`Version(0)`) → no marking |
| T3 | unit | Equal versions → no marking |
| T4 | regression | `dependencies.rs` `add_dependency_records_a_disagreeing_version_without_expiring` unchanged |

T1's recipe queries are validated with `liquers-validate --command upper --command make_text`.
Use `ImmediateAssetManager` for determinism, and repeat on the queued manager in the same file.
